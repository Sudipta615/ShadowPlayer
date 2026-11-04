use super::{
    add_component, engagement_score, jaccard, personal_score, RecommendationConfig,
    RecommendationReason, ScoreBreakdown, Track,
};
use crate::{ArtistId, FeatureVector, TasteProfile, TrackId};
use std::cmp::Ordering;

#[derive(Clone, Copy)]
pub(crate) struct ScoredCandidate {
    pub(crate) track_id: TrackId,
    pub(crate) artist_id: Option<ArtistId>,
    pub(crate) features: Option<FeatureVector>,
    pub(crate) base_score: f32,
    /// `base_score - recency_penalty`, clamped to [0, 1].
    pub(crate) priority: f32,
    /// Tiny deterministic tie-break to avoid library-order bias when evidence is equal.
    pub(crate) tie_breaker: f32,
    /// Running maximum similarity to any already-selected track.
    pub(crate) max_similarity: f32,
    pub(crate) order: usize,
    pub(crate) reasons: RecommendationReason,
    pub(crate) breakdown: ScoreBreakdown,
}

#[derive(Clone, Copy)]
pub(crate) struct HeapEntry {
    pub(crate) candidate: ScoredCandidate,
    pub(crate) order: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
            && self.candidate.priority.to_bits() == other.candidate.priority.to_bits()
            && self.candidate.tie_breaker.to_bits() == other.candidate.tie_breaker.to_bits()
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse score order so the BinaryHeap root is the worst retained item.
        other
            .candidate
            .priority
            .total_cmp(&self.candidate.priority)
            .then_with(|| {
                other
                    .candidate
                    .tie_breaker
                    .total_cmp(&self.candidate.tie_breaker)
            })
            .then_with(|| self.order.cmp(&other.order))
    }
}

pub(crate) fn score_candidate(
    candidate: &Track,
    seed: Option<&Track>,
    profile: &TasteProfile,
    now_ms: i64,
    config: &RecommendationConfig,
    total_weight: f32,
) -> (f32, ScoreBreakdown, RecommendationReason) {
    let content_similarity = match (seed.and_then(|track| track.features), candidate.features) {
        (Some(seed_features), Some(features)) => seed_features.similarity(&features),
        _ => None,
    };
    let personal_preference = personal_score(candidate.features, profile);
    let track_affinity = profile.track_score(candidate.id);
    let engagement = engagement_score(&candidate.stats);
    let genre_affinity = profile
        .genre_score(&candidate.metadata.genre_ids)
        .or_else(|| {
            seed.and_then(|seed| jaccard(&seed.metadata.genre_ids, &candidate.metadata.genre_ids))
        });
    let artist_affinity = candidate
        .metadata
        .artist_id
        .and_then(|artist| profile.artist_score(artist))
        .or_else(|| {
            let seed_artist = seed.and_then(|track| track.metadata.artist_id)?;
            let candidate_artist = candidate.metadata.artist_id?;
            if seed_artist == candidate_artist {
                Some(1.0)
            } else {
                None
            }
        });
    let album_affinity = candidate
        .metadata
        .album_id
        .and_then(|album| profile.album_score(album))
        .or_else(|| {
            let seed_album = seed.and_then(|track| track.metadata.album_id)?;
            let candidate_album = candidate.metadata.album_id?;
            if seed_album == candidate_album {
                Some(1.0)
            } else {
                None
            }
        });
    let transition_affinity = seed.and_then(|seed| profile.transition_score(seed.id, candidate.id));
    let play_affinity = if candidate.stats.play_count > 0 {
        let count = candidate.stats.play_count as f32;
        Some((count / (count + 5.0)).clamp(0.0, 1.0))
    } else {
        None
    };
    let novelty = (1.0 / (1.0 + (candidate.stats.play_count as f32).ln_1p())).clamp(0.0, 1.0);
    let freshness = candidate.metadata.added_at_ms.map(|added| {
        let age_ms = now_ms.saturating_sub(added).max(0) as f32;
        let half_life_ms = config.freshness_half_life_days * 86_400_000.0;
        (-std::f32::consts::LN_2 * age_ms / half_life_ms)
            .exp()
            .clamp(0.0, 1.0)
    });

    let mut breakdown = ScoreBreakdown {
        content_similarity,
        personal_preference,
        track_affinity,
        engagement,
        genre_affinity,
        artist_affinity,
        album_affinity,
        transition_affinity,
        play_affinity,
        novelty,
        freshness,
        coverage: 0.0,
    };

    let mut weighted_sum = 0.0_f32;
    let mut active_weight = 0.0_f32;
    let mut evidence_weight = 0.0_f32;
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        content_similarity,
        config.weight_content,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        personal_preference,
        config.weight_personal,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        track_affinity,
        config.weight_track_affinity,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        engagement,
        config.weight_engagement,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        genre_affinity,
        config.weight_genre,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        artist_affinity,
        config.weight_artist,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        album_affinity,
        config.weight_album,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        transition_affinity,
        config.weight_transition,
    );
    evidence_weight += add_component(
        &mut weighted_sum,
        &mut active_weight,
        play_affinity,
        config.weight_play_history,
    );
    add_component(
        &mut weighted_sum,
        &mut active_weight,
        Some(novelty),
        config.weight_novelty,
    );
    add_component(
        &mut weighted_sum,
        &mut active_weight,
        freshness,
        config.weight_freshness,
    );

    let raw = if active_weight > f32::EPSILON {
        weighted_sum / active_weight
    } else {
        config.prior_score
    };
    // Novelty and freshness are always available and therefore are not counted
    // as evidence. Coverage measures personalized/contextual evidence only.
    let evidence_total = (total_weight - config.weight_novelty - config.weight_freshness).max(0.0);
    let coverage = if evidence_total > f32::EPSILON {
        (evidence_weight / evidence_total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Shrink sparse candidates toward neutral instead of letting a single
    // always-available signal (novelty) win by renormalization.
    let base = (config.prior_score + (raw - config.prior_score) * coverage).clamp(0.0, 1.0);
    breakdown.coverage = coverage;

    let mut reasons = RecommendationReason::default();
    if content_similarity.is_some_and(|value| value >= 0.72) {
        reasons.insert(RecommendationReason::SIMILAR_TO_SEED);
    }
    if personal_preference.is_some_and(|value| value >= 0.70) {
        reasons.insert(RecommendationReason::MATCHES_TASTE);
    }
    if track_affinity.is_some_and(|value| value >= 0.75) {
        reasons.insert(RecommendationReason::TRACK_AFFINITY);
    }
    if candidate.stats.rating.is_some_and(|rating| rating >= 4.0)
        || (candidate.stats.like_count > 0
            && candidate.stats.like_count >= candidate.stats.dislike_count)
    {
        reasons.insert(RecommendationReason::HIGHLY_RATED);
    }
    if play_affinity.is_some_and(|value| value >= 0.50) {
        reasons.insert(RecommendationReason::FREQUENTLY_PLAYED);
    }
    if novelty >= 0.80 {
        reasons.insert(RecommendationReason::DISCOVERY);
    }
    if genre_affinity.is_some_and(|value| value >= 0.72) {
        reasons.insert(RecommendationReason::GENRE_MATCH);
    }
    if album_affinity.is_some_and(|value| value >= 0.75) {
        reasons.insert(RecommendationReason::ALBUM_MATCH);
    }
    if freshness.is_some_and(|value| value >= 0.80) {
        reasons.insert(RecommendationReason::RECENTLY_ADDED);
    }
    (base, breakdown, reasons)
}

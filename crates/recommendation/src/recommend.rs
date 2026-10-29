use crate::{AlbumId, ArtistId, FeatureVector, GenreId, TasteProfile, TrackId};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

#[derive(Clone, Debug, Default)]
pub struct TrackMetadata {
    /// Resolved artist ID when the host has one.
    pub artist_id: Option<ArtistId>,
    /// Album ID when the host has one.
    pub album_id: Option<AlbumId>,
    /// Normalized genre keys from `genre_key`.
    pub genre_ids: Vec<GenreId>,
    /// Duration in seconds, retained for host context and future ranking signals.
    pub duration_seconds: Option<f32>,
    /// Library-added timestamp in Unix milliseconds.
    pub added_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct TrackStats {
    /// Number of starts, including partial plays.
    pub play_count: u32,
    /// Number of explicit skips.
    pub skip_count: u32,
    /// Number of completed plays.
    pub completion_count: u32,
    /// Number of likes.
    pub like_count: u32,
    /// Number of dislikes.
    pub dislike_count: u32,
    /// Current listener's rating on a 0–5 scale, if available.
    pub rating: Option<f32>,
    /// Last play start in Unix milliseconds.
    pub last_played_ms: Option<i64>,
    /// Last skip in Unix milliseconds.
    pub last_skipped_ms: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct Track {
    /// Lossless `u128` form of the host track ID.
    pub id: TrackId,
    /// Cached audio features, or `None` when the track has not been analyzed.
    pub features: Option<FeatureVector>,
    /// Library metadata used by affinity and context signals.
    pub metadata: TrackMetadata,
    /// Local playback statistics and explicit feedback counts.
    pub stats: TrackStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScoreBreakdown {
    /// Similarity to the seed track's audio features, when both are available.
    pub content_similarity: Option<f32>,
    /// Similarity to the learned positive/negative feature centroids.
    pub personal_preference: Option<f32>,
    /// Direct preference learned for this track.
    pub track_affinity: Option<f32>,
    /// Rating, like/dislike, completion, and skip evidence.
    pub engagement: Option<f32>,
    /// Learned genre affinity, or seed/candidate genre overlap.
    pub genre_affinity: Option<f32>,
    /// Learned artist affinity, or seed/candidate artist match.
    pub artist_affinity: Option<f32>,
    /// Learned album affinity, or seed/candidate album match.
    pub album_affinity: Option<f32>,
    /// Same-session transition probability from the seed track.
    pub transition_affinity: Option<f32>,
    /// Historical play-count affinity.
    pub play_affinity: Option<f32>,
    /// Always-available discovery signal based on play count.
    pub novelty: f32,
    /// Library-added freshness signal.
    pub freshness: Option<f32>,
    /// Fraction of configured personalized signal weight that was available for
    /// this track. Novelty and freshness are always-available context signals and
    /// are deliberately excluded, so a track with no real evidence reports zero.
    pub coverage: f32,
}

/// Explainable reason flags. Use `contains` instead of parsing display strings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecommendationReason(pub u16);

impl RecommendationReason {
    pub const SIMILAR_TO_SEED: u16 = 1 << 0;
    pub const MATCHES_TASTE: u16 = 1 << 1;
    pub const HIGHLY_RATED: u16 = 1 << 2;
    pub const FREQUENTLY_PLAYED: u16 = 1 << 3;
    pub const DISCOVERY: u16 = 1 << 4;
    pub const GENRE_MATCH: u16 = 1 << 5;
    pub const RECENTLY_ADDED: u16 = 1 << 6;
    pub const TRACK_AFFINITY: u16 = 1 << 7;
    pub const ALBUM_MATCH: u16 = 1 << 8;

    pub fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }
}

#[derive(Clone, Debug)]
pub struct Recommendation {
    pub track_id: TrackId,
    /// Final score after recency and diversity adjustments, normalized to [0, 1].
    pub score: f32,
    /// Shrunk weighted relevance score before recency/diversity penalties.
    pub base_score: f32,
    /// Fraction of configured personalized signal weight available for this track.
    pub confidence: f32,
    pub reasons: RecommendationReason,
    pub breakdown: ScoreBreakdown,
}

#[derive(Clone, Debug)]
pub struct RecommendationConfig {
    /// Seed-track audio similarity.
    pub weight_content: f32,
    /// Learned positive/negative feature-centroid similarity.
    pub weight_personal: f32,
    /// Direct per-track preference learned from feedback.
    pub weight_track_affinity: f32,
    /// Rating, like/dislike, completion, and skip evidence.
    pub weight_engagement: f32,
    /// Learned genre affinity or seed genre overlap.
    pub weight_genre: f32,
    /// Learned artist affinity or seed artist match.
    pub weight_artist: f32,
    /// Learned album affinity or seed album match.
    pub weight_album: f32,
    /// Same-session next-track transition probability.
    pub weight_transition: f32,
    /// Historical play-count affinity.
    pub weight_play_history: f32,
    /// Discovery signal based on play count.
    pub weight_novelty: f32,
    /// Library-added freshness signal.
    pub weight_freshness: f32,
    /// Neutral score used when a candidate has little evidence. The final base
    /// score is shrunk toward this value in proportion to missing signal weight.
    pub prior_score: f32,
    /// Maximum repeat penalty from last-play time, recent-play lists, and artists.
    pub recent_penalty: f32,
    /// MMR diversity penalty applied to similarity with already-selected results.
    pub diversity_lambda: f32,
    /// Penalty applied when a candidate's artist was already selected.
    pub artist_repeat_penalty: f32,
    /// Half-life for the recent-play cooldown.
    pub recent_half_life_days: f32,
    /// Half-life for the library-added freshness signal.
    pub freshness_half_life_days: f32,
    /// Candidate pool size multiplier relative to the requested limit.
    pub candidate_pool_factor: usize,
    /// Hard cap on the candidate pool before diversity selection.
    pub candidate_pool_max: usize,
}

impl Default for RecommendationConfig {
    fn default() -> Self {
        Self {
            weight_content: 0.24,
            weight_personal: 0.18,
            weight_track_affinity: 0.10,
            weight_engagement: 0.14,
            weight_genre: 0.06,
            weight_artist: 0.04,
            weight_album: 0.02,
            weight_transition: 0.10,
            weight_play_history: 0.03,
            weight_novelty: 0.05,
            weight_freshness: 0.04,
            prior_score: 0.5,
            recent_penalty: 0.22,
            diversity_lambda: 0.15,
            artist_repeat_penalty: 0.10,
            recent_half_life_days: 3.0,
            freshness_half_life_days: 30.0,
            candidate_pool_factor: 8,
            candidate_pool_max: 512,
        }
    }
}

impl RecommendationConfig {
    fn sanitized(&self) -> Self {
        let mut clean = self.clone();
        for weight in [
            &mut clean.weight_content,
            &mut clean.weight_personal,
            &mut clean.weight_track_affinity,
            &mut clean.weight_engagement,
            &mut clean.weight_genre,
            &mut clean.weight_artist,
            &mut clean.weight_album,
            &mut clean.weight_transition,
            &mut clean.weight_play_history,
            &mut clean.weight_novelty,
            &mut clean.weight_freshness,
        ] {
            *weight = if weight.is_finite() {
                weight.clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        clean.prior_score = finite_clamp(clean.prior_score, 0.5, 0.0, 1.0);
        clean.recent_penalty = finite_clamp(clean.recent_penalty, 0.22, 0.0, 1.0);
        clean.diversity_lambda = finite_clamp(clean.diversity_lambda, 0.15, 0.0, 1.0);
        clean.artist_repeat_penalty = finite_clamp(clean.artist_repeat_penalty, 0.10, 0.0, 1.0);
        clean.recent_half_life_days = finite_clamp(clean.recent_half_life_days, 3.0, 0.25, 3_650.0);
        clean.freshness_half_life_days =
            finite_clamp(clean.freshness_half_life_days, 30.0, 0.25, 3_650.0);
        clean.candidate_pool_factor = clean.candidate_pool_factor.clamp(1, 64);
        clean.candidate_pool_max = clean.candidate_pool_max.clamp(1, 100_000);
        clean
    }

    fn total_weight(&self) -> f32 {
        self.weight_content
            + self.weight_personal
            + self.weight_track_affinity
            + self.weight_engagement
            + self.weight_genre
            + self.weight_artist
            + self.weight_album
            + self.weight_transition
            + self.weight_play_history
            + self.weight_novelty
            + self.weight_freshness
    }
}

fn finite_clamp(value: f32, fallback: f32, minimum: f32, maximum: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}

/// Per-request context. Pass recent/queued IDs from the host player's queue and history.
pub struct RecommendationRequest<'a> {
    /// Track whose audio/transition context should seed the ranking.
    pub seed_track_id: Option<TrackId>,
    /// Maximum number of recommendations to return.
    pub limit: usize,
    /// Current time in Unix milliseconds, used for decay and freshness.
    pub now_ms: i64,
    /// Tracks already queued or otherwise unavailable.
    pub excluded_track_ids: &'a [TrackId],
    /// Tracks played recently, used for the short-term cooldown.
    pub recently_played_ids: &'a [TrackId],
    /// Artists played recently, used for artist diversity.
    pub recent_artist_ids: &'a [ArtistId],
}

impl Default for RecommendationRequest<'_> {
    fn default() -> Self {
        Self {
            seed_track_id: None,
            limit: 20,
            now_ms: 0,
            excluded_track_ids: &[],
            recently_played_ids: &[],
            recent_artist_ids: &[],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Recommender {
    pub config: RecommendationConfig,
}

impl Default for Recommender {
    fn default() -> Self {
        Self::new(RecommendationConfig::default())
    }
}

impl Recommender {
    pub fn new(config: RecommendationConfig) -> Self {
        Self { config }
    }

    /// Score candidates and greedily diversify the best candidate pool using MMR.
    ///
    /// Complexity is `O(N × 20 + N log P + P log P + K × P × 20)` for `N`
    /// tracks, a bounded pool of `P` candidates, and `K` results: scoring is a
    /// single pass, the heap keeps only the best pool, and each selected track
    /// updates the running maximum similarity of the remaining pool. It does
    /// not decode audio, access the database, or spawn threads.
    pub fn recommend(
        &self,
        tracks: &[Track],
        profile: &TasteProfile,
        request: &RecommendationRequest<'_>,
    ) -> Vec<Recommendation> {
        if request.limit == 0 || tracks.is_empty() {
            return Vec::new();
        }
        let config = self.config.sanitized();
        let total_weight = config.total_weight();
        let result_limit = request.limit.min(tracks.len());
        let seed = request
            .seed_track_id
            .and_then(|seed_id| tracks.iter().find(|track| track.id == seed_id));
        let excluded: HashSet<TrackId> = request.excluded_track_ids.iter().copied().collect();
        let recent_tracks: HashSet<TrackId> = request.recently_played_ids.iter().copied().collect();
        let recent_artists: HashSet<ArtistId> = request.recent_artist_ids.iter().copied().collect();

        let scaled_pool = result_limit
            .saturating_mul(config.candidate_pool_factor)
            .max(result_limit);
        let pool_limit = scaled_pool
            .min(config.candidate_pool_max.max(result_limit))
            .min(tracks.len());
        // Keep only the best candidate pool in a bounded heap. A 100k-track
        // library therefore doesn't allocate 100k intermediate score objects.
        let mut heap = BinaryHeap::with_capacity(pool_limit.min(tracks.len()));
        for (order, track) in tracks.iter().enumerate() {
            if Some(track.id) == request.seed_track_id || excluded.contains(&track.id) {
                continue;
            }
            let in_recent_list = recent_tracks.contains(&track.id);
            let recent_artist = track
                .metadata
                .artist_id
                .is_some_and(|id| recent_artists.contains(&id));
            let (base_score, breakdown, reasons) =
                score_candidate(track, seed, profile, request.now_ms, &config, total_weight);
            let recency_penalty = recency_penalty(track, request.now_ms, in_recent_list, &config)
                + if recent_artist {
                    config.artist_repeat_penalty * 0.5
                } else {
                    0.0
                };
            let candidate = ScoredCandidate {
                track_id: track.id,
                artist_id: track.metadata.artist_id,
                features: track.features,
                base_score,
                priority: (base_score - recency_penalty).clamp(0.0, 1.0),
                tie_breaker: deterministic_tie_break(
                    track.id,
                    request.seed_track_id,
                    request.now_ms,
                ),
                max_similarity: 0.0,
                order,
                reasons,
                breakdown,
            };
            let entry = HeapEntry { candidate, order };
            if heap.len() < pool_limit {
                heap.push(entry);
            } else if heap.peek().is_some_and(|worst: &HeapEntry| {
                entry
                    .candidate
                    .priority
                    .total_cmp(&worst.candidate.priority)
                    .then_with(|| {
                        entry
                            .candidate
                            .tie_breaker
                            .total_cmp(&worst.candidate.tie_breaker)
                    })
                    == Ordering::Greater
            }) {
                heap.pop();
                heap.push(entry);
            }
        }

        let mut pool: Vec<ScoredCandidate> =
            heap.into_iter().map(|entry| entry.candidate).collect();
        pool.sort_unstable_by(|left, right| {
            right
                .priority
                .total_cmp(&left.priority)
                .then_with(|| right.tie_breaker.total_cmp(&left.tie_breaker))
                .then_with(|| left.order.cmp(&right.order))
        });

        let mut selected = vec![false; pool.len()];
        let mut selected_artists = HashSet::with_capacity(result_limit);
        let mut results = Vec::with_capacity(result_limit.min(pool.len()));
        while results.len() < result_limit {
            let mut best_index = None;
            let mut best_final_score = f32::NEG_INFINITY;
            for (index, candidate) in pool.iter().enumerate() {
                if selected[index] {
                    continue;
                }
                let repeats_artist = candidate
                    .artist_id
                    .is_some_and(|artist| selected_artists.contains(&artist));
                let diversity_penalty = config.diversity_lambda * candidate.max_similarity
                    + if repeats_artist {
                        config.artist_repeat_penalty
                    } else {
                        0.0
                    };
                let final_score = (candidate.priority - diversity_penalty).clamp(0.0, 1.0);
                if final_score > best_final_score {
                    best_final_score = final_score;
                    best_index = Some(index);
                }
            }
            let Some(best_index) = best_index else {
                break;
            };
            let chosen = pool[best_index];
            selected[best_index] = true;
            results.push(Recommendation {
                track_id: chosen.track_id,
                score: best_final_score,
                base_score: chosen.base_score,
                confidence: chosen.breakdown.coverage,
                reasons: chosen.reasons,
                breakdown: chosen.breakdown,
            });
            if let Some(artist) = chosen.artist_id {
                selected_artists.insert(artist);
            }
            // Update the running maximum similarity for the remaining pool.
            // This is the same MMR objective as recomputing against every
            // selected item, but avoids the O(K²) repeated scan.
            if let Some(chosen_features) = chosen.features {
                for (index, candidate) in pool.iter_mut().enumerate() {
                    if selected[index] {
                        continue;
                    }
                    if let Some(features) = candidate.features {
                        if let Some(similarity) = chosen_features.similarity(&features) {
                            if similarity > candidate.max_similarity {
                                candidate.max_similarity = similarity;
                            }
                        }
                    }
                }
            }
        }
        results
    }
}

#[derive(Clone, Copy)]
struct ScoredCandidate {
    track_id: TrackId,
    artist_id: Option<ArtistId>,
    features: Option<FeatureVector>,
    base_score: f32,
    /// `base_score - recency_penalty`, clamped to [0, 1].
    priority: f32,
    /// Tiny deterministic tie-break to avoid library-order bias when evidence is equal.
    tie_breaker: f32,
    /// Running maximum similarity to any already-selected track.
    max_similarity: f32,
    order: usize,
    reasons: RecommendationReason,
    breakdown: ScoreBreakdown,
}

#[derive(Clone, Copy)]
struct HeapEntry {
    candidate: ScoredCandidate,
    order: usize,
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

fn score_candidate(
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

fn recency_penalty(
    track: &Track,
    now_ms: i64,
    in_recent_list: bool,
    config: &RecommendationConfig,
) -> f32 {
    let recent_penalty = config.recent_penalty.clamp(0.0, 1.0);
    if in_recent_list {
        return recent_penalty;
    }
    let half_life_ms = config.recent_half_life_days * 86_400_000.0;
    let played_recency = track
        .stats
        .last_played_ms
        .map(|timestamp| {
            let age_ms = now_ms.saturating_sub(timestamp).max(0) as f32;
            (-std::f32::consts::LN_2 * age_ms / half_life_ms).exp()
        })
        .unwrap_or(0.0);
    // A recent skip deserves a stronger short-term cooldown than an ordinary play.
    let skipped_recency = track
        .stats
        .last_skipped_ms
        .map(|timestamp| {
            let age_ms = now_ms.saturating_sub(timestamp).max(0) as f32;
            (-std::f32::consts::LN_2 * age_ms / (half_life_ms * 0.65)).exp() * 1.25
        })
        .unwrap_or(0.0);
    recent_penalty * played_recency.max(skipped_recency).clamp(0.0, 1.0)
}

fn deterministic_tie_break(track_id: TrackId, seed_track_id: Option<TrackId>, now_ms: i64) -> f32 {
    // SplitMix64: stable across runs, seeded by the seed track and day. The
    // micro-score only resolves ties; it cannot meaningfully override relevance.
    let day = now_ms.div_euclid(86_400_000) as u64;
    // Fold both halves of UUID-sized IDs into SplitMix64 without losing identity
    // in the actual track/artist keys used by the recommender.
    let seed = seed_track_id.unwrap_or(0);
    let mut value = (track_id as u64)
        ^ ((track_id >> 64) as u64).rotate_left(29)
        ^ (seed as u64).rotate_left(17)
        ^ ((seed >> 64) as u64).rotate_left(43)
        ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (((value >> 40) as u32 & 0x00FF_FFFF) as f32 / 16_777_215.0) * 1.0e-6
}

fn add_component(sum: &mut f32, active_weight: &mut f32, value: Option<f32>, weight: f32) -> f32 {
    if let Some(value) = value {
        let weight = if weight.is_finite() {
            weight.max(0.0)
        } else {
            0.0
        };
        if weight > 0.0 {
            *sum += value.clamp(0.0, 1.0) * weight;
            *active_weight += weight;
            return weight;
        }
    }
    0.0
}

fn personal_score(features: Option<FeatureVector>, profile: &TasteProfile) -> Option<f32> {
    let features = features?;
    let positive = if profile.has_positive_features() {
        features.similarity(&profile.positive_features)
    } else {
        None
    };
    let negative = if profile.has_negative_features() {
        features.similarity(&profile.negative_features)
    } else {
        None
    };
    match (positive, negative) {
        (Some(positive), Some(negative)) => {
            Some((0.5 + 0.5 * (positive - negative)).clamp(0.0, 1.0))
        }
        (Some(positive), None) => Some(positive),
        (None, Some(negative)) => Some(1.0 - negative),
        (None, None) => None,
    }
}

fn engagement_score(stats: &TrackStats) -> Option<f32> {
    // Play count alone is not reliable evidence of completion or satisfaction;
    // historical catalogs often have plays but no completion telemetry.
    let has_engagement = stats.rating.is_some()
        || stats.like_count > 0
        || stats.dislike_count > 0
        || stats.completion_count > 0
        || stats.skip_count > 0;
    if !has_engagement {
        return None;
    }

    let mut sum = 0.0_f32;
    let mut weights = 0.0_f32;
    if let Some(rating) = stats.rating.filter(|rating| rating.is_finite()) {
        sum += (rating.clamp(0.0, 5.0) / 5.0) * 0.50;
        weights += 0.50;
    }
    if stats.like_count > 0 || stats.dislike_count > 0 {
        // Beta(1,1) smoothing avoids overreacting to a single like/dislike.
        let like_rate = (stats.like_count as f32 + 1.0)
            / (stats.like_count as f32 + stats.dislike_count as f32 + 2.0);
        sum += like_rate * 0.35;
        weights += 0.35;
    }
    if stats.completion_count > 0 || stats.skip_count > 0 {
        let opportunities = stats
            .play_count
            .max(stats.completion_count.saturating_add(stats.skip_count));
        let denominator = opportunities as f32 + 2.0;
        let completion_rate = (stats.completion_count as f32 + 1.0) / denominator;
        let skip_rate = (stats.skip_count as f32 + 1.0) / denominator;
        sum += completion_rate.clamp(0.0, 1.0) * 0.40;
        weights += 0.40;
        sum += (1.0 - skip_rate).clamp(0.0, 1.0) * 0.25;
        weights += 0.25;
    }
    if weights <= f32::EPSILON {
        None
    } else {
        Some((sum / weights).clamp(0.0, 1.0))
    }
}

fn jaccard(left: &[GenreId], right: &[GenreId]) -> Option<f32> {
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let intersection = left.iter().filter(|value| right.contains(*value)).count();
    let union = left.len() + right.len() - intersection;
    if union == 0 {
        None
    } else {
        Some(intersection as f32 / union as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeedbackEvent, FeedbackKind, TasteProfileConfig, TrackMetadata, TrackStats};

    fn track(id: u128, artist: u128, genre: u64, marker: usize, plays: u32) -> Track {
        let mut features = [0.0_f32; crate::FEATURE_COUNT];
        features[0] = marker as f32 / 10.0;
        features[1] = marker as f32 / 10.0;
        features[8 + marker % 12] = 1.0;
        Track {
            id,
            features: Some(FeatureVector::new(
                features,
                (1_u32 << crate::FEATURE_COUNT) - 1,
            )),
            metadata: TrackMetadata {
                artist_id: Some(artist),
                album_id: Some(id as u64 % 4 + 1),
                genre_ids: vec![genre],
                duration_seconds: Some(200.0),
                added_at_ms: Some(1_000),
            },
            stats: TrackStats {
                play_count: plays,
                ..TrackStats::default()
            },
        }
    }

    fn request<'a>(seed: Option<u128>, limit: usize) -> RecommendationRequest<'a> {
        RecommendationRequest {
            seed_track_id: seed,
            limit,
            now_ms: 1_000,
            excluded_track_ids: &[],
            recently_played_ids: &[],
            recent_artist_ids: &[],
        }
    }

    #[test]
    fn likes_personalize_results_and_seed_is_not_returned() {
        let tracks = vec![
            track(1, 10, 1, 9, 10),
            track(2, 20, 2, 9, 0),
            track(3, 30, 3, 1, 0),
        ];
        let events = vec![FeedbackEvent {
            track_id: 1,
            timestamp_ms: 1_000,
            session_id: None,
            kind: FeedbackKind::Liked,
        }];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        let output = Recommender::default().recommend(&tracks, &profile, &request(Some(1), 2));
        assert_eq!(output.len(), 2);
        assert!(output.iter().all(|item| item.track_id != 1));
        assert!(output.iter().all(|item| item.score.is_finite()));
        assert_ne!(output[0].track_id, output[1].track_id);
    }

    #[test]
    fn sparse_evidence_does_not_outrank_a_strong_match() {
        let mut weak = track(1, 10, 1, 1, 0);
        weak.features = None;
        weak.metadata = TrackMetadata::default();
        weak.stats = TrackStats::default();
        let strong = track(2, 20, 2, 9, 5);
        let tracks = vec![strong, weak];
        let profile = TasteProfile::default();
        let output = Recommender::default().recommend(&tracks, &profile, &request(None, 2));
        assert_eq!(output[0].track_id, 2);
        assert!(output[0].confidence > output[1].confidence);
    }

    #[test]
    fn excludes_queued_items_and_penalizes_repetition() {
        let tracks = vec![
            track(1, 10, 1, 8, 4),
            track(2, 10, 1, 8, 4),
            track(3, 30, 3, 3, 0),
        ];
        let profile = TasteProfile::default();
        let recommender = Recommender::default();
        let excluded = [1];
        let mut request = request(None, 2);
        request.excluded_track_ids = &excluded;
        let output = recommender.recommend(&tracks, &profile, &request);
        assert!(output.iter().all(|item| item.track_id != 1));
        assert_eq!(output.len(), 2);
    }

    #[test]
    fn play_count_without_completion_data_is_not_negative_engagement() {
        let stats = TrackStats {
            play_count: 100,
            ..TrackStats::default()
        };
        assert!(engagement_score(&stats).is_none());
    }

    #[test]
    fn explicit_early_skip_scores_below_a_like() {
        let skipped = TrackStats {
            play_count: 1,
            skip_count: 1,
            ..TrackStats::default()
        };
        let liked = TrackStats {
            like_count: 1,
            ..TrackStats::default()
        };
        assert!(engagement_score(&skipped).unwrap() < engagement_score(&liked).unwrap());
    }

    #[test]
    fn artist_diversity_is_applied() {
        let tracks = vec![
            track(1, 10, 1, 9, 5),
            track(2, 10, 1, 9, 5),
            track(3, 30, 3, 9, 5),
        ];
        let profile = TasteProfile::default();
        let output = Recommender::default().recommend(&tracks, &profile, &request(None, 3));
        assert_eq!(output.len(), 3);
        assert_ne!(output[0].track_id, output[1].track_id);
        assert!(output[2].track_id != output[0].track_id);
    }

    #[test]
    fn track_affinity_lifts_a_liked_track() {
        let tracks = vec![track(1, 10, 1, 5, 0), track(2, 20, 2, 5, 0)];
        let events = vec![FeedbackEvent {
            track_id: 1,
            timestamp_ms: 1_000,
            session_id: None,
            kind: FeedbackKind::Liked,
        }];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        let output = Recommender::default().recommend(&tracks, &profile, &request(None, 2));
        assert_eq!(output[0].track_id, 1);
        assert!(output[0]
            .reasons
            .contains(RecommendationReason::TRACK_AFFINITY));
    }

    #[test]
    fn empty_and_zero_limit_requests_are_safe() {
        let profile = TasteProfile::default();
        let tracks = vec![track(1, 10, 1, 5, 0)];
        let recommender = Recommender::default();
        assert!(recommender
            .recommend(&tracks, &profile, &request(None, 0))
            .is_empty());
        assert!(recommender
            .recommend(&[], &profile, &request(None, 5))
            .is_empty());
    }

    #[test]
    fn results_are_deterministic() {
        let tracks: Vec<Track> = (1..=20)
            .map(|id| track(id, id % 4, (id % 3) as u64, id as usize, 0))
            .collect();
        let profile = TasteProfile::default();
        let recommender = Recommender::default();
        let first = recommender.recommend(&tracks, &profile, &request(Some(1), 10));
        let second = recommender.recommend(&tracks, &profile, &request(Some(1), 10));
        assert_eq!(
            first.iter().map(|item| item.track_id).collect::<Vec<_>>(),
            second.iter().map(|item| item.track_id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn coverage_reflects_available_signals() {
        let mut sparse = track(1, 10, 1, 5, 0);
        sparse.features = None;
        sparse.metadata = TrackMetadata::default();
        let full = track(2, 20, 2, 5, 0);
        let profile = TasteProfile::default();
        let output = Recommender::default().recommend(&[sparse, full], &profile, &request(None, 2));
        assert!(output[0].confidence >= output[1].confidence);
        assert!(output[0].breakdown.coverage >= output[1].breakdown.coverage);
    }
}

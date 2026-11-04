use crate::{AlbumId, ArtistId, FeatureVector, GenreId, TrackId};
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

mod helpers;
mod misc;
mod recommender;
mod scoring;

pub(crate) use helpers::finite_clamp;
pub(crate) use misc::{
    add_component, deterministic_tie_break, engagement_score, jaccard, personal_score,
    recency_penalty,
};
pub use recommender::Recommender;
pub(crate) use scoring::{score_candidate, HeapEntry, ScoredCandidate};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TasteProfile;
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

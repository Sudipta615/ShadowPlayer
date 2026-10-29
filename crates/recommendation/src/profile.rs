use crate::{AlbumId, ArtistId, FeatureVector, GenreId, Track, TrackId, FEATURE_COUNT};
use std::collections::HashMap;
use std::hash::Hash;

/// A locally recorded user interaction. No account or telemetry service is required.
#[derive(Clone, Debug)]
pub struct FeedbackEvent {
    pub track_id: TrackId,
    pub timestamp_ms: i64,
    pub session_id: Option<u64>,
    pub kind: FeedbackKind,
}

#[derive(Clone, Copy, Debug)]
pub enum FeedbackKind {
    /// A play start is only a weak positive signal; it is not equivalent to a like.
    PlayStarted,
    Progress {
        listened_fraction: f32,
    },
    Completed,
    Skipped {
        listened_fraction: f32,
    },
    Liked,
    Disliked,
    Rated {
        rating: f32,
    },
    Replay,
    AddedToPlaylist,
}

impl FeedbackKind {
    /// Signed preference strength. Positive means reinforce the track's features;
    /// negative means suppress tracks with similar features.
    pub fn signal(&self) -> f32 {
        match *self {
            Self::PlayStarted => 0.12,
            Self::Progress { listened_fraction } => {
                let fraction = finite_fraction(listened_fraction);
                if fraction >= 0.80 {
                    0.55
                } else if fraction <= 0.10 {
                    -0.35
                } else {
                    0.08
                }
            }
            Self::Completed => 0.70,
            Self::Skipped { listened_fraction } => {
                let fraction = finite_fraction(listened_fraction);
                if fraction < 0.10 {
                    -1.20
                } else if fraction < 0.35 {
                    -0.80
                } else if fraction < 0.70 {
                    -0.35
                } else {
                    0.05
                }
            }
            Self::Liked => 2.0,
            Self::Disliked => -3.0,
            Self::Rated { rating } => {
                if rating.is_finite() {
                    (rating.clamp(0.0, 5.0) - 3.0) * 0.70
                } else {
                    0.0
                }
            }
            Self::Replay => 0.90,
            Self::AddedToPlaylist => 0.80,
        }
    }
}

fn finite_fraction(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug)]
pub struct TasteProfileConfig {
    /// Events lose half their weight after this many days.
    pub half_life_days: f32,
    /// Maximum accumulated decay weight one track may contribute. This prevents
    /// a single heavily repeated track from dominating the whole profile.
    pub max_track_weight: f32,
    /// Maximum gap between two play starts that still counts as a transition.
    pub transition_gap_ms: i64,
    /// Additive smoothing applied to genre/artist/album/track affinity ratios.
    /// A value of 1.0 keeps one like from producing a perfect affinity score.
    pub affinity_smoothing: f32,
    /// Scales affinity ratios before the saturating tanh transform.
    pub affinity_scale: f32,
}

impl Default for TasteProfileConfig {
    fn default() -> Self {
        Self {
            half_life_days: 45.0,
            max_track_weight: 3.0,
            transition_gap_ms: 3 * 60 * 60 * 1_000,
            affinity_smoothing: 1.0,
            affinity_scale: 0.75,
        }
    }
}

impl TasteProfileConfig {
    fn sanitized(&self) -> Self {
        Self {
            half_life_days: if self.half_life_days.is_finite() {
                self.half_life_days.clamp(1.0, 3_650.0)
            } else {
                45.0
            },
            max_track_weight: if self.max_track_weight.is_finite() {
                self.max_track_weight.clamp(0.25, 100.0)
            } else {
                3.0
            },
            transition_gap_ms: self.transition_gap_ms.clamp(60_000, 24 * 60 * 60 * 1_000),
            affinity_smoothing: if self.affinity_smoothing.is_finite() {
                self.affinity_smoothing.clamp(0.0, 100.0)
            } else {
                1.0
            },
            affinity_scale: if self.affinity_scale.is_finite() {
                self.affinity_scale.clamp(0.01, 10.0)
            } else {
                0.75
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AffinityAccumulator {
    signed_sum: f64,
    total_weight: f64,
}

impl AffinityAccumulator {
    fn add(&mut self, signal: f32, weight: f32) {
        let weight = weight.max(0.0) as f64;
        if weight <= 0.0 || !signal.is_finite() {
            return;
        }
        self.signed_sum += signal as f64 * weight;
        self.total_weight += weight;
    }

    fn score(self, smoothing: f32, scale: f32) -> Option<f32> {
        if self.total_weight <= f64::EPSILON {
            return None;
        }
        let denominator = self.total_weight + smoothing.max(0.0) as f64;
        if denominator <= f64::EPSILON {
            return None;
        }
        let ratio = (self.signed_sum / denominator) as f32;
        Some((0.5 + 0.5 * (ratio * scale).tanh()).clamp(0.0, 1.0))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct TrackAccumulator {
    signed_sum: f64,
    weight_sum: f64,
}

impl TrackAccumulator {
    fn add(&mut self, signal: f32, decay: f32) {
        if !signal.is_finite() || !decay.is_finite() || decay <= 0.0 {
            return;
        }
        self.signed_sum += signal as f64 * decay as f64;
        self.weight_sum += decay as f64;
    }

    fn mean_signal(self) -> f32 {
        if self.weight_sum <= f64::EPSILON {
            0.0
        } else {
            (self.signed_sum / self.weight_sum) as f32
        }
    }

    fn capped_weight(self, maximum: f32) -> f32 {
        self.weight_sum.min(maximum.max(0.0) as f64) as f32
    }
}

/// Precomputed long-term preferences. Build after new events arrive and cache it;
/// don't rebuild it on every queue refresh.
#[derive(Clone, Debug)]
pub struct TasteProfile {
    /// Decayed centroid of features from positively rated/played tracks.
    pub positive_features: FeatureVector,
    /// Decayed centroid of features from negatively rated/skipped tracks.
    pub negative_features: FeatureVector,
    /// Total saturated positive evidence weight.
    pub positive_event_weight: f32,
    /// Total saturated negative evidence weight.
    pub negative_event_weight: f32,
    /// Number of usable feedback events included in the profile.
    pub event_count: usize,
    genre_affinity: HashMap<GenreId, f32>,
    artist_affinity: HashMap<ArtistId, f32>,
    album_affinity: HashMap<AlbumId, f32>,
    track_affinity: HashMap<TrackId, f32>,
    transitions: HashMap<(TrackId, TrackId), f32>,
    transition_outgoing: HashMap<TrackId, f32>,
}

impl Default for TasteProfile {
    fn default() -> Self {
        Self {
            positive_features: FeatureVector::empty(),
            negative_features: FeatureVector::empty(),
            positive_event_weight: 0.0,
            negative_event_weight: 0.0,
            event_count: 0,
            genre_affinity: HashMap::new(),
            artist_affinity: HashMap::new(),
            album_affinity: HashMap::new(),
            track_affinity: HashMap::new(),
            transitions: HashMap::new(),
            transition_outgoing: HashMap::new(),
        }
    }
}

impl TasteProfile {
    pub fn build(
        tracks: &[Track],
        events: &[FeedbackEvent],
        now_ms: i64,
        config: &TasteProfileConfig,
    ) -> Self {
        let config = config.sanitized();
        let track_map: HashMap<TrackId, &Track> =
            tracks.iter().map(|track| (track.id, track)).collect();
        let half_life_ms = config.half_life_days as f64 * 86_400_000.0;
        let decay_constant = std::f64::consts::LN_2 / half_life_ms;
        let mut per_track: HashMap<TrackId, TrackAccumulator> = HashMap::new();
        let mut event_count = 0_usize;

        // Group first so each track's evidence can be saturated before it
        // influences the feature centroids or the affinity tables.
        for event in events {
            if !track_map.contains_key(&event.track_id) {
                continue;
            }
            let signal = event.kind.signal();
            if !signal.is_finite() || signal.abs() < 0.01 {
                continue;
            }
            let age_ms = now_ms.saturating_sub(event.timestamp_ms).max(0) as f64;
            let decay = (-decay_constant * age_ms).exp() as f32;
            if !decay.is_finite() || decay < 0.005 {
                continue;
            }
            per_track
                .entry(event.track_id)
                .or_default()
                .add(signal, decay);
            event_count += 1;
        }

        let mut track_entries: Vec<(TrackId, TrackAccumulator)> = per_track.into_iter().collect();
        track_entries.sort_unstable_by_key(|(track_id, _)| *track_id);

        let mut positive_sum = [0.0_f64; FEATURE_COUNT];
        let mut positive_weights = [0.0_f64; FEATURE_COUNT];
        let mut negative_sum = [0.0_f64; FEATURE_COUNT];
        let mut negative_weights = [0.0_f64; FEATURE_COUNT];
        let mut genre_affinity: HashMap<GenreId, AffinityAccumulator> = HashMap::new();
        let mut artist_affinity: HashMap<ArtistId, AffinityAccumulator> = HashMap::new();
        let mut album_affinity: HashMap<AlbumId, AffinityAccumulator> = HashMap::new();
        let mut track_affinity: HashMap<TrackId, AffinityAccumulator> = HashMap::new();
        let mut positive_event_weight = 0.0_f32;
        let mut negative_event_weight = 0.0_f32;

        for (track_id, accumulator) in track_entries {
            let Some(track) = track_map.get(&track_id).copied() else {
                continue;
            };
            let mean_signal = accumulator.mean_signal();
            let capped_weight = accumulator.capped_weight(config.max_track_weight);
            if mean_signal.abs() < 0.01 || capped_weight <= 0.0 {
                continue;
            }
            if mean_signal > 0.0 {
                positive_event_weight += mean_signal * capped_weight;
            } else {
                negative_event_weight += -mean_signal * capped_weight;
            }

            track_affinity
                .entry(track_id)
                .or_default()
                .add(mean_signal, capped_weight);

            if let Some(features) = track.features {
                let feature_weight = (mean_signal.abs() * capped_weight) as f64;
                if feature_weight >= 1.0e-6 {
                    for index in 0..FEATURE_COUNT {
                        if !features.is_valid(index) {
                            continue;
                        }
                        if mean_signal > 0.0 {
                            positive_sum[index] += features.values[index] as f64 * feature_weight;
                            positive_weights[index] += feature_weight;
                        } else {
                            negative_sum[index] += features.values[index] as f64 * feature_weight;
                            negative_weights[index] += feature_weight;
                        }
                    }
                }
            }

            for genre in &track.metadata.genre_ids {
                genre_affinity
                    .entry(*genre)
                    .or_default()
                    .add(mean_signal, capped_weight);
            }
            if let Some(artist_id) = track.metadata.artist_id {
                artist_affinity
                    .entry(artist_id)
                    .or_default()
                    .add(mean_signal, capped_weight);
            }
            if let Some(album_id) = track.metadata.album_id {
                album_affinity
                    .entry(album_id)
                    .or_default()
                    .add(mean_signal, capped_weight);
            }
        }

        // A tiny session-transition model inspired by sequential recommendation.
        // Only explicit PlayStarted events are used, avoiding double-counting
        // completion/progress/skip events as separate tracks in the sequence.
        let mut play_sequence: Vec<(u64, i64, TrackId)> = events
            .iter()
            .filter_map(|event| {
                if !matches!(event.kind, FeedbackKind::PlayStarted)
                    || !track_map.contains_key(&event.track_id)
                {
                    return None;
                }
                event
                    .session_id
                    .map(|session_id| (session_id, event.timestamp_ms, event.track_id))
            })
            .collect();
        play_sequence.sort_unstable_by_key(|(session_id, timestamp_ms, track_id)| {
            (*session_id, *timestamp_ms, *track_id)
        });
        let mut last_by_session: HashMap<u64, (TrackId, i64)> = HashMap::new();
        let mut transitions: HashMap<(TrackId, TrackId), f32> = HashMap::new();
        let mut transition_outgoing: HashMap<TrackId, f32> = HashMap::new();
        for (session_id, timestamp_ms, track_id) in play_sequence {
            if let Some((previous_id, previous_time)) = last_by_session.get(&session_id).copied() {
                let gap = timestamp_ms.saturating_sub(previous_time);
                if previous_id != track_id && (0..=config.transition_gap_ms).contains(&gap) {
                    let age_ms = now_ms.saturating_sub(timestamp_ms).max(0) as f64;
                    let transition_weight = (-decay_constant * age_ms).exp() as f32;
                    if transition_weight >= 0.005 {
                        *transitions.entry((previous_id, track_id)).or_insert(0.0) +=
                            transition_weight;
                        *transition_outgoing.entry(previous_id).or_insert(0.0) += transition_weight;
                    }
                }
            }
            last_by_session.insert(session_id, (track_id, timestamp_ms));
        }

        let (positive_features, negative_features) = build_feature_centroids(
            positive_sum,
            positive_weights,
            negative_sum,
            negative_weights,
        );
        Self {
            positive_features,
            negative_features,
            positive_event_weight,
            negative_event_weight,
            event_count,
            genre_affinity: finish_affinities(genre_affinity, &config),
            artist_affinity: finish_affinities(artist_affinity, &config),
            album_affinity: finish_affinities(album_affinity, &config),
            track_affinity: finish_affinities(track_affinity, &config),
            transitions,
            transition_outgoing,
        }
    }

    pub fn has_positive_features(&self) -> bool {
        !self.positive_features.is_empty()
    }

    pub fn has_negative_features(&self) -> bool {
        !self.negative_features.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.event_count == 0
    }

    pub fn genre_score(&self, genres: &[GenreId]) -> Option<f32> {
        let mut sum = 0.0_f32;
        let mut count = 0_u32;
        for genre in genres {
            if let Some(affinity) = self.genre_affinity.get(genre) {
                sum += *affinity;
                count += 1;
            }
        }
        if count == 0 {
            None
        } else {
            Some(sum / count as f32)
        }
    }

    pub fn artist_score(&self, artist_id: ArtistId) -> Option<f32> {
        self.artist_affinity.get(&artist_id).copied()
    }

    pub fn album_score(&self, album_id: AlbumId) -> Option<f32> {
        self.album_affinity.get(&album_id).copied()
    }

    /// Direct per-track preference learned from likes, ratings, skips, replays,
    /// and play history. This is stronger evidence than a feature-centroid match
    /// and is therefore a separate ranking signal.
    pub fn track_score(&self, track_id: TrackId) -> Option<f32> {
        self.track_affinity.get(&track_id).copied()
    }

    /// Estimated probability-like affinity that `to` follows `from` in this
    /// listener's sessions. Returns `None` until the transition is observed.
    pub fn transition_score(&self, from: TrackId, to: TrackId) -> Option<f32> {
        let pair_count = *self.transitions.get(&(from, to))?;
        let outgoing = *self.transition_outgoing.get(&from)?;
        if outgoing <= 0.0 {
            None
        } else {
            Some(((pair_count + 0.5) / (outgoing + 1.0)).clamp(0.0, 1.0))
        }
    }
}

fn finish_affinities<K: Eq + Hash + Copy>(
    accumulators: HashMap<K, AffinityAccumulator>,
    config: &TasteProfileConfig,
) -> HashMap<K, f32> {
    accumulators
        .into_iter()
        .filter_map(|(key, accumulator)| {
            accumulator
                .score(config.affinity_smoothing, config.affinity_scale)
                .map(|score| (key, score))
        })
        .collect()
}

fn build_feature_centroids(
    positive_sum: [f64; FEATURE_COUNT],
    positive_weights: [f64; FEATURE_COUNT],
    negative_sum: [f64; FEATURE_COUNT],
    negative_weights: [f64; FEATURE_COUNT],
) -> (FeatureVector, FeatureVector) {
    let mut positive = [0.0_f32; FEATURE_COUNT];
    let mut negative = [0.0_f32; FEATURE_COUNT];
    let mut positive_mask = 0_u32;
    let mut negative_mask = 0_u32;
    for i in 0..FEATURE_COUNT {
        if positive_weights[i] > f64::EPSILON {
            positive[i] = (positive_sum[i] / positive_weights[i]).clamp(0.0, 1.0) as f32;
            positive_mask |= 1_u32 << i;
        }
        if negative_weights[i] > f64::EPSILON {
            negative[i] = (negative_sum[i] / negative_weights[i]).clamp(0.0, 1.0) as f32;
            negative_mask |= 1_u32 << i;
        }
    }
    normalize_chroma(&mut positive, positive_mask);
    normalize_chroma(&mut negative, negative_mask);
    (
        FeatureVector::new(positive, positive_mask),
        FeatureVector::new(negative, negative_mask),
    )
}

fn normalize_chroma(values: &mut [f32; FEATURE_COUNT], mask: u32) {
    let chroma_mask = ((1_u32 << 12) - 1) << 8;
    if mask & chroma_mask == 0 {
        return;
    }
    let sum = values[8..20].iter().sum::<f32>();
    if sum > 1.0e-8 {
        for value in &mut values[8..20] {
            *value /= sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeatureVector, TrackMetadata, TrackStats};

    fn track(id: u128, genre: u64, artist: u128, marker: f32) -> Track {
        let mut values = [0.0_f32; FEATURE_COUNT];
        values[0] = marker;
        values[1] = marker;
        values[8 + (id as usize % 12)] = 1.0;
        Track {
            id,
            features: Some(FeatureVector::new(values, (1_u32 << FEATURE_COUNT) - 1)),
            metadata: TrackMetadata {
                artist_id: Some(artist),
                album_id: Some(id as u64 % 7 + 1),
                genre_ids: vec![genre],
                duration_seconds: Some(180.0),
                added_at_ms: None,
            },
            stats: TrackStats::default(),
        }
    }

    fn event(track_id: u128, timestamp_ms: i64, kind: FeedbackKind) -> FeedbackEvent {
        FeedbackEvent {
            track_id,
            timestamp_ms,
            session_id: Some(1),
            kind,
        }
    }

    #[test]
    fn likes_and_dislikes_create_separate_profiles() {
        let tracks = vec![track(1, 7, 10, 0.9), track(2, 9, 20, 0.1)];
        let events = vec![
            event(1, 1_000, FeedbackKind::Liked),
            event(2, 1_000, FeedbackKind::Disliked),
        ];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        assert!(profile.has_positive_features());
        assert!(profile.has_negative_features());
        assert!(profile.genre_score(&[7]).unwrap() > 0.5);
        assert!(profile.genre_score(&[9]).unwrap() < 0.5);
        assert!(profile.track_score(1).unwrap() > profile.track_score(2).unwrap());
    }

    #[test]
    fn affinity_magnitude_matters() {
        let tracks = vec![track(1, 7, 10, 0.8), track(2, 7, 20, 0.8)];
        let events = vec![
            event(1, 1_000, FeedbackKind::Liked),
            event(2, 1_000, FeedbackKind::PlayStarted),
        ];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        assert!(profile.track_score(1).unwrap() > profile.track_score(2).unwrap());
        assert!(profile.track_score(1).unwrap() > 0.75);
        assert!(profile.track_score(2).unwrap() < 0.60);
    }

    #[test]
    fn repeated_plays_are_saturated() {
        let tracks = vec![track(1, 7, 10, 0.8), track(2, 7, 20, 0.8)];
        let mut events: Vec<FeedbackEvent> = (0..100)
            .map(|index| event(1, 1_000 + index, FeedbackKind::PlayStarted))
            .collect();
        events.push(event(2, 2_000, FeedbackKind::Liked));
        let profile = TasteProfile::build(&tracks, &events, 2_000, &TasteProfileConfig::default());
        let repeated = profile.track_score(1).unwrap();
        let liked = profile.track_score(2).unwrap();
        assert!(
            liked > repeated,
            "liked {liked} should beat 100 plays {repeated}"
        );
        assert!(
            repeated < 0.65,
            "repeated plays were not saturated: {repeated}"
        );
    }

    #[test]
    fn session_plays_learn_next_track_transitions() {
        let tracks = vec![
            track(1, 10, 7, 0.9),
            track(2, 20, 9, 0.1),
            track(3, 30, 9, 0.2),
        ];
        let events = vec![
            event(1, 1_000, FeedbackKind::PlayStarted),
            event(2, 180_000, FeedbackKind::PlayStarted),
            event(1, 900_000, FeedbackKind::PlayStarted),
            event(2, 1_080_000, FeedbackKind::PlayStarted),
        ];
        let profile =
            TasteProfile::build(&tracks, &events, 1_080_000, &TasteProfileConfig::default());
        assert!(profile.transition_score(1, 2).unwrap() > 0.7);
        assert!(profile.transition_score(2, 3).is_none());
    }

    #[test]
    fn transitions_do_not_cross_a_long_gap() {
        let tracks = vec![track(1, 10, 7, 0.9), track(2, 20, 9, 0.1)];
        let events = vec![
            event(1, 0, FeedbackKind::PlayStarted),
            event(2, 4 * 60 * 60 * 1_000, FeedbackKind::PlayStarted),
        ];
        let profile = TasteProfile::build(
            &tracks,
            &events,
            4 * 60 * 60 * 1_000,
            &TasteProfileConfig::default(),
        );
        assert!(profile.transition_score(1, 2).is_none());
    }

    #[test]
    fn album_and_artist_affinity_are_learned() {
        let tracks = vec![
            track(1, 7, 10, 0.8),
            track(2, 7, 10, 0.7),
            track(3, 8, 30, 0.2),
        ];
        let events = vec![
            event(1, 1_000, FeedbackKind::Liked),
            event(3, 1_000, FeedbackKind::Disliked),
        ];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        assert!(profile.artist_score(10).unwrap() > 0.5);
        assert!(profile.artist_score(30).unwrap() < 0.5);
        assert!(profile.album_score(2).is_some());
    }

    #[test]
    fn future_timestamps_and_empty_events_do_not_panic() {
        let tracks = vec![track(1, 7, 10, 0.8)];
        let events = vec![event(1, 2_000, FeedbackKind::Liked)];
        let profile = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        assert!(profile.track_score(1).is_some());
        let empty = TasteProfile::build(&tracks, &[], 1_000, &TasteProfileConfig::default());
        assert!(empty.is_empty());
        assert!(empty.track_score(1).is_none());
    }

    #[test]
    fn profile_build_is_deterministic() {
        let tracks = vec![track(1, 7, 10, 0.8), track(2, 8, 20, 0.2)];
        let events = vec![
            event(1, 1_000, FeedbackKind::Liked),
            event(
                2,
                1_000,
                FeedbackKind::Skipped {
                    listened_fraction: 0.05,
                },
            ),
        ];
        let first = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        let second = TasteProfile::build(&tracks, &events, 1_000, &TasteProfileConfig::default());
        assert_eq!(
            first.positive_features.to_bytes(),
            second.positive_features.to_bytes()
        );
        assert_eq!(first.track_score(1), second.track_score(1));
        assert_eq!(first.genre_score(&[7]), second.genre_score(&[7]));
    }
}

use super::{RecommendationConfig, Track, TrackStats};
use crate::{FeatureVector, GenreId, TasteProfile, TrackId};

pub(crate) fn recency_penalty(
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

pub(crate) fn deterministic_tie_break(
    track_id: TrackId,
    seed_track_id: Option<TrackId>,
    now_ms: i64,
) -> f32 {
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

pub(crate) fn add_component(
    sum: &mut f32,
    active_weight: &mut f32,
    value: Option<f32>,
    weight: f32,
) -> f32 {
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

pub(crate) fn personal_score(
    features: Option<FeatureVector>,
    profile: &TasteProfile,
) -> Option<f32> {
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

pub(crate) fn engagement_score(stats: &TrackStats) -> Option<f32> {
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

pub(crate) fn jaccard(left: &[GenreId], right: &[GenreId]) -> Option<f32> {
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

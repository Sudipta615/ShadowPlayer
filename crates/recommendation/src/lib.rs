//! Lightweight, native Rust recommendation core for local music libraries.
//!
//! This crate intentionally has no runtime dependencies and starts no threads.
//! The host application controls decoding, scheduling, database persistence,
//! worker priority, cancellation, and UI integration.

mod audio;
mod features;
mod profile;
mod recommend;
mod session;

pub use audio::{downmix_and_resample, AudioReadError, AudioWindowSource, PcmWindow};
pub use features::{
    analyze_track, AnalysisConfig, AnalysisError, AnalysisProfile, AnalysisResult,
    FeatureCodecError, FeatureVector, FEATURE_COUNT, FEATURE_NAMES, FEATURE_SCHEMA_VERSION,
    FEATURE_WEIGHTS, SERIALIZED_FEATURE_LEN,
};
pub use profile::{FeedbackEvent, FeedbackKind, TasteProfile, TasteProfileConfig};
pub use recommend::{
    Recommendation, RecommendationConfig, RecommendationReason, RecommendationRequest, Recommender,
    ScoreBreakdown, Track, TrackMetadata, TrackStats,
};
pub use session::{RecommendationSession, RecommendationSessionConfig};

/// Lossless integer representation of ShadowPlayer's UUID-backed track ID.
/// Convert with `domain::TrackId.0.as_u128()` and restore with
/// `domain::TrackId::from_uuid(uuid::Uuid::from_u128(id))`.
pub type TrackId = u128;
/// Lossless integer representation of a UUID-backed artist ID.
pub type ArtistId = u128;
/// Album identifier used by the host's existing library database.
pub type AlbumId = u64;
/// Deterministic 64-bit key for a normalized genre label.
pub type GenreId = u64;

/// Return a stable, case-insensitive key for a genre label, or `None` if blank.
/// The 64-bit FNV-1a key avoids allocating owned strings during ranking. As
/// with every fixed-width hash, collisions are possible but extremely unlikely;
/// the original label should remain in the library database for display.
pub fn genre_key(label: &str) -> Option<GenreId> {
    let normalized = label.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in normalized.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    Some(hash)
}

#[cfg(test)]
mod identifier_tests {
    use super::genre_key;

    #[test]
    fn genre_keys_ignore_case_and_outer_whitespace() {
        assert_eq!(genre_key(" Ambient "), genre_key("ambient"));
        assert_eq!(genre_key("ROCK"), genre_key("rock"));
        assert_eq!(genre_key("   "), None);
    }
}

//! Demonstrates a rolling "For You" session: initial batch, playback feedback,
//! profile rebuild, and appended recommendations without duplicates.
//!
//! cargo run --release --example session_demo

use shadow_recommender::{
    FeatureVector, FeedbackEvent, FeedbackKind, RecommendationSession, RecommendationSessionConfig,
    Track, TrackMetadata, TrackStats, FEATURE_COUNT,
};

fn track(id: u128, artist_id: u128, marker: usize) -> Track {
    let mut values = [0.0_f32; FEATURE_COUNT];
    values[0] = marker as f32 / 10.0;
    values[1] = marker as f32 / 10.0;
    values[2] = 0.45;
    values[3] = 0.55;
    values[8 + marker % 12] = 1.0;
    Track {
        id,
        features: Some(FeatureVector::new(values, (1_u32 << FEATURE_COUNT) - 1)),
        metadata: TrackMetadata {
            artist_id: Some(artist_id),
            album_id: Some(id as u64 % 5 + 1),
            genre_ids: vec![id as u64 % 4 + 1],
            duration_seconds: Some(200.0),
            added_at_ms: Some(1_000),
        },
        stats: TrackStats::default(),
    }
}

fn print_batch(label: &str, batch: &[shadow_recommender::Recommendation]) {
    println!("{label}: {} tracks", batch.len());
    for recommendation in batch {
        println!(
            "  track={} score={:.3} confidence={:.3} reasons=0x{:04x}",
            recommendation.track_id,
            recommendation.score,
            recommendation.confidence,
            recommendation.reasons.0,
        );
    }
}

fn main() {
    let tracks: Vec<Track> = (1..=40)
        .map(|id| track(id, (id - 1) % 10 + 1, id as usize % 10))
        .collect();
    let now_ms = 1_000_000_i64;
    let config = RecommendationSessionConfig {
        batch_size: 10,
        seed_track_id: Some(1),
        ..RecommendationSessionConfig::default()
    };
    let mut session = RecommendationSession::new(&tracks, Vec::new(), now_ms, config);

    let first = session.start(&tracks, now_ms);
    print_batch("initial batch", &first);

    // Simulate the listener playing the first track and liking it, then
    // skipping the second one after a few seconds.
    session.mark_played(first[0].track_id);
    session.record_feedback(FeedbackEvent {
        track_id: first[0].track_id,
        timestamp_ms: now_ms + 30_000,
        session_id: Some(7),
        kind: FeedbackKind::Liked,
    });
    session.record_feedback(FeedbackEvent {
        track_id: first[1].track_id,
        timestamp_ms: now_ms + 45_000,
        session_id: Some(7),
        kind: FeedbackKind::Skipped {
            listened_fraction: 0.05,
        },
    });

    let next = session.refill(&tracks, now_ms + 60_000);
    print_batch("next batch after feedback", &next);
    println!(
        "pending={} total_recommended={} skipped_removed={}",
        session.pending_count(),
        session.items().len(),
        !session
            .items()
            .iter()
            .any(|item| item.track_id == first[1].track_id),
    );
}

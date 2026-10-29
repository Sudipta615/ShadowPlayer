//! Synthetic ranking microbenchmark. This measures ranker work only, not audio
//! decoding, profile building, disk I/O, or the GUI. Run in a release build.
//!
//! cargo run --release --example benchmark_recommendation -- 100000
//! cargo run --release --example benchmark_recommendation -- 100000 30 7

use shadow_recommender::{
    FeatureVector, FeedbackEvent, FeedbackKind, RecommendationRequest, Recommender, TasteProfile,
    TasteProfileConfig, Track, TrackMetadata, TrackStats, FEATURE_COUNT,
};
use std::hint::black_box;
use std::time::Instant;

fn make_track(id: u128) -> Track {
    let mut values = [0.0_f32; FEATURE_COUNT];
    let seed = id as u64 ^ ((id >> 64) as u64).rotate_left(23);
    for (index, value) in values.iter_mut().enumerate() {
        let mixed = seed
            .wrapping_add((index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_mul(0xBF58_476D_1CE4_E5B9);
        *value = ((mixed >> 40) as u32 & 0x00FF_FFFF) as f32 / 16_777_215.0;
    }
    // One normalized pitch-class bin makes the synthetic data resemble valid
    // chroma distributions instead of arbitrary vectors.
    values[8..20].fill(0.0);
    values[8 + (id as usize % 12)] = 1.0;

    Track {
        id,
        features: Some(FeatureVector::new(values, (1_u32 << FEATURE_COUNT) - 1)),
        metadata: TrackMetadata {
            artist_id: Some(id % 5_000 + 1),
            album_id: Some((id % 20_000 + 1) as u64),
            genre_ids: vec![id as u64 % 24 + 1],
            duration_seconds: Some(120.0 + (id % 300) as f32),
            added_at_ms: Some(1_800_000_000_000_i64 - (id % 60) as i64 * 86_400_000),
        },
        stats: TrackStats {
            play_count: (id % 100) as u32,
            skip_count: (id % 13 == 0) as u32,
            completion_count: (id % 5) as u32,
            like_count: (id % 17 == 0) as u32,
            dislike_count: 0,
            rating: if id % 11 == 0 { Some(4.0) } else { None },
            last_played_ms: if id % 4 == 0 {
                Some(1_800_000_000_000_i64 - (id % 10) as i64 * 86_400_000)
            } else {
                None
            },
            last_skipped_ms: None,
        },
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let count = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100_000)
        .clamp(2, 1_000_000);
    let limit = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(30)
        .clamp(1, 500);
    let iterations = arguments
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5)
        .clamp(1, 100);

    let tracks: Vec<Track> = (1..=count as u128).map(make_track).collect();
    let now_ms = 1_800_000_000_000_i64;
    let events: Vec<FeedbackEvent> = (1..=count.min(100) as u128)
        .map(|track_id| FeedbackEvent {
            track_id,
            timestamp_ms: now_ms - track_id as i64 * 60_000,
            session_id: Some(1),
            kind: if track_id % 3 == 0 {
                FeedbackKind::Disliked
            } else {
                FeedbackKind::Liked
            },
        })
        .collect();

    let profile_start = Instant::now();
    let profile = TasteProfile::build(&tracks, &events, now_ms, &TasteProfileConfig::default());
    let profile_ms = profile_start.elapsed().as_secs_f64() * 1_000.0;

    let queued: Vec<u128> = Vec::new();
    let recent: Vec<u128> = Vec::new();
    let recent_artists: Vec<u128> = Vec::new();
    let request = RecommendationRequest {
        seed_track_id: Some(1),
        limit,
        now_ms,
        excluded_track_ids: &queued,
        recently_played_ids: &recent,
        recent_artist_ids: &recent_artists,
    };
    let recommender = Recommender::default();

    // Warm caches and branch predictors before measuring.
    for _ in 0..2 {
        black_box(recommender.recommend(black_box(&tracks), &profile, &request));
    }

    let mut samples = Vec::with_capacity(iterations);
    let mut output_len = 0;
    for _ in 0..iterations {
        let start = Instant::now();
        let output = recommender.recommend(black_box(&tracks), &profile, &request);
        let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
        output_len = output.len();
        black_box(output);
        samples.push(elapsed);
    }
    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    println!(
        "tracks={count} limit={limit} recommendations={output_len} profile_ms={profile_ms:.3}"
    );
    println!(
        "ranking_ms min={:.3} p50={:.3} p95={:.3} max={:.3} mean={:.3} iterations={iterations}",
        samples[0],
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        samples[samples.len() - 1],
        mean,
    );
    println!("Note: synthetic in-memory ranking only; this is not an audio-analysis benchmark.");
}

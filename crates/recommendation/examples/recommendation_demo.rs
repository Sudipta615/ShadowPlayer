use shadow_recommender::{
    FeatureVector, FeedbackEvent, FeedbackKind, RecommendationRequest, Recommender, TasteProfile,
    TasteProfileConfig, Track, TrackMetadata, TrackStats, FEATURE_COUNT,
};

fn track(id: u128, artist_id: u128, genre_id: u64, chroma_bin: usize, loudness: f32) -> Track {
    let mut values = [0.0_f32; FEATURE_COUNT];
    values[0] = loudness;
    values[1] = 0.35;
    values[2] = 0.55;
    values[3] = 0.62;
    values[4] = 0.20;
    values[5] = 0.30;
    values[6] = 0.45;
    values[7] = 0.50;
    values[8 + chroma_bin % 12] = 1.0;
    Track {
        id,
        features: Some(FeatureVector::new(values, (1_u32 << FEATURE_COUNT) - 1)),
        metadata: TrackMetadata {
            artist_id: Some(artist_id),
            album_id: None,
            genre_ids: vec![genre_id],
            duration_seconds: Some(210.0),
            added_at_ms: None,
        },
        stats: TrackStats::default(),
    }
}

fn main() {
    let tracks = vec![
        track(1, 100, 10, 9, 0.72),
        track(2, 200, 10, 9, 0.70),
        track(3, 300, 11, 2, 0.40),
        track(4, 400, 10, 8, 0.68),
    ];
    let events = vec![
        FeedbackEvent {
            track_id: 1,
            timestamp_ms: 1_000,
            session_id: Some(7),
            kind: FeedbackKind::Liked,
        },
        FeedbackEvent {
            track_id: 1,
            timestamp_ms: 2_000,
            session_id: Some(7),
            kind: FeedbackKind::PlayStarted,
        },
        FeedbackEvent {
            track_id: 2,
            timestamp_ms: 180_000,
            session_id: Some(7),
            kind: FeedbackKind::PlayStarted,
        },
        FeedbackEvent {
            track_id: 3,
            timestamp_ms: 360_000,
            session_id: Some(7),
            kind: FeedbackKind::Skipped {
                listened_fraction: 0.08,
            },
        },
    ];
    let now_ms = 1_000_000;
    let profile = TasteProfile::build(&tracks, &events, now_ms, &TasteProfileConfig::default());
    let request = RecommendationRequest {
        seed_track_id: Some(1),
        limit: 3,
        now_ms,
        excluded_track_ids: &[],
        recently_played_ids: &[],
        recent_artist_ids: &[],
    };
    let recommendations = Recommender::default().recommend(&tracks, &profile, &request);
    for recommendation in recommendations {
        println!(
            "track={} score={:.3} base={:.3} confidence={:.3} reasons=0x{:04x}",
            recommendation.track_id,
            recommendation.score,
            recommendation.base_score,
            recommendation.confidence,
            recommendation.reasons.0,
        );
        println!("  breakdown={:?}", recommendation.breakdown);
    }
}

#[cfg(test)]
mod tests {
    use crate::coordinator::PlaybackCoordinator;
    use audio_session::AudioSession;
    use database::{Database, QueueRepo, TrackRepo};
    use domain::ids::TrackId;
    use domain::models::Track;
    use engine::sink::NoopSink;
    use engine::EngineConfig;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn sample_track(id: TrackId, title: &str) -> Track {
        Track {
            id,
            path: PathBuf::from(format!("/music/{}.flac", title)),
            title: title.to_string(),
            artist: "Test Artist".to_string(),
            album: "Test Album".to_string(),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 180.0,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 10_000_000,
            mtime_unix_secs: 0,
            is_favorite: false,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
        }
    }

    #[test]
    fn test_queue_mutations() {
        let config = EngineConfig::default();
        let session = Arc::new(AudioSession::with_sink(config, Box::new(NoopSink)).unwrap());
        let db = Database::in_memory().unwrap();
        let track_repo = TrackRepo::new(db.clone());
        let queue_repo = QueueRepo::new(db.clone());

        let t1 = sample_track(TrackId::new(), "Track 1");
        let t2 = sample_track(TrackId::new(), "Track 2");
        let t3 = sample_track(TrackId::new(), "Track 3");
        let t_next = sample_track(TrackId::new(), "Track Next");

        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();
        track_repo.upsert(&t3).unwrap();
        track_repo.upsert(&t_next).unwrap();

        let mut coord = PlaybackCoordinator::new(session, db);

        // 1. Initial queue load via play_track
        coord.play_track(t1.clone(), Some(vec![t1.clone(), t2.clone(), t3.clone()]));
        assert_eq!(coord.queue().len(), 3);
        assert_eq!(coord.current_index(), Some(0));

        // 2. Add to queue next (should be placed immediately after index 0, so index 1)
        coord.add_to_queue_next(t_next.clone());
        assert_eq!(coord.queue().len(), 4);
        assert_eq!(coord.queue()[1].title, "Track Next");
        assert_eq!(coord.queue()[2].title, "Track 2");

        // 3. Add to end of queue
        let t_end = sample_track(TrackId::new(), "Track End");
        track_repo.upsert(&t_end).unwrap();
        coord.add_to_queue(t_end.clone());
        assert_eq!(coord.queue().len(), 5);
        assert_eq!(coord.queue()[4].title, "Track End");

        // 4. Remove from queue
        // Remove index 1 ("Track Next")
        coord.remove_from_queue(1);
        assert_eq!(coord.queue().len(), 4);
        assert_eq!(coord.queue()[1].title, "Track 2");
        // Current index should still be 0
        assert_eq!(coord.current_index(), Some(0));

        // Verify DB persistence of queue
        let persisted = queue_repo.load_queue().unwrap();
        assert_eq!(persisted.len(), 4);
    }
}

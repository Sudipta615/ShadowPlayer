#[cfg(test)]
mod unit {
    use crate::session::AudioSession;
    use domain::models::AudioProfileChoice;
    use engine::sink::NoopSink;
    use engine::EngineConfig;
    use std::time::Duration;

    #[test]
    fn test_audio_session_with_sink_lifecycle() {
        let config = EngineConfig::default();
        let sink = Box::new(NoopSink);

        let session = AudioSession::with_sink(config, sink)
            .expect("Failed to initialize AudioSession with NoopSink");

        // Verify initial state
        assert!(!session.is_playing());
        let info = session.playback_info();
        assert_eq!(info.state, engine::PlaybackState::Stopped);

        // Test volume controls
        session.set_volume(0.85);
        std::thread::sleep(Duration::from_millis(20));
        assert!((session.volume() - 0.85).abs() < 0.05);

        // Test profile changes
        session.apply_profile(AudioProfileChoice::Fidelity);
        session.apply_profile(AudioProfileChoice::Normal);
        session.apply_profile(AudioProfileChoice::LegacyLowPower);

        // Test DSP commands
        session.set_bit_perfect(true);
        session.set_bit_perfect(false);
        session.set_eq_enabled(true);
        session.set_bass_shelf(2.5);
        session.set_treble_shelf(-1.0);

        // Drop initiates graceful shutdown
        drop(session);
    }
}

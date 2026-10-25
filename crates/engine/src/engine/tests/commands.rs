use config::EngineConfig;

use super::helpers::*;
use crate::{
    buffer::{EngineCommand, PlaybackState},
    engine::AudioEngine,
};

#[test]
fn test_engine_new_default() {
    let result = AudioEngine::new_default();
    assert!(
        result.is_ok(),
        "Engine creation should succeed: {:?}",
        result.err()
    );
    let engine = result.unwrap();
    assert!(
        !engine.is_running(),
        "Engine should not be running after creation"
    );
}

#[test]
fn test_engine_new_with_config() {
    let config = EngineConfig::default();
    let result = AudioEngine::new(config);
    assert!(
        result.is_ok(),
        "Engine creation with config should succeed: {:?}",
        result.err()
    );
}

#[test]
fn test_engine_not_running_initially() {
    let engine = AudioEngine::new_default().unwrap();
    assert!(!engine.is_running());
}

#[test]
fn test_engine_pipeline_accessors() {
    let mut engine = AudioEngine::new_default().unwrap();
    let _ = engine.pipeline().volume().processor.current_gain();
    engine.pipeline_mut().set_volume(0.5);
    for _ in 0..50000 {
        engine
            .pipeline_mut()
            .process_block(&mut [0.0, 0.0], &mut [0.0, 0.0]);
    }
    assert!((engine.pipeline().volume().processor.current_gain() - 0.5).abs() < 0.01);
}

#[test]
fn test_graph_latency_is_authoritative_sum() {
    let mut engine = AudioEngine::new_default().unwrap();

    let empty = engine.graph_latency();
    assert_eq!(empty.resampler_latency_ms, 0.0);
    assert_eq!(empty.ring_buffer_latency_ms, 0.0);
    assert_eq!(empty.output_device_latency_ms, 0.0);

    engine.pipeline_mut().set_limiter_enabled(true);
    engine.pipeline_mut().with_graph(|g| {
        g.convolution_mut().engine.set_enabled(true);
    });
    let ir: Vec<(f32, f32)> = (0..2048)
        .map(|i| {
            let e = (-i as f32 / 512.0).exp() * 0.5;
            (e, e * 0.9)
        })
        .collect();
    engine.pipeline_mut().with_graph(|g| {
        g.convolution_mut()
            .engine
            .load_ir_from_samples(&ir)
            .unwrap();
    });

    let report = engine.graph_latency();
    let limiter_ms = engine.pipeline().limiter().limiter.lookahead_ms();
    let detector_ms = engine.pipeline().limiter().limiter.detector_delay_ms();
    let conv_ms = engine.pipeline().convolution().engine.latency_ms();

    assert!(limiter_ms > 0.0, "enabled limiter must have nonzero delay");
    assert!(
        conv_ms > 0.0,
        "loaded convolution IR must report partition latency"
    );

    assert!((report.limiter_lookahead_ms - limiter_ms).abs() < 1e-4);
    assert!((report.limiter_detector_delay_ms - detector_ms).abs() < 1e-4);
    assert!((report.convolution_latency_ms - conv_ms).abs() < 1e-4);

    assert!(report.limiter_detector_delay_ms <= report.limiter_lookahead_ms + 1e-6);

    // The limiter's full audio delay line is `lookahead + detector`; the total
    // must include BOTH. This previously omitted the detector term, which
    // under-counted the total by 50 samples (~1.04 ms at 48 kHz) and was fed
    // straight into playhead compensation.
    let expected_total = report.limiter_lookahead_ms
        + report.limiter_detector_delay_ms
        + report.convolution_latency_ms
        + report.resampler_latency_ms
        + report.ring_buffer_latency_ms
        + report.output_device_latency_ms;
    assert!((report.total_latency_ms - expected_total).abs() < 1e-4);
    // Guard against the specific regression: the total must be strictly
    // greater than the window alone whenever a detector delay exists.
    if report.limiter_detector_delay_ms > 0.0 {
        assert!(report.total_latency_ms > report.limiter_lookahead_ms);
    }
}

#[test]
fn test_send_command_does_not_panic() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::Play);
    engine.send_command(EngineCommand::Pause);
    engine.send_command(EngineCommand::Stop);
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.send_command(EngineCommand::SetSpeed(1.5));
    engine.send_command(EngineCommand::Shutdown);
}

#[test]
fn test_send_command_channel() {
    let mut engine = AudioEngine::new_default().unwrap();
    let tx = engine.send_command_channel();
    assert!(tx.send(EngineCommand::Play).is_ok());
}

#[test]
fn test_set_source() {
    let mut engine = AudioEngine::new_default().unwrap();
    let source = crate::source::AudioSource::File(std::path::PathBuf::from("/path/to/test.flac"));
    engine.set_source(source.clone());
    let info = engine.playback_info();
    assert_eq!(info.current_source, Some(source));
}

#[test]
fn test_set_config() {
    let mut engine = AudioEngine::new_default().unwrap();
    let mut new_config = EngineConfig::default();
    new_config.eq.enabled = true;
    new_config.dither_enabled = false;
    engine.set_config(new_config);
}

#[test]
fn test_tick_without_start() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.tick();
    engine.tick();
    engine.tick();
}

#[test]
fn test_tick_publishes_spatial_telemetry() {
    // The SpatialNode's output meters + voice plan land on the
    // lock-free PlaybackInfo snapshot every telemetry tick (2 s cadence).
    let mut engine = AudioEngine::new_default().unwrap();
    // Force the telemetry window to be due so the publish block actually
    // runs (the default `spatial: Some(_)` would otherwise mask it).
    engine.telemetry.last_cpu_reset = std::time::Instant::now() - std::time::Duration::from_secs(3);
    engine.tick();
    let info = engine.playback_info();
    let spatial = info
        .spatial
        .expect("spatial telemetry published on the tick");
    assert!(spatial.peak_db_l.is_finite() && spatial.peak_db_r.is_finite());
    assert!(spatial.rms_db_l.is_finite() && spatial.rms_db_r.is_finite());

    // Spatial health rides the same cadence. The default engine leaves the
    // spatial master disabled, so the published report is a coherent
    // Inactive state — the publication path itself is what we assert here
    // (per-source status is exercised in the SpatialNode tests).
    let health = info
        .spatial_health
        .expect("spatial health published on the tick");
    assert_eq!(health.status, crate::spatial::health::HealthLevel::Inactive);
    assert_eq!(health.active_sources, 0);
    assert!(health.per_source.is_empty());
}

#[test]
fn test_spatial_quality_and_voice_config_via_handle() {
    // Hosts configure SpatialQuality / VoicePriority through the
    // engine handle, and the commands reach the graph's SpatialNode.
    let mut engine = AudioEngine::new_default().unwrap();
    let handle = engine.handle();
    handle.set_spatial_quality(config::SpatialQuality::High);
    handle.set_spatial_voice(true, 3, 1, config::VoicePriority::GainWeighted);
    engine.tick(); // drain the commands
    assert_eq!(
        engine.graph.spatial().quality(),
        crate::spatial::SpatialQuality::High
    );
    let budget = engine
        .graph
        .spatial()
        .voice_budget()
        .expect("voice budget applied");
    assert_eq!(budget.capacity, 3);
    assert_eq!(budget.full_quality_capacity, 1);
    assert!(matches!(
        budget.policy,
        crate::spatial::VoicePriority::GainWeighted
    ));

    // A disable clears the budget (full admission).
    handle.set_spatial_voice(false, 0, 0, config::VoicePriority::Fixed);
    engine.tick();
    assert!(engine.graph.spatial().voice_budget().is_none());
}

#[test]
fn test_seek_command_validation() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::Seek(-1.0));
    engine.send_command(EngineCommand::Seek(f32::NAN));
    engine.send_command(EngineCommand::Seek(f32::INFINITY));
    engine.send_command(EngineCommand::Seek(30.0));
}

#[test]
fn test_speed_command_validation() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetSpeed(0.5));
    engine.send_command(EngineCommand::SetSpeed(f32::NAN));
    engine.send_command(EngineCommand::SetSpeed(f32::INFINITY));
}

#[test]
fn test_volume_command() {
    let engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetVolume(0.75));
}

#[test]
fn test_eq_auto_headroom_command() {
    let mut engine = AudioEngine::new_default().unwrap();

    engine.send_command(EngineCommand::SetEqAutoHeadroom(true));
    engine.tick();
    assert!(engine.config.eq.auto_headroom);
    assert!(engine.pipeline().eq().eq.is_auto_headroom());

    engine.send_command(EngineCommand::SetEqAutoHeadroom(false));
    engine.tick();
    assert!(!engine.config.eq.auto_headroom);
    assert!(!engine.pipeline().eq().eq.is_auto_headroom());
}

#[test]
fn test_is_resampler_disabled_no_stream() {
    let engine = AudioEngine::new_default().unwrap();
    assert!(!engine.is_resampler_disabled());
}

#[test]
fn test_engine_command_debug() {
    let cmd = EngineCommand::SetVolume(0.5);
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("SetVolume"));
}

#[test]
fn test_audio_source_open_command() {
    let source = crate::source::AudioSource::Uri("file:///path/to/file.mp3".to_string());
    let cmd = EngineCommand::Open(source.clone());
    if let EngineCommand::Open(s) = cmd {
        assert_eq!(s, source);
    } else {
        panic!("Expected Open command");
    }
}

#[test]
fn test_engine_drop_does_not_panic() {
    let engine = AudioEngine::new_default().unwrap();
    drop(engine);
}

#[test]
fn test_engine_stop_idempotent() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.stop();
    engine.stop();
}

#[test]
fn test_engine_stats_eq_auto_headroom() {
    let mut config = EngineConfig::default();
    config.eq.enabled = true;
    config.eq.auto_headroom = true;

    let mut pipeline = crate::dsp::pipeline::DspPipeline::from_config(&config, 44100.0);
    let stats = pipeline.engine_stats(44100, 44100, 24, 32, false, true);
    assert!(stats.eq_auto_headroom);

    pipeline.set_eq_auto_headroom(false);
    let stats_off = pipeline.engine_stats(44100, 44100, 24, 32, false, true);
    assert!(!stats_off.eq_auto_headroom);
}

#[test]
fn test_engine_stats_bit_perfect_conditions() {
    let mut pipeline =
        crate::dsp::pipeline::DspPipeline::from_config(&EngineConfig::default(), 44100.0);

    let stats_default = pipeline.engine_stats(44100, 44100, 24, 32, false, true);
    assert!(!stats_default.bit_perfect_report.is_bit_perfect);
    assert!(!stats_default.bit_perfect_report.dynamics_bypassed);
    assert!(!stats_default.bit_perfect_report.limiter_bypassed);
    assert!(stats_default.bit_perfect_report.compressor_bypassed);
    assert!(stats_default.bit_perfect_report.convolution_bypassed);
    assert!(stats_default.bit_perfect_report.crossfeed_bypassed);
    assert!(stats_default.bit_perfect_report.stereo_bypassed);
    assert!(stats_default.bit_perfect_report.loudness_bypassed);

    pipeline.set_volume(1.0);
    pipeline.set_eq_enabled(false);
    pipeline.set_limiter_enabled(false);

    let stats_bp_unity = pipeline.engine_stats_with_output_format(
        44100,
        44100,
        24,
        32,
        crate::dsp::pipeline::OutputSampleFormat::F32,
        false,
        true,
        0.0,
        0.0,
        0.0,
    );
    assert!(
        stats_bp_unity.bit_perfect,
        "Should be bit-perfect with unity volume and matched rates"
    );
    assert!(stats_bp_unity.bit_perfect_report.is_bit_perfect);
    assert!(stats_bp_unity.bit_perfect_report.volume_unity);
    assert!(stats_bp_unity.bit_perfect_report.sample_rate_matched);
    assert!(stats_bp_unity.bit_perfect_report.bit_depth_not_truncated);
    assert!(stats_bp_unity.bit_perfect_report.output_exclusive);
    assert!(stats_bp_unity.bit_perfect_report.bit_perfect_samples);
    assert!(stats_bp_unity.bit_perfect_report.bit_perfect_transport);
    assert_eq!(
        stats_bp_unity.bit_perfect_report.result,
        crate::dsp::pipeline::BitPerfectResult::BitPerfect
    );

    pipeline.set_volume(0.5);
    let stats_attenuated = pipeline.engine_stats(44100, 44100, 24, 32, false, true);
    assert!(!stats_attenuated.bit_perfect);
    assert!(!stats_attenuated.bit_perfect_report.volume_unity);
    pipeline.set_volume(1.0);

    let stats_mismatched_rate = pipeline.engine_stats(44100, 48000, 24, 32, true, true);
    assert!(!stats_mismatched_rate.bit_perfect);
    assert!(!stats_mismatched_rate.bit_perfect_report.sample_rate_matched);

    let stats_shared = pipeline.engine_stats_with_output_format(
        44100,
        44100,
        24,
        32,
        crate::dsp::pipeline::OutputSampleFormat::F32,
        false,
        false,
        0.0,
        0.0,
        0.0,
    );
    assert!(!stats_shared.bit_perfect);
    assert!(!stats_shared.bit_perfect_report.output_exclusive);
    assert!(stats_shared.bit_perfect_report.bit_perfect_samples);
    assert!(!stats_shared.bit_perfect_report.bit_perfect_transport);
    assert_eq!(
        stats_shared.bit_perfect_report.result,
        crate::dsp::pipeline::BitPerfectResult::Unknown
    );

    let stats_i32 = pipeline.engine_stats(44100, 44100, 32, 32, false, true);
    assert!(!stats_i32.bit_perfect);
    assert!(!stats_i32.bit_perfect_report.format_conversion_lossless);

    let stats_unknown_source = pipeline.engine_stats(44100, 44100, 0, 32, false, true);
    assert!(!stats_unknown_source.bit_perfect);
    assert!(
        !stats_unknown_source
            .bit_perfect_report
            .bit_depth_not_truncated
    );
    assert!(
        !stats_unknown_source
            .bit_perfect_report
            .format_conversion_lossless
    );
    assert!(stats_unknown_source
        .bit_perfect_report
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("unknown")));

    let stats_unknown_output = pipeline.engine_stats(44100, 44100, 24, 0, false, true);
    assert!(!stats_unknown_output.bit_perfect);
    assert!(
        !stats_unknown_output
            .bit_perfect_report
            .bit_depth_not_truncated
    );
}

#[test]
fn test_tick_populates_engine_stats_diagnostics() {
    let path = write_test_wav_at(44_100, "stats");
    let mut engine = AudioEngine::new_default().unwrap();
    engine.load_track(&path).expect("load track");

    engine.telemetry.last_cpu_reset = std::time::Instant::now() - std::time::Duration::from_secs(3);

    engine.update_playback_state(PlaybackState::Playing);
    engine.tick();

    let info = engine.playback_info();
    let stats = info
        .engine_stats
        .expect("tick should publish EngineStats after the reporting window elapses");

    assert_eq!(
        stats.buffer_capacity_frames,
        engine.output_buffer.capacity()
    );
    assert!(stats.buffer_available_frames <= stats.buffer_capacity_frames);
    assert!((0.0..=1.0).contains(&stats.buffer_fill_ratio));
    assert!(
        stats.buffer_available_frames > 0,
        "decode loop should have filled the ring"
    );

    assert_eq!(stats.source_bit_depth, 16);
    assert!(!stats.decoder_format.is_empty());
    assert!(
        stats.decoder_format.contains("Hz"),
        "decoder_format: {}",
        stats.decoder_format
    );

    #[cfg(feature = "resample")]
    {
        assert!(!stats.resampler_requested_quality.is_empty());
        assert!(!stats.resampler_effective_quality.is_empty());
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_graphic_eq_slider_syncs_into_pipeline() {
    let mut engine = AudioEngine::new_default().unwrap();
    let mut gains = vec![0.0; 31];
    gains[17] = 6.0;

    // The application selects the mode first, then sends the whole curve as
    // one atomic state. A lone slider command still works for a direct host,
    // but the app path is what guarantees that a layout change and every band
    // land together.
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains,
        preamp_db: 0.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    assert!(
        engine.pipeline().eq().eq.is_enabled(),
        "the atomic snapshot carries the master enable"
    );
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 31);

    let band = engine.pipeline().eq().eq.band_params(17).unwrap();
    assert!((band.frequency - 1000.0).abs() < 1e-3);
    assert!((band.gain_db - 6.0).abs() < 1e-4);
    assert!(band.enabled);
    assert!(engine.config.graphic_eq.enabled);
    assert_eq!(engine.config.graphic_eq.gains_db[17], 6.0);
    assert_eq!(engine.config.eq.mode, config::EqMode::Graphic);
}

#[test]
fn test_graphic_eq_64band_grows_pipeline_band_count() {
    let mut engine = AudioEngine::new_default().unwrap();
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 10);

    engine.send_command(EngineCommand::SetGraphicEqLayout(
        config::GraphicEqLayout::SixtyFourBand,
    ));
    engine.tick();
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 64);
    assert_eq!(
        engine
            .pipeline()
            .eq()
            .eq
            .band_params(63)
            .unwrap()
            .filter_type,
        crate::dsp::equalizer::EqFilterType::HighShelf
    );
}

#[test]
fn test_graphic_eq_disable_keeps_model() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetGraphicEqLayout(
        config::GraphicEqLayout::TenBand,
    ));
    engine.tick();
    engine.send_command(EngineCommand::SetGraphicEqSlider {
        band: 3,
        gain_db: 3.0,
    });
    engine.tick();
    engine.send_command(EngineCommand::SetGraphicEqEnabled(false));
    engine.tick();
    assert!(!engine.pipeline().eq().eq.is_enabled());
    assert!(!engine.config.graphic_eq.enabled);
    assert_eq!(engine.config.graphic_eq.gains_db[3], 3.0);
}

#[test]
fn test_set_eq_preset_replaces_bands_and_preamp() {
    let mut engine = AudioEngine::new_default().unwrap();
    let preset = config::EqPreset {
        name: "test-autoeq".into(),
        output_device_pattern: None,
        preamp_db: -4.0,
        bands: vec![
            config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::LowShelf,
                frequency: 105.0,
                gain_db: 5.5,
                q: 0.71,
            },
            config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::Peaking,
                frequency: 4650.0,
                gain_db: 1.5,
                q: 2.5,
            },
        ],
    };
    engine.send_command(EngineCommand::SetEqPreset(preset));
    engine.tick();
    assert!(engine.pipeline().eq().eq.is_enabled());
    assert!((engine.pipeline().eq().eq.preamp_db() - (-4.0)).abs() < 1e-4);
    let b0 = engine.pipeline().eq().eq.band_params(0).unwrap();
    assert!((b0.frequency - 105.0).abs() < 1e-3);
    assert!((b0.gain_db - 5.5).abs() < 1e-4);
    let b1 = engine.pipeline().eq().eq.band_params(1).unwrap();
    assert!((b1.frequency - 4650.0).abs() < 1e-3);
}

#[test]
fn test_graphic_eq_from_config_applies_on_set_config() {
    let mut engine = AudioEngine::new_default().unwrap();
    let mut config = EngineConfig::default();
    config.graphic_eq.enabled = true;
    config.graphic_eq.layout = config::GraphicEqLayout::ThirtyTwoBand;
    config.graphic_eq.gains_db = vec![0.0; 32];
    config.graphic_eq.gains_db[5] = 2.5;
    config.graphic_eq.preamp_db = -1.0;

    engine.set_config(config);
    assert!(engine.pipeline().eq().eq.is_enabled());
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 32);
    assert_eq!(engine.config.eq.mode, config::EqMode::Graphic);
    assert!(engine.config.eq.enabled);
    assert!(engine.config.graphic_eq.enabled);
    assert!((engine.config.eq.preamp_db - (-1.0)).abs() < 1e-4);
    let band = engine.pipeline().eq().eq.band_params(5).unwrap();
    assert!((band.gain_db - 2.5).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-1.0)).abs() < 1e-4);

    // The migrated config must survive a save/load round trip.
    let migrated = engine.config.clone();
    engine.set_config(migrated);
    assert!(engine.pipeline().eq().eq.is_enabled());
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 32);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-1.0)).abs() < 1e-4);
}

#[test]
fn test_graphic_eq_snapshot_installs_every_band_and_shrinks_cleanly() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));

    let mut gains = vec![0.0; 64];
    gains[0] = 3.0;
    gains[31] = -5.5;
    gains[63] = 8.0;
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::SixtyFourBand,
        gains_db: gains,
        preamp_db: -2.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();

    assert_eq!(engine.pipeline().eq().eq.num_bands(), 64);
    assert_eq!(engine.config.graphic_eq.gains_db.len(), 64);
    assert_eq!(engine.config.graphic_eq.gains_db[0], 3.0);
    assert_eq!(engine.config.graphic_eq.gains_db[31], -5.5);
    assert_eq!(engine.config.graphic_eq.gains_db[63], 8.0);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-2.0)).abs() < 1e-4);

    // Shrinking must rebuild the live bank, not leave the old tail (and its
    // filters) processing behind a shorter layout.
    let mut ten = vec![0.0; 10];
    ten[5] = 4.0;
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::TenBand,
        gains_db: ten,
        preamp_db: -1.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();

    assert_eq!(engine.pipeline().eq().eq.num_bands(), 10);
    assert_eq!(engine.config.graphic_eq.gains_db.len(), 10);
    assert!(engine.pipeline().eq().eq.band_params(10).is_none());
    let band = engine.pipeline().eq().eq.band_params(5).unwrap();
    assert!((band.frequency - 1000.0).abs() < 1e-3);
    assert!((band.gain_db - 4.0).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-1.0)).abs() < 1e-4);
}

/// Steady-state sine gain through the live EQ, measured after the coefficient
/// and gain ramps have settled.
fn measured_eq_gain_db(engine: &mut AudioEngine, freq_hz: f32) -> f32 {
    let sample_rate = engine.pipeline().sample_rate();
    let settle = (sample_rate * 0.25) as usize;
    let period = (sample_rate / freq_hz).ceil() as usize;
    let mut peak = 0.0f32;
    engine.pipeline_mut().with_graph(|graph| {
        let eq = &mut graph.eq_mut().eq;
        let mut phase = 0.0f32;
        for i in 0..settle + period * 2 {
            let sample = (phase * std::f32::consts::TAU).sin();
            phase += freq_hz / sample_rate;
            let (left, _) = eq.process(sample, sample);
            if i >= settle {
                peak = peak.max(left.abs());
            }
        }
    });
    20.0 * peak.max(1e-9).log10()
}

#[test]
fn test_set_preamp_clamps_to_the_dsp_range_and_ignores_non_finite_values() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqEnabled(true));
    engine.send_command(EngineCommand::SetPreamp(100.0));
    engine.tick();
    assert!((engine.config.eq.preamp_db - 30.0).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - 30.0).abs() < 1e-4);

    engine.send_command(EngineCommand::SetPreamp(f32::NAN));
    engine.tick();
    assert!(engine.config.eq.preamp_db.is_finite());
    assert!((engine.config.eq.preamp_db - 30.0).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - 30.0).abs() < 1e-4);
}

#[test]
fn test_graphic_snapshot_carries_the_shared_master_and_preamp_in_parametric_mode() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Parametric));
    engine.send_command(EngineCommand::SetEqEnabled(true));
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::TenBand,
        gains_db: vec![0.0; 10],
        preamp_db: -8.0,
        enabled: false,
        auto_headroom: false,
    });
    engine.tick();

    assert_eq!(engine.config.eq.mode, config::EqMode::Parametric);
    assert!(!engine.config.eq.enabled);
    assert!(!engine.pipeline().eq().eq.is_enabled());
    assert!((engine.config.eq.preamp_db - (-8.0)).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-8.0)).abs() < 1e-4);

    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::TenBand,
        gains_db: vec![0.0; 10],
        preamp_db: -8.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    assert_eq!(engine.config.eq.mode, config::EqMode::Parametric);
    assert!(engine.pipeline().eq().eq.is_enabled());
}

#[test]
fn test_eq_preset_clamps_and_sanitizes_its_preamp() {
    fn preset(preamp_db: f32) -> config::EqPreset {
        config::EqPreset {
            name: "preamp-test".into(),
            output_device_pattern: None,
            preamp_db,
            bands: vec![config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::Peaking,
                frequency: 1000.0,
                gain_db: 1.0,
                q: 1.0,
            }],
        }
    }

    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqPreset(preset(100.0)));
    engine.tick();
    assert!((engine.config.eq.preamp_db - 30.0).abs() < 1e-4);
    assert!((engine.pipeline().eq().eq.preamp_db() - 30.0).abs() < 1e-4);

    engine.send_command(EngineCommand::SetEqPreset(preset(f32::NAN)));
    engine.tick();
    assert!(engine.config.eq.preamp_db.is_finite());
    assert_eq!(engine.config.eq.preamp_db, 0.0);
    assert_eq!(engine.pipeline().eq().eq.preamp_db(), 0.0);
}

#[test]
fn test_graphic_eq_snapshot_boost_is_audible_at_center() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));

    let mut gains = vec![0.0; 31];
    let center = config::GraphicEqLayout::ThirtyOneBand
        .frequencies()
        .iter()
        .position(|f| (*f - 1000.0).abs() < 1.0)
        .unwrap();
    gains[center] = 12.0;
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains,
        preamp_db: 0.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();

    let gain_db = measured_eq_gain_db(&mut engine, 1000.0);
    assert!(
        (gain_db - 12.0).abs() < 0.6,
        "+12 dB slider should measure at the center, got {gain_db:.2} dB"
    );
}

#[test]
fn test_graphic_eq_extreme_boost_and_preamp_compose_without_a_hidden_cap() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));

    let flat = vec![0.0; 31];
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: flat,
        preamp_db: -18.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    let preamp_db = measured_eq_gain_db(&mut engine, 1000.0);
    assert!(
        (preamp_db + 18.0).abs() < 0.6,
        "the preamp should apply in full, got {preamp_db:.2} dB"
    );

    let mut gains = vec![0.0; 31];
    gains[17] = 24.0;
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains,
        preamp_db: -18.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    let extreme_db = measured_eq_gain_db(&mut engine, 1000.0);
    assert!(
        (extreme_db - 6.0).abs() < 0.6,
        "+24 dB minus an 18 dB preamp should measure +6 dB, got {extreme_db:.2} dB"
    );
}

#[test]
fn test_graphic_eq_auto_headroom_is_static_and_predictable() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));

    let mut gains = vec![0.0; 31];
    let center = config::GraphicEqLayout::ThirtyOneBand
        .frequencies()
        .iter()
        .position(|f| (*f - 1000.0).abs() < 1.0)
        .unwrap();
    gains[center] = 12.0;

    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains.clone(),
        preamp_db: 0.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    assert!((engine.pipeline().eq().eq.headroom_db() - 0.0).abs() < 1e-4);

    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains,
        preamp_db: 0.0,
        enabled: true,
        auto_headroom: true,
    });
    engine.tick();

    let headroom = engine.pipeline().eq().eq.headroom_db();
    assert!(
        (headroom + 12.0).abs() < 0.6,
        "auto headroom should reserve the +12 dB boost, got {headroom:.2} dB"
    );
    let gain_db = measured_eq_gain_db(&mut engine, 1000.0);
    assert!(
        gain_db.abs() < 0.6,
        "static headroom should cancel the boost instead of relying on the limiter, got {gain_db:.2} dB"
    );
}

#[test]
fn test_parametric_mode_survives_graphic_snapshots_and_round_trips() {
    let mut engine = AudioEngine::new_default().unwrap();
    let preset = config::EqPreset {
        name: "test-autoeq".into(),
        output_device_pattern: None,
        preamp_db: -4.0,
        bands: vec![
            config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::LowShelf,
                frequency: 105.0,
                gain_db: 5.5,
                q: 0.71,
            },
            config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::Peaking,
                frequency: 4650.0,
                gain_db: 1.5,
                q: 2.5,
            },
        ],
    };
    engine.send_command(EngineCommand::SetEqPreset(preset));
    engine.tick();
    assert_eq!(engine.config.eq.mode, config::EqMode::Parametric);
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 2);

    // The UI mirrors its shared preamp and graphic model on every sync. That
    // must not silently replace an AutoEQ curve with a flat graphic bank.
    let mut gains = vec![0.0; 31];
    gains[17] = 9.0;
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::ThirtyOneBand,
        gains_db: gains,
        preamp_db: -4.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    assert_eq!(engine.config.eq.mode, config::EqMode::Parametric);
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 2);
    assert!((engine.pipeline().eq().eq.band_params(0).unwrap().frequency - 105.0).abs() < 1e-3);
    assert!(!engine.config.graphic_eq.enabled);
    assert_eq!(engine.config.graphic_eq.gains_db[17], 9.0);

    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));
    engine.tick();
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 31);
    assert!((engine.pipeline().eq().eq.band_params(17).unwrap().gain_db - 9.0).abs() < 1e-4);

    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Parametric));
    engine.tick();
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 2);
    assert!((engine.pipeline().eq().eq.band_params(0).unwrap().frequency - 105.0).abs() < 1e-3);
    assert_eq!(engine.config.graphic_eq.gains_db[17], 9.0);
}

#[test]
fn test_set_eq_bands_replaces_a_64_band_autoeq_curve_atomically() {
    let mut engine = AudioEngine::new_default().unwrap();
    let preset = config::EqPreset {
        name: "autoeq-64".into(),
        output_device_pattern: None,
        preamp_db: -4.0,
        bands: (0..64)
            .map(|i| config::EqBandConfig {
                enabled: true,
                filter_type: config::FilterType::Peaking,
                frequency: 20.0 * 2.0f32.powf(i as f32 / 6.0),
                gain_db: 3.0,
                q: 4.0,
            })
            .collect(),
    };
    engine.send_command(EngineCommand::SetEqPreset(preset));
    engine.tick();
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 64);
    assert!(engine.pipeline().eq().eq.is_enabled());

    let flat: Vec<config::EqBandConfig> = (0..10)
        .map(|i| config::EqBandConfig {
            enabled: true,
            frequency: 31.5 * 2.0f32.powi(i),
            gain_db: 0.0,
            ..Default::default()
        })
        .collect();
    engine.send_command(EngineCommand::SetEqBands(flat));
    engine.tick();

    assert_eq!(engine.config.eq.mode, config::EqMode::Parametric);
    assert!(engine.pipeline().eq().eq.is_enabled());
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 10);
    for i in 0..10 {
        assert!(
            engine
                .pipeline()
                .eq()
                .eq
                .band_params(i)
                .unwrap()
                .gain_db
                .abs()
                < 1e-6
        );
    }
    assert!((engine.pipeline().eq().eq.preamp_db() - (-4.0)).abs() < 1e-4);
}

#[test]
fn test_set_eq_bands_does_not_steal_graphic_mode() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEqMode(config::EqMode::Graphic));
    engine.send_command(EngineCommand::SetGraphicEqState {
        layout: config::GraphicEqLayout::TenBand,
        gains_db: vec![4.0; 10],
        preamp_db: 0.0,
        enabled: true,
        auto_headroom: false,
    });
    engine.tick();
    assert!((engine.pipeline().eq().eq.band_params(0).unwrap().gain_db - 4.0).abs() < 1e-4);

    let flat: Vec<config::EqBandConfig> = (0..10)
        .map(|i| config::EqBandConfig {
            enabled: true,
            frequency: 31.5 * 2.0f32.powi(i),
            gain_db: 0.0,
            ..Default::default()
        })
        .collect();
    engine.send_command(EngineCommand::SetEqBands(flat));
    engine.tick();

    assert_eq!(engine.config.eq.mode, config::EqMode::Graphic);
    assert!((engine.config.eq.bands[0].gain_db).abs() < 1e-6);
    assert!(
        (engine.pipeline().eq().eq.band_params(0).unwrap().gain_db - 4.0).abs() < 1e-4,
        "storing the parametric source must not overwrite the audible graphic curve"
    );
}

#[test]
fn test_explicit_graphic_mode_follows_the_master_enable() {
    let mut engine = AudioEngine::new_default().unwrap();
    let mut config = EngineConfig::default();
    config.eq.mode = config::EqMode::Graphic;
    config.eq.enabled = true;
    config.eq.preamp_db = -3.0;
    // Stale mirrors from an old save: the mode and masters say audible and
    // -3 dB, so the engine must not load a silent or wrongly-levelled stage.
    config.graphic_eq.enabled = false;
    config.graphic_eq.preamp_db = 0.0;
    config.graphic_eq.gains_db = vec![2.0; 10];
    engine.set_config(config);

    assert!(engine.pipeline().eq().eq.is_enabled());
    assert!(engine.config.graphic_eq.enabled);
    assert!((engine.pipeline().eq().eq.preamp_db() - (-3.0)).abs() < 1e-4);
    assert!((engine.config.graphic_eq.preamp_db - (-3.0)).abs() < 1e-4);
    assert_eq!(engine.pipeline().eq().eq.num_bands(), 10);

    let mut config = engine.config.clone();
    config.eq.enabled = false;
    engine.set_config(config);
    assert!(!engine.pipeline().eq().eq.is_enabled());
    assert!(!engine.config.graphic_eq.enabled);
}

#[test]
fn test_set_output_profile_applies_dsp_bundle() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetOutputProfile(test_profile(
        "profile-a",
        -1.0,
    )));
    engine.tick();

    assert!(engine.pipeline().eq().eq.is_enabled());
    assert!((engine.pipeline().eq().eq.preamp_db() - (-2.0)).abs() < 1e-4);
    let band = engine.pipeline().eq().eq.band_params(0).unwrap();
    assert!((band.frequency - 1000.0).abs() < 1e-3);
    assert!((band.gain_db - 4.0).abs() < 1e-4);
    assert!(engine.pipeline().crossfeed().crossfeed.is_enabled());
    assert!((engine.pipeline().limiter().limiter.ceiling_db() - (-1.0)).abs() < 1e-3);
    assert_eq!(
        engine.config.sample_rate_policy,
        config::SampleRatePolicy::Fixed(96000)
    );
    assert_eq!(engine.config.dsd_output, config::DsdOutput::DoP);
    assert_eq!(engine.config.volume_mode, config::VolumeMode::SoftwareOnly);
    assert_eq!(
        engine.playback_info().active_output_profile.as_deref(),
        Some("profile-a")
    );
}

#[test]
fn test_clear_output_profile_removes_active_id() {
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetOutputProfile(test_profile(
        "profile-a",
        -1.0,
    )));
    engine.tick();
    assert_eq!(
        engine.playback_info().active_output_profile.as_deref(),
        Some("profile-a")
    );

    engine.send_command(EngineCommand::ClearOutputProfile);
    engine.tick();
    assert_eq!(engine.playback_info().active_output_profile, None);
    assert!(engine.output_profile.is_none());
}

#[test]
fn test_apply_output_profile_is_deterministic_across_devices() {
    let mut engine = AudioEngine::new_default().unwrap();
    let profile = test_profile("profile-a", -1.0);
    engine.output_profile = Some(profile.clone());
    engine.apply_output_profile(&profile);
    let first_band = *engine.pipeline().eq().eq.band_params(0).unwrap();
    engine.apply_output_profile(&profile);
    let second_band = *engine.pipeline().eq().eq.band_params(0).unwrap();
    assert_eq!(first_band, second_band);
    assert_eq!(
        engine.playback_info().active_output_profile.as_deref(),
        Some("profile-a")
    );
}

#[test]
fn test_output_profile_library_selection_follows_device() {
    use crate::output::OutputProfileLibrary;
    let dac = test_profile("dac-a", -0.3);
    let speakers = crate::output::OutputProfile {
        id: "speakers".into(),
        name: "speakers".into(),
        device_match: vec!["speaker".into()],
        ..Default::default()
    };
    let lib = OutputProfileLibrary::with_profiles(vec![dac, speakers]);

    assert_eq!(lib.select_for_device("USB dac-a MkII").unwrap().id, "dac-a");
    assert_eq!(lib.select_for_device("USB dac-a MkII").unwrap().id, "dac-a");
    assert_eq!(
        lib.select_for_device("Desktop Speakers").unwrap().id,
        "speakers"
    );
    assert!(lib.select_for_device("HDMI TV").is_none());
}

#[test]
fn test_volume_software_only_never_touches_endpoint() {
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut engine = AudioEngine::new_default().unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();

    assert_eq!(
        engine.config.volume_mode,
        config::VolumeMode::SoftwareOnly,
        "default volume mode must be SoftwareOnly"
    );
    assert_eq!(
        *hardware_db.lock().unwrap(),
        None,
        "SoftwareOnly must never call set_hardware_volume_db"
    );
    assert!(
        (engine.pipeline().volume().processor.target_gain - 0.5).abs() < 1e-4,
        "software gain target must be 0.5, got {}",
        engine.pipeline().volume().processor.target_gain
    );
    let pb = engine.playback_info();
    assert_eq!(
        pb.volume_path,
        Some(crate::dsp::pipeline::VolumePath::Software)
    );
    assert!(pb.volume_error.is_none());
    assert!((pb.volume - 0.5).abs() < 1e-4);
}

#[test]
fn test_volume_hardware_preferred_routes_to_endpoint() {
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwarePreferred;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();

    let expected_db = 20.0 * 0.5f32.log10();
    assert_eq!(*hardware_db.lock().unwrap(), Some(expected_db));
    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "DSP must stay at unity when hardware owns the level"
    );
    let pb = engine.playback_info();
    assert_eq!(
        pb.volume_path,
        Some(crate::dsp::pipeline::VolumePath::Hardware)
    );
    assert!(pb.volume_error.is_none());
    assert!((pb.volume - 0.5).abs() < 1e-4);
}

#[test]
fn test_volume_hardware_preferred_falls_back_without_endpoint() {
    let (fake, hardware_db) = FakeOutput::new(false);
    let mut engine = AudioEngine::new_default().unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolumeMode(
        config::VolumeMode::HardwarePreferred,
    ));
    engine.tick();

    assert_eq!(
        engine.config.volume_mode,
        config::VolumeMode::HardwarePreferred
    );
    let pb = engine.playback_info();
    assert_eq!(
        pb.volume_path,
        Some(crate::dsp::pipeline::VolumePath::Software)
    );
    assert!(
        pb.volume_error.is_some(),
        "fallback must be reported in volume diagnostics"
    );

    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();
    assert_eq!(
        *hardware_db.lock().unwrap(),
        None,
        "fallback must never call set_hardware_volume_db"
    );
    assert!(
        (engine.pipeline().volume().processor.target_gain - 0.5).abs() < 1e-4,
        "fallback applies the gain in software"
    );
    assert!(
        (engine.playback_info().volume - 0.5).abs() < 1e-4,
        "volume must be applied (not silently dropped) in fallback mode"
    );
}

#[test]
fn test_public_set_volume_routes_like_the_command_path() {
    // The C FFI and embedders call `AudioEngine::set_volume` directly; it has
    // to route exactly like `EngineCommand::SetVolume`, or a
    // `HardwarePreferred` host gets software gain while the UI would have
    // gotten hardware volume.
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwarePreferred;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));

    engine.set_volume(0.5);
    engine.tick();

    let expected_db = 20.0 * 0.5f32.log10();
    assert_eq!(*hardware_db.lock().unwrap(), Some(expected_db));
    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "the endpoint owns the level; the DSP must stay at unity"
    );
    let pb = engine.playback_info();
    assert_eq!(
        pb.volume_path,
        Some(crate::dsp::pipeline::VolumePath::Hardware)
    );
    assert!(pb.volume_error.is_none());
}

#[test]
fn test_public_set_volume_db_routes_like_the_command_path() {
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwarePreferred;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));

    engine.set_volume_db(-6.0);
    engine.tick();

    assert_eq!(*hardware_db.lock().unwrap(), Some(-6.0));
    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "the endpoint owns the level; the DSP must stay at unity"
    );
}

#[test]
fn test_public_set_volume_reports_the_software_fallback() {
    // Without endpoint volume the public method must apply the gain in
    // software and say so, not silently drop the change.
    let (fake, hardware_db) = FakeOutput::new(false);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwarePreferred;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));

    engine.set_volume(0.25);
    engine.tick();

    assert_eq!(*hardware_db.lock().unwrap(), None);
    assert!(
        (engine.pipeline().volume().processor.target_gain - 0.25).abs() < 1e-4,
        "the fallback applies the gain in software"
    );
    // The level path is the durable record of where a set actually landed;
    // the setup-time `volume_error` note ("endpoint volume unavailable") has
    // already done its job by the time the software gain succeeds.
    let pb = engine.playback_info();
    assert_eq!(
        pb.volume_path,
        Some(crate::dsp::pipeline::VolumePath::Software)
    );
    assert!((pb.volume - 0.25).abs() < 1e-4);
}

#[test]
fn test_public_set_volume_never_softens_hardware_only() {
    // `HardwareOnly` means exactly that: no software gain may appear, even
    // when the public method is the one asking.
    let (fake, _hardware_db) = FakeOutput::new(false);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwareOnly;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));

    engine.set_volume(0.5);
    engine.tick();

    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "HardwareOnly must keep the signal at unity"
    );
    let pb = engine.playback_info();
    assert!(pb.volume_path.is_none());
    assert!(pb.volume_error.is_some());
}

#[test]
fn test_volume_hardware_only_routes_to_endpoint() {
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut config = EngineConfig::default();
    config.volume_mode = config::VolumeMode::HardwareOnly;
    let mut engine = AudioEngine::new(config).unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();

    let expected_db = 20.0 * 0.5f32.log10();
    assert_eq!(*hardware_db.lock().unwrap(), Some(expected_db));
    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "HardwareOnly: DSP must stay at unity"
    );
    assert_eq!(
        engine.playback_info().volume_path,
        Some(crate::dsp::pipeline::VolumePath::Hardware)
    );
}

#[test]
fn test_volume_hardware_only_never_falls_back_to_software() {
    let (fake, hardware_db) = FakeOutput::new(false);
    let mut engine = AudioEngine::new_default().unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolumeMode(
        config::VolumeMode::HardwareOnly,
    ));
    engine.tick();

    assert_eq!(engine.config.volume_mode, config::VolumeMode::HardwareOnly);
    let pb = engine.playback_info();
    assert!(
        pb.volume_error.is_some(),
        "HardwareOnly failure must be reported"
    );
    assert!(
        pb.volume_path.is_none(),
        "HardwareOnly must not claim a software path when hardware is unavailable"
    );

    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();
    assert_eq!(*hardware_db.lock().unwrap(), None);
    assert!(
        (engine.pipeline().volume().processor.target_gain - 1.0).abs() < 1e-4,
        "HardwareOnly must keep the software pipeline at unity (untouched signal)"
    );
    assert!(
        engine.playback_info().volume_error.is_some(),
        "the failed hardware volume set must remain visible"
    );
}

#[test]
fn test_volume_software_allowed_uses_software_path() {
    let (fake, hardware_db) = FakeOutput::new(true);
    let mut engine = AudioEngine::new_default().unwrap();
    engine.audio_output = Some(Box::new(fake));
    engine.send_command(EngineCommand::SetVolumeMode(
        config::VolumeMode::SoftwareAllowed,
    ));
    engine.send_command(EngineCommand::SetVolume(0.5));
    engine.tick();

    assert_eq!(*hardware_db.lock().unwrap(), None);
    assert!(
        (engine.pipeline().volume().processor.target_gain - 0.5).abs() < 1e-4,
        "SoftwareAllowed applies the gain in software"
    );
    assert_eq!(
        engine.playback_info().volume_path,
        Some(crate::dsp::pipeline::VolumePath::Software)
    );
}

#[test]
fn test_multichannel_commands_dispatch_correctly() {
    let mut engine = AudioEngine::new_default().unwrap();

    // 1. ChannelMix
    let mix_cfg = config::ChannelMixConfig {
        enabled: true,
        template: config::ChannelMixTemplate::StereoToFiveOne,
    };
    engine.send_command(EngineCommand::SetChannelMix(mix_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.channel_mix, mix_cfg);

    // 2. ChannelPolicy
    let policy = config::ChannelPolicy::MaxChannels(6);
    engine.send_command(EngineCommand::SetChannelPolicy(policy));
    engine.tick();
    assert_eq!(engine.config.channel_policy, policy);

    // 3. ChannelTrim
    let trim_cfg = config::ChannelTrimConfig {
        enabled: true,
        entries: vec![config::ChannelTrimEntry {
            channel: 0,
            gain_db: -2.5,
            delay_ms: 1.5,
            invert: false,
        }],
    };
    engine.send_command(EngineCommand::SetChannelTrim(trim_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.channel_trim, trim_cfg);

    // 4. ChannelRouting
    let routing_cfg = config::ChannelRoutingConfig {
        enabled: true,
        matrix: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
    };
    engine.send_command(EngineCommand::SetChannelRouting(routing_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.channel_routing, routing_cfg);

    // 5. ChannelEq
    let eq_cfg = config::ChannelEqConfig {
        enabled: true,
        entries: vec![config::ChannelEqEntry {
            channel: 0,
            bands: vec![config::EqBandConfig {
                frequency: 120.0,
                gain_db: 3.0,
                q: 1.0,
                filter_type: config::FilterType::Peaking,
                enabled: true,
            }],
        }],
    };
    engine.send_command(EngineCommand::SetChannelEq(eq_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.channel_eq, eq_cfg);

    // 6. LfeConfig
    let lfe_cfg = config::LfeConfig {
        enabled: true,
        gain_db: 10.0,
        crossover_hz: Some(120.0),
    };
    engine.send_command(EngineCommand::SetLfeConfig(lfe_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.lfe, lfe_cfg);

    // 7. BassManagement
    let bass_cfg = config::BassManagementConfig {
        enabled: true,
        mains_highpass_enabled: true,
        crossover_hz: 80.0,
        q: std::f32::consts::FRAC_1_SQRT_2,
        ..Default::default()
    };
    engine.send_command(EngineCommand::SetBassManagement(bass_cfg.clone()));
    engine.tick();
    assert_eq!(engine.config.bass_management, bass_cfg);
}

#[test]
fn test_engine_handle_controls_and_telemetry() {
    let mut engine = AudioEngine::new_default().unwrap();
    let handle = engine.handle();

    // 1. Telemetry reads
    assert_eq!(handle.state(), crate::buffer::PlaybackState::Stopped);
    assert!(!handle.is_playing());

    // 2. Dispatch commands via handle
    handle.set_volume(0.8);
    handle.set_speed(1.25);
    handle.set_crossfeed_enabled(true);
    handle.set_crossfeed_profile(config::CrossfeedProfile::Bauer);

    engine.tick();

    assert!((engine.pipeline().volume().processor.target_gain - 0.8).abs() < 1e-4);
    assert!((engine.speed - 1.25).abs() < 1e-4);
    assert!(engine.pipeline().crossfeed().crossfeed.is_enabled());

    // 3. Bit-perfect via handle
    handle.set_bit_perfect(true);
    engine.tick();
    assert!(engine.pipeline().is_bit_perfect());

    // 3. Multichannel through handle
    let mix_cfg = config::ChannelMixConfig {
        enabled: true,
        template: config::ChannelMixTemplate::FiveOneToStereo,
    };
    handle.set_channel_mix(mix_cfg.clone());
    engine.tick();
    assert_eq!(engine.config.channel_mix, mix_cfg);

    // 4. Verify debug representation doesn't panic
    let debug_str = format!("{:?}", handle);
    assert!(debug_str.contains("EngineHandle"));

    // 5. Verify events receiver is accessible
    assert!(handle.events().is_empty());
}

#[test]
fn test_set_aux_insert_command_dispatches_and_publishes() {
    // The aux-insert runtime toggle must flow engine command →
    // graph control → mirrored control-bus state → PlaybackInfo telemetry
    // (the read path the FFI exposes to hosts).
    let ir_path = std::env::temp_dir().join(format!(
        "cmd_aux_ir_{}_{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut left = vec![0i16; 2048];
    left[0] = i16::MAX; // impulse: the IR engine is created + observable
    let right = vec![0i16; 2048];
    write_i16_wav(&ir_path, 48_000, &left, &right);

    let mut config = EngineConfig::default();
    config.aux.enabled = true;
    config.aux.return_gain = 0.5;
    config.aux.insert_enabled = false;
    config.aux.insert_wet_mix = 1.0;
    config.aux.insert_ir_path = Some(ir_path.display().to_string());
    let mut engine = AudioEngine::new(config).unwrap();

    // Construction applied the config: insert engine exists, disabled.
    assert!(
        !engine.pipeline_mut().control_handle().aux_insert_state().0,
        "insert starts disabled per config"
    );

    // Runtime toggle ON at 25% wet through the engine command.
    engine.send_command(EngineCommand::SetAuxInsert {
        enabled: true,
        wet_mix: 0.25,
    });
    engine.tick();
    let (enabled, wet) = engine.pipeline_mut().control_handle().aux_insert_state();
    assert!(enabled, "SetAuxInsert enables the insert engine");
    assert!(
        (wet - 0.25).abs() < 1e-6,
        "SetAuxInsert sets the wet mix (got {wet})"
    );

    // The 2 s telemetry cadence mirrors it into PlaybackInfo for FFI reads.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    let mut published = false;
    while std::time::Instant::now() < deadline {
        engine.tick();
        let pb = engine.playback_info();
        if pb.aux_insert_enabled && (pb.aux_insert_wet_mix - 0.25).abs() < 1e-6 {
            published = true;
            break;
        }
    }
    assert!(published, "aux insert state published to PlaybackInfo");
    let _ = std::fs::remove_file(&ir_path);
}

#[test]
fn test_endpoint_commands_update_config_without_output() {
    // The endpoint routing commands must manage `endpoint_configs` even
    // before any audio output exists (no device is opened, no failure).
    let mut engine = AudioEngine::new_default().unwrap();
    engine.send_command(EngineCommand::SetEndpoints(vec![]));
    engine.tick();
    assert!(engine.endpoint_configs.is_empty());

    engine.send_command(EngineCommand::UpsertEndpoint(config::EndpointConfig {
        id: "ffi-test".to_string(),
        backend: config::AudioBackend::Auto,
        device: None,
        gain: 0.8,
        enabled: true,
        drift_correction: true,
    }));
    engine.tick();
    assert_eq!(engine.endpoint_configs.len(), 1, "upsert adds the endpoint");
    assert_eq!(engine.endpoint_configs[0].id, "ffi-test");
    assert!((engine.endpoint_configs[0].gain - 0.8).abs() < 1e-6);

    // Upserting the same id replaces, not duplicates.
    engine.send_command(EngineCommand::UpsertEndpoint(config::EndpointConfig {
        id: "ffi-test".to_string(),
        backend: config::AudioBackend::Auto,
        device: Some("other".to_string()),
        gain: 0.5,
        enabled: false,
        drift_correction: false,
    }));
    engine.tick();
    assert_eq!(engine.endpoint_configs.len(), 1, "upsert replaces by id");
    assert_eq!(engine.endpoint_configs[0].device.as_deref(), Some("other"));
    assert!(!engine.endpoint_configs[0].enabled);

    engine.send_command(EngineCommand::RemoveEndpoint("ffi-test".to_string()));
    engine.tick();
    assert!(engine.endpoint_configs.is_empty(), "remove clears by id");
}

#[test]
fn test_tick_blocking_processes_waking_command() {
    let mut engine = AudioEngine::new_default().unwrap();
    let handle = engine.handle();

    // Volume default is 1.0. Send SetVolume command while engine is about to tick_blocking.
    handle.set_volume(0.42);
    engine.tick_blocking(std::time::Duration::from_millis(50));

    assert!(
        (handle.volume() - 0.42).abs() < 1e-4,
        "Command waking tick_blocking must be executed, not dropped. Volume: {}",
        handle.volume()
    );
}

#[test]
fn test_tick_blocking_processes_burst_commands_in_order() {
    let mut engine = AudioEngine::new_default().unwrap();
    let handle = engine.handle();

    // Send a burst of commands: set volume, then set speed, then enqueue source.
    handle.set_volume(0.75);
    handle.set_speed(1.25);
    handle.enqueue_file(std::path::PathBuf::from("/nonexistent/dummy_track.flac"));

    engine.tick_blocking(std::time::Duration::from_millis(50));

    assert!((handle.volume() - 0.75).abs() < 1e-4);
    assert!((handle.speed() - 1.25).abs() < 1e-4);
    assert_eq!(handle.playlist_len(), 1);
}

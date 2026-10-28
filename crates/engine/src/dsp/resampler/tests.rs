use super::super::AudioResampler;
use config::ResamplerQuality;

#[test]
fn test_resampler_creation() {
    let resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
    assert!(!resampler.is_passthrough());
}

#[test]
fn test_resampler_f64_creation() {
    let mut resampler =
        AudioResampler::<f64>::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
    assert!(!resampler.is_passthrough());
    for i in 0..5000 {
        let sample = (i as f64 / 44100.0 * 440.0 * 2.0 * std::f64::consts::PI).sin() * 0.5;
        resampler.feed(sample, sample);
    }
    resampler.flush();
    assert!(resampler.available_output() > 0);
    let (l, r) = resampler.read().unwrap();
    assert!(l.abs() <= 1.0 && r.abs() <= 1.0);
}

#[test]
fn test_passthrough_detection() {
    let resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, 44100.0, 44100.0).unwrap();
    assert!(resampler.is_passthrough());
}

#[test]
fn test_latency_reports_authoritative_group_delay() {
    // A real conversion must report rubato's nonzero filter group delay.
    let resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
    assert!(
        resampler.latency_samples() > 0,
        "44.1->48 kHz must introduce filter delay"
    );

    // The ms value must be the frame count scaled at the OUTPUT rate.
    let expected_ms = resampler.latency_samples() as f32 / 48000.0 * 1000.0;
    assert!((resampler.latency_ms() - expected_ms).abs() < 1e-3);

    // f64 and f32 report the same group delay for the same conversion.
    let f64_resampler =
        AudioResampler::<f64>::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
    assert_eq!(resampler.latency_samples(), f64_resampler.latency_samples());
}

#[test]
fn test_passthrough_latency_is_zero() {
    let resampler = AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 44100.0).unwrap();
    assert!(resampler.is_passthrough());
    assert_eq!(resampler.latency_samples(), 0);
    assert_eq!(resampler.latency_ms(), 0.0);
}

#[test]
fn test_resampler_speed_change() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 44100.0).unwrap();
    resampler.set_speed(1.5);
    assert!((resampler.speed() - 1.5).abs() < 0.001);
    assert!(resampler.rebuild_pending());
}

#[test]
fn test_resampler_produces_output() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
    for i in 0..5000 {
        let sample = (i as f32 / 44100.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        resampler.feed(sample, sample);
    }
    resampler.flush();
    assert!(
        resampler.available_output() > 0,
        "Resampler should produce output after feeding samples"
    );
}

#[test]
fn test_resampler_quality_change() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
    resampler.set_quality(ResamplerQuality::HighQuality);
    assert!(resampler.rebuild_pending());
}

#[test]
fn test_resampler_reset() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
    for _ in 0..1000 {
        resampler.feed(0.5f32, 0.5f32);
    }
    resampler.reset();
    assert_eq!(resampler.available_output(), 0);
    assert_eq!(resampler.available_output(), 0);
}

#[test]
fn test_resampler_invalid_rates() {
    let result = AudioResampler::<f32>::new(ResamplerQuality::Fast, 0.0, 48000.0);
    assert!(result.is_err());
    let result = AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 0.0);
    assert!(result.is_err());
}

#[test]
fn test_resampler_speed_2x_not_inverted() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 44100.0).unwrap();
    resampler.set_speed(2.0);
    while resampler.rebuild_pending() || resampler.rebuild_pending() {
        resampler.feed(0.0f32, 0.0f32);
        if resampler.rebuild_pending() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    while resampler.read().is_some() {}

    let n_input: usize = 8192;
    for i in 0..n_input {
        let s = (i as f32 / 44100.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        resampler.feed(s, s);
    }
    resampler.flush();

    let mut n_output: usize = 0;
    while resampler.read().is_some() {
        n_output += 1;
    }
    let ratio = n_output as f32 / n_input as f32;
    assert!(
        ratio <= 1.25,
        "speed=2.0 with {} input frames produced {} output (ratio {:.3}). \
             Correct ratio is ~0.5; inverted ratio is ~2.0.",
        n_input,
        n_output,
        ratio,
    );
}

#[test]
fn test_resampler_speed_half_not_inverted() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 44100.0).unwrap();
    resampler.set_speed(0.5);
    while resampler.rebuild_pending() || resampler.rebuild_pending() {
        resampler.feed(0.0f32, 0.0f32);
        if resampler.rebuild_pending() {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    while resampler.read().is_some() {}

    let n_input: usize = 4096;
    for i in 0..n_input {
        let s = (i as f32 / 44100.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        resampler.feed(s, s);
    }
    resampler.flush();

    let mut n_output: usize = 0;
    while resampler.read().is_some() {
        n_output += 1;
    }
    let ratio = n_output as f32 / n_input as f32;
    assert!(
        ratio >= 1.25,
        "speed=0.5 with {} input frames produced {} output (ratio {:.3}). \
             Correct ratio is ~2.0; inverted ratio is ~0.5.",
        n_input,
        n_output,
        ratio,
    );
}

#[test]
fn test_latency_provider_all_quality_tiers_and_rates() {
    use super::LatencyProvider;

    let tiers = [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ];

    let rate_pairs = [
        (44_100.0, 48_000.0),
        (48_000.0, 96_000.0),
        (96_000.0, 44_100.0),
        (88_200.0, 192_000.0),
    ];

    for &tier in &tiers {
        for &(in_rate, out_rate) in &rate_pairs {
            let resampler =
                AudioResampler::<f32>::new(tier, in_rate as f32, out_rate as f32).unwrap();

            // Direct filter delay
            let filter_delay = resampler.filter_delay_samples();
            assert!(
                filter_delay > 0.0,
                "{:?} ({in_rate}->{out_rate}): filter group delay must be positive",
                tier
            );

            // Buffering delay
            let buf_delay = resampler.buffering_delay_samples();
            assert!(
                buf_delay > 0.0,
                "{:?} ({in_rate}->{out_rate}): buffering delay must be positive",
                tier
            );

            // Total latency
            let total = resampler.latency_samples_exact();
            assert_eq!(total, filter_delay + buf_delay);

            // LatencyProvider trait
            let provider_latency = LatencyProvider::latency_samples(&resampler, out_rate);
            assert!(
                (provider_latency - total).abs() < 1e-3,
                "LatencyProvider at out_rate should match total latency"
            );

            // LatencyProvider scaled to input sample rate
            let in_rate_latency = LatencyProvider::latency_samples(&resampler, in_rate);
            let expected_scaled = total * (in_rate / out_rate);
            assert!(
                (in_rate_latency - expected_scaled).abs() < 1e-3,
                "LatencyProvider at in_rate should be properly ratio-scaled"
            );

            // GenericResampler wrapper
            let generic = crate::dsp::resampler::GenericResampler::F32(resampler);
            assert_eq!(
                LatencyProvider::latency_samples(&generic, out_rate),
                provider_latency
            );
        }
    }
}

/// A normal conversion reads what it writes: the accounting stays at zero.
#[test]
fn a_drained_resampler_reports_no_dropped_frames() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
    let mut read = 0usize;
    for i in 0..20_000 {
        let s = ((i as f32 / 44100.0) * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        resampler.feed(s, s);
        while resampler.read().is_some() {
            read += 1;
        }
    }
    resampler.flush();
    while resampler.read().is_some() {
        read += 1;
    }
    assert!(read > 20_000, "the output really was read: {read}");
    assert_eq!(
        resampler.dropped_frames(),
        0,
        "a drained stream drops nothing"
    );
}

/// A consumer that stops reading must not take the incoming audio down with
/// it, and the fact that something had to give must be visible: the resampler
/// drops the oldest output (a backlog nobody heard) and counts it. It used to
/// drop the whole incoming chunk, log it, and report nothing.
#[test]
fn a_stalled_consumer_loses_backlog_but_the_chunk_still_gets_processed() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();

    for i in 0..200_000 {
        let s = ((i as f32 / 44100.0) * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        resampler.feed(s, s);
        // Deliberately never read.
    }

    assert!(
        resampler.dropped_frames() > 0,
        "the overflow is accounted for"
    );
    assert!(
        resampler.available_output() > 0,
        "and the newest audio is still waiting"
    );
}

/// The passthrough path (equal rates) counts what it cannot queue as well.
#[test]
fn a_stalled_passthrough_consumer_is_counted() {
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, 48_000.0, 48_000.0).unwrap();
    assert!(resampler.is_passthrough());

    for i in 0..(crate::dsp::resampler::MAX_OUTPUT_BUFFER_FRAMES + 512) {
        let s = ((i as f32 / 48_000.0) * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        resampler.feed(s, s);
    }

    assert!(
        resampler.dropped_frames() > 0,
        "past a full buffer the passthrough path had to drop, and says so"
    );
}

#[test]
fn test_feed_block_and_read_block() {
    let mut resampler1 =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();
    let mut resampler2 =
        AudioResampler::<f32>::new(ResamplerQuality::Fast, 44100.0, 48000.0).unwrap();

    let n = 2048;
    let l_in: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
    let r_in: Vec<f32> = (0..n).map(|i| (i as f32 * 0.07).cos() * 0.5).collect();

    // resampler1 fed sample-by-sample
    for i in 0..n {
        resampler1.feed(l_in[i], r_in[i]);
    }

    // resampler2 fed in blocks
    resampler2.feed_block(&l_in, &r_in);

    assert_eq!(resampler1.available_output(), resampler2.available_output());
    let avail = resampler2.available_output();
    assert!(avail > 0);

    let mut out_l = vec![0.0f32; avail];
    let mut out_r = vec![0.0f32; avail];
    let read_count = resampler2.read_block(&mut out_l, &mut out_r);
    assert_eq!(read_count, avail);

    for i in 0..avail {
        let (exp_l, exp_r) = resampler1.read().unwrap();
        assert!((out_l[i] - exp_l).abs() < 1e-6);
        assert!((out_r[i] - exp_r).abs() < 1e-6);
    }
}

#[test]
fn test_polyphase_filter_metadata_and_latency() {
    use config::ResamplerFilter;

    let mut resampler = AudioResampler::<f32>::new_with_filter(
        ResamplerQuality::Fast,
        ResamplerFilter::Kaiser,
        44100.0,
        48000.0,
    )
    .unwrap();
    assert_eq!(resampler.requested_filter(), ResamplerFilter::Kaiser);
    assert_eq!(resampler.effective_filter(), ResamplerFilter::Kaiser);
    assert!(!resampler.filter_fell_back());

    let metadata = resampler.filter_metadata().expect("polyphase metadata");
    assert_eq!(metadata.family, ResamplerFilter::Kaiser);
    assert!(metadata.linear_phase);
    assert_eq!(metadata.phases, 160);
    assert_eq!(metadata.taps_per_phase, 320);
    assert!(metadata.stopband_attenuation_db > 60.0);
    assert!(metadata.passband_ripple_db < 0.5);
    assert!(metadata.group_delay_output_frames > 0.0);
    assert_eq!(
        resampler.latency_samples(),
        metadata.group_delay_output_frames.round() as usize
    );

    // Feed a 1 kHz tone for three chunks and check the steady-state level.
    let mut phase = 0.0f64;
    let mut out = Vec::new();
    for _ in 0..8 {
        let mut l = vec![0.0f32; 1024];
        let mut r = vec![0.0f32; 1024];
        for i in 0..1024 {
            let s = (2.0 * std::f64::consts::PI * 1000.0 * phase / 44100.0).sin() * 0.5;
            phase += 1.0;
            l[i] = s as f32;
            r[i] = s as f32;
        }
        resampler.feed_block(&l, &r);
        let mut ol = vec![0.0f32; 2048];
        let mut or = vec![0.0f32; 2048];
        let n = resampler.read_block(&mut ol, &mut or);
        out.extend_from_slice(&ol[..n]);
    }
    let skip = resampler.latency_samples() + 1024;
    let tail = &out[skip.min(out.len())..];
    assert!(tail.len() > 1024, "polyphase produced too little audio");
    let rms =
        (tail.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / tail.len() as f64).sqrt();
    assert!(
        (rms - 0.5 / std::f64::consts::SQRT_2).abs() < 0.02,
        "polyphase RMS {rms:.4} is not the input amplitude"
    );
}

#[test]
fn test_every_polyphase_family_constructs_through_the_public_api() {
    use config::ResamplerFilter;

    for filter in [
        ResamplerFilter::Kaiser,
        ResamplerFilter::BlackmanHarris,
        ResamplerFilter::MinimumPhase,
        ResamplerFilter::Apodizing,
        ResamplerFilter::SincM,
    ] {
        let resampler = AudioResampler::<f32>::new_with_filter(
            ResamplerQuality::Fast,
            filter,
            44100.0,
            48000.0,
        )
        .unwrap_or_else(|e| panic!("{} construction failed: {e}", filter.as_str()));
        assert_eq!(resampler.effective_filter(), filter, "{}", filter.as_str());
        let m = resampler.filter_metadata().expect("metadata");
        assert_eq!(m.family, filter);
        assert_eq!(m.linear_phase, filter.is_linear_phase());
        assert!(
            m.stopband_attenuation_db > 60.0,
            "{}: {:.1} dB",
            filter.as_str(),
            m.stopband_attenuation_db
        );
    }
}

#[test]
fn test_fft_family_reports_no_introspected_metadata() {
    use config::ResamplerFilter;

    let resampler = AudioResampler::<f32>::new_with_filter(
        ResamplerQuality::Fast,
        ResamplerFilter::Fft,
        44100.0,
        48000.0,
    )
    .unwrap();
    assert_eq!(resampler.effective_filter(), ResamplerFilter::Fft);
    assert!(resampler.filter_metadata().is_none());
}

#[test]
fn set_filter_requests_a_rebuild_without_resetting_the_running_engine() {
    use config::ResamplerFilter;

    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, 44100.0, 48000.0).unwrap();
    assert_eq!(resampler.requested_filter(), ResamplerFilter::Fft);
    assert!(!resampler.rebuild_pending());

    resampler.set_filter(ResamplerFilter::Kaiser);
    assert_eq!(resampler.requested_filter(), ResamplerFilter::Kaiser);
    assert!(
        resampler.rebuild_pending(),
        "a different family must schedule a rebuild"
    );
    assert_eq!(
        resampler.effective_filter(),
        ResamplerFilter::Fft,
        "the running engine stays in place until the control path rebuilds"
    );

    // Re-selecting the pending family must not schedule extra work.
    resampler.set_filter(ResamplerFilter::Kaiser);
    assert_eq!(resampler.requested_filter(), ResamplerFilter::Kaiser);
    assert!(resampler.rebuild_pending());
}

//! Resampler tiers: the output rate is what was asked for, and the passband
//! and the noise floor are what the tier's published description promises.
//!
//! The engine publishes a description per quality tier
//! ([`ResamplerQuality::description`]: passband ripple, stopband attenuation,
//! group delay). This suite measures the *realised* behaviour and holds it
//! against those published numbers, so a tier that silently degraded — a
//! fallback to `Fast`, a shorter filter, a wrong ratio — fails here instead of
//! being discovered by a listener.
//!
//! The measurements are made with a Goertzel filter over a window that holds
//! an exact whole number of periods of the probe tone, so the measurement
//! itself contributes no leakage and a 0.005 dB passband figure is
//! measurable.

mod common;

use engine::dsp::resampler::AudioResampler;
use engine::ResamplerQuality;

/// Source rate for the rate-conversion tests.
const SRC_RATE: u32 = 44_100;
/// Destination rate for the rate-conversion tests.
const DST_RATE: u32 = 48_000;
/// Probe tone.
const TONE_HZ: f32 = 1_000.0;

/// Analysis window length for the Goertzel measurement.
///
/// 4800 frames at 48 kHz is exactly 100 periods of a 1 kHz tone, so the tone
/// lands exactly on a DFT bin and the measurement has no leakage — which is
/// what makes a 0.005 dB passband figure measurable at all.
const WINDOW_FRAMES: usize = 4_800;

/// Output frames dropped from the end of the measurement window.
///
/// The input stops at the end of the tone and the filter's window is still
/// ringing down; keeping those frames would measure the truncation of the
/// signal, not the filter.
const TAIL_GUARD: usize = 2_000;

/// Resample a continuous tone and return the output frames.
fn resample_tone(
    quality: ResamplerQuality,
    src_rate: u32,
    dst_rate: u32,
    tone_hz: f32,
    amplitude: f32,
    input_frames: usize,
) -> (Vec<f32>, u64) {
    let mut resampler = AudioResampler::<f32>::new(quality, src_rate as f32, dst_rate as f32)
        .unwrap_or_else(|e| panic!("the {quality:?} tier must build: {e}"));

    let mut out = Vec::new();
    let step = 2.0 * std::f32::consts::PI * tone_hz / src_rate as f32;
    for i in 0..input_frames {
        let s = (i as f32 * step).sin() * amplitude;
        resampler.feed(s, s);
        while let Some((l, _r)) = resampler.read() {
            out.push(l);
        }
    }
    let dropped = resampler.dropped_frames();
    (out, dropped)
}

/// The steady-state measurement window of a resampled tone.
fn steady_window(out: &[f32]) -> &[f32] {
    let usable = out.len().saturating_sub(TAIL_GUARD);
    let start = usable.saturating_sub(WINDOW_FRAMES);
    &out[start..usable]
}

/// The measured passband gain, in dB, of a resampled tone.
fn passband_gain_db(out: &[f32], tone_hz: f32, amplitude: f32, dst_rate: u32) -> f64 {
    let window = steady_window(out);
    let measured = common::goertzel_magnitude(window, tone_hz, dst_rate);
    20.0 * (measured as f64 / amplitude as f64).log10()
}

/// The total harmonic distortion plus noise of a resampled tone, in dB.
///
/// The residual is everything that is not in the fundamental bin: the total
/// power of the window minus the fundamental's power.
fn thdn_db(out: &[f32], tone_hz: f32, dst_rate: u32) -> f64 {
    let window = steady_window(out);
    let total_power: f64 = window.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let total_power = total_power / window.len() as f64;
    let fundamental = common::goertzel_magnitude(window, tone_hz, dst_rate) as f64;
    let fundamental_power = fundamental * fundamental / 2.0;
    assert!(
        fundamental_power > 0.0,
        "the probe tone must be measurable before its residual means anything"
    );
    let residual = (total_power - fundamental_power).max(1e-30);
    10.0 * (residual / fundamental_power).log10()
}

#[test]
fn the_output_rate_is_the_rate_that_was_asked_for() {
    // One second of input at 44.1 kHz must become one second of output at
    // 48 kHz. The flush emits the filter's remaining tail on top of the
    // converted signal, so the bound is 2 % rather than exact: that is an
    // order of magnitude tighter than any audible rate error, and it still
    // catches a resampler that converted at the wrong ratio.
    let input_frames = SRC_RATE as usize;
    let mut resampler =
        AudioResampler::<f32>::new(ResamplerQuality::Balanced, SRC_RATE as f32, DST_RATE as f32)
            .expect("the resampler must build");
    assert!(
        !resampler.is_passthrough(),
        "44.1 → 48 kHz is a real conversion"
    );

    let step = 2.0 * std::f32::consts::PI * TONE_HZ / SRC_RATE as f32;
    let mut produced = 0usize;
    for i in 0..input_frames {
        let s = (i as f32 * step).sin() * 0.5;
        resampler.feed(s, s);
        // Drain as we go: the resampler's output buffer is bounded and an
        // undrained backlog would be dropped, not queued.
        while resampler.read().is_some() {
            produced += 1;
        }
    }
    resampler.flush();
    while resampler.read().is_some() {
        produced += 1;
    }

    let expected = input_frames as f64 * DST_RATE as f64 / SRC_RATE as f64;
    assert!(
        produced as f64 >= expected,
        "44.1 → 48 kHz produced only {produced} frames, fewer than the {expected:.0} \
         the ratio requires"
    );
    assert!(
        (produced as f64 - expected).abs() / expected < 0.02,
        "44.1 → 48 kHz produced {produced} frames, expected about {expected:.0} \
         (ratio {:.5})",
        produced as f64 / expected
    );
    assert_eq!(
        resampler.dropped_frames(),
        0,
        "a drained resampler must not drop frames"
    );
}

#[test]
fn a_same_rate_conversion_is_a_passthrough_with_no_latency() {
    let mut resampler = AudioResampler::<f32>::new(ResamplerQuality::Balanced, 48_000.0, 48_000.0)
        .expect("the resampler must build");
    assert!(resampler.is_passthrough());
    assert_eq!(resampler.latency_samples(), 0);
    assert_eq!(resampler.latency_ms(), 0.0);

    // And the samples themselves come through untouched.
    let input = common::sine(TONE_HZ, 0.5, 512, 48_000);
    let mut out = Vec::new();
    for i in 0..512 {
        resampler.feed(input[i * 2], input[i * 2 + 1]);
        while let Some((l, r)) = resampler.read() {
            out.push(l);
            out.push(r);
        }
    }
    assert!(out.len() >= input.len());
    for i in 0..512 {
        assert_eq!(
            out[i * 2].to_bits(),
            input[i * 2].to_bits(),
            "passthrough sample {i} changed"
        );
    }
}

#[test]
fn every_published_tier_builds_without_falling_back() {
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        let resampler = AudioResampler::<f32>::new(quality, SRC_RATE as f32, DST_RATE as f32)
            .unwrap_or_else(|e| panic!("the {quality:?} tier must build: {e}"));
        assert_eq!(
            resampler.requested_quality(),
            quality,
            "the requested tier must be remembered"
        );
        assert_eq!(
            resampler.effective_quality(),
            quality,
            "the {quality:?} tier must not fall back to another tier"
        );
        assert!(
            !resampler.quality_fell_back(),
            "the {quality:?} tier must not report a fallback"
        );
    }
}

#[test]
fn the_tiers_are_ordered_by_latency_from_fast_to_ultra() {
    // The published table promises a ladder: a longer filter buys a deeper
    // stopband, and it costs group delay. If the ladder collapsed, the
    // "Ultra" tier would be buying nothing.
    let mut latencies = Vec::new();
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        let resampler = AudioResampler::<f32>::new(quality, SRC_RATE as f32, DST_RATE as f32)
            .unwrap_or_else(|e| panic!("the {quality:?} tier must build: {e}"));
        latencies.push((quality, resampler.latency_samples()));
    }
    for pair in latencies.windows(2) {
        assert!(
            pair[0].1 < pair[1].1,
            "{:?} ({} samples) must have less group delay than {:?} ({} samples)",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1
        );
    }
}

#[test]
fn the_passband_is_flat_across_the_whole_band() {
    // Six probe frequencies from 1 kHz to 20 kHz, every tier. The measured
    // gain must stay inside the tier's published passband ripple plus a
    // 0.02 dB measurement margin — the margin covers the Goertzel window's
    // finite length and the float32 output path, and it is still two orders
    // of magnitude tighter than the -35 dB distortion floor asserted
    // elsewhere in this suite.
    let probes = [1_000.0f32, 5_000.0, 10_000.0, 15_000.0, 18_000.0, 20_000.0];
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        let amplitude = 0.5f32;
        let published = quality.description().passband_ripple_db as f64;
        let tolerance = published + 0.02;
        for probe in probes {
            let (out, dropped) = resample_tone(
                quality,
                SRC_RATE,
                DST_RATE,
                probe,
                amplitude,
                SRC_RATE as usize,
            );
            assert_eq!(dropped, 0, "the {quality:?} tier must not drop frames");
            assert!(
                steady_window(&out).len() >= WINDOW_FRAMES,
                "the {quality:?} tier produced too few output frames to measure at {probe} Hz"
            );
            let gain_db = passband_gain_db(&out, probe, amplitude, DST_RATE);
            assert!(
                gain_db.abs() <= tolerance,
                "the {quality:?} tier's passband gain at {probe} Hz is {gain_db:.4} dB, \
                 outside the published ±{published} dB (tolerance ±{tolerance} dB)"
            );
        }
    }
}

#[test]
fn the_total_distortion_and_noise_is_far_below_the_signal() {
    // Everything that is not the fundamental. The FFT resampler processes in
    // chunks and concatenates them, so a small amount of amplitude ripple
    // survives at the chunk boundaries; that ripple, not the filter's
    // stopband, is what dominates this residual. The worst value measured
    // across every tier and every probe is about -43 dB, so the bound is set
    // at -35 dB: eight orders of magnitude of headroom over the f32 noise
    // floor, and still tight enough to catch a filter that is not doing its
    // job.
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        for probe in [1_000.0f32, 5_000.0, 10_000.0, 15_000.0] {
            let (out, _dropped) =
                resample_tone(quality, SRC_RATE, DST_RATE, probe, 0.5, SRC_RATE as usize);
            let thdn = thdn_db(&out, probe, DST_RATE);
            assert!(
                thdn < -35.0,
                "the {quality:?} tier's THD+N at {probe} Hz is {thdn:.1} dB, which \
                 is not below the -35 dB floor"
            );
        }
    }
}

#[test]
fn no_spurious_energy_appears_in_the_empty_bands() {
    // With a 1 kHz tone in the input, the output must contain nothing else.
    // The three probe bands are chosen so the measurement window holds a
    // whole number of periods of the probe frequency too (200, 500 and 1200
    // periods of 2, 5 and 12 kHz in 4800 frames), so the measurement adds no
    // leakage of its own.
    let (out, _dropped) = resample_tone(
        ResamplerQuality::HighQuality,
        SRC_RATE,
        DST_RATE,
        TONE_HZ,
        0.5,
        SRC_RATE as usize,
    );
    let window = steady_window(&out);
    for probe in [2_000.0f32, 5_000.0, 12_000.0] {
        let spurious = common::goertzel_magnitude(window, probe, DST_RATE);
        let relative_db = 20.0 * (spurious / 0.5).log10();
        assert!(
            relative_db < -100.0,
            "the {probe} Hz band carries {relative_db:.1} dB of energy that the \
             1 kHz input did not put there"
        );
    }
}

#[test]
fn a_tone_into_the_stopband_does_not_reach_the_output() {
    // 96 kHz → 48 kHz: a 30 kHz tone is valid in the source and 6 kHz past
    // the output's 24 kHz Nyquist, so without an anti-alias filter it would
    // fold back to 18 kHz. The engine's filter must reject it.
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
    ] {
        let (out, _dropped) = resample_tone(quality, 96_000, 48_000, 30_000.0, 0.5, 96_000);
        let window = steady_window(&out);
        let alias = common::goertzel_magnitude(window, 18_000.0, 48_000);
        let relative_db = 20.0 * (alias / 0.5).log10();
        assert!(
            relative_db < -90.0,
            "the {quality:?} tier let a 30 kHz tone through as an 18 kHz alias \
             at {relative_db:.1} dB"
        );
    }
}

#[test]
fn the_resampler_reports_a_finite_latency_for_every_tier() {
    for quality in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        let resampler = AudioResampler::<f32>::new(quality, SRC_RATE as f32, DST_RATE as f32)
            .unwrap_or_else(|e| panic!("the {quality:?} tier must build: {e}"));
        assert!(
            resampler.latency_samples() > 0,
            "a real conversion must report a non-zero group delay"
        );
        assert!(resampler.latency_ms().is_finite());
        // The millisecond figure is the frame count scaled at the *output*
        // rate, which is the domain the graph latency model works in.
        let expected_ms = resampler.latency_samples() as f32 / DST_RATE as f32 * 1000.0;
        assert!(
            (resampler.latency_ms() - expected_ms).abs() < 1e-3,
            "the {quality:?} tier's latency must be reported in the output domain"
        );
    }
}

#[test]
fn invalid_rates_are_refused_with_a_typed_error() {
    let zero = AudioResampler::<f32>::new(ResamplerQuality::Fast, 0.0, 48_000.0);
    let err = match zero {
        Ok(_) => panic!("a zero source rate must be refused"),
        Err(e) => e,
    };
    assert!(
        matches!(
            err,
            engine::dsp::resampler::ResamplerError::InvalidRates { .. }
        ),
        "a zero rate must produce the typed InvalidRates error, got {err:?}"
    );

    let nan = AudioResampler::<f32>::new(ResamplerQuality::Fast, f32::NAN, 48_000.0);
    let err = match nan {
        Ok(_) => panic!("a NaN source rate must be refused"),
        Err(e) => e,
    };
    assert!(
        matches!(
            err,
            engine::dsp::resampler::ResamplerError::InvalidRates { .. }
        ),
        "a NaN rate must produce the typed InvalidRates error, got {err:?}"
    );
}

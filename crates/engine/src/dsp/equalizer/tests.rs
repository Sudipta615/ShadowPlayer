use super::super::{EqBandParams, ParametricEq};
use crate::dsp::filter_design::BandwidthMode;

/// Steady-state gain (dB) of the EQ at `freq_hz`, measured with an exact-bin
/// DFT over 1024 samples so a 3-samples-per-period tone still measures its
/// real amplitude instead of a peak-detected underestimate.
fn probe_gain_db(eq: &mut ParametricEq, freq_hz: f32, sr: f32) -> f64 {
    const N: usize = 1024;
    const SETTLE: usize = 4096;
    let w = std::f64::consts::TAU * (freq_hz as f64) / (sr as f64);
    for i in 0..SETTLE {
        let s = (w * i as f64).sin() as f32;
        eq.process(s, s);
    }
    let mut re = 0.0f64;
    let mut im = 0.0f64;
    for i in 0..N {
        let s = (w * (i + SETTLE) as f64).sin() as f32;
        let (l, _) = eq.process(s, s);
        re += l as f64 * (w * i as f64).cos();
        im -= l as f64 * (w * i as f64).sin();
    }
    let amplitude = 2.0 * (re * re + im * im).sqrt() / N as f64;
    20.0 * amplitude.max(1e-12).log10()
}

/// The octave bandwidth model is reachable through the EQ, keeps the band's
/// gain at its centre, and widens the top octaves instead of narrowing them.
#[test]
fn the_octave_bandwidth_model_keeps_a_high_band_wide() {
    let sr = 48_000.0;
    let center = 12_000.0;
    let mut eq = ParametricEq::new(1, sr);
    eq.set_enabled(true);
    // A 1/3-octave band: Q ≈ 4.318 is the octave width's low-frequency
    // equivalent, which is what a graphic EQ band generator stores.
    eq.set_band(
        0,
        EqBandParams::peaking(center, 6.0, config::q_from_octaves(1.0 / 3.0) as f32),
    );

    let q_center = probe_gain_db(&mut eq, center, sr);
    // 15 kHz is 0.32 octave above the centre — just past the -3 dB edge of a
    // 1/3-octave band.
    let q_edge = probe_gain_db(&mut eq, 15_000.0, sr);
    assert!(
        (q_center - 6.0).abs() < 0.3,
        "the band should still peak at +6 dB, measured {q_center:.2}"
    );

    assert!(eq.set_bandwidth_mode(BandwidthMode::NyquistOctaves));
    assert_eq!(eq.bandwidth_mode(), BandwidthMode::NyquistOctaves);
    let oct_center = probe_gain_db(&mut eq, center, sr);
    let oct_edge = probe_gain_db(&mut eq, 15_000.0, sr);
    assert!(
        (oct_center - 6.0).abs() < 0.3,
        "center gain must not move with the width model: {oct_center:.2}"
    );
    assert!(
        oct_edge > q_edge + 0.5,
        "the octave model must hold the band open: {oct_edge:.2} dB at 15 kHz vs \
         {q_edge:.2} dB with the Q model"
    );

    // Switching back restores the previous behaviour exactly, and is a no-op
    // the second time.
    assert!(eq.set_bandwidth_mode(BandwidthMode::EquivalentQ));
    assert!(!eq.set_bandwidth_mode(BandwidthMode::EquivalentQ));
    let restored = probe_gain_db(&mut eq, 15_000.0, sr);
    assert!(
        (restored - q_edge).abs() < 0.05,
        "returning to the Q model should measure {q_edge:.2} dB again, got {restored:.2}"
    );
}

/// A band installed with a file-format width lands on the same filter as the
/// equivalent Q, and the headroom estimate follows the curve immediately.
#[test]
fn importing_a_width_lands_on_the_equivalent_q() {
    let mut eq = ParametricEq::new(1, 44_100.0);
    eq.set_enabled(true);
    eq.set_auto_headroom(true);
    eq.set_band_width(
        0,
        EqBandParams::peaking(1_000.0, 9.0, 0.0),
        config::Bandwidth::Octaves(0.5),
    );
    let band = eq.band_params(0).unwrap();
    assert!(
        (band.q - 2.8717).abs() < 1e-3,
        "half an octave must import as Q ≈ 2.87, got {}",
        band.q
    );
    assert!(
        (eq.headroom_db() + 9.0).abs() < 0.1,
        "auto headroom should follow the imported band, got {}",
        eq.headroom_db()
    );
}

/// The bandwidth model must survive a band-layout change, or a live preset
/// edit would silently revert to the Q model.
#[test]
fn the_bandwidth_model_survives_a_layout_change() {
    let mut eq = ParametricEq::new(2, 48_000.0);
    let _ = eq.set_bandwidth_mode(BandwidthMode::NyquistOctaves);
    eq.set_band_layout(&[
        EqBandParams::peaking(1_000.0, 3.0, 1.0),
        EqBandParams::peaking(2_000.0, -3.0, 1.0),
        EqBandParams::peaking(4_000.0, 1.5, 1.0),
    ]);
    assert_eq!(eq.bandwidth_mode(), BandwidthMode::NyquistOctaves);
    assert_eq!(eq.num_bands(), 3);

    eq.set_band_count(5);
    assert_eq!(eq.bandwidth_mode(), BandwidthMode::NyquistOctaves);
    assert_eq!(eq.num_bands(), 5);
}

#[test]
fn test_eq_passthrough_when_disabled() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    let (l, r) = eq.process(0.5, 0.5);
    assert!((l - 0.5).abs() < 1e-5);
    assert!((r - 0.5).abs() < 1e-5);
}

#[test]
fn test_eq_enabled_zero_gain() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    // After settling, zero-gain EQ should pass signal through
    for _ in 0..500 {
        eq.process(0.5, 0.5);
    }
    let (l, _r) = eq.process(0.5, 0.5);
    assert!(
        (l - 0.5).abs() < 0.05,
        "Zero-gain EQ should be near passthrough"
    );
}

#[test]
fn test_eq_set_band() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_band(0, EqBandParams::peaking(100.0, 6.0, 1.4));
    let params = eq.band_params(0).unwrap();
    assert_eq!(params.frequency, 100.0);
    assert_eq!(params.gain_db, 6.0);
}

#[test]
fn test_stereo_imaging_preserved() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    eq.set_band(0, EqBandParams::peaking(1000.0, 6.0, 1.4));
    // Process same signal on both channels
    for _ in 0..200 {
        eq.process(0.5, 0.5);
    }
    let (l, r) = eq.process(0.5, 0.5);
    assert!((l - r).abs() < 0.01, "Stereo imaging should be preserved");
}

#[test]
fn test_eq_headroom_is_static_not_dynamic() {
    // Headroom is a STATIC pre-EQ attenuation: a fixed -3 dB gain applied
    // before the filters. It must NOT behave like a compressor — no
    // attack/release pumping, just a constant linear scale.
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    eq.set_headroom_db(-3.0);

    let expected = 2.0 * 10.0_f32.powf(-3.0 / 20.0); // ≈ 1.414

    // Feed a loud signal; after the filters' transient response settles,
    // the gain must be exactly the -3 dB headroom (plus unity band gain).
    for _ in 0..5000 {
        let _ = eq.process(2.0, 2.0);
    }
    let (l, r) = eq.process(2.0, 2.0);
    assert!(
        (l - expected).abs() < 0.02 && (r - expected).abs() < 0.02,
        "static headroom: expected ~{expected:.3}, got l={l:.3}, r={r:.3}"
    );

    // Constant gain regardless of input level => linear, not dynamic.
    let (l2, _) = eq.process(0.1, 0.1);
    let gain_ratio_hi = l / 2.0;
    let gain_ratio_lo = l2 / 0.1;
    assert!(
        (gain_ratio_hi - gain_ratio_lo).abs() < 0.01,
        "headroom must be level-independent (static), got {gain_ratio_hi} vs {gain_ratio_lo}"
    );

    // Default headroom is 0 dB (unity) — no unintended attenuation.
    let mut eq2 = ParametricEq::default_10_band(44100.0);
    eq2.set_enabled(true);
    for _ in 0..5000 {
        let _ = eq2.process(0.5, 0.5);
    }
    let (l3, _) = eq2.process(0.5, 0.5);
    assert!(
        (l3 - 0.5).abs() < 0.02,
        "default headroom must be unity, got {l3}"
    );
}

#[test]
fn test_eq_reset_clears_filter_state() {
    // After reset, the EQ filters should be in a clean state (no
    // ringing from prior processing). This is the property the old
    // test_headroom_resets_to_unity was really verifying — that
    // reset() returns runtime state to a known-good baseline.
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);

    // Run a loud signal to populate filter state.
    for _ in 0..1000 {
        eq.process(2.0, 2.0);
    }
    // After reset, processing silence should produce near-silence
    // (filter state is cleared, no ringing).
    eq.reset();
    let mut max_out: f32 = 0.0;
    for _ in 0..100 {
        let (l, r) = eq.process(0.0, 0.0);
        max_out = max_out.max(l.abs()).max(r.abs());
    }
    assert!(
        max_out < 1e-4,
        "After reset, processing silence should produce near-silence; got max={}",
        max_out
    );
}

#[test]
fn test_headroom_estimator_catches_high_q_peak() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    // A +12 dB, Q=50 peak is only ~20 Hz wide at 1 kHz. The previous
    // 151-point log sweep stepped ~47 Hz there and missed it entirely.
    eq.set_band(3, EqBandParams::peaking(1000.0, 12.0, 50.0));
    let boost = eq.combined_max_gain_db(44100.0);
    assert!(
        (boost - 12.0).abs() < 0.05,
        "Q=50 peak should measure ~12 dB, got {boost}"
    );
}

#[test]
fn test_headroom_estimator_extreme_q_and_low_frequency() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    // Q=100 is the biquad's clamp limit; at 40 Hz its -3 dB bandwidth is
    // only 0.4 Hz — the hardest practical target for the estimator.
    eq.set_band(3, EqBandParams::peaking(40.0, 9.0, 100.0));
    let boost = eq.combined_max_gain_db(44100.0);
    assert!(
        (boost - 9.0).abs() < 0.05,
        "low-frequency Q=100 peak should measure ~9 dB, got {boost}"
    );
}

#[test]
fn test_headroom_estimator_low_shelf_dc_gain() {
    // A Butterworth (Q=0.707) low shelf is monotonic, so its maximum is
    // exactly the +6 dB DC gain — which lives below the 20 Hz sweep floor
    // and must come from the analytic DC evaluation.
    let mut eq = ParametricEq::new(1, 44100.0);
    eq.set_enabled(true);
    eq.set_band(0, EqBandParams::lowshelf(100.0, 6.0, 0.707));
    let boost = eq.combined_max_gain_db(44100.0);
    assert!(
        (boost - 6.0).abs() < 0.05,
        "low-shelf boost should measure ~6 dB at DC, got {boost}"
    );
}

#[test]
fn test_auto_headroom_recomputes_on_band_change() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    eq.set_auto_headroom(true);

    eq.set_band(3, EqBandParams::peaking(1000.0, 8.0, 1.0));
    assert!(
        (eq.headroom_db() - (-8.0)).abs() < 0.05,
        "auto headroom should be -8 dB, got {}",
        eq.headroom_db()
    );

    eq.set_band(3, EqBandParams::peaking(1000.0, 4.0, 1.0));
    assert!(
        (eq.headroom_db() - (-4.0)).abs() < 0.05,
        "auto headroom should track the curve to -4 dB, got {}",
        eq.headroom_db()
    );

    eq.set_band(3, EqBandParams::peaking(1000.0, -4.0, 1.0));
    assert!(
        eq.headroom_db().abs() < 0.05,
        "cut-only curve should need no headroom, got {}",
        eq.headroom_db()
    );

    eq.set_auto_headroom(false);
    eq.set_headroom_db(-2.0);
    eq.set_band(3, EqBandParams::peaking(1000.0, 12.0, 1.0));
    assert!(
        (eq.headroom_db() - (-2.0)).abs() < 1e-6,
        "manual headroom must be preserved when auto headroom is off"
    );
}

#[test]
fn test_auto_headroom_disable_restores_manual() {
    let mut eq = ParametricEq::default_10_band(44100.0);
    eq.set_enabled(true);
    eq.set_headroom_db(-3.0);
    eq.set_auto_headroom(true);
    eq.set_band(3, EqBandParams::peaking(1000.0, 9.0, 1.0));
    assert!(
        (eq.headroom_db() - (-9.0)).abs() < 0.05,
        "auto headroom should override the manual value, got {}",
        eq.headroom_db()
    );

    eq.set_auto_headroom(false);
    assert!(
        (eq.headroom_db() - (-3.0)).abs() < 1e-6,
        "disabling auto headroom should restore the manual -3 dB, got {}",
        eq.headroom_db()
    );
}

#[test]
fn test_disabled_lowpass_band_is_flat_passthrough() {
    let sr = 48_000.0;
    let mut eq = ParametricEq::new(1, sr);
    eq.set_enabled(true);

    // Band 0 is a LowPass filter at 1000 Hz, but DISABLED
    let params = EqBandParams {
        filter_type: super::super::EqFilterType::LowPass,
        frequency: 1000.0,
        q: 0.707,
        gain_db: 0.0,
        enabled: false,
    };
    eq.set_band(0, params);

    // Warm up past smoothing window
    for _ in 0..1000 {
        let _ = eq.process(0.5, 0.5);
    }

    // A tone at 5 kHz (well above the 1 kHz cutoff) must pass through with 0 dB attenuation (unity)
    let gain_5k = probe_gain_db(&mut eq, 5000.0, sr);
    assert!(
        gain_5k.abs() < 0.1,
        "disabled LowPass band must not attenuate 5 kHz tone, measured {gain_5k:.2} dB"
    );
}

#[test]
fn test_headroom_estimator_includes_resonant_lowpass() {
    let sr = 48_000.0;
    let mut eq = ParametricEq::new(1, sr);
    eq.set_enabled(true);
    eq.set_auto_headroom(true);

    // Resonant lowpass at 1 kHz with Q=2.0 produces a ~+6 dB resonance peak
    let params = EqBandParams {
        filter_type: super::super::EqFilterType::LowPass,
        frequency: 1000.0,
        q: 2.0,
        gain_db: 0.0,
        enabled: true,
    };
    eq.set_band(0, params);

    let max_boost = eq.combined_max_gain_db(sr);
    assert!(
        max_boost > 5.0 && max_boost < 7.0,
        "resonant Q=2.0 Lowpass peak must be detected as ~+6 dB boost, got {max_boost:.2} dB"
    );
    assert!(
        (eq.headroom_db() + max_boost).abs() < 0.1,
        "auto headroom must attenuate to protect against the resonant peak"
    );
}

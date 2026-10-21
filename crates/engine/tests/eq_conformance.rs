//! Biquad conformance: every filter type the engine offers, checked against
//! the RBJ cookbook formula recomputed here from scratch.
//!
//! The engine computes its biquad coefficients in f64 from the RBJ (Audio EQ
//! Cookbook) definitions. This suite re-derives those coefficients
//! *independently* — the formulas are written out again in this file, not
//! imported from the engine — and holds two things against them:
//!
//! * the engine's own analytic magnitude response
//!   (`BiquadCoeffs::evaluate_magnitude`), and
//! * the engine's *measured* response: a sine fed through the filter and
//!   measured at the output.
//!
//! The second one is the one that matters. A filter can have a perfect
//! magnitude formula and still be wired into the chain wrongly; the
//! time-domain measurement is what a listener would hear.

use engine::dsp::biquad::{BiquadCoeffs, BiquadState};
use engine::dsp::equalizer::EqFilterType;
use engine::dsp::FilterType;

/// Sample rate for the measurements.
const SR: f32 = 48_000.0;
/// Sample rate as f64, for the independent computation.
const SR_F64: f64 = 48_000.0;

/// Probe tones used by the measurement. Every one of them divides the
/// measurement window (4800 frames = 0.1 s) into a whole number of periods,
/// so the Goertzel measurement adds no leakage of its own.
const PROBE_LOW: f32 = 100.0;
const PROBE_MID: f32 = 1_000.0;
const PROBE_HIGH: f32 = 2_000.0;
const PROBE_TOP: f32 = 10_000.0;

/// One filter under test.
#[derive(Debug, Clone, Copy)]
struct Case {
    filter: FilterType,
    /// Corner / centre frequency.
    f0: f32,
    /// Gain in dB (meaningful for the gain-shaped types).
    gain_db: f32,
    /// Quality factor.
    q: f32,
    /// Frequency at which the response is measured.
    probe_hz: f32,
}

const CASES: &[Case] = &[
    Case {
        filter: FilterType::Lowpass,
        f0: 1_000.0,
        gain_db: 0.0,
        q: 0.707,
        probe_hz: PROBE_MID,
    },
    Case {
        filter: FilterType::Highpass,
        f0: 1_000.0,
        gain_db: 0.0,
        q: 0.707,
        probe_hz: PROBE_MID,
    },
    Case {
        filter: FilterType::Bandpass,
        f0: 1_000.0,
        gain_db: 0.0,
        q: 4.0,
        probe_hz: PROBE_HIGH,
    },
    Case {
        filter: FilterType::Notch,
        f0: 1_000.0,
        gain_db: 0.0,
        q: 0.707,
        probe_hz: PROBE_HIGH,
    },
    Case {
        filter: FilterType::Allpass,
        f0: 1_000.0,
        gain_db: 0.0,
        q: 0.707,
        probe_hz: PROBE_MID,
    },
    Case {
        filter: FilterType::Peaking,
        f0: 1_000.0,
        gain_db: 6.0,
        q: 1.0,
        probe_hz: PROBE_MID,
    },
    Case {
        filter: FilterType::Lowshelf,
        f0: 1_000.0,
        gain_db: 6.0,
        q: 0.707,
        probe_hz: PROBE_LOW,
    },
    Case {
        filter: FilterType::Highshelf,
        f0: 1_000.0,
        gain_db: 6.0,
        q: 0.707,
        probe_hz: PROBE_TOP,
    },
];

/// The RBJ cookbook coefficients for `case`, computed here from scratch.
///
/// This is deliberately a *second* implementation: if it were the engine's
/// own code the comparison would be circular.
fn rbj_coefficients(case: &Case) -> (f64, f64, f64, f64, f64) {
    let w0 = 2.0 * std::f64::consts::PI * f64::from(case.f0) / SR_F64;
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let alpha = sin_w0 / (2.0 * f64::from(case.q));
    let a = 10f64.powf(f64::from(case.gain_db) / 40.0);

    let (b0, b1, b2, a0, a1, a2) = match case.filter {
        FilterType::Lowpass => (
            (1.0 - cos_w0) / 2.0,
            1.0 - cos_w0,
            (1.0 - cos_w0) / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        FilterType::Highpass => (
            (1.0 + cos_w0) / 2.0,
            -(1.0 + cos_w0),
            (1.0 + cos_w0) / 2.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        FilterType::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha),
        FilterType::Notch => (
            1.0,
            -2.0 * cos_w0,
            1.0,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        FilterType::Allpass => (
            1.0 - alpha,
            -2.0 * cos_w0,
            1.0 + alpha,
            1.0 + alpha,
            -2.0 * cos_w0,
            1.0 - alpha,
        ),
        FilterType::Peaking => (
            1.0 + alpha * a,
            -2.0 * cos_w0,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w0,
            1.0 - alpha / a,
        ),
        FilterType::Lowshelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + t),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - t),
                (a + 1.0) + (a - 1.0) * cos_w0 + t,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - t,
            )
        }
        FilterType::Highshelf => {
            let t = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + t),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - t),
                (a + 1.0) - (a - 1.0) * cos_w0 + t,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - t,
            )
        }
    };

    (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// The expected magnitude response of `case` at its probe frequency, in dB.
fn expected_db(case: &Case) -> f64 {
    let (b0, b1, b2, a1, a2) = rbj_coefficients(case);
    let w = 2.0 * std::f64::consts::PI * f64::from(case.probe_hz) / SR_F64;
    let (sin_w, cos_w) = w.sin_cos();
    let (sin_2w, cos_2w) = (2.0 * w).sin_cos();

    let num_re = b0 + b1 * cos_w + b2 * cos_2w;
    let num_im = -b1 * sin_w - b2 * sin_2w;
    let den_re = 1.0 + a1 * cos_w + a2 * cos_2w;
    let den_im = -a1 * sin_w - a2 * sin_2w;

    let mag = ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).sqrt();
    20.0 * mag.log10()
}

/// The engine's coefficients for `case`.
fn engine_coefficients(case: &Case) -> BiquadCoeffs<f64> {
    case.filter
        .compute_coeffs::<f64>(SR, case.f0, case.gain_db, case.q)
}

/// Measure a filter's response in the time domain: feed a sine at the probe
/// frequency through the filter and measure the output amplitude.
fn measured_db(case: &Case, coeffs: &BiquadCoeffs<f64>) -> f64 {
    let total_frames = 48_000usize; // one second
    let window = 4_800usize; // the last 0.1 s
    let mut state = BiquadState::<f64>::default();
    let step = 2.0 * std::f64::consts::PI * f64::from(case.probe_hz) / SR_F64;
    let mut tail = Vec::with_capacity(window);
    for i in 0..total_frames {
        let x = (i as f64 * step).sin();
        let y = state.process(x, coeffs);
        if i >= total_frames - window {
            tail.push(y as f32);
        }
    }
    // Goertzel over a window that holds a whole number of probe periods.
    let measured = goertzel(&tail, f64::from(case.probe_hz));
    // The input amplitude is 1.0.
    20.0 * measured.log10()
}

/// Goertzel magnitude of `samples` at `freq_hz` (the sample rate is the
/// measurement rate).
fn goertzel(samples: &[f32], freq_hz: f64) -> f64 {
    let n = samples.len() as f64;
    let k = 2.0 * std::f64::consts::PI * freq_hz / SR_F64;
    let coeff = 2.0 * k.cos();
    let mut s1 = 0.0f64;
    let mut s2 = 0.0f64;
    for &x in samples {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let real = s1 - s2 * k.cos();
    let imag = s2 * k.sin();
    2.0 * (real * real + imag * imag).sqrt() / n
}

#[test]
fn every_filter_type_the_engine_offers_is_covered_by_this_suite() {
    // A new filter type must not be able to slip past this suite: the case
    // list has to name every variant of the enum.
    let covered: Vec<FilterType> = CASES.iter().map(|c| c.filter).collect();
    let all = [
        FilterType::Lowpass,
        FilterType::Highpass,
        FilterType::Bandpass,
        FilterType::Notch,
        FilterType::Allpass,
        FilterType::Peaking,
        FilterType::Lowshelf,
        FilterType::Highshelf,
    ];
    for filter in all {
        assert!(
            covered.contains(&filter),
            "the {filter:?} filter type is not covered by this suite"
        );
    }
    assert_eq!(
        covered.len(),
        all.len(),
        "the suite must have exactly one case per filter type"
    );
}

#[test]
fn the_engines_magnitude_response_matches_the_rbj_formula() {
    for case in CASES {
        let coeffs = engine_coefficients(case);
        let expected = expected_db(case);
        let engine = coeffs.evaluate_magnitude(case.probe_hz, SR);
        let engine_db = 20.0 * engine.log10();
        assert!(
            (engine_db - expected).abs() < 1e-9,
            "{:?}: the engine's magnitude response at {} Hz is {engine_db:.9} dB, \
             the RBJ formula says {expected:.9} dB",
            case.filter,
            case.probe_hz
        );
    }
}

#[test]
fn the_filtered_signal_measures_the_response_the_formula_predicts() {
    for case in CASES {
        let coeffs = engine_coefficients(case);
        let expected = expected_db(case);
        let measured = measured_db(case, &coeffs);
        // The time-domain measurement is limited by the filter's numerical
        // noise floor and by the transient of the IIR (whose time constant is
        // Q / (π · f0) — 1.3 ms at the worst case here, so a one-second run
        // settles it completely). 0.05 dB is two orders of magnitude tighter
        // than the smallest effect any of these filters has, and it is still
        // generous against f64 round-off.
        assert!(
            (measured - expected).abs() < 0.05,
            "{:?}: the filtered signal measures {measured:.4} dB at {} Hz, \
             the RBJ formula says {expected:.4} dB",
            case.filter,
            case.probe_hz
        );
    }
}

#[test]
fn the_well_known_reference_points_are_what_they_should_be() {
    // Independent of the general comparison: the textbook values.
    let lp = BiquadCoeffs::<f64>::lowpass(SR, 1_000.0, 0.707);
    let lp_db = 20.0 * lp.evaluate_magnitude(1_000.0, SR).log10();
    assert!(
        (lp_db + 3.0103).abs() < 0.01,
        "a Butterworth low-pass is −3.01 dB at its corner, got {lp_db:.4} dB"
    );

    let hp = BiquadCoeffs::<f64>::highpass(SR, 1_000.0, 0.707);
    let hp_db = 20.0 * hp.evaluate_magnitude(1_000.0, SR).log10();
    assert!(
        (hp_db + 3.0103).abs() < 0.01,
        "a Butterworth high-pass is −3.01 dB at its corner, got {hp_db:.4} dB"
    );

    let peak = BiquadCoeffs::<f64>::peaking(SR, 1_000.0, 6.0, 1.0);
    let peak_db = 20.0 * peak.evaluate_magnitude(1_000.0, SR).log10();
    assert!(
        (peak_db - 6.0).abs() < 1e-6,
        "a peaking filter is exactly its gain at its centre, got {peak_db:.6} dB"
    );

    let allpass = BiquadCoeffs::<f64>::allpass(SR, 1_000.0, 0.707);
    for probe in [100.0f32, 1_000.0, 5_000.0, 15_000.0] {
        let db = 20.0 * allpass.evaluate_magnitude(probe, SR).log10();
        assert!(
            db.abs() < 1e-9,
            "an all-pass filter has a flat magnitude, at {probe} Hz it is {db:.9} dB"
        );
    }

    // A notch removes its centre frequency entirely.
    let notch = BiquadCoeffs::<f64>::notch(SR, 1_000.0, 0.707);
    let notch_db = 20.0 * notch.evaluate_magnitude(1_000.0, SR).log10();
    assert!(
        notch_db < -100.0,
        "a notch must remove its centre frequency, it is at {notch_db:.2} dB"
    );
}

#[test]
fn the_eq_module_s_filter_vocabulary_maps_onto_the_same_filters() {
    // Two spellings of the same eight filters exist in the engine: the
    // DSP-internal `FilterType` and the config-facing `EqFilterType`. They
    // must not drift apart.
    let pairs = [
        (EqFilterType::Peaking, FilterType::Peaking),
        (EqFilterType::LowShelf, FilterType::Lowshelf),
        (EqFilterType::HighShelf, FilterType::Highshelf),
        (EqFilterType::LowPass, FilterType::Lowpass),
        (EqFilterType::HighPass, FilterType::Highpass),
        (EqFilterType::Bandpass, FilterType::Bandpass),
        (EqFilterType::Notch, FilterType::Notch),
        (EqFilterType::AllPass, FilterType::Allpass),
    ];
    for (eq_type, filter_type) in pairs {
        assert_eq!(
            eq_type.to_filter_type(),
            filter_type,
            "the EQ vocabulary must map {eq_type:?} onto {filter_type:?}"
        );
        let from_eq = eq_type
            .to_filter_type()
            .compute_coeffs::<f64>(SR, 1_000.0, 6.0, 0.707);
        let from_filter = filter_type.compute_coeffs::<f64>(SR, 1_000.0, 6.0, 0.707);
        assert_eq!(
            from_eq.b0.to_bits(),
            from_filter.b0.to_bits(),
            "the two vocabularies must produce the same coefficients for {eq_type:?}"
        );
        assert_eq!(
            from_eq.b1.to_bits(),
            from_filter.b1.to_bits(),
            "the two vocabularies must produce the same coefficients for {eq_type:?}"
        );
    }
}

#[test]
fn the_parametric_eq_produces_the_response_its_band_type_promises() {
    // The EQ is the user-facing surface; the band it builds must have the
    // response the filter type promises.
    use engine::dsp::equalizer::{EqBandParams, ParametricEq};

    let probes = [
        (EqFilterType::Peaking, 1_000.0f32, 6.0f32, 1.0f32, PROBE_MID),
        (EqFilterType::LowShelf, 1_000.0, 6.0, 0.707, PROBE_LOW),
        (EqFilterType::HighShelf, 1_000.0, 6.0, 0.707, PROBE_TOP),
    ];
    for (eq_type, f0, gain_db, q, probe_hz) in probes {
        let mut eq = ParametricEq::new(4, SR);
        eq.set_enabled(true);
        eq.set_band(
            0,
            EqBandParams {
                enabled: true,
                filter_type: eq_type,
                frequency: f0,
                gain_db,
                q,
            },
        );

        let case = Case {
            filter: eq_type.to_filter_type(),
            f0,
            gain_db,
            q,
            probe_hz,
        };
        let expected = expected_db(&case);

        // Measure the EQ's own output.
        let total_frames = 48_000usize;
        let window = 4_800usize;
        let mut left = Vec::with_capacity(total_frames);
        let mut right = Vec::with_capacity(total_frames);
        let step = 2.0 * std::f64::consts::PI * f64::from(probe_hz) / SR_F64;
        for i in 0..total_frames {
            let x = (i as f64 * step).sin() as f32;
            let (l, r) = eq.process(x, x);
            left.push(l);
            right.push(r);
        }
        let measured = 20.0 * goertzel(&left[total_frames - window..], f64::from(probe_hz)).log10();

        assert!(
            (measured - expected).abs() < 0.1,
            "{eq_type:?}: the EQ measures {measured:.4} dB at {probe_hz} Hz, \
             the RBJ formula says {expected:.4} dB",
        );
    }
}

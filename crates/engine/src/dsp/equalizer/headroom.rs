//! Peak-gain estimation for a cascaded biquad curve.
//!
//! # What it is for
//!
//! The EQ's automatic headroom reserves exactly the boost the current curve
//! asks for, so the limiter downstream never has to absorb the EQ's own gain.
//! It runs on **every** band edit — a slider drag is dozens of estimates a
//! second on the UI/command thread — so the estimator has to be both accurate
//! (an underestimate means the limiter has to catch the EQ's peak) and cheap.
//!
//! # How it works
//!
//! 1. **DC and Nyquist analytically.** A shelving band reaches its extremum
//!    exactly at a band edge, where the sweep endpoints are not; both are
//!    evaluated in closed form from the transfer function
//!    (`|H(1)| = |(b₀+b₁+b₂)/(1+a₁+a₂)|`, `|H(−1)| = |(b₀−b₁+b₂)/(1−a₁+a₂)|`).
//! 2. **A log sweep for the interior**, using the `φ = sin²(ω/2)` closed form
//!    from [`crate::dsp::filter_design::magnitude_squared_at_phi`]: one `sin`
//!    per frequency for the whole cascade, instead of four trigonometric calls
//!    per band per frequency. This is the step that makes the estimate
//!    affordable — the previous implementation spent ~4·N trigonometry calls
//!    on each of 4096 sweep points.
//! 3. **Golden-section refinement of the strongest lobes.** A narrow, high-Q
//!    peak is located by the sweep but its apex generally falls between grid
//!    points (a Q = 100 band at 40 Hz is 0.4 Hz wide while the grid step there
//!    is ~0.14 Hz, so the sampled maximum can sit half a dB low). Refining the
//!    best few local maxima recovers the apex to well under 0.01 dB.
//!
//! # Contract
//!
//! The result is an **upper bound in practice**: it never reports less than the
//! curve's true maximum by more than the refinement tolerance
//! (`tests::the_estimate_matches_a_brute_force_sweep` holds it to 0.01 dB
//! against a 200 000-point sweep over several shapes). That direction matters:
//! too much headroom costs a little level, too little costs limiter action.

use crate::dsp::biquad::BiquadCoeffs;
/// Coarse log-sweep resolution: 1024 points is ~0.68 % per step across a
/// 20 Hz … 20 kHz band — 0.14 Hz at 20 Hz, which is inside the 0.2 Hz lobe of
/// the narrowest band the EQ can express (Q = 100 at 20 Hz). The sweep only has
/// to *locate* each lobe; refinement measures its apex.
const SWEEP_POINTS: usize = 1024;

/// How many of the strongest sweep lobes get golden-section refinement.
/// Between adjacent peaking bands the total curve can have several local
/// maxima; the highest one wins, and this many covers a dense graphic curve
/// without making the estimate expensive.
const REFINED_LOBES: usize = 8;

/// Converge to this log-frequency distance (natural log of the frequency).
const REFINE_TOLERANCE_LN: f64 = 1e-4;

/// Largest combined **boost** of `coeffs` in `[f_min, f_max]`, in dB.
///
/// Returns `0.0` for an empty cascade or for a curve that only attenuates, so
/// a caller can use the result directly as `-boost` headroom.
pub fn max_curve_gain_db(
    coeffs: &[BiquadCoeffs<f64>],
    sample_rate: f64,
    f_min: f64,
    f_max: f64,
) -> f64 {
    if coeffs.is_empty() {
        return 0.0;
    }
    let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    };
    let nyquist = sr * 0.5;

    // Band edges, in the open interval the transfer function is defined on.
    let lo = f_min.max(1e-3).min(nyquist * 0.999);
    let hi = f_max.max(lo).min(nyquist * 0.999_9);

    let mut max_mag = 1.0_f64;
    max_mag = max_mag.max(dc_gain(coeffs));
    max_mag = max_mag.max(nyquist_gain(coeffs));

    if hi > lo {
        max_mag = max_mag.max(sweep_and_refine(coeffs, sr, lo, hi));
    } else {
        max_mag = max_mag.max(magnitude_at(coeffs, sr, lo));
    }

    if max_mag > 1.0 {
        20.0 * max_mag.log10()
    } else {
        0.0
    }
}

/// Combined linear magnitude of the cascade at one frequency.
fn magnitude_at(coeffs: &[BiquadCoeffs<f64>], sample_rate: f64, freq_hz: f64) -> f64 {
    let omega = std::f64::consts::TAU * freq_hz / sample_rate;
    let sin_half = (omega * 0.5).sin();
    let phi = sin_half * sin_half;
    let mut total = 1.0_f64;
    for coeff in coeffs {
        total *= crate::dsp::filter_design::magnitude_squared_at_phi(coeff, phi);
    }
    total.max(0.0).sqrt()
}

/// `|H(1)|` — the response at DC.
fn dc_gain(coeffs: &[BiquadCoeffs<f64>]) -> f64 {
    let mut total = 1.0_f64;
    for coeff in coeffs {
        let den = 1.0 + coeff.a1 + coeff.a2;
        if den.abs() > 1e-12 {
            total *= ((coeff.b0 + coeff.b1 + coeff.b2) / den).abs();
        }
    }
    if total.is_finite() {
        total
    } else {
        1.0
    }
}

/// `|H(−1)|` — the response at Nyquist.
fn nyquist_gain(coeffs: &[BiquadCoeffs<f64>]) -> f64 {
    let mut total = 1.0_f64;
    for coeff in coeffs {
        let den = 1.0 - coeff.a1 + coeff.a2;
        if den.abs() > 1e-12 {
            total *= ((coeff.b0 - coeff.b1 + coeff.b2) / den).abs();
        }
    }
    if total.is_finite() {
        total
    } else {
        1.0
    }
}

/// The coarse log sweep, followed by refinement of the strongest lobes.
fn sweep_and_refine(coeffs: &[BiquadCoeffs<f64>], sample_rate: f64, lo: f64, hi: f64) -> f64 {
    let log_lo = lo.ln();
    let log_hi = hi.ln();
    let step = (log_hi - log_lo) / SWEEP_POINTS as f64;

    // Coarse pass: keep the magnitudes and remember where the curve turns over.
    let mut values = [0.0_f64; SWEEP_POINTS + 1];
    let mut best = 0.0_f64;
    for (i, slot) in values.iter_mut().enumerate() {
        let f = (log_lo + i as f64 * step).exp();
        let m = magnitude_at(coeffs, sample_rate, f);
        *slot = m;
        best = best.max(m);
    }

    // Local maxima, strongest first. The endpoints are covered by the analytic
    // DC/Nyquist evaluation, so only interior turns are candidates.
    let mut lobes: Vec<(f64, usize)> = Vec::with_capacity(REFINED_LOBES * 2);
    for i in 1..SWEEP_POINTS {
        if values[i] >= values[i - 1] && values[i] >= values[i + 1] {
            lobes.push((values[i], i));
        }
    }
    lobes.sort_by(|a, b| b.0.total_cmp(&a.0));

    for &(sampled, index) in lobes.iter().take(REFINED_LOBES) {
        if sampled < best * 0.5 {
            // A lobe this far below the best one cannot hold the maximum.
            break;
        }
        let a = log_lo + (index as f64 - 1.0) * step;
        let b = log_lo + (index as f64 + 1.0) * step;
        let refined = refine(coeffs, sample_rate, a, b);
        best = best.max(refined);
    }
    best
}

/// Golden-section maximisation of the cascade magnitude over
/// `[a_ln, b_ln]` (natural-log frequency).
fn refine(coeffs: &[BiquadCoeffs<f64>], sample_rate: f64, mut a_ln: f64, mut b_ln: f64) -> f64 {
    const INV_PHI: f64 = 0.618_033_988_749_894_9;
    let f = |ln_f: f64| magnitude_at(coeffs, sample_rate, ln_f.exp());

    let mut c = b_ln - INV_PHI * (b_ln - a_ln);
    let mut d = a_ln + INV_PHI * (b_ln - a_ln);
    let mut fc = f(c);
    let mut fd = f(d);
    let mut best = fc.max(fd);
    while (b_ln - a_ln).abs() > REFINE_TOLERANCE_LN {
        if fc < fd {
            a_ln = c;
            c = d;
            fc = fd;
            d = a_ln + INV_PHI * (b_ln - a_ln);
            fd = f(d);
        } else {
            b_ln = d;
            d = c;
            fd = fc;
            c = b_ln - INV_PHI * (b_ln - a_ln);
            fc = f(c);
        }
        best = best.max(fc).max(fd);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::biquad::FilterType;

    const SR: f64 = 48_000.0;

    fn band(kind: FilterType, freq: f64, gain: f64, q: f64) -> BiquadCoeffs<f64> {
        kind.compute_coeffs::<f64>(SR as f32, freq as f32, gain as f32, q as f32)
    }

    /// Brute-force reference: 200 000 log-spaced points, DC and Nyquist
    /// included, measured with the complex response (fully independent of the
    /// `φ` identity the estimator uses).
    fn brute_force_max(coeffs: &[BiquadCoeffs<f64>], f_min: f64, f_max: f64) -> f64 {
        const N: usize = 200_000;
        let log_lo = f_min.ln();
        let log_hi = f_max.ln();
        let mut best = 1.0_f64;
        for i in 0..=N {
            let f = (log_lo + (log_hi - log_lo) * i as f64 / N as f64).exp();
            let mut m = 1.0_f64;
            for c in coeffs {
                m *= c.evaluate_magnitude(f as f32, SR as f32);
            }
            best = best.max(m);
        }
        // Edges, where shelves peak.
        for f in [f_min, f_max] {
            let mut m = 1.0_f64;
            for c in coeffs {
                m *= c.evaluate_magnitude(f as f32, SR as f32);
            }
            best = best.max(m);
        }
        if best > 1.0 {
            20.0 * best.log10()
        } else {
            0.0
        }
    }

    fn estimate(coeffs: &[BiquadCoeffs<f64>]) -> f64 {
        max_curve_gain_db(coeffs, SR, 20.0, 20_000.0_f64.min(SR * 0.49))
    }

    #[test]
    fn an_empty_or_flat_curve_needs_no_headroom() {
        assert_eq!(max_curve_gain_db(&[], SR, 20.0, 20_000.0), 0.0);
        assert_eq!(
            max_curve_gain_db(
                &[band(FilterType::Peaking, 1_000.0, 0.0, 1.0)],
                SR,
                20.0,
                20_000.0
            ),
            0.0
        );
    }

    #[test]
    fn a_cut_only_curve_needs_no_headroom() {
        let coeffs = [
            band(FilterType::Peaking, 100.0, -6.0, 1.0),
            band(FilterType::Peaking, 3_000.0, -9.0, 2.0),
        ];
        assert_eq!(max_curve_gain_db(&coeffs, SR, 20.0, 20_000.0), 0.0);
    }

    /// The shape from the EQ's own regression suite: a +12 dB Q = 50 peak is
    /// ~20 Hz wide at 1 kHz, and a +9 dB Q = 100 peak at 40 Hz is 0.4 Hz wide.
    /// Both must be found exactly, not approximately.
    #[test]
    fn narrow_peaks_are_measured_at_their_apex() {
        let cases = [
            (1_000.0, 12.0, 50.0),
            (40.0, 9.0, 100.0),
            (16_000.0, 15.0, 8.0),
            (20_000.0, 6.0, 0.5),
        ];
        for (freq, gain, q) in cases {
            let coeffs = [band(FilterType::Peaking, freq, gain, q)];
            let measured = estimate(&coeffs);
            assert!(
                (measured - gain).abs() < 0.02,
                "{freq} Hz {gain} dB Q={q} measured {measured:.4} dB"
            );
        }
    }

    /// The estimator against a brute-force sweep, on curves with several lobes,
    /// a shelf that peaks at DC, and a broad tilt.
    #[test]
    fn the_estimate_matches_a_brute_force_sweep() {
        let curves: Vec<Vec<BiquadCoeffs<f64>>> = vec![
            vec![
                band(FilterType::Peaking, 80.0, 6.0, 1.4),
                band(FilterType::Peaking, 1_000.0, -4.0, 2.0),
                band(FilterType::Peaking, 6_300.0, 8.0, 3.0),
            ],
            vec![
                band(FilterType::Lowshelf, 100.0, 9.0, 0.707),
                band(FilterType::Peaking, 2_000.0, 3.0, 1.0),
            ],
            vec![
                band(FilterType::Peaking, 500.0, 5.0, 30.0),
                band(FilterType::Peaking, 530.0, 5.0, 30.0),
            ],
            // A rising tilt: no interior lobe, the maximum is at the top edge.
            vec![band(FilterType::Highshelf, 4_000.0, 6.0, 0.9)],
        ];
        for (index, curve) in curves.iter().enumerate() {
            let estimate_db = estimate(curve);
            let reference_db = brute_force_max(curve, 20.0, 20_000.0);
            assert!(
                estimate_db >= reference_db - 0.01,
                "curve {index}: estimate {estimate_db:.4} dB underestimates the \
                 brute-force maximum {reference_db:.4} dB"
            );
            assert!(
                estimate_db <= reference_db + 0.05,
                "curve {index}: estimate {estimate_db:.4} dB overshoots the \
                 brute-force maximum {reference_db:.4} dB"
            );
        }
    }

    #[test]
    fn the_maximum_above_nyquist_is_not_invented() {
        // f_max above Nyquist is clamped instead of evaluated, which the
        // transfer function at |z| = 1 cannot do.
        let coeffs = [band(FilterType::Peaking, 20_000.0, 6.0, 1.0)];
        let measured = max_curve_gain_db(&coeffs, 44_100.0, 20.0, 40_000.0);
        assert!(measured.is_finite() && measured > 0.0 && measured < 8.0);
    }

    #[test]
    fn a_degenerate_corner_needs_no_sweep() {
        let coeffs = [band(FilterType::Peaking, 1_000.0, 6.0, 1.0)];
        // f_min > f_max would have produced an empty (and unfindable) range.
        let measured = max_curve_gain_db(&coeffs, SR, 5_000.0, 100.0);
        assert!(measured.is_finite());
    }
}

//! Filter design: width models, Nyquist-aware prototypes, Butterworth
//! cascades, and a cheap exact magnitude formula.
//!
//! # Why a design layer next to [`crate::dsp::biquad`]
//!
//! [`crate::dsp::biquad`] answers "what are the coefficients of a peaking
//! filter with Q". That is enough until the Q has to come from somewhere other
//! than a slider — a `BW Oct` field read out of an imported Equalizer APO file,
//! a shelf *slope*, a 4th-order Butterworth crossover — and until the band sits
//! close enough to Nyquist that the plain cookbook design stops describing what
//! was asked for. This module is that layer:
//!
//! * [`Bandwidth`] — the four ways the reference implementations spell a band
//!   width, converted through [`config::eq_width`] so the importer and the DSP
//!   cannot disagree.
//! * [`FilterDesign`] — which prototype maps the analog response onto the
//!   digital one: the engine's historical cookbook bilinear transform, the
//!   Nyquist-compensated octave form, or Vicanek's matched second-order
//!   design.
//! * [`design`] — one entry point producing [`BiquadCoeffs<f64>`] from any
//!   combination of the above. With `Bandwidth::Q` and `FilterDesign::Bilinear`
//!   it **delegates** to the existing constructors, so the engine's default
//!   path is bit-identical to before this module existed.
//! * [`butterworth_q`] / [`butterworth_sections`] — the per-section Q values
//!   that turn a biquad bank into a Butterworth filter of arbitrary even order.
//! * [`magnitude_squared_at`] — the closed-form `|H|²` (no complex arithmetic,
//!   one `sin` for the whole cascade), which is what makes the EQ's headroom
//!   estimate affordable (see `dsp::equalizer::headroom`).
//!
//! # Provenance of each formula
//!
//! | Formula | Where it comes from |
//! | --- | --- |
//! | `alpha = sin(ω₀)·sinh(ln2/2 · bw · ω₀/sin ω₀)` | RBJ cookbook case 2 as implemented in Equalizer APO (`filters/BiQuad.cpp`) and bmc0/dsp (`biquad.c`, width `o`) |
//! | `alpha = sin(ω₀)/2 · sqrt((A + 1/A)(1/S − 1) + 2)` | RBJ cookbook case 3, same two implementations (width `s`) |
//! | Vicanek matched design | Martin Vicanek, *Matched Second Order Digital Filters*, as ported by Signalsmith Audio (`filters.h`, `BiquadDesign::vicanek`) |
//! | Butterworth section Q `1/(2·sin(π/N·(k − ½)))` | bmc0/dsp `parse_width` (`bwN`), the standard Butterworth pole placement |
//! | `|H|²` from `φ = sin²(ω/2)` | J. O. Smith's allpass/energy identity, in the exact arrangement Equalizer APO's `BiQuad::gainAt` uses |
//!
//! Nothing here allocates, and nothing here touches the realtime thread beyond
//! a handful of transcendental calls at coefficient time.

use crate::dsp::biquad::{BiquadCoeffs, FilterType};
use config::{Bandwidth, MAX_OCTAVES, MAX_SLOPE, MIN_OCTAVES, MIN_SLOPE};

/// Which prototype maps the analog filter onto the digital one.
///
/// The three differ *only* in how they handle frequency warping near Nyquist
/// (and, for [`Self::Vicanek`], how the zeros are chosen once the poles are
/// fixed). Two filters designed with the same width and different prototypes
/// have the same gain at `f₀` and the same DC/Nyquist asymptotes; they differ
/// in the shape between those points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterDesign {
    /// The engine's historical path: the bilinear transform with
    /// `alpha = sin(ω₀)/(2Q)`. Bandwidth shrinks as `f₀ → f_s/2`.
    #[default]
    Bilinear,
    /// The bilinear transform with the octave width held constant up to
    /// Nyquist: `alpha = sin(ω₀)·sinh(ln2/2 · bw · ω₀/sin ω₀)`.
    ///
    /// This is the Equalizer APO / bmc0 `BW Oct` form. It is strictly better
    /// for a band written as an octave width (a 1/3-octave graphic EQ keeps
    /// 1/3 octave at 16 kHz instead of narrowing to ~15 % of it), and it is
    /// what an imported `BW Oct` band should use if the width came from a
    /// measurement rather than from a slider.
    NyquistOctaves,
    /// Vicanek's matched second-order design: poles from the impulse-invariant
    /// transform, zeros chosen to match the analog prototype's magnitude.
    ///
    /// This is the only one of the three that keeps a low-pass's Nyquist
    /// behaviour and a peaking band's skirt shape close to the analog
    /// prototype instead of forcing them to the bilinear image.
    Vicanek,
}

/// Which model a band's stored `Q` means.
///
/// `Q` is the only width the engine stores (an `EqBandConfig` carries `q`), so
/// a band's width *meaning* has to be declared separately when it is not the
/// cookbook Q.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BandwidthMode {
    /// `q` is the cookbook `Q`: `alpha = sin(ω₀)/(2Q)`. The engine's default
    /// and the only behaviour that existed before this module.
    #[default]
    EquivalentQ,
    /// `q` is the low-frequency equivalent of an octave width —
    /// `q = 1/(2·sinh(ln2·bw/2))` — and the filter is designed with
    /// [`FilterDesign::NyquistOctaves`] so that width holds up to Nyquist.
    NyquistOctaves,
}

/// Design one biquad from an explicit width and prototype.
///
/// The parameters are validated with the same bounds the biquad constructors
/// use (`f₀` clamped to `1 … f_s·0.499`, `Q` to `0.01 … 100`, gain to
/// `±48 dB`), so a design can never produce coefficients the rest of the engine
/// would refuse to run.
///
/// `Bandwidth::Q` with [`FilterDesign::Bilinear`] delegates to
/// [`FilterType::compute_coeffs`] and is therefore bit-identical to the path
/// the engine used before this module existed.
pub fn design(
    kind: FilterType,
    filter_design: FilterDesign,
    sample_rate: f32,
    frequency: f32,
    gain_db: f32,
    width: Bandwidth,
) -> BiquadCoeffs<f64> {
    // The legacy path, delegated rather than re-derived: identical arithmetic
    // guarantees identical coefficients, which is what lets the EQ keep its
    // current behaviour by default.
    if filter_design == FilterDesign::Bilinear {
        if let Bandwidth::Q(q) = width {
            return kind.compute_coeffs::<f64>(sample_rate, frequency, gain_db, q);
        }
    }

    let sr = validate_sample_rate(sample_rate);
    let f0 = validate_frequency(sr, frequency);
    let gain = validate_gain(gain_db);

    let w0 = std::f64::consts::TAU * (f0 as f64) / (sr as f64);
    let sin_w0 = w0.sin();
    let cos_w0 = w0.cos();

    // `inv2q` is `alpha / sin(ω₀)`. Keeping the width in this form lets the
    // Vicanek branch reuse it directly (it wants `1/(2Q)`), and keeps a single
    // place where each width spelling is converted.
    let inv2q = inv_two_q(width, filter_design, w0, sin_w0, f0 as f64, gain);
    let alpha = sin_w0 * inv2q;

    if filter_design == FilterDesign::Vicanek {
        if let Some(coeffs) = vicanek(kind, f0 as f64 / sr as f64, w0, cos_w0, inv2q, gain) {
            return coeffs;
        }
        // Shelves and all-passes have no Vicanek closed form; Signalsmith
        // falls back to the bandwidth-compensated cookbook design, and so do
        // we — the fallback is deliberate, not a silent gap.
    }

    cookbook(kind, cos_w0, alpha, gain)
}

/// The cookbook biquad for an arbitrary `alpha`.
fn cookbook(kind: FilterType, cos_w0: f64, alpha: f64, gain_db: f64) -> BiquadCoeffs<f64> {
    let a = 10.0_f64.powf(gain_db / 40.0);
    let beta = 2.0 * a.sqrt() * alpha;

    let (b0, b1, b2, a0, a1, a2) = match kind {
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
        FilterType::Lowshelf => (
            a * ((a + 1.0) - (a - 1.0) * cos_w0 + beta),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
            a * ((a + 1.0) - (a - 1.0) * cos_w0 - beta),
            (a + 1.0) + (a - 1.0) * cos_w0 + beta,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
            (a + 1.0) + (a - 1.0) * cos_w0 - beta,
        ),
        FilterType::Highshelf => (
            a * ((a + 1.0) + (a - 1.0) * cos_w0 + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
            a * ((a + 1.0) + (a - 1.0) * cos_w0 - beta),
            (a + 1.0) - (a - 1.0) * cos_w0 + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
            (a + 1.0) - (a - 1.0) * cos_w0 - beta,
        ),
    };
    BiquadCoeffs::from_f64_raw(b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
}

/// Vicanek's matched second-order design (Signalsmith `BiquadDesign::vicanek`).
///
/// Returns `None` for the types the paper does not cover (shelves, all-pass),
/// which the caller turns into a documented fallback.
fn vicanek(
    kind: FilterType,
    scaled_freq: f64,
    w0: f64,
    cos_w0: f64,
    inv2q: f64,
    gain_db: f64,
) -> Option<BiquadCoeffs<f64>> {
    let sqrt_gain = 10.0_f64.powf(gain_db / 40.0);

    let mut inv2q = inv2q;
    if kind == FilterType::Notch {
        // Heuristic for notches near Nyquist: the notch must not close before
        // Nyquist, so narrow it as the centre approaches it.
        inv2q *= 1.0 - scaled_freq * 0.5;
    }

    // Poles come from the impulse-invariant transform of the analog prototype:
    // `z = exp(-q·ω₀ ± i·ω₀·sqrt(1 - q²))`.
    let q = (if kind == FilterType::Peaking {
        sqrt_gain
    } else {
        1.0
    }) * inv2q;
    let big_q = (if kind == FilterType::Peaking {
        0.5 * sqrt_gain
    } else {
        0.5
    }) / inv2q;

    let exp_mqw = (-q * w0).exp();
    let (a1, a2) = if q <= 1.0 {
        (
            -2.0 * exp_mqw * ((1.0 - q * q).sqrt() * w0).cos(),
            exp_mqw * exp_mqw,
        )
    } else {
        (
            -2.0 * exp_mqw * ((q * q - 1.0).sqrt() * w0).cosh(),
            exp_mqw * exp_mqw,
        )
    };

    // Numerator fitting uses the energy at three points, expressed through
    // `φ = sin²(ω₀/2)`; these are the paper's `p0, p1, p2`.
    let sin_pd2 = (w0 / 2.0).sin();
    let p0 = 1.0 - sin_pd2 * sin_pd2;
    let p1 = sin_pd2 * sin_pd2;
    let p2 = 4.0 * p0 * p1;
    let a0 = 1.0 + a1 + a2;
    let a1s = 1.0 - a1 + a2;
    let a2s = -4.0 * a2;
    let a0 = a0 * a0;
    let a1s = a1s * a1s;

    let (b0, b1, b2) = match kind {
        FilterType::Lowpass => {
            let r1 = (a0 * p0 + a1s * p1 + a2s * p2) * big_q * big_q;
            let b0n = a0;
            let b1n = (r1 - b0n * p0) / p1;
            let b0 = 0.5 * (b0n.sqrt() + b1n.max(0.0).sqrt());
            (b0, b0n.sqrt() - b0, 0.0)
        }
        FilterType::Highpass => {
            let b = ((a0 * p0 + a1s * p1 + a2s * p2).max(0.0)).sqrt() * big_q / (4.0 * p1);
            (b, -2.0 * b, b)
        }
        FilterType::Bandpass => {
            let r1 = a0 * p0 + a1s * p1 + a2s * p2;
            let r2 = -a0 + a1s + 4.0 * (p0 - p1) * a2s;
            let b2n = (r1 - r2 * p1) / (4.0 * p1 * p1);
            let b1n = r2 + 4.0 * (p1 - p0) * b2n;
            let b1 = -0.5 * b1n.max(0.0).sqrt();
            let b0 = 0.5 * ((b2n + 0.25 * b1n).max(0.0)).sqrt() - b1;
            (b0, b1, -b0 - b1)
        }
        FilterType::Notch => {
            let b0 = 1.0;
            let b1 = -2.0 * cos_w0;
            let b2 = 1.0;
            let scale = a0.sqrt() / (b0 + b1 + b2);
            (b0 * scale, b1 * scale, b2 * scale)
        }
        FilterType::Peaking => {
            let g2 = (sqrt_gain * sqrt_gain) * (sqrt_gain * sqrt_gain);
            let r1 = (a0 * p0 + a1s * p1 + a2s * p2) * g2;
            let r2 = (-a0 + a1s + 4.0 * (p0 - p1) * a2s) * g2;
            let b0n = a0;
            let b2n = (r1 - r2 * p1 - b0n) / (4.0 * p1 * p1);
            let b1n = r2 + b0n + 4.0 * (p1 - p0) * b2n;
            let w = 0.5 * (b0n.sqrt() + b1n.max(0.0).sqrt());
            let b0 = 0.5 * (w + (w * w + b2n).max(0.0).sqrt());
            let b1 = 0.5 * (b0n.sqrt() - b1n.max(0.0).sqrt());
            let b2 = if b0.abs() > 1e-12 {
                -b2n / (4.0 * b0)
            } else {
                0.0
            };
            (b0, b1, b2)
        }
        FilterType::Lowshelf | FilterType::Highshelf | FilterType::Allpass => return None,
    };

    if !(b0.is_finite() && b1.is_finite() && b2.is_finite() && a1.is_finite() && a2.is_finite()) {
        return None;
    }
    Some(BiquadCoeffs::from_f64_raw(b0, b1, b2, a1, a2))
}

/// Convert any width spelling into `1/(2Q)` — i.e. `alpha / sin(ω₀)`.
fn inv_two_q(
    width: Bandwidth,
    filter_design: FilterDesign,
    w0: f64,
    sin_w0: f64,
    center_hz: f64,
    gain_db: f64,
) -> f64 {
    match width {
        Bandwidth::Q(q) => 0.5 / clamp_q(q as f64),
        Bandwidth::Octaves(octaves) => {
            let bw = (octaves as f64).clamp(MIN_OCTAVES as f64, MAX_OCTAVES as f64);
            // `Bilinear` is the plain cookbook width: the octaves are quoted to
            // a Q at the low-frequency limit, so the band narrows as it climbs
            // toward Nyquist. `NyquistOctaves` multiplies by `ω₀/sin ω₀` — the
            // exact RBJ case-2 form Equalizer APO and bmc0 implement — which
            // holds the octave width across the whole band.
            let ratio = if filter_design == FilterDesign::NyquistOctaves && sin_w0.abs() > 1e-9 {
                w0 / sin_w0
            } else {
                1.0
            };
            (std::f64::consts::LN_2 * 0.5 * bw * ratio).sinh()
        }
        Bandwidth::Hz(hz) => {
            // Q = f₀ / bw, i.e. 1/(2Q) = bw / (2 f₀). A width wider than the
            // centre frequency would ask for Q < 0.5; clamp like any other Q.
            let hz = (hz as f64).max(1e-3);
            0.5 * hz / center_hz.max(1e-3)
        }
        Bandwidth::Slope(slope) => {
            // RBJ case 3: alpha = sin/2 · sqrt((A + 1/A)(1/S − 1) + 2).
            let s = (slope as f64).clamp(MIN_SLOPE as f64, MAX_SLOPE as f64);
            let a = 10.0_f64.powf(gain_db / 40.0);
            let term = (a + 1.0 / a) * (1.0 / s - 1.0) + 2.0;
            0.5 * term.max(0.0).sqrt()
        }
    }
    // `1/(2Q)` for the biquad's accepted `Q` range (0.01 … 100).
    .clamp(0.005, 50.0)
}

/// Clamp a Q into the range the biquad constructors accept.
#[inline]
fn clamp_q(q: f64) -> f64 {
    if q.is_finite() {
        q.clamp(0.01, 100.0)
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    }
}

#[inline]
fn validate_sample_rate(sample_rate: f32) -> f32 {
    if sample_rate > 0.0 && sample_rate.is_finite() {
        sample_rate
    } else {
        44_100.0
    }
}

#[inline]
fn validate_frequency(sample_rate: f32, frequency: f32) -> f32 {
    if !frequency.is_finite() || frequency <= 0.0 {
        return 20.0;
    }
    let hi = (sample_rate * 0.499).max(1.0);
    frequency.clamp(1.0, hi)
}

#[inline]
fn validate_gain(gain_db: f32) -> f64 {
    if gain_db.is_finite() {
        (gain_db.clamp(-48.0, 48.0)) as f64
    } else {
        0.0
    }
}

/// Q factor of one section of an `order`-th-order Butterworth filter.
///
/// A Butterworth response of order `N` is `N/2` cascaded biquads (plus a
/// first-order section for odd `N`, which this engine does not build). Section
/// `stage = 0` is the pair closest to the stop band; the Q values are
/// `1/(2·sin(π/N·(k − ½)))`, the same ladder bmc0/dsp builds for its `bwN`
/// width spelling.
///
/// `order` is rounded up to an even number ≥ 2, and an out-of-range `stage`
/// saturates to the last section rather than panicking on the audio thread.
pub fn butterworth_q(order: usize, stage: usize) -> f64 {
    let n = order.max(2);
    let sections = n / 2;
    let index = stage.min(sections.saturating_sub(1));
    // Index from the innermost conjugate pair, as the pole ladder is usually
    // written; stage 0 is the widest (lowest-Q) section.
    let k = (sections - index) as f64;
    1.0 / (2.0 * (std::f64::consts::PI / n as f64 * (k - 0.5)).sin())
}

/// The per-section Q values of an `order`-th-order Butterworth filter.
pub fn butterworth_sections(order: usize) -> Vec<f64> {
    let n = order.max(2) / 2;
    (0..n).map(|stage| butterworth_q(order, stage)).collect()
}

/// `|H(e^{jω})|²` of a biquad, from `φ = sin²(ω/2)`.
///
/// This is the closed form Equalizer APO uses in `BiQuad::gainAt`
/// (`10·log10(num) − 10·log10(den)` with the same `φ` polynomial): one `sin`
/// for the whole cascade instead of four trigonometric calls per section, and
/// no complex arithmetic. `|H|²` can come out very slightly negative for a
/// filter whose response *is* zero (a notch at its centre), which is clamped.
#[inline]
pub fn magnitude_squared_at(coeffs: &BiquadCoeffs<f64>, omega: f64) -> f64 {
    let sin_half = (omega * 0.5).sin();
    magnitude_squared_at_phi(coeffs, sin_half * sin_half)
}

/// `|H(e^{jω})|²` from a precomputed `φ = sin²(ω/2)`.
///
/// The same identity as [`magnitude_squared_at`], with the one trigonometric
/// call factored out so a *cascade* evaluated at one frequency pays for a
/// single `sin` rather than one per section
/// (see `dsp::equalizer::headroom`).
#[inline]
pub fn magnitude_squared_at_phi(coeffs: &BiquadCoeffs<f64>, phi: f64) -> f64 {
    let phi2 = phi * phi;

    let num = (coeffs.b0 + coeffs.b1 + coeffs.b2).powi(2)
        - 4.0 * (coeffs.b0 * coeffs.b1 + 4.0 * coeffs.b0 * coeffs.b2 + coeffs.b1 * coeffs.b2) * phi
        + 16.0 * coeffs.b0 * coeffs.b2 * phi2;
    let den = (1.0 + coeffs.a1 + coeffs.a2).powi(2)
        - 4.0 * (coeffs.a1 + 4.0 * coeffs.a2 + coeffs.a1 * coeffs.a2) * phi
        + 16.0 * coeffs.a2 * phi2;

    if den.abs() <= f64::MIN_POSITIVE {
        return 1.0;
    }
    (num / den).max(0.0)
}

/// `|H|` in dB of a biquad at `freq_hz`.
#[inline]
pub fn magnitude_db_at(coeffs: &BiquadCoeffs<f64>, freq_hz: f64, sample_rate: f64) -> f64 {
    if sample_rate <= 0.0 || freq_hz <= 0.0 {
        return 0.0;
    }
    let omega = std::f64::consts::TAU * freq_hz / sample_rate;
    10.0 * magnitude_squared_at(coeffs, omega).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const SR_F64: f64 = 48_000.0;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// Every filter type with the legacy width must produce *exactly* the
    /// coefficients the engine produced before this module existed. This is
    /// what lets `ParametricEq` keep its current sound by default.
    #[test]
    fn the_legacy_width_is_bit_identical_to_the_biquad_constructors() {
        for kind in [
            FilterType::Lowpass,
            FilterType::Highpass,
            FilterType::Bandpass,
            FilterType::Notch,
            FilterType::Allpass,
            FilterType::Peaking,
            FilterType::Lowshelf,
            FilterType::Highshelf,
        ] {
            for (freq, gain, q) in [
                (1_000.0_f32, 0.0_f32, 0.707_f32),
                (60.0, 6.0, 0.52),
                (16_000.0, -9.0, 4.31),
                (20_000.0, 12.0, 0.9),
            ] {
                let legacy = kind.compute_coeffs::<f64>(SR, freq, gain, q);
                let designed = design(
                    kind,
                    FilterDesign::Bilinear,
                    SR,
                    freq,
                    gain,
                    Bandwidth::Q(q),
                );
                assert_eq!(legacy.b0.to_bits(), designed.b0.to_bits(), "{kind:?} b0");
                assert_eq!(legacy.b1.to_bits(), designed.b1.to_bits(), "{kind:?} b1");
                assert_eq!(legacy.b2.to_bits(), designed.b2.to_bits(), "{kind:?} b2");
                assert_eq!(legacy.a1.to_bits(), designed.a1.to_bits(), "{kind:?} a1");
                assert_eq!(legacy.a2.to_bits(), designed.a2.to_bits(), "{kind:?} a2");
            }
        }
    }

    /// The closed form `|H|²` must agree with the complex evaluation the rest of
    /// the engine uses — including at a notch's null, where it is the only
    /// place the clamp matters.
    #[test]
    fn the_closed_form_magnitude_matches_the_complex_response() {
        for kind in [
            FilterType::Lowpass,
            FilterType::Highpass,
            FilterType::Bandpass,
            FilterType::Notch,
            FilterType::Allpass,
            FilterType::Peaking,
            FilterType::Lowshelf,
            FilterType::Highshelf,
        ] {
            let coeffs = kind.compute_coeffs::<f64>(SR, 1_000.0, 9.0, 1.4);
            for freq in [20.0_f64, 100.0, 1_000.0, 5_000.0, 20_000.0] {
                let omega = std::f64::consts::TAU * freq / SR_F64;
                let closed = magnitude_squared_at(&coeffs, omega).sqrt();
                let complex = coeffs.evaluate_magnitude(freq as f32, SR);
                // The two forms differ only by catastrophic cancellation at a
                // perfect null (a notch at its centre measures ~1e-7 here where
                // the complex form is ~1e-15). An absolute tolerance covers
                // both: on the pass band the arithmetic agrees to f64 rounding,
                // and at a null both are silence.
                assert!(
                    (closed - complex).abs() <= 1e-6 + 1e-9 * complex,
                    "{kind:?} at {freq} Hz: closed {closed} vs complex {complex}"
                );
            }
        }
    }

    /// A slope of `S = 1` is the Butterworth slope at every gain, so a shelf
    /// designed from it must be identical to the same shelf designed with
    /// `Q = 1/√2`.
    #[test]
    fn slope_one_designs_the_butterworth_shelf() {
        let butterworth = std::f64::consts::FRAC_1_SQRT_2 as f32;
        for gain in [-24.0_f32, -6.0, 6.0, 18.0] {
            for kind in [FilterType::Lowshelf, FilterType::Highshelf] {
                let by_slope = design(
                    kind,
                    FilterDesign::Bilinear,
                    SR,
                    200.0,
                    gain,
                    Bandwidth::Slope(1.0),
                );
                let by_q = kind.compute_coeffs::<f64>(SR, 200.0, gain, butterworth);
                for (a, b) in [
                    (by_slope.b0, by_q.b0),
                    (by_slope.b1, by_q.b1),
                    (by_slope.b2, by_q.b2),
                    (by_slope.a1, by_q.a1),
                    (by_slope.a2, by_q.a2),
                ] {
                    // The two paths spell the same formula in a different
                    // order, so this is an algebraic identity check, not a
                    // bit-identity one.
                    assert!(close(a, b, 1e-8), "{kind:?} {gain} dB: {a} vs {b}");
                }
            }
        }
    }

    /// An octave width is held all the way up the band with the Nyquist form,
    /// and compresses with the plain cookbook form. Measured as the −3 dB half
    /// width of a 6 dB peaking band at 16 kHz.
    #[test]
    fn the_octave_width_survives_up_to_nyquist_with_the_nyquist_design() {
        let sr = 48_000.0_f32;
        let f0 = 16_000.0_f64;
        let bw = 1.0_f32 / 3.0;

        let half_power_upper_edge = |coeffs: &BiquadCoeffs<f64>| {
            let target = 3.0; // dB below the peak
            let peak = magnitude_db_at(coeffs, f0, sr as f64);
            let mut best = f0;
            let mut f = f0;
            while f < sr as f64 * 0.499 {
                f *= 1.0005;
                if magnitude_db_at(coeffs, f, sr as f64) >= peak - target {
                    best = f;
                } else {
                    break;
                }
            }
            best
        };

        let nyquist = design(
            FilterType::Peaking,
            FilterDesign::NyquistOctaves,
            sr,
            f0 as f32,
            6.0,
            Bandwidth::Octaves(bw),
        );
        let cookbook_q = design(
            FilterType::Peaking,
            FilterDesign::Bilinear,
            sr,
            f0 as f32,
            6.0,
            Bandwidth::Octaves(bw),
        );

        // 1/3 octave above 16 kHz is 16·2^(1/6) ≈ 18.0 kHz.
        let expected = f0 * 2.0_f64.powf(1.0 / 6.0);
        let nyq_edge = half_power_upper_edge(&nyquist);
        let cb_edge = half_power_upper_edge(&cookbook_q);
        eprintln!(
            "upper half-power edge: nyquist {nyq_edge:.0} Hz, cookbook {cb_edge:.0} Hz, \
             target {expected:.0} Hz"
        );
        assert!(
            (nyq_edge - expected).abs() < 500.0,
            "Nyquist design edge {nyq_edge:.0} Hz should be near {expected:.0} Hz"
        );
        assert!(
            cb_edge < nyq_edge - 100.0,
            "cookbook design should compress the band: {cb_edge:.0} vs {nyq_edge:.0} Hz"
        );
    }

    /// The Vicanek low-pass keeps its -3 dB point and does not collapse toward
    /// Nyquist the way a bilinear low-pass does at a high corner.
    #[test]
    fn the_vicanek_lowpass_holds_its_corner_near_nyquist() {
        let sr = 44_100.0_f32;
        let f0 = 15_000.0_f32;
        let legacy = FilterType::Lowpass.compute_coeffs::<f64>(sr, f0, 0.0, 0.707);
        let vicanek = design(
            FilterType::Lowpass,
            FilterDesign::Vicanek,
            sr,
            f0,
            0.0,
            Bandwidth::Q(0.707),
        );
        // Both are Butterworth at f0 by construction.
        assert!(close(
            magnitude_db_at(&legacy, f0 as f64, sr as f64),
            -3.0103,
            0.05
        ));
        assert!(close(
            magnitude_db_at(&vicanek, f0 as f64, sr as f64),
            -3.0103,
            0.05
        ));
        // The bilinear design's stop band is pulled down hard before Nyquist;
        // the matched design stays a smooth 12 dB/octave roll-off.
        let at_20k_legacy = magnitude_db_at(&legacy, 20_000.0, sr as f64);
        let at_20k_vicanek = magnitude_db_at(&vicanek, 20_000.0, sr as f64);
        assert!(
            at_20k_vicanek > at_20k_legacy,
            "matched design should not dive early: {at_20k_vicanek:.2} vs {at_20k_legacy:.2} dB"
        );
        assert!(at_20k_vicanek < 0.0);
    }

    /// A peaking band designed by Vicanek still has exactly its gain at its
    /// centre frequency, and the same DC/Nyquist asymptotes.
    #[test]
    fn the_vicanek_peak_has_its_gain_at_the_centre() {
        for gain in [-12.0_f32, -3.0, 6.0, 15.0] {
            let coeffs = design(
                FilterType::Peaking,
                FilterDesign::Vicanek,
                SR,
                2_000.0,
                gain,
                Bandwidth::Q(1.0),
            );
            assert!(
                close(magnitude_db_at(&coeffs, 2_000.0, SR_F64), gain as f64, 0.05),
                "{gain} dB peak measured {:.3} dB",
                magnitude_db_at(&coeffs, 2_000.0, SR_F64)
            );
            assert!(magnitude_db_at(&coeffs, 20.0, SR_F64).abs() < 0.2);
        }
    }

    /// A Vicanek design for a type the paper does not cover falls back to the
    /// bandwidth-compensated cookbook design rather than to identity.
    #[test]
    fn vicanek_falls_back_to_the_cookbook_for_shelves_and_allpass() {
        for kind in [
            FilterType::Lowshelf,
            FilterType::Highshelf,
            FilterType::Allpass,
        ] {
            let coeffs = design(
                kind,
                FilterDesign::Vicanek,
                SR,
                500.0,
                6.0,
                Bandwidth::Q(0.707),
            );
            let cookbook = design(
                kind,
                FilterDesign::NyquistOctaves,
                SR,
                500.0,
                6.0,
                Bandwidth::Q(0.707),
            );
            assert!(close(coeffs.b0, cookbook.b0, 1e-12), "{kind:?}");
            assert!(coeffs.a2.is_finite());
        }
    }

    /// `butterworth_q` reproduces the textbook ladders, and a cascade of them
    /// is -3 dB at the corner with a monotone 6·N dB/octave skirt.
    #[test]
    fn butterworth_sections_are_the_textbook_ladder() {
        let fourth = butterworth_sections(4);
        assert_eq!(fourth.len(), 2);
        assert!(close(fourth[0], 0.541_196_1, 1e-6));
        assert!(close(fourth[1], 1.306_563, 1e-6));

        let eighth = butterworth_sections(8);
        assert_eq!(eighth.len(), 4);
        assert!(close(eighth[0], 0.509_795_6, 1e-6));
        assert!(close(eighth[3], 2.562_915, 1e-6));

        // Odd orders round up; an out-of-range stage saturates.
        assert_eq!(butterworth_sections(3).len(), 1);
        assert!(butterworth_q(2, 99).is_finite());
    }

    #[test]
    fn a_butterworth_cascade_is_minus_three_db_at_the_corner() {
        let sr = 48_000.0_f32;
        let fc = 1_000.0_f32;
        for order in [2_usize, 4, 8] {
            let coeffs: Vec<_> = butterworth_sections(order)
                .into_iter()
                .map(|q| FilterType::Lowpass.compute_coeffs::<f64>(sr, fc, 0.0, q as f32))
                .collect();
            let total: f64 = coeffs
                .iter()
                .map(|c| magnitude_squared_at(c, std::f64::consts::TAU * fc as f64 / sr as f64))
                .product();
            let db = 10.0 * total.log10();
            assert!(
                close(db, -3.0103, 0.01),
                "order {order} measures {db:.3} dB at fc"
            );
        }
    }

    #[test]
    fn invalid_parameters_never_produce_non_finite_coefficients() {
        for width in [
            Bandwidth::Q(f32::NAN),
            Bandwidth::Octaves(0.0),
            Bandwidth::Octaves(1e6),
            Bandwidth::Hz(-5.0),
            Bandwidth::Slope(f32::INFINITY),
        ] {
            for kind in [FilterType::Peaking, FilterType::Lowshelf] {
                for design_kind in [
                    FilterDesign::Bilinear,
                    FilterDesign::NyquistOctaves,
                    FilterDesign::Vicanek,
                ] {
                    let c = design(kind, design_kind, 0.0, f32::NAN, f32::NAN, width);
                    assert!(
                        c.b0.is_finite()
                            && c.b1.is_finite()
                            && c.b2.is_finite()
                            && c.a1.is_finite()
                            && c.a2.is_finite(),
                        "{kind:?} {design_kind:?} {width:?} produced {c:?}"
                    );
                }
            }
        }
    }
}

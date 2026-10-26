//! Band-width ↔ Q conversions for equaliser bands.
//!
//! # Why this module exists
//!
//! Every EQ file format in the wild spells a band's width differently:
//!
//! * AutoEQ / REW / Equalizer APO parametric text — `Q 0.70`
//! * Equalizer APO — `BW Oct 0.5` (octaves) and `BW Hz 100` (hertz)
//! * Equalizer APO shelf bands — `S 0.50` (RBJ shelf slope)
//!
//! The engine's biquads are parameterised by `Q` (`alpha = sin(ω₀)/(2Q)`), so
//! every one of those spellings has to become a `Q` somewhere. That somewhere
//! is here: the importer ([`parse_curve`](crate::parse_curve)), the exporter,
//! and the engine's DSP design layer (`engine::dsp::filter_design`) all call
//! these functions, so the three cannot disagree about what a width means.
//!
//! # The formulas, and where they come from
//!
//! * **Octaves** — RBJ's Audio EQ Cookbook, case 2, evaluated at the
//!   low-frequency limit (`ω₀ / sin ω₀ → 1`, i.e. `f₀ ≪ f_s`):
//!   `Q = 1 / (2·sinh(ln 2 · bw / 2))`.
//!   Both Equalizer APO (`filters/BiQuad.cpp`) and bmc0/dsp (`biquad.c`)
//!   spell the *frequency-dependent* form
//!   `alpha = sin ω₀ · sinh(ln2/2 · bw · ω₀ / sin ω₀)`, which reduces to the
//!   same `Q` as `f₀ → 0` and keeps the width intact up to Nyquist. That form
//!   needs `ω₀`, so it lives in the design layer (`engine::dsp::filter_design`
//!   in the engine crate), not here.
//! * **Hertz** — RBJ's case 1 inverted: a band `bw` Hz wide at `f₀` has
//!   `Q = f₀ / bw`.
//! * **Shelf slope `S`** — RBJ's case 3:
//!   `alpha = sin ω₀ / 2 · sqrt((A + 1/A)(1/S − 1) + 2)` with
//!   `A = 10^(gain/40)`. Equating that to `sin ω₀ / (2Q)` gives
//!   `Q = 1 / sqrt((A + 1/A)(1/S − 1) + 2)`.
//!   `S = 1` is the steepest slope that does not overshoot, and it is exactly
//!   the Butterworth `Q = 1/√2` — the sanity check this module's tests pin.
//!
//! # Honesty note
//!
//! The conversions are exact inverses of each other for a *fixed* pair of
//! `(f₀, gain)`, but an octave width and a Q are only equivalent at
//! low frequency: the engine's `Q`-parameterised biquad compresses the
//! bandwidth near Nyquist by construction. A band imported from `BW Oct`
//! therefore keeps its `Q` and reproduces the source file's intent at low
//! frequencies; a caller that wants the width held all the way to Nyquist uses
//! the design layer's octave mode, which is a separate, explicit choice.

/// Smallest octave bandwidth accepted, in octaves (≈ 2.5 % of an octave).
pub const MIN_OCTAVES: f32 = 1.0 / 24.0;
/// Largest octave bandwidth accepted, in octaves (8 octaves ≈ 20 Hz … 5 kHz).
pub const MAX_OCTAVES: f32 = 8.0;
/// Smallest shelf slope accepted. RBJ defines `S ∈ (0, 1]`.
pub const MIN_SLOPE: f32 = 0.05;
/// Largest shelf slope accepted (1.0 = steepest without ripple).
pub const MAX_SLOPE: f32 = 1.0;

/// Q factor equivalent to an octave bandwidth.
///
/// `Q = 1 / (2·sinh(ln 2 · bw / 2))`. This is the conversion the engine's own
/// band generators (`standard_31_band`, `standard_64_band`,
/// `GraphicEqLayout::q`) already use for their 1/3- and 1/6-octave ladders,
/// so an imported `BW Oct 0.333` lands on the same `Q` as a generated band.
#[inline]
pub fn q_from_octaves(octaves: f64) -> f64 {
    let bw = octaves.clamp(MIN_OCTAVES as f64, MAX_OCTAVES as f64);
    1.0 / (2.0 * (std::f64::consts::LN_2 * 0.5 * bw).sinh())
}

/// Octave bandwidth equivalent to a Q factor — the inverse of
/// [`q_from_octaves`].
///
/// `bw = 2·asinh(1 / (2Q)) / ln 2`. `Q = 1/√2` gives the textbook
/// `1.89997…` octaves (`Q = 0.7071` ≈ 1.9 octaves), which is the constant
/// Signalsmith's `defaultBandwidth` publishes for the same reason.
#[inline]
pub fn octaves_from_q(q: f64) -> f64 {
    let q = if q.is_finite() && q > 0.0 {
        q
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    };
    2.0 * (0.5 / q).asinh() / std::f64::consts::LN_2
}

/// Q factor equivalent to a bandwidth in hertz at a centre frequency.
///
/// `Q = f₀ / bw`. Returns `None` for a non-finite or non-positive width so a
/// caller can fall back to a documented default instead of installing a
/// filter the biquad would clamp to nonsense.
#[inline]
pub fn q_from_hz(bandwidth_hz: f64, center_hz: f64) -> Option<f64> {
    if !bandwidth_hz.is_finite() || bandwidth_hz <= 0.0 {
        return None;
    }
    if !center_hz.is_finite() || center_hz <= 0.0 {
        return None;
    }
    Some(center_hz / bandwidth_hz)
}

/// Bandwidth in hertz of a band with Q factor `q` at `center_hz`.
#[inline]
pub fn hz_from_q(q: f64, center_hz: f64) -> f64 {
    let q = if q.is_finite() && q > 0.0 {
        q
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    };
    center_hz / q
}

/// Q factor equivalent to an RBJ shelf slope `S` at a given shelf gain.
///
/// `Q = 1 / sqrt((A + 1/A)(1/S − 1) + 2)` with `A = 10^(gain/40)`.
///
/// `S = 1` and any gain gives `Q = 1/√2` (Butterworth); a smaller `S` is a
/// shallower transition. The engine's `EqBandParams::q` is a Q, so this is
/// what an imported `S` becomes.
#[inline]
pub fn q_from_slope(slope: f64, gain_db: f64) -> f64 {
    let s = if slope.is_finite() {
        slope.clamp(MIN_SLOPE as f64, MAX_SLOPE as f64)
    } else {
        MAX_SLOPE as f64
    };
    let a = 10.0_f64.powf(clamp_gain(gain_db) / 40.0);
    1.0 / ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt()
}

/// RBJ shelf slope equivalent to a Q factor at a given shelf gain — the
/// inverse of [`q_from_slope`].
///
/// `S = 1 / ((1/Q² − 2)/(A + 1/A) + 1)`, clamped to RBJ's `(0, 1]` because a
/// slope above 1 describes a shelf with ripple, which the cookbook formulas do
/// not produce.
#[inline]
pub fn slope_from_q(q: f64, gain_db: f64) -> f64 {
    let q = if q.is_finite() && q > 0.0 {
        q
    } else {
        std::f64::consts::FRAC_1_SQRT_2
    };
    let a = 10.0_f64.powf(clamp_gain(gain_db) / 40.0);
    let a_plus = a + 1.0 / a;
    let denominator = (1.0 / (q * q) - 2.0) / a_plus + 1.0;
    if denominator.abs() < 1e-12 {
        return MAX_SLOPE as f64;
    }
    (1.0 / denominator).clamp(MIN_SLOPE as f64, MAX_SLOPE as f64)
}

/// The gain clamp the biquad applies (`BiquadCoeffs::validate_gain_db`), so a
/// slope conversion never disagrees with the filter it describes.
#[inline]
fn clamp_gain(gain_db: f64) -> f64 {
    if gain_db.is_finite() {
        gain_db.clamp(-48.0, 48.0)
    } else {
        0.0
    }
}

/// How a band's width is specified.
///
/// This is the single vocabulary shared by the text importer's dialects
/// ([`EqTextError`](crate::EqTextError)), the configuration structs, and the
/// engine's DSP design layer. `Q` is the engine's native parameterisation; the
/// other three are converted through this module's functions before they reach
/// a biquad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bandwidth {
    /// Quality factor as the biquads consume it.
    Q(f32),
    /// Band width in octaves (`BW Oct`).
    Octaves(f32),
    /// Band width in hertz at the band's centre frequency (`BW Hz`).
    Hz(f32),
    /// RBJ shelf slope `S`, meaningful for low/high shelf bands.
    Slope(f32),
}

impl Bandwidth {
    /// Convert this width into the `Q` the engine's biquads use.
    ///
    /// `center_hz` and `gain_db` are required by the `Hz` and `Slope` forms
    /// respectively; the other two ignore them, so passing the band's own
    /// values is always correct. The result is clamped to the range the
    /// biquad accepts (`0.01 … 100`) so an imported width can never install a
    /// filter the coefficient validator would silently rewrite.
    pub fn to_q(self, center_hz: f32, gain_db: f32) -> f32 {
        let q = match self {
            Self::Q(q) => q as f64,
            Self::Octaves(octaves) => q_from_octaves(octaves as f64),
            Self::Hz(hz) => {
                q_from_hz(hz as f64, center_hz as f64).unwrap_or(std::f64::consts::FRAC_1_SQRT_2)
            }
            Self::Slope(slope) => q_from_slope(slope as f64, gain_db as f64),
        };
        if !q.is_finite() {
            return crate::eq_text::MIN_Q;
        }
        (q.clamp(crate::eq_text::MIN_Q as f64, crate::eq_text::MAX_Q as f64)) as f32
    }

    /// The width this `Q` would be, expressed in octaves.
    ///
    /// Used by the design layer's Nyquist-exact octave mode, which needs the
    /// bandwidth rather than the Q: a `Q`-parameterised biquad compresses its
    /// bandwidth as `f₀ → f_s/2`, so the octave value is the Q's *low
    /// frequency* equivalent, which is what a band generator means by it.
    pub fn octaves_for_q(q: f32) -> f32 {
        octaves_from_q(q as f64) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn one_octave_is_the_textbook_q() {
        // 1 octave ↔ Q ≈ 1.4142 — the value `default_10_band` uses for its
        // octave-spaced peaking bands.
        assert!(close(
            q_from_octaves(1.0),
            std::f64::consts::FRAC_1_SQRT_2 * 2.0,
            1e-9
        ));
        assert!(close(
            octaves_from_q(std::f64::consts::FRAC_1_SQRT_2),
            1.899_968_626_952_991_6,
            1e-9
        ));
        assert!(close(octaves_from_q(q_from_octaves(1.9)), 1.9, 1e-9));
    }

    #[test]
    fn third_and_sixth_octave_match_the_engine_band_generators() {
        // The engine's 31-band ladder uses Q = 4.31 and its 64-band ladder
        // derives Q from 1/6 octave (≈ 8.65). An imported `BW Oct` has to land
        // on the same numbers or a graphic curve would not survive a round trip.
        assert!(close(q_from_octaves(1.0 / 3.0), 4.318_03, 1e-3));
        assert!(close(q_from_octaves(1.0 / 6.0), 8.651_36, 1e-3));
        assert!(close(octaves_from_q(4.31), 1.0 / 3.0, 5e-3));
    }

    #[test]
    fn hertz_width_is_the_centre_over_the_width() {
        // 100 Hz wide at 1 kHz is Q = 10.
        assert_eq!(q_from_hz(100.0, 1000.0), Some(10.0));
        assert!(close(hz_from_q(10.0, 1000.0), 100.0, 1e-9));
        // Degenerate inputs are refused rather than clamped into a filter.
        assert_eq!(q_from_hz(0.0, 1000.0), None);
        assert_eq!(q_from_hz(f64::NAN, 1000.0), None);
        assert_eq!(q_from_hz(100.0, 0.0), None);
    }

    #[test]
    fn shelf_slope_one_is_butterworth_for_every_gain() {
        for gain in [-24.0, -6.0, 0.5, 6.0, 24.0] {
            assert!(close(
                q_from_slope(1.0, gain),
                std::f64::consts::FRAC_1_SQRT_2,
                1e-12
            ));
        }
        // A shallower slope is a lower Q, a steeper one (still ≤ 1) a higher Q.
        assert!(q_from_slope(0.5, 6.0) < q_from_slope(1.0, 6.0));
        assert!(q_from_slope(0.7071, 6.0) > q_from_slope(0.5, 6.0));
    }

    /// Every Q up to `1/√2` is a ripple-free shelf slope, and the two forms
    /// convert back and forth without loss. Above `1/√2` the shelf has ripple,
    /// which RBJ cannot express — [`slope_from_q`] clamps there, and this test
    /// pins the boundary rather than pretending the round trip is total.
    #[test]
    fn slope_and_q_round_trip_up_to_the_butterworth_q() {
        for gain in [-12.0, 0.0, 9.0] {
            for q in [0.2_f64, 0.4, 0.5, 0.6, std::f64::consts::FRAC_1_SQRT_2] {
                let s = slope_from_q(q, gain);
                assert!(
                    close(q_from_slope(s, gain), q, 1e-9),
                    "gain {gain} q {q} -> S {s} -> q {}",
                    q_from_slope(s, gain)
                );
            }
        }
        // Q = 1/√2 is exactly the steepest ripple-free shelf, S = 1.
        assert!(close(
            slope_from_q(std::f64::consts::FRAC_1_SQRT_2, 12.0),
            1.0,
            1e-9
        ));
    }

    #[test]
    fn slope_is_clamped_into_the_ripple_free_range() {
        // A very wide band (tiny Q) is a slope near zero; a Q above 1/√2 is a
        // rippled shelf, which clamps to the steepest ripple-free slope.
        assert_eq!(slope_from_q(0.01, 6.0), MIN_SLOPE as f64);
        assert_eq!(slope_from_q(10.0, 6.0), MAX_SLOPE as f64);
        // Out-of-range slopes are clamped before conversion, never NaN.
        assert!(q_from_slope(100.0, 6.0).is_finite());
        assert!(q_from_slope(f64::NAN, 6.0).is_finite());
    }

    #[test]
    fn bandwidth_to_q_covers_every_spelling() {
        // Q passes through untouched.
        assert!(close(Bandwidth::Q(2.5).to_q(1000.0, 0.0) as f64, 2.5, 1e-6));
        // Octaves and Hz are converted.
        assert!(close(
            Bandwidth::Octaves(1.0).to_q(1000.0, 0.0) as f64,
            std::f64::consts::SQRT_2,
            1e-5
        ));
        assert!(close(
            Bandwidth::Hz(100.0).to_q(1000.0, 0.0) as f64,
            10.0,
            1e-6
        ));
        // Slope depends on the band's gain, not on its frequency.
        assert!(close(
            Bandwidth::Slope(1.0).to_q(1000.0, 12.0) as f64,
            std::f64::consts::FRAC_1_SQRT_2,
            1e-6
        ));
        // Everything stays inside the biquad's accepted range.
        for width in [
            Bandwidth::Octaves(0.0),
            Bandwidth::Octaves(1e6),
            Bandwidth::Hz(0.0),
            Bandwidth::Q(f32::NAN),
            Bandwidth::Q(-3.0),
        ] {
            let q = width.to_q(1000.0, 0.0);
            assert!(
                (crate::eq_text::MIN_Q..=crate::eq_text::MAX_Q).contains(&q),
                "{width:?} produced Q {q}"
            );
        }
    }
}

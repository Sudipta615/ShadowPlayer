//! State Variable Filter (SVF) — Andy Simper / Cytomic trapezoidal integrated model.
//!
//! # Overview
//!
//! The State Variable Filter resolves continuous analog 2-pole state equations
//! into the digital domain using the bilinear transform without instantaneous
//! feedback dilemmas (trapezoidal integration).
//!
//! ## Advantages over Direct Form II Transposed (DFII-T) biquads
//!
//! 1. **Modulation Stability**: Filter cutoff frequency and Q can be swept
//!    at audio rates or stepped instantaneously without state explosions or
//!    transient clicks.
//! 2. **Multi-mode Coherency**: Low-pass, high-pass, band-pass, notch, and
//!    peaking outputs are derived simultaneously from the same shared state
//!    variables.
//! 3. **Linear Gain Tap**: Changing the peak/bell gain only scales an output
//!    feedforward tap rather than recomputing the feedback denominator $a_1, a_2$.
//!
//! # What the gain-shaped outputs are (and are not)
//!
//! The bell output is Simper's `m0 = 1, m1 = k·(G − 1), m2 = 0` tap set, which
//! is the same magnitude response as the RBJ peaking biquad to within the
//! difference between trapezoidal prewarping and the cookbook's
//! `alpha = sin(ω₀)/(2Q)` (measured: < 0.07 dB at Q = 1, < 0.7 dB for a
//! Q = 0.9 band at 10 kHz).
//!
//! The shelf outputs are the **LP/HP-scaled** shelf family:
//! `y = x + (G − 1)·lowpass` (low shelf) and `y = x + (G − 1)·highpass`
//! (high shelf). They reach exactly `G` at the flat end and exactly 1 at the
//! other, so the asymptotes are the ones a shelf is defined by, but the
//! transition is *not* byte-for-byte the RBJ shelf: RBJ's shelving filters put
//! the gain into their denominator (`β = 2·√A·α`), which a filter with a fixed
//! `k = 1/Q` cannot reproduce. A band that must match an imported RBJ shelf
//! coefficient-for-coefficient uses [`crate::dsp::biquad::FilterType`]
//! (which is what [`super::ParametricEq`] runs); this filter is the choice when
//! the cutoff has to move without state explosions, where the scaled shelves'
//! fixed denominator is the whole point.
//!
//! # Precision
//!
//! Generic over [`AudioFloat`] (`f32` for performance, `f64` for quality/mastering).

use crate::dsp::float::AudioFloat;

/// State Variable Filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvfMode {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peaking,
    AllPass,
    LowShelf,
    HighShelf,
}

/// 2-pole Trapezoidally Integrated State Variable Filter.
#[derive(Debug, Clone, Copy)]
pub struct SvfFilter<T: AudioFloat = f64> {
    // State variables (integrator capacitor charges)
    s1: T,
    s2: T,
    // Intermediate tuning coefficients
    g: T,
    k: T,
    a1: T,
    a2: T,
    a3: T,
    // Bell / shelf linear gain
    gain_linear: T,
    // Sample rate
    sample_rate: T,
}

impl<T: AudioFloat> Default for SvfFilter<T> {
    fn default() -> Self {
        Self::new(T::from_f64(44100.0))
    }
}

impl<T: AudioFloat> SvfFilter<T> {
    /// Create a new SVF initialized for the given sample rate.
    pub fn new(sample_rate: T) -> Self {
        let sr = if sample_rate <= T::zero() || !sample_rate.is_finite() {
            T::from_f64(44100.0)
        } else {
            sample_rate
        };
        let mut filter = Self {
            s1: T::zero(),
            s2: T::zero(),
            g: T::zero(),
            k: T::from_f64(1.0),
            a1: T::zero(),
            a2: T::zero(),
            a3: T::zero(),
            gain_linear: T::one(),
            sample_rate: sr,
        };
        filter.set_params(
            T::from_f64(1000.0),
            T::from_f64(1.0 / std::f64::consts::SQRT_2),
        );
        filter
    }

    /// Update cutoff frequency and Q factor.
    ///
    /// Frequency is clamped safely between 10 Hz and 0.499 * sample_rate.
    /// Q is clamped between 0.01 and 100.0.
    #[inline]
    pub fn set_params(&mut self, cutoff_hz: T, q: T) {
        let fc_val = if cutoff_hz.is_finite() {
            cutoff_hz
        } else {
            T::from_f64(1000.0)
        };
        let q_val = if q.is_finite() {
            q
        } else {
            T::from_f64(std::f64::consts::FRAC_1_SQRT_2)
        };
        let pi = T::from_f64(std::f64::consts::PI);
        let min_fc = T::from_f64(10.0);
        let max_fc = (self.sample_rate * T::from_f64(0.499)).max(min_fc);
        let fc = fc_val.clamp(min_fc, max_fc);

        let q_clamped = q_val.clamp(T::from_f64(0.01), T::from_f64(100.0));

        // Prewarped frequency parameter g = tan(pi * fc / fs)
        let g = (pi * fc / self.sample_rate).tan();
        self.g = if g.is_finite() {
            g.max(T::zero())
        } else {
            T::from_f64(1.0)
        };
        self.k = T::one() / q_clamped;

        let denom = T::one() + self.g * (self.g + self.k);
        let a1 = if denom.abs() > T::from_f64(1e-12) {
            T::one() / denom
        } else {
            T::one()
        };
        self.a1 = a1;
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    /// Set gain in decibels for peaking and shelving responses.
    #[inline]
    pub fn set_gain_db(&mut self, gain_db: T) {
        let db = if gain_db.is_finite() {
            gain_db.clamp(T::from_f64(-48.0), T::from_f64(48.0))
        } else {
            T::zero()
        };
        // linear = 10^(gain_db / 20)
        self.gain_linear = (T::from_f64(10.0)).powf(db / T::from_f64(20.0));
    }

    /// Set linear gain directly.
    #[inline]
    pub fn set_gain_linear(&mut self, gain_linear: T) {
        let gl = if gain_linear.is_finite() {
            gain_linear.clamp(T::from_f64(0.00398), T::from_f64(251.18))
        } else {
            T::one()
        };
        self.gain_linear = gl;
    }

    /// Reset internal state memories to silence.
    #[inline]
    pub fn reset(&mut self) {
        self.s1 = T::zero();
        self.s2 = T::zero();
    }

    /// Process a single sample and return all simultaneous filter outputs:
    /// `(low_pass, band_pass, high_pass, notch)`
    #[inline(always)]
    pub fn process_multimode(&mut self, x: T) -> (T, T, T, T) {
        if !x.is_finite() {
            self.s1 = T::zero();
            self.s2 = T::zero();
            return (T::zero(), T::zero(), T::zero(), T::zero());
        }

        let two = T::from_f64(2.0);
        let v0 = x;
        let v3 = v0 - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;

        let new_s1 = two * v1 - self.s1;
        let new_s2 = two * v2 - self.s2;
        self.s1 = T::from_f64(crate::buffer::flush_denormal_f64(new_s1.to_f64()));
        self.s2 = T::from_f64(crate::buffer::flush_denormal_f64(new_s2.to_f64()));

        let lp = v2;
        let bp = v1;
        let hp = v0 - self.k * v1 - v2;
        let notch = v0 - self.k * v1;

        if !lp.is_finite() || !bp.is_finite() || !hp.is_finite() || !notch.is_finite() {
            self.s1 = T::zero();
            self.s2 = T::zero();
            return (T::zero(), T::zero(), T::zero(), T::zero());
        }

        (lp, bp, hp, notch)
    }

    /// Process a sample in peaking/bell mode with the configured gain.
    #[inline(always)]
    pub fn process_peaking(&mut self, x: T) -> T {
        let (_, bp, _, _) = self.process_multimode(x);
        let out = x + (self.gain_linear - T::one()) * self.k * bp;
        if out.is_finite() {
            out
        } else {
            T::zero()
        }
    }

    /// Process a sample in low-shelf mode.
    #[inline(always)]
    pub fn process_lowshelf(&mut self, x: T) -> T {
        let (lp, _, _, _) = self.process_multimode(x);
        let out = x + (self.gain_linear - T::one()) * lp;
        if out.is_finite() {
            out
        } else {
            T::zero()
        }
    }

    /// Process a sample in high-shelf mode.
    #[inline(always)]
    pub fn process_highshelf(&mut self, x: T) -> T {
        let (_, _, hp, _) = self.process_multimode(x);
        let out = x + (self.gain_linear - T::one()) * hp;
        if out.is_finite() {
            out
        } else {
            T::zero()
        }
    }

    /// Process a sample according to the specified [`SvfMode`].
    #[inline(always)]
    pub fn process(&mut self, x: T, mode: SvfMode) -> T {
        match mode {
            SvfMode::LowPass => {
                let (lp, _, _, _) = self.process_multimode(x);
                lp
            }
            SvfMode::HighPass => {
                let (_, _, hp, _) = self.process_multimode(x);
                hp
            }
            SvfMode::BandPass => {
                let (_, bp, _, _) = self.process_multimode(x);
                bp
            }
            SvfMode::Notch => {
                let (_, _, _, notch) = self.process_multimode(x);
                notch
            }
            SvfMode::Peaking => self.process_peaking(x),
            SvfMode::LowShelf => self.process_lowshelf(x),
            SvfMode::HighShelf => self.process_highshelf(x),
            SvfMode::AllPass => {
                let (lp, bp, hp, _) = self.process_multimode(x);
                let out = hp - self.k * bp + lp;
                if out.is_finite() {
                    out
                } else {
                    T::zero()
                }
            }
        }
    }

    /// Process a block of samples in place for the given mode.
    pub fn process_block(&mut self, buffer: &mut [T], mode: SvfMode) {
        match mode {
            SvfMode::LowPass => {
                for s in buffer.iter_mut() {
                    let (lp, _, _, _) = self.process_multimode(*s);
                    *s = lp;
                }
            }
            SvfMode::HighPass => {
                for s in buffer.iter_mut() {
                    let (_, _, hp, _) = self.process_multimode(*s);
                    *s = hp;
                }
            }
            SvfMode::BandPass => {
                for s in buffer.iter_mut() {
                    let (_, bp, _, _) = self.process_multimode(*s);
                    *s = bp;
                }
            }
            SvfMode::Notch => {
                for s in buffer.iter_mut() {
                    let (_, _, _, notch) = self.process_multimode(*s);
                    *s = notch;
                }
            }
            SvfMode::Peaking => {
                for s in buffer.iter_mut() {
                    *s = self.process_peaking(*s);
                }
            }
            SvfMode::LowShelf => {
                for s in buffer.iter_mut() {
                    *s = self.process_lowshelf(*s);
                }
            }
            SvfMode::HighShelf => {
                for s in buffer.iter_mut() {
                    *s = self.process_highshelf(*s);
                }
            }
            SvfMode::AllPass => {
                for s in buffer.iter_mut() {
                    *s = self.process(*s, SvfMode::AllPass);
                }
            }
        }
    }

    /// Process a stereo frame through a stereo pair of SVF filters in place.
    #[inline(always)]
    pub fn process_stereo_frame(
        left_filter: &mut Self,
        right_filter: &mut Self,
        xl: T,
        xr: T,
        mode: SvfMode,
    ) -> (T, T) {
        (
            left_filter.process(xl, mode),
            right_filter.process(xr, mode),
        )
    }

    /// Process a stereo block through a stereo pair of SVF filters in place.
    ///
    /// Hoists the filter mode dispatch out of the per-sample loop to maximize instruction
    /// cache efficiency and auto-vectorization.
    pub fn process_stereo_block(
        left_filter: &mut Self,
        right_filter: &mut Self,
        left: &mut [T],
        right: &mut [T],
        mode: SvfMode,
    ) {
        let n = left.len().min(right.len());
        match mode {
            SvfMode::LowPass => {
                for i in 0..n {
                    let (lp, _, _, _) = left_filter.process_multimode(left[i]);
                    left[i] = lp;
                    let (rp, _, _, _) = right_filter.process_multimode(right[i]);
                    right[i] = rp;
                }
            }
            SvfMode::HighPass => {
                for i in 0..n {
                    let (_, _, hp, _) = left_filter.process_multimode(left[i]);
                    left[i] = hp;
                    let (_, _, rp, _) = right_filter.process_multimode(right[i]);
                    right[i] = rp;
                }
            }
            SvfMode::BandPass => {
                for i in 0..n {
                    let (_, bp, _, _) = left_filter.process_multimode(left[i]);
                    left[i] = bp;
                    let (_, rp, _, _) = right_filter.process_multimode(right[i]);
                    right[i] = rp;
                }
            }
            SvfMode::Notch => {
                for i in 0..n {
                    let (_, _, _, notch) = left_filter.process_multimode(left[i]);
                    left[i] = notch;
                    let (_, _, _, rp) = right_filter.process_multimode(right[i]);
                    right[i] = rp;
                }
            }
            SvfMode::Peaking => {
                for i in 0..n {
                    left[i] = left_filter.process_peaking(left[i]);
                    right[i] = right_filter.process_peaking(right[i]);
                }
            }
            SvfMode::LowShelf => {
                for i in 0..n {
                    left[i] = left_filter.process_lowshelf(left[i]);
                    right[i] = right_filter.process_lowshelf(right[i]);
                }
            }
            SvfMode::HighShelf => {
                for i in 0..n {
                    left[i] = left_filter.process_highshelf(left[i]);
                    right[i] = right_filter.process_highshelf(right[i]);
                }
            }
            SvfMode::AllPass => {
                for i in 0..n {
                    left[i] = left_filter.process(left[i], SvfMode::AllPass);
                    right[i] = right_filter.process(right[i], SvfMode::AllPass);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svf_dc_gain_lowpass() {
        let mut svf = SvfFilter::<f64>::new(44100.0);
        svf.set_params(1000.0, std::f64::consts::FRAC_1_SQRT_2);

        // Feed DC (constant 1.0)
        let mut out = 0.0;
        for _ in 0..1000 {
            out = svf.process(1.0, SvfMode::LowPass);
        }
        assert!(
            (out - 1.0).abs() < 1e-3,
            "lowpass DC gain must be 1.0, got {out}"
        );
    }

    #[test]
    fn test_svf_dc_gain_highpass() {
        let mut svf = SvfFilter::<f64>::new(44100.0);
        svf.set_params(1000.0, std::f64::consts::FRAC_1_SQRT_2);

        // Feed DC (constant 1.0) - highpass should block it
        let mut out = 0.0;
        for _ in 0..1000 {
            out = svf.process(1.0, SvfMode::HighPass);
        }
        assert!(out.abs() < 1e-4, "highpass DC gain must be 0.0, got {out}");
    }

    #[test]
    fn test_svf_peaking_unity_at_zero_db() {
        let mut svf = SvfFilter::<f64>::new(44100.0);
        svf.set_params(1000.0, 1.414);
        svf.set_gain_db(0.0);

        for i in 0..100 {
            let s = (i as f64 * 0.1).sin();
            let out = svf.process_peaking(s);
            assert!(
                (out - s).abs() < 1e-6,
                "0 dB peak must be an exact passthrough"
            );
        }
    }

    /// The shelf asymptotes are what define the scaled shelf family: the flat
    /// end reaches the full gain and the other end is exact unity. (The
    /// transition differs from an RBJ shelf by design — see the module docs.)
    #[test]
    fn test_svf_shelf_asymptotes_are_exact() {
        for gain_db in [-12.0_f64, -3.0, 6.0, 18.0] {
            let gain = 10f64.powf(gain_db / 20.0);
            let mut svf = SvfFilter::<f64>::new(48_000.0);
            svf.set_params(200.0, std::f64::consts::FRAC_1_SQRT_2);
            svf.set_gain_db(gain_db);
            let mut low_dc = 0.0;
            for _ in 0..20_000 {
                low_dc = svf.process(1.0, SvfMode::LowShelf);
            }
            assert!(
                (low_dc - gain).abs() < 1e-3,
                "low shelf DC must be the full {gain_db} dB, got {low_dc}"
            );

            // A high shelf is the mirror image: unity at DC, the full gain at
            // Nyquist (an alternating ±1 input sits exactly there).
            let mut high_dc = 0.0;
            svf.reset();
            for _ in 0..20_000 {
                high_dc = svf.process(1.0, SvfMode::HighShelf);
            }
            assert!(
                (high_dc - 1.0).abs() < 1e-3,
                "high shelf must pass DC at unity, got {high_dc}"
            );

            let mut high_nyquist = 0.0;
            for i in 0..20_000 {
                let x = if i % 2 == 0 { 1.0 } else { -1.0 };
                high_nyquist = svf.process(x, SvfMode::HighShelf);
            }
            assert!(
                (high_nyquist.abs() - gain).abs() < 1e-2,
                "high shelf must reach the full {gain_db} dB at Nyquist, got {high_nyquist}"
            );
        }
    }

    /// The bell output matches the RBJ peaking biquad at its centre and at DC,
    /// which is the equivalence the module documents.
    #[test]
    fn test_svf_bell_matches_the_rbj_biquad_at_the_centre() {
        use crate::dsp::biquad::{BiquadCoeffs, BiquadState};
        let sr = 48_000.0_f32;
        let mut svf = SvfFilter::<f64>::new(sr as f64);
        svf.set_params(1_000.0, 1.0);
        svf.set_gain_db(6.0);
        let coeffs = BiquadCoeffs::<f64>::peaking(sr, 1_000.0, 6.0, 1.0);
        let mut state = BiquadState::<f64>::default();

        // Measure both at the centre frequency over a settled window.
        let w = std::f64::consts::TAU * 1_000.0 / sr as f64;
        let mut svf_peak: f64 = 0.0;
        let mut rbj_peak: f64 = 0.0;
        for i in 0..8_000 {
            let x = (w * i as f64).sin();
            let a = svf.process(x, SvfMode::Peaking);
            let b = state.process(x, &coeffs);
            if i > 4_000 {
                svf_peak = svf_peak.max(a.abs());
                rbj_peak = rbj_peak.max(b.abs());
            }
        }
        let svf_db = 20.0 * svf_peak.log10();
        let rbj_db = 20.0 * rbj_peak.log10();
        assert!(
            (svf_db - rbj_db).abs() < 0.1,
            "bell centre gain: SVF {svf_db:.3} dB vs RBJ {rbj_db:.3} dB"
        );
        assert!((rbj_db - 6.0).abs() < 0.2);
    }

    #[test]
    fn test_svf_stability_under_drastic_modulation() {
        let mut svf = SvfFilter::<f64>::new(48000.0);

        // Rapidly sweep frequency across 3 decades on every single sample
        let mut max_val = 0.0f64;
        for i in 0..10000 {
            let freq = 50.0 + 15000.0 * ((i as f64 * 0.05).sin().abs());
            let q = 0.5 + 10.0 * ((i as f64 * 0.02).cos().abs());
            svf.set_params(freq, q);
            svf.set_gain_db(12.0 * (i as f64 * 0.03).sin());

            let input = if i % 100 == 0 { 1.0 } else { 0.0 };
            let out = svf.process(input, SvfMode::Peaking);
            assert!(
                out.is_finite(),
                "SVF produced non-finite output under modulation"
            );
            max_val = max_val.max(out.abs());
        }
        assert!(
            max_val < 50.0,
            "SVF state did not explode under extreme modulation"
        );
    }

    #[test]
    fn test_svf_non_finite_inputs_do_not_latch() {
        let mut svf = SvfFilter::<f64>::new(48000.0);
        svf.set_params(1000.0, 1.0);

        // Warm up with valid audio
        for _ in 0..100 {
            let out = svf.process(0.5, SvfMode::LowPass);
            assert!(out.is_finite());
        }

        // Feed non-finite values (NaN, +Inf, -Inf)
        let out_nan = svf.process(f64::NAN, SvfMode::LowPass);
        assert_eq!(out_nan, 0.0, "NaN input must return 0.0");

        let out_inf = svf.process(f64::INFINITY, SvfMode::LowPass);
        assert_eq!(out_inf, 0.0, "+Inf input must return 0.0");

        let out_neginf = svf.process(f64::NEG_INFINITY, SvfMode::LowPass);
        assert_eq!(out_neginf, 0.0, "-Inf input must return 0.0");

        // Immediately following valid audio must produce finite numbers without state latching
        for _ in 0..100 {
            let out = svf.process(0.5, SvfMode::LowPass);
            assert!(
                out.is_finite(),
                "SVF state remained poisoned after non-finite input"
            );
        }
    }

    #[test]
    fn test_svf_non_finite_params_are_sanitized() {
        let mut svf = SvfFilter::<f64>::new(48000.0);

        // NaN cutoff and Q
        svf.set_params(f64::NAN, f64::NAN);
        svf.set_gain_db(f64::NAN);

        let out = svf.process(0.5, SvfMode::Peaking);
        assert!(out.is_finite(), "Output must be finite despite NaN params");

        // Infinite cutoff and Q
        svf.set_params(f64::INFINITY, f64::INFINITY);
        svf.set_gain_db(f64::INFINITY);

        let out2 = svf.process(0.5, SvfMode::Peaking);
        assert!(out2.is_finite(), "Output must be finite despite Inf params");
    }

    #[test]
    fn test_svf_stereo_block_equivalence() {
        let mut left_filter = SvfFilter::<f64>::new(48000.0);
        let mut right_filter = SvfFilter::<f64>::new(48000.0);
        left_filter.set_params(1200.0, 1.4);
        right_filter.set_params(1200.0, 1.4);
        left_filter.set_gain_db(4.0);
        right_filter.set_gain_db(4.0);

        let mut ref_l = left_filter;
        let mut ref_r = right_filter;

        let mut l_block: Vec<f64> = (0..256).map(|i| (i as f64 * 0.1).sin()).collect();
        let mut r_block: Vec<f64> = (0..256).map(|i| (i as f64 * 0.15).cos()).collect();

        let l_expected: Vec<f64> = l_block
            .iter()
            .map(|&x| ref_l.process(x, SvfMode::Peaking))
            .collect();
        let r_expected: Vec<f64> = r_block
            .iter()
            .map(|&x| ref_r.process(x, SvfMode::Peaking))
            .collect();

        SvfFilter::process_stereo_block(
            &mut left_filter,
            &mut right_filter,
            &mut l_block,
            &mut r_block,
            SvfMode::Peaking,
        );

        for i in 0..256 {
            assert!(
                (l_block[i] - l_expected[i]).abs() < 1e-12,
                "Left sample {i} mismatch"
            );
            assert!(
                (r_block[i] - r_expected[i]).abs() < 1e-12,
                "Right sample {i} mismatch"
            );
        }
    }
}

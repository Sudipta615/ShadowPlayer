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
        let pi = T::from_f64(std::f64::consts::PI);
        let min_fc = T::from_f64(10.0);
        let max_fc = self.sample_rate * T::from_f64(0.499);
        let fc = cutoff_hz.clamp(min_fc, max_fc);

        let q_clamped = q.clamp(T::from_f64(0.01), T::from_f64(100.0));

        // Prewarped frequency parameter g = tan(pi * fc / fs)
        self.g = (pi * fc / self.sample_rate).tan();
        self.k = T::one() / q_clamped;

        let denom = T::one() + self.g * (self.g + self.k);
        self.a1 = T::one() / denom;
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    /// Set gain in decibels for peaking and shelving responses.
    #[inline]
    pub fn set_gain_db(&mut self, gain_db: T) {
        let clamped_db = gain_db.clamp(T::from_f64(-48.0), T::from_f64(48.0));
        // linear = 10^(gain_db / 20)
        self.gain_linear = (T::from_f64(10.0)).powf(clamped_db / T::from_f64(20.0));
    }

    /// Set linear gain directly.
    #[inline]
    pub fn set_gain_linear(&mut self, gain_linear: T) {
        self.gain_linear = gain_linear.clamp(T::from_f64(0.00398), T::from_f64(251.18));
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
        let two = T::from_f64(2.0);
        let v0 = x;
        let v3 = v0 - self.s2;
        let v1 = self.a1 * self.s1 + self.a2 * v3;
        let v2 = self.s2 + self.a2 * self.s1 + self.a3 * v3;

        self.s1 = two * v1 - self.s1;
        self.s2 = two * v2 - self.s2;

        let lp = v2;
        let bp = v1;
        let hp = v0 - self.k * v1 - v2;
        let notch = v0 - self.k * v1;

        (lp, bp, hp, notch)
    }

    /// Process a sample in peaking/bell mode with the configured gain.
    #[inline(always)]
    pub fn process_peaking(&mut self, x: T) -> T {
        let (_, bp, _, _) = self.process_multimode(x);
        x + (self.gain_linear - T::one()) * self.k * bp
    }

    /// Process a sample in low-shelf mode.
    #[inline(always)]
    pub fn process_lowshelf(&mut self, x: T) -> T {
        let (lp, _, _, _) = self.process_multimode(x);
        x + (self.gain_linear - T::one()) * lp
    }

    /// Process a sample in high-shelf mode.
    #[inline(always)]
    pub fn process_highshelf(&mut self, x: T) -> T {
        let (_, _, hp, _) = self.process_multimode(x);
        x + (self.gain_linear - T::one()) * hp
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
                hp - self.k * bp + lp
            }
        }
    }

    /// Process a block of samples in place for the given mode.
    pub fn process_block(&mut self, buffer: &mut [T], mode: SvfMode) {
        for s in buffer.iter_mut() {
            *s = self.process(*s, mode);
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
}

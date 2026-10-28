//! Half-band cascade interpolator for the final working-rate → DSD-rate step.
//!
//! The input is already band-limited (the engine's resampler produced the
//! working rate and the PCM source never exceeds its own Nyquist), so this
//! stage only has to remove the images created by zero-stuffing. A single
//! long polyphase FIR would do it, but every output sample would pay for
//! every tap. A cascade of half-band stages does the same job with a short
//! filter at each rate: the even phase of a half-band is a pure delay, so
//! only the odd phase multiplies — about *half* the taps per output, and
//! each output is produced exactly once.
//!
//! Geometry: the prototype is `2 × 8 + 1 = 17` taps with the center at
//! index 8 (even). The polyphase split is therefore
//!
//! * even output  = `x[n - 4]` (one delay tap, unity gain),
//! * odd output   = 8 MACs against the odd taps.
//!
//! The 8 odd taps are a windowed sinc at a quarter of the *high* rate
//! (Blackman-Harris, ≈ -92 dB sidelobes), normalized so the odd phase also
//! has unity DC gain. Each stage is flat far past 20 kHz and its stopband
//! begins well before the first image, so the cascade preserves the audio
//! band and rejects the images.
//!
//! For a 44.1 kHz source at DSD256 (factor 32, five stages) the cascade
//! performs ~4.5 MAC per output sample instead of 24; the measurements in
//! `dsp_pipeline` show the effect.

/// Odd-phase tap count for the 17-tap prototype.
const HALF_TAPS: usize = 8;

/// Four-term Blackman-Harris window (≈ -92 dB sidelobes).
#[inline]
fn window(n: usize, len: usize) -> f64 {
    if len <= 1 {
        return 1.0;
    }
    let x = 2.0 * std::f64::consts::PI * n as f64 / (len - 1) as f64;
    const A0: f64 = 0.358_75;
    const A1: f64 = 0.488_29;
    const A2: f64 = 0.141_28;
    const A3: f64 = 0.011_68;
    A0 - A1 * x.cos() + A2 * (2.0 * x).cos() - A3 * (3.0 * x).cos()
}

/// One 2× half-band stage.
#[derive(Clone)]
struct HalfBandStage {
    /// Odd-phase coefficients, oldest input first.
    odd: [f64; HALF_TAPS],
    /// Input history: `history[0]` is the newest sample.
    history: [f64; HALF_TAPS],
}

impl HalfBandStage {
    fn new() -> Self {
        const LEN: usize = 2 * HALF_TAPS + 1;
        const CENTER: usize = HALF_TAPS;
        let mut odd = [0.0f64; HALF_TAPS];
        let mut sum = 0.0;
        for (i, slot) in odd.iter_mut().enumerate() {
            let index = 2 * i + 1;
            let x = index as f64 - CENTER as f64;
            // Ideal half-band prototype h[n] = sinc(x/2), normalized to a
            // unity even phase (the center tap is 1).
            let sinc = (std::f64::consts::PI * x / 2.0).sin() / (std::f64::consts::PI * x / 2.0);
            *slot = sinc * window(index, LEN);
            sum += *slot;
        }
        // Windowing perturbs the DC gain; restore it exactly.
        for slot in &mut odd {
            *slot /= sum;
        }
        Self {
            odd,
            history: [0.0; HALF_TAPS],
        }
    }

    /// Produce the two high-rate outputs for one new input sample.
    #[inline]
    fn process(&mut self, sample: f64) -> (f64, f64) {
        self.history.copy_within(0..HALF_TAPS - 1, 1);
        self.history[0] = sample;
        // Even phase: center tap at index 4.
        let even = self.history[HALF_TAPS / 2];
        // Odd phase: eight products against the newest eight inputs, summed
        // as a balanced tree so the stage's output latency is three adds.
        let h = &self.history;
        let p0 = self.odd[0] * h[0];
        let p1 = self.odd[1] * h[1];
        let p2 = self.odd[2] * h[2];
        let p3 = self.odd[3] * h[3];
        let p4 = self.odd[4] * h[4];
        let p5 = self.odd[5] * h[5];
        let p6 = self.odd[6] * h[6];
        let p7 = self.odd[7] * h[7];
        (even, ((p0 + p2) + (p4 + p6)) + ((p1 + p3) + (p5 + p7)))
    }

    fn reset(&mut self) {
        self.history = [0.0; HALF_TAPS];
    }
}

/// A 2×-per-stage interpolator cascade.
#[derive(Clone)]
pub(crate) struct PolyphaseInterpolator {
    factor: usize,
    stages: Vec<HalfBandStage>,
    buf_a: Vec<f64>,
    buf_b: Vec<f64>,
}

impl PolyphaseInterpolator {
    /// Build an interpolator for a power-of-two `factor` (8/16/32/64).
    ///
    /// `taps` is the half-band prototype length; only the 17-tap design is
    /// implemented (8 odd taps), because that is the geometry the audio
    /// path uses and it is what the tests pin.
    pub(crate) fn new(factor: usize, taps: usize) -> Option<Self> {
        if factor < 2 || !factor.is_power_of_two() || taps != 2 * HALF_TAPS + 1 {
            return None;
        }
        let stages = vec![HalfBandStage::new(); factor.trailing_zeros() as usize];
        Some(Self {
            factor,
            stages,
            buf_a: vec![0.0; factor],
            buf_b: vec![0.0; factor],
        })
    }

    /// Interpolate one input sample, writing exactly `factor` output samples
    /// to `out`.
    #[inline]
    pub(crate) fn process_sample(&mut self, sample: f64, out: &mut [f64]) {
        debug_assert_eq!(out.len(), self.factor);
        self.buf_a[0] = sample;
        let mut len = 1;
        for stage in &mut self.stages {
            for i in 0..len {
                let (even, odd) = stage.process(self.buf_a[i]);
                self.buf_b[2 * i] = even;
                self.buf_b[2 * i + 1] = odd;
            }
            std::mem::swap(&mut self.buf_a, &mut self.buf_b);
            len *= 2;
        }
        debug_assert_eq!(len, self.factor);
        out.copy_from_slice(&self.buf_a[..self.factor]);
    }

    /// Clear every stage's delay line.
    pub(crate) fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
        self.buf_a.fill(0.0);
        self.buf_b.fill(0.0);
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn factor(&self) -> usize {
        self.factor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interpolate(interp: &mut PolyphaseInterpolator, input: &[f64]) -> Vec<f64> {
        let factor = interp.factor();
        let mut out = vec![0.0; factor];
        let mut result = Vec::with_capacity(input.len() * factor);
        for &x in input {
            interp.process_sample(x, &mut out);
            result.extend_from_slice(&out);
        }
        result
    }

    #[test]
    fn all_factors_are_supported() {
        for factor in [2, 4, 8, 16, 32, 64] {
            assert!(PolyphaseInterpolator::new(factor, 17).is_some());
        }
        assert!(PolyphaseInterpolator::new(12, 17).is_none());
        assert!(PolyphaseInterpolator::new(8, 24).is_none());
    }

    #[test]
    fn the_odd_phase_has_unity_dc_gain() {
        let stage = HalfBandStage::new();
        let sum: f64 = stage.odd.iter().sum();
        assert!((sum - 1.0).abs() < 1e-12, "sum {sum}");
    }

    #[test]
    fn constant_input_stays_constant() {
        for factor in [8, 16, 32, 64] {
            let mut interp = PolyphaseInterpolator::new(factor, 17).expect("interpolator");
            let input = vec![0.25; 200];
            let out = interpolate(&mut interp, &input);
            // Skip the cascade's warm-up: each stage's 17-tap prototype must
            // fill, and the stages are sequential.
            let skip = 64 * factor;
            for &v in &out[skip..] {
                assert!((v - 0.25).abs() < 1e-9, "factor {factor}: {v}");
            }
        }
    }

    #[test]
    fn impulse_response_is_normalized() {
        for factor in [8, 16, 32] {
            let mut interp = PolyphaseInterpolator::new(factor, 17).expect("interpolator");
            let mut input = vec![0.0; 64];
            input[0] = 1.0;
            let out = interpolate(&mut interp, &input);
            let sum: f64 = out.iter().sum();
            assert!(
                (sum - factor as f64).abs() < 1e-9,
                "factor {factor}: sum {sum}"
            );
        }
    }

    #[test]
    fn passband_is_flat_and_the_image_is_rejected() {
        // Work at a 352.8 kHz working rate. 1 kHz and 20 kHz must pass with
        // the same gain; the first image of 1 kHz (351.8 kHz) must be gone.
        let working = 352_800.0;
        let factor = 8;
        let dsd_rate = working * factor as f64;
        let n = 16_384;

        let mut interp = PolyphaseInterpolator::new(factor, 17).expect("interpolator");
        let mut input = Vec::with_capacity(n);
        for i in 0..n {
            input.push((2.0 * std::f64::consts::PI * 1_000.0 * i as f64 / working).sin());
        }
        let out = interpolate(&mut interp, &input);
        let start = 256 * factor;

        let pass_rms = |signal: &[f64]| {
            (signal[start..].iter().map(|v| v * v).sum::<f64>() / (signal.len() - start) as f64)
                .sqrt()
        };
        let low = pass_rms(&out);

        // 20 kHz at the same amplitude.
        let mut interp = PolyphaseInterpolator::new(factor, 17).expect("interpolator");
        let mut input = Vec::with_capacity(n);
        for i in 0..n {
            input.push((2.0 * std::f64::consts::PI * 20_000.0 * i as f64 / working).sin());
        }
        let high = pass_rms(&interpolate(&mut interp, &input));
        let ratio_db = 20.0 * (high / low).log10();
        assert!(
            ratio_db.abs() < 0.1,
            "20 kHz passband deviation {ratio_db:.3} dB"
        );

        // Single-bin DFT at the image frequency.
        let mut image = 0.0;
        let mut image_q = 0.0;
        for (i, &v) in out.iter().enumerate().skip(start) {
            let phase = 2.0 * std::f64::consts::PI * (working - 1_000.0) * i as f64 / dsd_rate;
            image += v * phase.cos();
            image_q += v * phase.sin();
        }
        let bins = (out.len() - start) as f64;
        let image_rms = ((image * image + image_q * image_q).sqrt() / bins) * 2.0_f64.sqrt();
        assert!(image_rms < 1e-4, "image rms {image_rms} (pass {low})");
    }

    #[test]
    fn every_stage_adds_the_expected_number_of_samples() {
        for factor in [2usize, 4, 8, 16, 32, 64] {
            let interp = PolyphaseInterpolator::new(factor, 17).expect("interpolator");
            assert_eq!(interp.stages.len(), factor.trailing_zeros() as usize);
            assert_eq!(interp.buf_a.len(), factor);
        }
    }

    #[test]
    fn reset_clears_history() {
        let mut interp = PolyphaseInterpolator::new(8, 17).expect("interpolator");
        let mut out = vec![0.0; 8];
        for _ in 0..20 {
            interp.process_sample(1.0, &mut out);
        }
        interp.reset();
        interp.process_sample(0.0, &mut out);
        assert!(out.iter().all(|v| v.abs() < 1e-12));
    }
}

#![allow(dead_code)]

//! Minimal CELT FFT primitives for decoder path.
//!
//! This module intentionally implements only the inverse transform behavior
//! needed by the decoder. It follows the libopus scaling convention where the
//! inverse FFT is unscaled and the forward path applies `1/N`.

use core::f32::consts::PI;

/// Complex number used by CELT FFT/MDCT primitives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Complex32 {
    /// Real component.
    pub re: f32,
    /// Imaginary component.
    pub im: f32,
}

impl Complex32 {
    /// Create a complex value from real and imaginary parts.
    ///
    /// Params: `re` is the real component, `im` is the imaginary component.
    /// Returns: a new `Complex32`.
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }
}

/// Inverse-only FFT plan for CELT decoder usage.
#[derive(Debug, Clone)]
pub(crate) struct KissFft {
    nfft: usize,
    inv_twiddles: Vec<Complex32>,
}

impl KissFft {
    /// Build an inverse FFT plan with runtime twiddle generation.
    ///
    /// Params: `nfft` is the FFT size.
    /// Returns: a plan containing precomputed inverse twiddles.
    pub fn new(nfft: usize) -> Self {
        let mut inv_twiddles = Vec::with_capacity(nfft);
        for k in 0..nfft {
            let phase = 2.0 * PI * (k as f32) / (nfft as f32);
            inv_twiddles.push(Complex32::new(phase.cos(), phase.sin()));
        }
        Self { nfft, inv_twiddles }
    }

    /// Return FFT size configured in this plan.
    ///
    /// Params: none.
    /// Returns: number of points in the transform.
    pub fn len(&self) -> usize {
        self.nfft
    }

    /// Compute unscaled inverse FFT.
    ///
    /// Params: `input` is frequency-domain complex spectrum, `output` is
    /// destination time-domain buffer and must have length `self.len()`.
    /// Returns: `Ok(())` on success, otherwise a static validation error.
    pub fn ifft(&self, input: &[Complex32], output: &mut [Complex32]) -> Result<(), &'static str> {
        if input.len() != self.nfft || output.len() != self.nfft {
            return Err("kiss_fft length mismatch");
        }
        for out in output.iter_mut() {
            *out = Complex32::new(0.0, 0.0);
        }
        for (n, out) in output.iter_mut().enumerate() {
            let mut acc_re = 0.0f32;
            let mut acc_im = 0.0f32;
            for (k, xk) in input.iter().enumerate() {
                let tw = self.inv_twiddles[(k * n) % self.nfft];
                acc_re += xk.re * tw.re - xk.im * tw.im;
                acc_im += xk.re * tw.im + xk.im * tw.re;
            }
            *out = Complex32::new(acc_re, acc_im);
        }
        Ok(())
    }

    /// Compute unscaled forward FFT.
    ///
    /// Params: `input` is time-domain complex vector, `output` is destination
    /// frequency-domain buffer and must have length `self.len()`.
    /// Returns: `Ok(())` on success, otherwise a static validation error.
    pub fn fft(&self, input: &[Complex32], output: &mut [Complex32]) -> Result<(), &'static str> {
        if input.len() != self.nfft || output.len() != self.nfft {
            return Err("kiss_fft length mismatch");
        }
        for out in output.iter_mut() {
            *out = Complex32::new(0.0, 0.0);
        }
        for (n, out) in output.iter_mut().enumerate() {
            let mut acc_re = 0.0f32;
            let mut acc_im = 0.0f32;
            for (k, xk) in input.iter().enumerate() {
                let tw = self.inv_twiddles[(k * n) % self.nfft];
                // Conjugate inverse twiddle gives forward phase.
                acc_re += xk.re * tw.re + xk.im * tw.im;
                acc_im += -xk.re * tw.im + xk.im * tw.re;
            }
            *out = Complex32::new(acc_re, acc_im);
        }
        Ok(())
    }
}

/// Precomputed twiddles for an unscaled forward DFT of length `n`.
///
/// The CELT MDCT runs one forward transform per frame per channel, so the
/// twiddle factors must be computed once when the plan is built. Computing
/// `sin_cos()` inside the summation instead costs O(n²) transcendental calls
/// per frame — at 48 kHz that is hundreds of thousands of libm calls per
/// audio block and pins a core just to decode one Opus stream.
#[derive(Debug, Clone)]
pub(crate) struct ForwardDft {
    n: usize,
    cos: Vec<f64>,
    sin: Vec<f64>,
}

impl ForwardDft {
    /// Build the twiddle table for a transform of length `n`.
    ///
    /// Params: `n` is the (complex) transform size.
    /// Returns: a plan whose table holds `e^{-2πim/n}` for every `m`.
    pub fn new(n: usize) -> Self {
        let mut cos = Vec::with_capacity(n);
        let mut sin = Vec::with_capacity(n);
        for m in 0..n {
            let angle = -2.0 * core::f64::consts::PI * (m as f64) / (n as f64);
            let (s, c) = angle.sin_cos();
            cos.push(c);
            sin.push(s);
        }
        Self { n, cos, sin }
    }

    /// Return the configured transform size.
    pub fn len(&self) -> usize {
        self.n
    }

    /// Forward DFT of a flat interleaved complex buffer, unscaled.
    ///
    /// Params: `input`/`output` are `[re0, im0, re1, im1, …]` with at least
    /// `n` complex samples each.
    /// Returns: nothing; the transform is written to `output`.
    pub(crate) fn transform(&self, input: &[f32], output: &mut [f32]) {
        let n = self.n;
        assert!(input.len() >= 2 * n && output.len() >= 2 * n);
        if n == 0 {
            return;
        }
        for k in 0..n {
            let mut sum_re = 0.0f64;
            let mut sum_im = 0.0f64;
            // The twiddle index is `(k * j) % n`. Stepping it by `k` and
            // folding once keeps that out of the inner loop, where an integer
            // remainder would cost a division per sample.
            let mut idx = 0usize;
            for j in 0..n {
                let re = input[2 * j] as f64;
                let im = input[2 * j + 1] as f64;
                let c = self.cos[idx];
                let s = self.sin[idx];
                sum_re += re * c - im * s;
                sum_im += re * s + im * c;
                idx += k;
                if idx >= n {
                    idx -= n;
                }
            }
            output[2 * k] = sum_re as f32;
            output[2 * k + 1] = sum_im as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Complex32, ForwardDft, KissFft};
    use core::f32::consts::PI;

    /// Compute libopus-style forward DFT with `1/N` scaling for tests.
    ///
    /// Params: `input` is time-domain complex vector.
    /// Returns: scaled frequency-domain vector.
    fn forward_scaled(input: &[Complex32]) -> Vec<Complex32> {
        let n = input.len();
        let mut out = vec![Complex32::new(0.0, 0.0); n];
        for (k, yk) in out.iter_mut().enumerate() {
            let mut acc_re = 0.0f32;
            let mut acc_im = 0.0f32;
            for (n_idx, xn) in input.iter().enumerate() {
                let phase = -2.0 * PI * (k as f32) * (n_idx as f32) / (n as f32);
                let c = phase.cos();
                let s = phase.sin();
                acc_re += xn.re * c - xn.im * s;
                acc_im += xn.re * s + xn.im * c;
            }
            *yk = Complex32::new(acc_re / (n as f32), acc_im / (n as f32));
        }
        out
    }

    #[test]
    fn ifft_roundtrip_matches_input() {
        let n = 60usize;
        let fft = KissFft::new(n);
        let mut input = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / n as f32;
            input.push(Complex32::new(
                (2.0 * PI * 3.0 * t).sin(),
                (2.0 * PI * 5.0 * t).cos(),
            ));
        }
        let freq = forward_scaled(&input);
        let mut recon = vec![Complex32::new(0.0, 0.0); n];
        fft.ifft(&freq, &mut recon).expect("ifft must succeed");
        for (a, b) in input.iter().zip(recon.iter()) {
            assert!(
                (a.re - b.re).abs() < 2e-5,
                "re mismatch: {} vs {}",
                a.re,
                b.re
            );
            assert!(
                (a.im - b.im).abs() < 2e-5,
                "im mismatch: {} vs {}",
                a.im,
                b.im
            );
        }
    }

    /// The plan's table-driven transform must agree with the definitional
    /// `sin_cos()`-per-bin DFT it replaced. Every CELT MDCT size uses a
    /// transform of 30·2^k, so those are the sizes the decoder can ask for.
    #[test]
    fn table_driven_forward_dft_matches_the_naive_definition() {
        fn naive(input: &[f32], n: usize) -> Vec<f32> {
            let mut out = vec![0.0f32; 2 * n];
            for k in 0..n {
                let mut sum_re = 0.0f64;
                let mut sum_im = 0.0f64;
                for j in 0..n {
                    let angle = -2.0 * core::f64::consts::PI * (k as f64) * (j as f64) / (n as f64);
                    let (sin_a, cos_a) = angle.sin_cos();
                    let re = input[2 * j] as f64;
                    let im = input[2 * j + 1] as f64;
                    sum_re += re * cos_a - im * sin_a;
                    sum_im += re * sin_a + im * cos_a;
                }
                out[2 * k] = sum_re as f32;
                out[2 * k + 1] = sum_im as f32;
            }
            out
        }

        // Deterministic pseudo-random input: a decoder feeds real audio, but
        // the transform must be checked against the full dynamic range.
        let mut state = 0x1234_5678u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };

        for n in [60usize, 120, 240, 480] {
            let input: Vec<f32> = (0..2 * n).map(|_| next()).collect();
            let mut fast = vec![0.0f32; 2 * n];
            let plan = ForwardDft::new(n);
            assert_eq!(plan.len(), n);
            plan.transform(&input, &mut fast);

            let reference = naive(&input, n);
            for i in 0..2 * n {
                let diff = (fast[i] - reference[i]).abs();
                assert!(
                    diff <= 1e-5,
                    "n={n} bin {i}: fast {} vs reference {} (diff {diff})",
                    fast[i],
                    reference[i]
                );
            }
        }
    }
}

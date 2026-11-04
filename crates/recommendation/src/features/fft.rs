use std::f32::consts::PI;

#[derive(Clone, Copy, Default)]
pub(crate) struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    #[inline]
    pub(crate) fn conjugate(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }
}

/// Iterative radix-2 complex FFT with a precomputed bit-reversal table and
/// twiddle factors. The table lookup avoids the accumulated error of updating
/// a complex twiddle by recurrence in every butterfly.
pub(crate) struct ComplexFft {
    n: usize,
    twiddles: Vec<Complex>,
    bit_reverse: Vec<usize>,
}

impl ComplexFft {
    pub(crate) fn new(n: usize) -> Self {
        assert!(n.is_power_of_two(), "FFT size must be a power of two");
        let twiddles = (0..n / 2)
            .map(|k| {
                let angle = -2.0 * PI * k as f32 / n as f32;
                Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                }
            })
            .collect();
        let bits = n.trailing_zeros() as usize;
        let bit_reverse = (0..n)
            .map(|index| {
                if bits == 0 {
                    0
                } else {
                    index.reverse_bits() >> (usize::BITS as usize - bits)
                }
            })
            .collect();
        Self {
            n,
            twiddles,
            bit_reverse,
        }
    }

    pub(crate) fn forward(&self, values: &mut [Complex]) {
        debug_assert_eq!(values.len(), self.n);
        for (index, reversed) in self.bit_reverse.iter().enumerate() {
            if index < *reversed {
                values.swap(index, *reversed);
            }
        }
        let mut len = 2_usize;
        while len <= self.n {
            let half = len / 2;
            let step = self.n / len;
            for base in (0..self.n).step_by(len) {
                for offset in 0..half {
                    let twiddle = self.twiddles[offset * step];
                    let even = values[base + offset];
                    let odd_value = values[base + offset + half];
                    let odd = Complex {
                        re: odd_value.re * twiddle.re - odd_value.im * twiddle.im,
                        im: odd_value.re * twiddle.im + odd_value.im * twiddle.re,
                    };
                    values[base + offset] = Complex {
                        re: even.re + odd.re,
                        im: even.im + odd.im,
                    };
                    values[base + offset + half] = Complex {
                        re: even.re - odd.re,
                        im: even.im - odd.im,
                    };
                }
            }
            len <<= 1;
        }
    }
}

/// Real-input FFT using the standard N/2 complex packing. It produces the same
/// one-sided power spectrum as a complex N-point FFT at roughly half the cost.
pub(crate) struct RealFft {
    n: usize,
    half: usize,
    inner: ComplexFft,
    full_twiddles: Vec<Complex>,
    scratch: Vec<Complex>,
}

impl RealFft {
    pub(crate) fn new(n: usize) -> Self {
        assert!(n.is_power_of_two() && n >= 4, "real FFT needs N >= 4");
        let half = n / 2;
        let full_twiddles = (0..half)
            .map(|k| {
                let angle = -2.0 * PI * k as f32 / n as f32;
                Complex {
                    re: angle.cos(),
                    im: angle.sin(),
                }
            })
            .collect();
        Self {
            n,
            half,
            inner: ComplexFft::new(half),
            full_twiddles,
            scratch: vec![Complex::default(); half],
        }
    }

    pub(crate) fn forward_into(&mut self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.n);
        debug_assert_eq!(output.len(), self.half + 1);
        for (index, value) in self.scratch.iter_mut().enumerate() {
            *value = Complex {
                re: input[index * 2],
                im: input[index * 2 + 1],
            };
        }
        self.inner.forward(&mut self.scratch);

        let first = self.scratch[0];
        output[0] = (first.re + first.im) * (first.re + first.im);
        output[self.half] = (first.re - first.im) * (first.re - first.im);
        for (k, output_value) in output.iter_mut().enumerate().take(self.half).skip(1) {
            let current = self.scratch[k];
            let mirrored = self.scratch[self.half - k].conjugate();
            let even_re = 0.5 * (current.re + mirrored.re);
            let even_im = 0.5 * (current.im + mirrored.im);
            let diff_re = current.re - mirrored.re;
            let diff_im = current.im - mirrored.im;
            let odd_re = 0.5 * diff_im;
            let odd_im = -0.5 * diff_re;
            let twiddle = self.full_twiddles[k];
            let rotated_re = twiddle.re * odd_re - twiddle.im * odd_im;
            let rotated_im = twiddle.re * odd_im + twiddle.im * odd_re;
            let real = even_re + rotated_re;
            let imaginary = even_im + rotated_im;
            *output_value = real * real + imaginary * imaginary;
        }
    }
}

//! One-bit delta-sigma modulator core.
//!
//! The modulator is realized in the CRFB (cascade of resonators, feedback)
//! topology from the coefficients in [`super::ntf`]. The state-space update
//! is sparse (at most a handful of terms per state), runs in `f64`, and
//! allocates nothing.
//!
//! ## Modulation depth
//!
//! A one-bit modulator has a finite maximum stable input amplitude. Rather
//! than let a 0 dBFS signal drive the loop into overload, the input is
//! attenuated by [`MODULATION_DEPTH`] (0.5, i.e. 50 %). This is the same
//! policy HQPlayer documents — "maximum modulation depth is monitored and
//! when necessary limited to 50 %" — and it keeps the loop stable for the
//! full digital scale. The resulting analog level is 6 dB below the raw
//! bitstream's full scale; the DAC's DSD gain stage provides that headroom.
//!
//! ## Safety leash
//!
//! The design is unconditionally bounded for inputs inside ±1.0 in exact
//! arithmetic; the leash is a last-resort guard against a numerical or
//! pathological state (non-finite values, or a state magnitude far outside
//! the nominal < 2 range). When it engages it scales the state back and
//! increments [`SdmModulator::state_recoveries`], which is surfaced in
//! telemetry so a silent rescue can never be mistaken for clean output.

use super::ntf;

/// Largest supported modulator order.
pub(crate) const MAX_ORDER: usize = 7;
/// Input attenuation: the 50 % modulation-depth limit.
pub(crate) const MODULATION_DEPTH: f64 = 0.5;
/// State magnitude that trips the safety leash.
const STATE_LEASH: f64 = 64.0;

/// A single one-bit modulator with its own state.
#[derive(Clone)]
pub struct SdmModulator {
    order: usize,
    multiple: u32,
    matrix: [[f64; MAX_ORDER]; MAX_ORDER],
    b1: [f64; MAX_ORDER],
    b2: [f64; MAX_ORDER],
    c: [f64; MAX_ORDER],
    d1: f64,
    state: [f64; MAX_ORDER],
    clip_count: u64,
    state_recoveries: u64,
    input_peak: f64,
    ones: u64,
    samples: u64,
}

impl SdmModulator {
    /// Build a modulator of `order` (5 or 7) for a DSD `multiple`
    /// (64/128/256/512). Returns `None` for an unsupported combination.
    pub fn new(order: usize, multiple: u32) -> Option<Self> {
        let coeffs = ntf::coefficients(order, multiple)?;
        let order = coeffs.order;
        debug_assert!(order <= MAX_ORDER);

        // Dense A/B/C/D assembly (control path only), then compressed to
        // sparse rows for the audio path.
        let mut abcd = [[0.0f64; MAX_ORDER + 2]; MAX_ORDER + 1];
        for (i, row) in abcd.iter_mut().enumerate().take(order) {
            row[i] = 1.0;
            row[order] = coeffs.a[i];
            row[order + 1] = -coeffs.a[i];
        }

        let odd = order % 2;
        let even = 1 - odd;

        // Sub-diagonal (delaying integrator couplings). The final entry for
        // an odd-order design writes the output row (C), exactly as the
        // toolbox's stuffABCD('CRFB') does.
        let mut sub_cols = [0usize; MAX_ORDER];
        let mut sub_len = 0;
        for k in (even..order).step_by(2) {
            abcd[k + 1][k] = 1.0;
            sub_cols[sub_len] = k;
            sub_len += 1;
        }
        // Super-diagonal resonator couplings -g.
        for (idx, &k) in sub_cols[..sub_len].iter().skip(odd).enumerate() {
            abcd[k - 1][k] = -coeffs.g[idx];
        }
        // Delaying integrators fold the row above into their own update.
        for d in (1 + odd..order).step_by(2) {
            let (above, rest) = abcd.split_at_mut(d);
            for (dst, src) in rest[0].iter_mut().zip(above[d - 1].iter()) {
                *dst += *src;
            }
        }

        let mut matrix = [[0.0f64; MAX_ORDER]; MAX_ORDER];
        let mut b1 = [0.0; MAX_ORDER];
        let mut b2 = [0.0; MAX_ORDER];
        let mut c = [0.0; MAX_ORDER];
        for i in 0..order {
            b1[i] = abcd[i][order];
            b2[i] = abcd[i][order + 1];
            c[i] = abcd[order][i];
            matrix[i][..order].copy_from_slice(&abcd[i][..order]);
        }

        Some(Self {
            order,
            multiple: coeffs.multiple,
            matrix,
            b1,
            b2,
            c,
            d1: abcd[order][order],
            state: [0.0; MAX_ORDER],
            clip_count: 0,
            state_recoveries: 0,
            input_peak: 0.0,
            ones: 0,
            samples: 0,
        })
    }

    /// Modulate one PCM sample (`[-1.0, 1.0]` nominal) into one DSD bit
    /// (`+1.0` or `-1.0`).
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let peak = input.abs();
        if peak > self.input_peak {
            self.input_peak = peak;
        }

        // Limit the digital input to full scale and apply the 50 % depth.
        let u = if input > 1.0 {
            self.clip_count += 1;
            MODULATION_DEPTH
        } else if input < -1.0 {
            self.clip_count += 1;
            -MODULATION_DEPTH
        } else {
            input * MODULATION_DEPTH
        };

        // Specialising the order lets the compiler fully unroll the dense
        // matrix update. The four accumulators are independent, so the
        // loop-carried latency is one add tree, not one add per term.
        let y = match self.order {
            5 => self.step::<5>(u),
            7 => self.step::<7>(u),
            _ => 0.0,
        };

        self.samples += 1;
        if y > 0.0 {
            self.ones += 1;
        }
        y
    }

    // Kept out of line: inlining the dense state update into the caller's
    // interpolation loop spills the tap registers and costs more than the
    // call. The call is once per DSD sample and the state update is the
    // loop-carried path.
    #[inline(never)]
    fn step<const N: usize>(&mut self, u: f64) -> f64 {
        let mut c = [0.0f64; 4];
        let mut k = 0;
        while k < N {
            let mut j = 0;
            while j < 4 && k + j < N {
                c[j] += self.c[k + j] * self.state[k + j];
                j += 1;
            }
            k += 4;
        }
        let v = self.d1 * u + ((c[0] + c[1]) + (c[2] + c[3]));
        let y = if v >= 0.0 { 1.0 } else { -1.0 };

        let mut next = [0.0f64; MAX_ORDER];
        for (i, slot) in next.iter_mut().enumerate().take(N) {
            let mut a = [self.b1[i] * u + self.b2[i] * y, 0.0, 0.0, 0.0];
            let row = &self.matrix[i];
            let mut k = 0;
            while k < N {
                let mut j = 0;
                while j < 4 && k + j < N {
                    a[j] += row[k + j] * self.state[k + j];
                    j += 1;
                }
                k += 4;
            }
            *slot = (a[0] + a[1]) + (a[2] + a[3]);
        }

        let mut max_state = 0.0f64;
        for &s in &next[..N] {
            max_state = max_state.max(s.abs());
        }
        if !max_state.is_finite() {
            next[..N].fill(0.0);
            self.state_recoveries += 1;
        } else if max_state > STATE_LEASH {
            let scale = STATE_LEASH / max_state;
            for s in &mut next[..N] {
                *s *= scale;
            }
            self.state_recoveries += 1;
        }
        self.state = next;
        y
    }

    /// Clear signal state and per-track telemetry.
    pub fn reset(&mut self) {
        self.state = [0.0; MAX_ORDER];
        self.clip_count = 0;
        self.state_recoveries = 0;
        self.input_peak = 0.0;
        self.ones = 0;
        self.samples = 0;
    }

    #[inline]
    pub fn order(&self) -> usize {
        self.order
    }

    #[inline]
    pub fn multiple(&self) -> u32 {
        self.multiple
    }

    #[inline]
    pub fn clip_count(&self) -> u64 {
        self.clip_count
    }

    #[inline]
    pub fn state_recoveries(&self) -> u64 {
        self.state_recoveries
    }

    /// Peak absolute input seen since [`Self::reset`].
    #[inline]
    pub fn input_peak(&self) -> f64 {
        self.input_peak
    }

    /// Number of one bits emitted since [`Self::reset`].
    #[inline]
    pub fn ones(&self) -> u64 {
        self.ones
    }

    /// Number of samples emitted since [`Self::reset`].
    #[inline]
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Fraction of output bits that are one (≈0.5 for silence).
    #[inline]
    pub fn ones_ratio(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.ones as f64 / self.samples as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(order: usize, multiple: u32, input: impl Fn(usize) -> f64, n: usize) -> SdmModulator {
        let mut m = SdmModulator::new(order, multiple).expect("modulator");
        for i in 0..n {
            m.process_sample(input(i));
        }
        m
    }

    #[test]
    fn sparse_realization_has_finite_state_for_all_orders() {
        for order in [5, 7] {
            for multiple in [64, 128, 256, 512] {
                let m = SdmModulator::new(order, multiple).expect("modulator");
                assert_eq!(m.order(), order);
                assert_eq!(m.multiple(), multiple);
                for row in &m.matrix[..order] {
                    assert!(row[..order].iter().any(|c| *c != 0.0));
                    assert!(row[..order].iter().all(|c| c.is_finite()));
                }
                assert!(m.d1.is_finite());
            }
        }
    }

    #[test]
    fn idle_output_is_bounded_and_centred() {
        for order in [5, 7] {
            for multiple in [64, 128, 256, 512] {
                let m = run(order, multiple, |_| 0.0, 50_000);
                assert_eq!(m.state_recoveries(), 0, "order {order} / {multiple}");
                assert_eq!(m.clip_count(), 0);
                // The bitstream must stay near 50 % ones for silence; a
                // stuck or wildly offset output is a design failure.
                let ratio = m.ones_ratio();
                assert!((ratio - 0.5).abs() < 0.05, "ratio {ratio}");
            }
        }
    }

    #[test]
    fn full_scale_input_clamps_and_stays_bounded() {
        for order in [5, 7] {
            let mut m = SdmModulator::new(order, 64).expect("modulator");
            for i in 0..100_000 {
                let x = 1.4 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 2_822_400.0).sin();
                let y = m.process_sample(x);
                assert!(y == 1.0 || y == -1.0);
            }
            assert_eq!(m.state_recoveries(), 0, "order {order}");
            assert!(m.clip_count() > 0);
            assert!(m.input_peak() >= 1.0);
        }
    }

    #[test]
    fn silence_recovers_after_signal() {
        let mut m = SdmModulator::new(5, 256).expect("modulator");
        for i in 0..20_000 {
            m.process_sample(0.4 * (i as f64 * 0.01).sin());
        }
        for _ in 0..50_000 {
            m.process_sample(0.0);
        }
        assert_eq!(m.state_recoveries(), 0);
        assert!((m.ones_ratio() - 0.5).abs() < 0.05);
    }

    #[test]
    fn reset_clears_state_and_telemetry() {
        let mut m = SdmModulator::new(5, 64).expect("modulator");
        for _ in 0..10_000 {
            m.process_sample(1.5);
        }
        m.reset();
        assert_eq!(m.clip_count(), 0);
        assert_eq!(m.input_peak(), 0.0);
        assert_eq!(m.ones_ratio(), 0.0);
    }
}

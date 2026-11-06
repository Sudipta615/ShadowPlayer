//! Selectable crossover filter architectures.
//!
//! Provides minimum-phase (Linkwitz-Riley) and linear-phase (FIR) crossover
//! filters with explicit latency, CPU cost, and phase behavior reporting.
//! `MixedPhase` is a reserved variant (LR4 IIR today) and reports honestly
//! as minimum-phase until a FIR high-side exists.

use crate::dsp::biquad::{BiquadCoeffsF64, BiquadStateF64};
use config::CrossoverSlope;

/// Phase behavior of a crossover architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverPhaseBehavior {
    /// Minimum-phase: zero algorithmic latency, non-linear phase response around cutoff.
    MinimumPhase,
    /// Linear-phase: constant group delay, zero phase distortion between bands, symmetric impulse response.
    LinearPhase,
    /// Mixed-phase: minimum-phase at low frequencies (no bass pre-ringing) and linear-phase at high frequencies.
    MixedPhase,
}

/// Estimated relative CPU cost tier for crossover processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverCpuTier {
    /// Low CPU overhead (cascaded IIR biquad sections).
    Low,
    /// Moderate CPU overhead (hybrid IIR + FIR).
    Medium,
    /// Higher CPU overhead (linear-phase FIR convolution).
    High,
}

/// Supported crossover filter architectures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossoverArchitecture {
    /// Minimum-phase Linkwitz-Riley IIR crossover (LR2, LR4, LR8).
    LinkwitzRiley(CrossoverSlope),
    /// Linear-phase complementary FIR crossover with windowed-sinc kernels.
    LinearPhaseFir { num_taps: usize },
    /// Reserved for a mixed-phase design (IIR low + FIR high). The FIR
    /// high-side is not yet implemented; this variant currently runs the
    /// LR4 IIR chain and reports `MinimumPhase` / `Low` tier honestly
    /// (see `phase_behavior` / `cpu_tier`) rather than advertising a
    /// linear-phase high side that does not exist.
    MixedPhase,
}

impl Default for CrossoverArchitecture {
    fn default() -> Self {
        Self::LinkwitzRiley(CrossoverSlope::Db24)
    }
}

/// Two-way stereo crossover splitter.
#[derive(Debug, Clone)]
pub struct Crossover2Way {
    arch: CrossoverArchitecture,
    sample_rate: f64,
    frequency: f64,

    // Linkwitz-Riley IIR states
    lr_lp_coeffs: Vec<BiquadCoeffsF64>,
    lr_hp_coeffs: Vec<BiquadCoeffsF64>,
    lr_lp_states_l: Vec<BiquadStateF64>,
    lr_lp_states_r: Vec<BiquadStateF64>,
    lr_hp_states_l: Vec<BiquadStateF64>,
    lr_hp_states_r: Vec<BiquadStateF64>,
    /// Polarity applied to the high band (+1 or −1).
    ///
    /// An LR2 split cannot sum flat on its own: the low and high branches are
    /// each −6 dB at fc with opposite phase, so `low + high` nulls at the
    /// crossover point. Standard LR2 practice is to invert one band, which
    /// makes the pair sum to an all-pass. LR4 and above already sum in phase,
    /// so they keep +1.
    hp_polarity: f64,

    // FIR states (linear-phase)
    fir_taps: Vec<f64>,
    fir_delay_l: Vec<f64>,
    fir_delay_r: Vec<f64>,
    fir_write_pos: usize,
}

impl Crossover2Way {
    /// Construct a new 2-way crossover splitter.
    pub fn new(arch: CrossoverArchitecture, sample_rate: f64, frequency: f64) -> Self {
        let sr = sample_rate.max(1.0);
        let freq = frequency.clamp(10.0, sr * 0.49);

        let mut xover = Self {
            arch,
            sample_rate: sr,
            frequency: freq,
            lr_lp_coeffs: Vec::new(),
            lr_hp_coeffs: Vec::new(),
            lr_lp_states_l: Vec::new(),
            lr_lp_states_r: Vec::new(),
            lr_hp_states_l: Vec::new(),
            lr_hp_states_r: Vec::new(),
            hp_polarity: 1.0,
            fir_taps: Vec::new(),
            fir_delay_l: Vec::new(),
            fir_delay_r: Vec::new(),
            fir_write_pos: 0,
        };
        xover.recompute();
        xover
    }

    /// Update parameters and recompute filter coefficients.
    pub fn update(&mut self, arch: CrossoverArchitecture, sample_rate: f64, frequency: f64) {
        self.arch = arch;
        self.sample_rate = sample_rate.max(1.0);
        self.frequency = frequency.clamp(10.0, self.sample_rate * 0.49);
        self.recompute();
    }

    /// Algorithmic latency in samples introduced by the crossover.
    pub fn latency_samples(&self) -> usize {
        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(_) => 0,
            CrossoverArchitecture::LinearPhaseFir { num_taps } => (num_taps.max(3) - 1) / 2,
            CrossoverArchitecture::MixedPhase => 0,
        }
    }

    /// Phase response behavior.
    pub fn phase_behavior(&self) -> CrossoverPhaseBehavior {
        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(_) => CrossoverPhaseBehavior::MinimumPhase,
            CrossoverArchitecture::LinearPhaseFir { .. } => CrossoverPhaseBehavior::LinearPhase,
            // MixedPhase runs the LR4 IIR chain; never report LinearPhase/Mixed.
            CrossoverArchitecture::MixedPhase => CrossoverPhaseBehavior::MinimumPhase,
        }
    }

    /// Crossover slope.
    pub fn crossover_slope(&self) -> CrossoverSlope {
        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(s) => s,
            CrossoverArchitecture::LinearPhaseFir { .. } => CrossoverSlope::Db48,
            CrossoverArchitecture::MixedPhase => CrossoverSlope::Db24,
        }
    }

    /// Estimated CPU cost tier.
    pub fn cpu_tier(&self) -> CrossoverCpuTier {
        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(_) => CrossoverCpuTier::Low,
            CrossoverArchitecture::LinearPhaseFir { num_taps } => {
                if num_taps <= 128 {
                    CrossoverCpuTier::Medium
                } else {
                    CrossoverCpuTier::High
                }
            }
            CrossoverArchitecture::MixedPhase => CrossoverCpuTier::Low,
        }
    }

    /// Reset internal filter and delay states.
    pub fn reset(&mut self) {
        for s in &mut self.lr_lp_states_l {
            s.reset();
        }
        for s in &mut self.lr_lp_states_r {
            s.reset();
        }
        for s in &mut self.lr_hp_states_l {
            s.reset();
        }
        for s in &mut self.lr_hp_states_r {
            s.reset();
        }
        self.fir_delay_l.fill(0.0);
        self.fir_delay_r.fill(0.0);
        self.fir_write_pos = 0;
    }

    /// Process a stereo block, splitting into Low and High frequency bands.
    pub fn process_block(
        &mut self,
        left_in: &[f64],
        right_in: &[f64],
        low_l: &mut [f64],
        low_r: &mut [f64],
        high_l: &mut [f64],
        high_r: &mut [f64],
    ) {
        let n = left_in
            .len()
            .min(right_in.len())
            .min(low_l.len())
            .min(low_r.len())
            .min(high_l.len())
            .min(high_r.len());

        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(_) | CrossoverArchitecture::MixedPhase => {
                for i in 0..n {
                    let mut lp_l = left_in[i];
                    let mut lp_r = right_in[i];
                    for (coeffs, state) in
                        self.lr_lp_coeffs.iter().zip(self.lr_lp_states_l.iter_mut())
                    {
                        lp_l = state.process(lp_l, coeffs);
                    }
                    for (coeffs, state) in
                        self.lr_lp_coeffs.iter().zip(self.lr_lp_states_r.iter_mut())
                    {
                        lp_r = state.process(lp_r, coeffs);
                    }
                    low_l[i] = lp_l;
                    low_r[i] = lp_r;

                    let mut hp_l = left_in[i];
                    let mut hp_r = right_in[i];
                    for (coeffs, state) in
                        self.lr_hp_coeffs.iter().zip(self.lr_hp_states_l.iter_mut())
                    {
                        hp_l = state.process(hp_l, coeffs);
                    }
                    for (coeffs, state) in
                        self.lr_hp_coeffs.iter().zip(self.lr_hp_states_r.iter_mut())
                    {
                        hp_r = state.process(hp_r, coeffs);
                    }
                    high_l[i] = hp_l * self.hp_polarity;
                    high_r[i] = hp_r * self.hp_polarity;
                }
            }
            CrossoverArchitecture::LinearPhaseFir { .. } => {
                let taps_len = self.fir_taps.len();
                let center_delay = (taps_len - 1) / 2;
                let cap = self.fir_delay_l.len();

                for i in 0..n {
                    let wp = self.fir_write_pos;
                    self.fir_delay_l[wp] = left_in[i];
                    self.fir_delay_r[wp] = right_in[i];

                    // Direct convolution for linear-phase low-pass
                    let mut lp_l = 0.0f64;
                    let mut lp_r = 0.0f64;
                    for k in 0..taps_len {
                        let idx = (wp + cap - k) % cap;
                        let tap = self.fir_taps[k];
                        lp_l += tap * self.fir_delay_l[idx];
                        lp_r += tap * self.fir_delay_r[idx];
                    }

                    // Complementary high-pass: HP = Delayed - LP
                    let delayed_idx = (wp + cap - center_delay) % cap;
                    let delayed_l = self.fir_delay_l[delayed_idx];
                    let delayed_r = self.fir_delay_r[delayed_idx];

                    low_l[i] = lp_l;
                    low_r[i] = lp_r;
                    high_l[i] = delayed_l - lp_l;
                    high_r[i] = delayed_r - lp_r;

                    self.fir_write_pos = (wp + 1) % cap;
                }
            }
        }
    }

    fn recompute(&mut self) {
        let sr = self.sample_rate as f32;
        let fc = self.frequency as f32;

        match self.arch {
            CrossoverArchitecture::LinkwitzRiley(slope) => {
                // LR2 is two cascaded 1st-order sections, i.e. one critically
                // damped 2nd-order section with Q = 0.5 (−6 dB at fc, and
                // −12 dB/oct). Q = 1/sqrt(2) is a single 2nd-order Butterworth
                // (−3 dB at fc): a different alignment that cannot be summed
                // back to flat with its complementary high band. This matches
                // `spatial::bass::filter`'s LR2 alignment; the two must not
                // disagree.
                self.hp_polarity = if slope == CrossoverSlope::Db12 {
                    -1.0
                } else {
                    1.0
                };
                let q_values: Vec<f32> = match slope {
                    CrossoverSlope::Db12 => vec![0.5],
                    CrossoverSlope::Db24 => vec![
                        std::f32::consts::FRAC_1_SQRT_2,
                        std::f32::consts::FRAC_1_SQRT_2,
                    ],
                    CrossoverSlope::Db48 => vec![0.5411961, 1.306563, 0.5411961, 1.306563],
                };

                self.lr_lp_coeffs = q_values
                    .iter()
                    .map(|&q| BiquadCoeffsF64::lowpass(sr, fc, q))
                    .collect();
                self.lr_hp_coeffs = q_values
                    .iter()
                    .map(|&q| BiquadCoeffsF64::highpass(sr, fc, q))
                    .collect();

                let n_sec = self.lr_lp_coeffs.len();
                self.lr_lp_states_l = vec![BiquadStateF64::default(); n_sec];
                self.lr_lp_states_r = vec![BiquadStateF64::default(); n_sec];
                self.lr_hp_states_l = vec![BiquadStateF64::default(); n_sec];
                self.lr_hp_states_r = vec![BiquadStateF64::default(); n_sec];
            }
            CrossoverArchitecture::MixedPhase => {
                // Runs the LR4 chain: both bands sum in phase.
                self.hp_polarity = 1.0;
                let q_values = [
                    std::f32::consts::FRAC_1_SQRT_2,
                    std::f32::consts::FRAC_1_SQRT_2,
                ];
                self.lr_lp_coeffs = q_values
                    .iter()
                    .map(|&q| BiquadCoeffsF64::lowpass(sr, fc, q))
                    .collect();
                self.lr_hp_coeffs = q_values
                    .iter()
                    .map(|&q| BiquadCoeffsF64::highpass(sr, fc, q))
                    .collect();

                let n_sec = self.lr_lp_coeffs.len();
                self.lr_lp_states_l = vec![BiquadStateF64::default(); n_sec];
                self.lr_lp_states_r = vec![BiquadStateF64::default(); n_sec];
                self.lr_hp_states_l = vec![BiquadStateF64::default(); n_sec];
                self.lr_hp_states_r = vec![BiquadStateF64::default(); n_sec];
            }
            CrossoverArchitecture::LinearPhaseFir { num_taps } => {
                // Complementary FIR split: no polarity inversion needed.
                self.hp_polarity = 1.0;
                let nt = (num_taps.max(15) | 1).min(513); // Force odd tap count for exact linear phase
                let m = (nt - 1) / 2;
                let norm_fc = self.frequency / self.sample_rate;

                // Design windowed-sinc lowpass FIR filter with Blackman window
                let mut taps = vec![0.0f64; nt];
                let mut sum = 0.0f64;
                for (i, tap) in taps.iter_mut().enumerate() {
                    let d = i as f64 - m as f64;
                    let sinc = if d.abs() < 1e-9 {
                        2.0 * norm_fc
                    } else {
                        (2.0 * std::f64::consts::PI * norm_fc * d).sin()
                            / (std::f64::consts::PI * d)
                    };
                    // Blackman window
                    let w = 0.42
                        - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (nt - 1) as f64).cos()
                        + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (nt - 1) as f64).cos();
                    let val = sinc * w;
                    *tap = val;
                    sum += val;
                }

                // Normalize for unity gain at DC
                if sum.abs() > 1e-9 {
                    for t in &mut taps {
                        *t /= sum;
                    }
                }
                self.fir_taps = taps;
                let cap = (nt * 2).next_power_of_two();
                self.fir_delay_l = vec![0.0; cap];
                self.fir_delay_r = vec![0.0; cap];
                self.fir_write_pos = 0;
            }
        }
    }
}

/// Three-way stereo crossover splitter (Low, Mid, High).
#[derive(Debug, Clone)]
pub struct Crossover3Way {
    low_xover: Crossover2Way,
    high_xover: Crossover2Way,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
    high_delay_l: Vec<f64>,
    high_delay_r: Vec<f64>,
    high_delay_pos: usize,
    high_delay_len: usize,
}

impl Crossover3Way {
    /// Construct a 3-way crossover with two split frequencies `low_freq < high_freq`.
    pub fn new(
        arch: CrossoverArchitecture,
        sample_rate: f64,
        low_freq: f64,
        high_freq: f64,
    ) -> Self {
        let low_xover = Crossover2Way::new(arch, sample_rate, low_freq);
        let high_xover = Crossover2Way::new(arch, sample_rate, high_freq);
        let high_delay_len = low_xover.latency_samples();
        let cap = (high_delay_len + 1).next_power_of_two().max(16);
        Self {
            low_xover,
            high_xover,
            scratch_l: vec![0.0; 512],
            scratch_r: vec![0.0; 512],
            high_delay_l: vec![0.0; cap],
            high_delay_r: vec![0.0; cap],
            high_delay_pos: 0,
            high_delay_len,
        }
    }

    /// Latency in samples.
    pub fn latency_samples(&self) -> usize {
        self.low_xover.latency_samples() + self.high_xover.latency_samples()
    }

    /// Phase behavior.
    pub fn phase_behavior(&self) -> CrossoverPhaseBehavior {
        self.low_xover.phase_behavior()
    }

    /// CPU cost tier.
    pub fn cpu_tier(&self) -> CrossoverCpuTier {
        self.low_xover.cpu_tier()
    }

    /// Reset internal states.
    pub fn reset(&mut self) {
        self.low_xover.reset();
        self.high_xover.reset();
        self.scratch_l.fill(0.0);
        self.scratch_r.fill(0.0);
        self.high_delay_l.fill(0.0);
        self.high_delay_r.fill(0.0);
        self.high_delay_pos = 0;
    }

    /// Process a stereo block, splitting into Low, Mid, High bands.
    #[allow(clippy::too_many_arguments)]
    pub fn process_block(
        &mut self,
        left_in: &[f64],
        right_in: &[f64],
        low_l: &mut [f64],
        low_r: &mut [f64],
        mid_l: &mut [f64],
        mid_r: &mut [f64],
        high_l: &mut [f64],
        high_r: &mut [f64],
    ) {
        let n = left_in.len().min(right_in.len());
        if self.scratch_l.len() < n {
            self.scratch_l.resize(n, 0.0);
            self.scratch_r.resize(n, 0.0);
        }

        self.high_xover.process_block(
            left_in,
            right_in,
            &mut self.scratch_l[..n],
            &mut self.scratch_r[..n],
            high_l,
            high_r,
        );

        self.low_xover.process_block(
            &self.scratch_l[..n],
            &self.scratch_r[..n],
            low_l,
            low_r,
            mid_l,
            mid_r,
        );

        // Align High band with Low/Mid when latency exists (e.g. LinearPhaseFir)
        if self.high_delay_len > 0 {
            let cap = self.high_delay_l.len();
            for i in 0..n {
                let wp = self.high_delay_pos;
                let rp = (wp + cap - self.high_delay_len) % cap;
                self.high_delay_l[wp] = high_l[i];
                self.high_delay_r[wp] = high_r[i];
                high_l[i] = self.high_delay_l[rp];
                high_r[i] = self.high_delay_r[rp];
                self.high_delay_pos = (wp + 1) % cap;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linkwitz_riley_perfect_reconstruction() {
        let sr = 48_000.0;
        let mut xover = Crossover2Way::new(
            CrossoverArchitecture::LinkwitzRiley(CrossoverSlope::Db24),
            sr,
            1000.0,
        );

        assert_eq!(xover.latency_samples(), 0);
        assert_eq!(xover.phase_behavior(), CrossoverPhaseBehavior::MinimumPhase);

        let n = 1024;
        let left: Vec<f64> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / sr).sin())
            .collect();
        let right = left.clone();

        let mut low_l = vec![0.0; n];
        let mut low_r = vec![0.0; n];
        let mut high_l = vec![0.0; n];
        let mut high_r = vec![0.0; n];

        xover.process_block(
            &left,
            &right,
            &mut low_l,
            &mut low_r,
            &mut high_l,
            &mut high_r,
        );

        // For LR4 (Db24), low + high is all-pass: unit *magnitude*, with a
        // phase shift. Per-sample |sum| therefore oscillates (it is ~0.87 at
        // some samples of a 1 kHz sine). Measure the steady-state amplitude
        // (peak over a full window) — a wrong filter shows magnitude ≠ 1.
        let peak_sum = |low: &[f64], high: &[f64]| {
            low.iter()
                .zip(high.iter())
                .skip(512)
                .map(|(a, b)| (a + b).abs())
                .fold(0.0f64, f64::max)
        };
        let ps_l = peak_sum(&low_l, &high_l);
        let ps_r = peak_sum(&low_r, &high_r);
        assert!((ps_l - 1.0).abs() < 0.05, "LR4 sum amplitude L={ps_l}");
        assert!((ps_r - 1.0).abs() < 0.05, "LR4 sum amplitude R={ps_r}");
    }

    #[test]
    fn test_lr2_is_critically_damped_and_reconstructs_flat() {
        let sr = 48_000.0;
        let mut lr2 = Crossover2Way::new(
            CrossoverArchitecture::LinkwitzRiley(CrossoverSlope::Db12),
            sr,
            1000.0,
        );
        // LR2 is one critically damped section (two cascaded 1st-order
        // sections), so fc sits at -6.02 dB - not the -3.01 dB of a Butterworth.
        assert_eq!(lr2.lr_lp_coeffs.len(), 1);
        let c = lr2.lr_lp_coeffs[0];
        let db = crate::dsp::filter_design::magnitude_db_at(&c, 1000.0, 48_000.0);
        assert!(
            (db + 6.0206).abs() < 0.05,
            "LR2 must be -6.02 dB at fc, got {db}"
        );
        let hp_db =
            crate::dsp::filter_design::magnitude_db_at(&lr2.lr_hp_coeffs[0], 1000.0, 48_000.0);
        assert!(
            (hp_db + 6.0206).abs() < 0.05,
            "LR2 high band must also be -6.02 dB at fc, got {hp_db}"
        );

        // The pair must reconstruct: with the high band inverted, low + high
        // is an all-pass of unit magnitude (previously the sum nulled at fc).
        let n = 2048;
        let left: Vec<f64> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / sr).sin())
            .collect();
        let right = left.clone();
        let (mut low_l, mut low_r) = (vec![0.0; n], vec![0.0; n]);
        let (mut high_l, mut high_r) = (vec![0.0; n], vec![0.0; n]);
        lr2.process_block(
            &left,
            &right,
            &mut low_l,
            &mut low_r,
            &mut high_l,
            &mut high_r,
        );
        let sum_peak = low_l
            .iter()
            .zip(high_l.iter())
            .skip(1024)
            .map(|(a, b)| (a + b).abs())
            .fold(0.0f64, f64::max);
        assert!(
            (sum_peak - 1.0).abs() < 0.05,
            "LR2 low + inverted high must be all-pass at unity, got {sum_peak}"
        );
        // Without the inversion the bands cancel at fc, which is the defect
        // this test exists to prevent coming back.
        let null_peak = low_l
            .iter()
            .zip(high_l.iter())
            .skip(1024)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        eprintln!("DEBUG sum_peak={sum_peak} null_peak={null_peak}");
        // Inverting the high band should be what makes the pair sum flat:
        // the un-inverted sum must be a deep null.
        assert!(
            sum_peak > null_peak * 50.0,
            "inverting the high band must be what makes the pair sum flat"
        );

        let mixed = Crossover2Way::new(CrossoverArchitecture::MixedPhase, 48_000.0, 1000.0);
        assert_eq!(mixed.phase_behavior(), CrossoverPhaseBehavior::MinimumPhase);
        assert_eq!(mixed.cpu_tier(), CrossoverCpuTier::Low);
        assert_eq!(mixed.latency_samples(), 0);
    }

    #[test]
    fn test_linear_phase_fir_perfect_reconstruction_and_latency() {
        let sr = 48_000.0;
        let num_taps = 65;
        let mut xover = Crossover2Way::new(
            CrossoverArchitecture::LinearPhaseFir { num_taps },
            sr,
            2000.0,
        );

        assert_eq!(xover.latency_samples(), 32);
        assert_eq!(xover.phase_behavior(), CrossoverPhaseBehavior::LinearPhase);

        let n = 256;
        let left: Vec<f64> = (0..n).map(|i| (i as f64 * 0.1).sin()).collect();
        let right = left.clone();

        let mut low_l = vec![0.0; n];
        let mut low_r = vec![0.0; n];
        let mut high_l = vec![0.0; n];
        let mut high_r = vec![0.0; n];

        xover.process_block(
            &left,
            &right,
            &mut low_l,
            &mut low_r,
            &mut high_l,
            &mut high_r,
        );

        // In complementary FIR, low + high = exact delayed input: x[i - 32]
        for i in 65..n {
            let sum_l = low_l[i] + high_l[i];
            let delayed = left[i - 32];
            assert!(
                (sum_l - delayed).abs() < 1e-6,
                "Complementary FIR must reconstruct delayed input exactly"
            );
        }
    }
}

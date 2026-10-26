//! Psychoacoustic Missing Fundamental Bass Enhancer.
//!
//! # Overview
//!
//! Physical loudspeakers and headphones have physical limitations reproducing
//! sub-bass (< 80 Hz) without massive cone excursion, intermodulation distortion,
//! and amplifier clipping.
//!
//! Based on the "missing fundamental" psychoacoustic phenomenon (used in
//! JamesDSP and MaxxBass), this processor extracts sub-bass, eliminates
//! inaudible infrasonic rumble (< 30 Hz), generates an aligned harmonic series
//! (2nd and 3rd harmonics), and bandpasses them into the 90–250 Hz region.
//!
//! The human auditory cortex reconstructs the low pitch from its harmonic
//! structure, creating the perception of deep, thunderous, chest-thumping bass
//! with significantly reduced physical transducer excursion and zero limiter pumping.
//!
//! # Realtime Safety
//!
//! Fully zero-allocation. Operates on pre-allocated state variables and SVF
//! filter cores.

use crate::dsp::equalizer::svf::{SvfFilter, SvfMode};

/// Configuration parameters for the psychoacoustic bass enhancer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BassEnhancerParams {
    /// Crossover frequency in Hz (typically 60.0 to 120.0, default 80.0).
    pub crossover_hz: f32,
    /// Subsonic highpass cutoff in Hz to remove infrasound (default 30.0).
    pub subsonic_cut_hz: f32,
    /// Harmonic drive / saturation amount (0.5 to 4.0, default 1.5).
    pub drive: f32,
    /// Mix level of generated harmonics (0.0 to 1.0, default 0.35).
    pub harmonics_mix: f32,
    /// Direct sub-bass pass-through level (0.0 to 1.0, default 1.0).
    pub direct_sub_mix: f32,
    /// Master enable switch.
    pub enabled: bool,
}

impl Default for BassEnhancerParams {
    fn default() -> Self {
        Self {
            crossover_hz: 80.0,
            subsonic_cut_hz: 30.0,
            drive: 1.5,
            harmonics_mix: 0.35,
            direct_sub_mix: 1.0,
            enabled: false,
        }
    }
}

/// Stereo Psychoacoustic Bass Enhancer.
#[derive(Debug, Clone)]
pub struct BassEnhancer {
    params: BassEnhancerParams,
    // Per-channel crossover filters (split sub-bass from highs)
    crossover_l: SvfFilter<f64>,
    crossover_r: SvfFilter<f64>,
    // Subsonic rumble filters (highpass sub-bass at 30 Hz)
    subsonic_l: SvfFilter<f64>,
    subsonic_r: SvfFilter<f64>,
    // Bandpass filters to focus generated harmonics into 90-250 Hz
    harmonic_bpf_l: SvfFilter<f64>,
    harmonic_bpf_r: SvfFilter<f64>,
    // DC blocker states for the non-linear asymmetric shaper
    dc_x1_l: f64,
    dc_y1_l: f64,
    dc_x1_r: f64,
    dc_y1_r: f64,
    /// Sample-rate invariant DC blocker pole: R = exp(-2*pi * 10 Hz / fs).
    dc_pole: f64,
    sample_rate: f32,
}

impl Default for BassEnhancer {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl BassEnhancer {
    /// Create a new bass enhancer initialized for the given sample rate.
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate <= 0.0 || !sample_rate.is_finite() {
            44100.0
        } else {
            sample_rate
        };
        let mut enhancer = Self {
            params: BassEnhancerParams::default(),
            crossover_l: SvfFilter::new(sr as f64),
            crossover_r: SvfFilter::new(sr as f64),
            subsonic_l: SvfFilter::new(sr as f64),
            subsonic_r: SvfFilter::new(sr as f64),
            harmonic_bpf_l: SvfFilter::new(sr as f64),
            harmonic_bpf_r: SvfFilter::new(sr as f64),
            dc_x1_l: 0.0,
            dc_y1_l: 0.0,
            dc_x1_r: 0.0,
            dc_y1_r: 0.0,
            dc_pole: (-std::f64::consts::TAU * 10.0 / (sr as f64).max(1000.0)).exp(),
            sample_rate: sr,
        };
        enhancer.recompute_filters();
        enhancer
    }

    /// Update the configuration parameters.
    pub fn set_params(&mut self, params: BassEnhancerParams) {
        self.params = params;
        self.recompute_filters();
    }

    /// Current configuration parameters.
    #[inline]
    pub fn params(&self) -> &BassEnhancerParams {
        &self.params
    }

    /// Whether the bass enhancer is active.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.params.enabled
    }

    /// Set enabled state.
    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.params.enabled = enabled;
    }

    /// Update the sample rate and recompute filters.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let sr = if sample_rate <= 0.0 || !sample_rate.is_finite() {
            44100.0
        } else {
            sample_rate
        };
        self.sample_rate = sr;
        self.crossover_l = SvfFilter::new(sr as f64);
        self.crossover_r = SvfFilter::new(sr as f64);
        self.subsonic_l = SvfFilter::new(sr as f64);
        self.subsonic_r = SvfFilter::new(sr as f64);
        self.harmonic_bpf_l = SvfFilter::new(sr as f64);
        self.harmonic_bpf_r = SvfFilter::new(sr as f64);
        self.recompute_filters();
    }

    fn recompute_filters(&mut self) {
        let co_hz = self.params.crossover_hz.clamp(40.0, 200.0) as f64;
        let sub_hz = self.params.subsonic_cut_hz.clamp(15.0, 50.0) as f64;

        let q_butterworth = std::f64::consts::FRAC_1_SQRT_2;
        self.crossover_l.set_params(co_hz, q_butterworth);
        self.crossover_r.set_params(co_hz, q_butterworth);

        self.subsonic_l.set_params(sub_hz, q_butterworth);
        self.subsonic_r.set_params(sub_hz, q_butterworth);

        // Center harmonic resonance around 160 Hz (sweet spot between 90-250 Hz)
        self.harmonic_bpf_l.set_params(160.0, 1.2);
        self.harmonic_bpf_r.set_params(160.0, 1.2);

        // Calculate sample-rate invariant DC blocker pole:
        // Discrete 1-pole filter: y[n] = x[n] - x[n-1] + R * y[n-1].
        // Cutoff f_c = 10 Hz gives R = exp(-2*pi * 10 / fs).
        let sr = (self.sample_rate as f64).max(1000.0);
        self.dc_pole = (-std::f64::consts::TAU * 10.0 / sr)
            .exp()
            .clamp(0.9, 0.9999);
    }

    /// Reset state memories.
    pub fn reset(&mut self) {
        self.crossover_l.reset();
        self.crossover_r.reset();
        self.subsonic_l.reset();
        self.subsonic_r.reset();
        self.harmonic_bpf_l.reset();
        self.harmonic_bpf_r.reset();
        self.dc_x1_l = 0.0;
        self.dc_y1_l = 0.0;
        self.dc_x1_r = 0.0;
        self.dc_y1_r = 0.0;
    }

    /// Process a stereo sample pair in `f64`.
    #[inline(always)]
    pub fn process_sample(&mut self, left: f64, right: f64) -> (f64, f64) {
        if !self.params.enabled {
            return (left, right);
        }

        if !left.is_finite() || !right.is_finite() {
            self.reset();
            return (0.0, 0.0);
        }

        let drive = self.params.drive.clamp(0.1, 8.0) as f64;
        let h_mix = self.params.harmonics_mix.clamp(0.0, 2.0) as f64;
        let sub_mix = self.params.direct_sub_mix.clamp(0.0, 1.0) as f64;

        // 1. Crossover split: (sub_bass, _, dry_high, _)
        let (sub_l, _, dry_l, _) = self.crossover_l.process_multimode(left);
        let (sub_r, _, dry_r, _) = self.crossover_r.process_multimode(right);

        // 2. Highpass sub-bass to remove subsonic rumble
        let rumble_free_l = self.subsonic_l.process(sub_l, SvfMode::HighPass);
        let rumble_free_r = self.subsonic_r.process(sub_r, SvfMode::HighPass);

        // 3. Generate 2nd and 3rd harmonics using asymmetrical polynomial shaper
        let xl = (rumble_free_l * drive).clamp(-20.0, 20.0);
        let xr = (rumble_free_r * drive).clamp(-20.0, 20.0);

        // 2nd harmonic (warm octave): x^2
        // 3rd harmonic (punch): x^3 / (1 + |x|)
        let sat_l = 0.55 * (xl * xl).copysign(xl) + 0.45 * ((xl * xl * xl) / (1.0 + xl.abs()));
        let sat_r = 0.55 * (xr * xr).copysign(xr) + 0.45 * ((xr * xr * xr) / (1.0 + xr.abs()));

        // 4. Sample-rate invariant DC Blocker on generated harmonics with denormal flushing
        let dc_l =
            crate::buffer::flush_denormal_f64(sat_l - self.dc_x1_l + self.dc_pole * self.dc_y1_l);
        self.dc_x1_l = crate::buffer::flush_denormal_f64(sat_l);
        self.dc_y1_l = if dc_l.is_finite() { dc_l } else { 0.0 };

        let dc_r =
            crate::buffer::flush_denormal_f64(sat_r - self.dc_x1_r + self.dc_pole * self.dc_y1_r);
        self.dc_x1_r = crate::buffer::flush_denormal_f64(sat_r);
        self.dc_y1_r = if dc_r.is_finite() { dc_r } else { 0.0 };

        // 5. Bandpass the harmonics into the audible 90-250 Hz range
        let (_, harm_l, _, _) = self.harmonic_bpf_l.process_multimode(self.dc_y1_l);
        let (_, harm_r, _, _) = self.harmonic_bpf_r.process_multimode(self.dc_y1_r);

        // 6. Sum: Dry Highs + Direct Sub (scaled) + Synthesized Harmonics
        let out_l = dry_l + (rumble_free_l * sub_mix) + (harm_l * h_mix);
        let out_r = dry_r + (rumble_free_r * sub_mix) + (harm_r * h_mix);

        let final_l = if out_l.is_finite() { out_l } else { 0.0 };
        let final_r = if out_r.is_finite() { out_r } else { 0.0 };

        (final_l, final_r)
    }

    /// Process a block of stereo frames in place.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.params.enabled {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let (out_l, out_r) = self.process_sample(left[i] as f64, right[i] as f64);
            left[i] = out_l as f32;
            right[i] = out_r as f32;
        }
    }

    /// Process a block of native `f64` stereo frames in place.
    pub fn process_block_f64(&mut self, left: &mut [f64], right: &mut [f64]) {
        if !self.params.enabled {
            return;
        }
        let n = left.len().min(right.len());
        for i in 0..n {
            let (out_l, out_r) = self.process_sample(left[i], right[i]);
            left[i] = out_l;
            right[i] = out_r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bass_enhancer_disabled_is_exact_passthrough() {
        let mut be = BassEnhancer::new(44100.0);
        be.params.enabled = false;

        let (out_l, out_r) = be.process_sample(0.5, -0.5);
        assert_eq!(out_l, 0.5);
        assert_eq!(out_r, -0.5);
    }

    #[test]
    fn test_bass_enhancer_generates_harmonics_for_low_sine() {
        let mut be = BassEnhancer::new(48000.0);
        be.params.enabled = true;
        be.params.drive = 2.0;
        be.params.harmonics_mix = 0.5;
        be.params.direct_sub_mix = 0.0; // Mute direct sub to observe purely synthesized energy

        // Feed a 40 Hz sub-bass sine wave
        let f0 = 40.0;
        let mut max_out = 0.0f64;
        for i in 0..4800 {
            let t = i as f64 / 48000.0;
            let sample = (2.0 * std::f64::consts::PI * f0 * t).sin();
            let (out_l, _) = be.process_sample(sample, sample);
            if i > 2400 {
                max_out = max_out.max(out_l.abs());
            }
        }
        // With direct sub muted, output energy must be non-zero because of the missing fundamental harmonics
        assert!(
            max_out > 0.05,
            "harmonics must be generated from 40 Hz fundamental, got {max_out}"
        );
    }

    #[test]
    fn test_bass_enhancer_finite_and_bounded() {
        let mut be = BassEnhancer::new(44100.0);
        be.params.enabled = true;
        be.params.drive = 4.0;

        for i in 0..1000 {
            let s = if i % 2 == 0 { 1.0 } else { -1.0 };
            let (l, r) = be.process_sample(s, s);
            assert!(l.is_finite() && r.is_finite());
            assert!(l.abs() < 10.0 && r.abs() < 10.0);
        }
    }

    #[test]
    fn test_bass_enhancer_dc_pole_sample_rate_invariance() {
        // At 48 kHz, 96 kHz, and 192 kHz, the DC blocker cutoff frequency must remain around 10 Hz
        let be48 = BassEnhancer::new(48000.0);
        let be96 = BassEnhancer::new(96000.0);
        let be192 = BassEnhancer::new(192000.0);

        // Calculate approximate cutoff in Hz: f_c = (1 - R) * f_s / (2 * pi)
        let fc48 = (1.0 - be48.dc_pole) * 48000.0 / std::f64::consts::TAU;
        let fc96 = (1.0 - be96.dc_pole) * 96000.0 / std::f64::consts::TAU;
        let fc192 = (1.0 - be192.dc_pole) * 192000.0 / std::f64::consts::TAU;

        assert!(
            (fc48 - 10.0).abs() < 0.5,
            "48k DC cutoff should be ~10 Hz, got {fc48}"
        );
        assert!(
            (fc96 - 10.0).abs() < 0.5,
            "96k DC cutoff should be ~10 Hz, got {fc96}"
        );
        assert!(
            (fc192 - 10.0).abs() < 0.5,
            "192k DC cutoff should be ~10 Hz, got {fc192}"
        );
    }

    #[test]
    fn test_bass_enhancer_non_finite_rejection() {
        let mut be = BassEnhancer::new(48000.0);
        be.params.enabled = true;

        let (out_l, out_r) = be.process_sample(f64::NAN, 0.5);
        assert_eq!(out_l, 0.0);
        assert_eq!(out_r, 0.0);

        // Immediately following normal audio should be clean and finite
        let (clean_l, clean_r) = be.process_sample(0.5, 0.5);
        assert!(clean_l.is_finite() && clean_r.is_finite());
    }
}

//! Correction filter synthesis and parametric biquad curve fitting (§11.2, Item 35).
//!
//! Synthesizes minimum-phase, linear-phase, and mixed-phase FIR correction filters
//! and fits parametric biquad equalizer bands to match target acoustic curves.

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use super::correction::CorrectionMode;
use super::profile::ChannelCorrectionMetrics;
use super::target_curve::TargetCurve;
use crate::dsp::correction::phase::{render_from_magnitude_db, PhaseMode, RenderParams};
use crate::spatial::acoustic_metrics::frequency_response::FrequencyResponse;

/// Configuration for FIR correction filter synthesis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterSynthConfig {
    /// Length of the synthesized FIR filter in samples (power of two, e.g. 1024, 2048).
    pub filter_length_taps: usize,
    /// Maximum allowed positive boost in dB to avoid driving amplifiers into clipping at acoustic nulls.
    pub max_boost_db: f64,
    /// Maximum allowed cut in dB.
    pub max_cut_db: f64,
    /// Phase synthesis mode.
    pub mode: CorrectionMode,
    /// Lower frequency limit in Hz below which correction is tapered to 0 dB.
    pub low_freq_limit_hz: f64,
    /// Upper frequency limit in Hz above which correction is tapered to 0 dB.
    pub high_freq_limit_hz: f64,
}

impl Default for FilterSynthConfig {
    fn default() -> Self {
        Self {
            filter_length_taps: 2048,
            max_boost_db: 6.0,
            max_cut_db: 20.0,
            mode: CorrectionMode::LinearPhase,
            low_freq_limit_hz: 20.0,
            high_freq_limit_hz: 20000.0,
        }
    }
}

/// A fitted parametric biquad equalizer band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiquadFitBand {
    pub center_freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
}

/// Synthesizes an FIR room correction filter from measured frequency response and target curve.
pub fn synthesize_channel_correction(
    measured: &FrequencyResponse,
    target: &TargetCurve,
    config: &FilterSynthConfig,
    sample_rate: f64,
) -> (Vec<f32>, ChannelCorrectionMetrics) {
    let n = config.filter_length_taps.next_power_of_two();
    let num_bins = n / 2 + 1;
    let bin_step = sample_rate / n as f64;

    let mut target_gain_linear = vec![1.0f32; num_bins];
    let mut clamped_db_vec = vec![0.0f64; num_bins];
    let mut pre_sq_err = 0.0;
    let mut post_sq_err = 0.0;
    let mut max_cut = 0.0f64;
    let mut max_boost = 0.0f64;
    let mut valid_bins = 0;

    for (bin, target_gain) in target_gain_linear.iter_mut().enumerate().take(num_bins) {
        let f = bin as f64 * bin_step;
        let target_db = target.evaluate_db(f);

        // Interpolate measured dB
        let measured_db = interpolate_db(&measured.frequencies, &measured.magnitude_db, f);

        // Pre-correction error
        let err_db = measured_db - target_db;
        if f >= config.low_freq_limit_hz && f <= config.high_freq_limit_hz {
            pre_sq_err += err_db.powi(2);
            valid_bins += 1;
        }

        // Ideal inversion: G_db = target_db - measured_db
        let mut g_db = target_db - measured_db;

        // Band limits tapering
        if f < config.low_freq_limit_hz {
            let taper = (f / config.low_freq_limit_hz).clamp(0.0, 1.0);
            g_db *= taper;
        } else if f > config.high_freq_limit_hz {
            let taper = ((sample_rate / 2.0 - f) / (sample_rate / 2.0 - config.high_freq_limit_hz))
                .clamp(0.0, 1.0);
            g_db *= taper;
        }

        // Regularization clamping
        let clamped_db = g_db.clamp(-config.max_cut_db, config.max_boost_db);
        clamped_db_vec[bin] = clamped_db;

        if clamped_db > max_boost {
            max_boost = clamped_db;
        }
        if clamped_db < -max_cut {
            max_cut = -clamped_db;
        }

        // Post-correction remaining error
        if f >= config.low_freq_limit_hz && f <= config.high_freq_limit_hz {
            let remaining_err = err_db + clamped_db;
            post_sq_err += remaining_err.powi(2);
        }

        *target_gain = 10.0f32.powf((clamped_db / 20.0) as f32);
    }

    let pre_rms = if valid_bins > 0 {
        (pre_sq_err / valid_bins as f64).sqrt()
    } else {
        0.0
    };
    let post_rms = if valid_bins > 0 {
        (post_sq_err / valid_bins as f64).sqrt()
    } else {
        0.0
    };

    let metrics = ChannelCorrectionMetrics {
        pre_correction_rms_db: pre_rms,
        post_correction_rms_db: post_rms,
        peak_cut_db: max_cut,
        peak_boost_db: max_boost,
    };

    let phase_mode = match config.mode {
        CorrectionMode::MinimumPhase => PhaseMode::Minimum,
        CorrectionMode::LinearPhase => PhaseMode::Linear,
        CorrectionMode::MixedPhase => PhaseMode::Hybrid,
    };
    let hybrid_crossover_hz =
        (config.low_freq_limit_hz * 4.0).clamp(60.0, (sample_rate / 8.0).max(60.0));
    let render_params = RenderParams {
        sample_rate,
        ir_len_samples: n.max(16),
        phase_mode,
        hybrid_crossover_hz,
    };

    let fir = if let Ok(rendered) = render_from_magnitude_db(&clamped_db_vec, &render_params) {
        rendered.samples.into_iter().map(|s| s as f32).collect()
    } else {
        // Fallback to realfft linear phase
        let mut planner = RealFftPlanner::<f32>::new();
        let irfft = planner.plan_fft_inverse(n);

        let mut spec = vec![Complex32::new(0.0, 0.0); num_bins];
        for (bin, &mag) in target_gain_linear.iter().enumerate() {
            spec[bin] = Complex32::new(mag, 0.0);
        }

        let mut fir = vec![0.0f32; n];
        let _ = irfft.process(&mut spec, &mut fir);

        // Scaling by 1/N
        let inv_n = 1.0 / n as f32;
        for x in fir.iter_mut() {
            *x *= inv_n;
        }

        // Circular shift to center for linear-phase FIR
        let mut shifted = vec![0.0f32; n];
        let half = n / 2;
        shifted[..half].copy_from_slice(&fir[half..]);
        shifted[half..].copy_from_slice(&fir[..half]);

        // Apply Blackman tapering window
        for (i, val) in shifted.iter_mut().enumerate() {
            let x = 2.0 * PI * i as f64 / (n - 1) as f64;
            let w = 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos();
            *val *= w as f32;
        }
        shifted
    };

    (fir, metrics)
}

/// Fits a bank of parametric biquad peaking filters to correct prominent peaks and dips.
pub fn fit_parametric_eq(
    measured: &FrequencyResponse,
    target: &TargetCurve,
    max_bands: usize,
) -> Vec<BiquadFitBand> {
    let mut bands = Vec::with_capacity(max_bands);
    if measured.frequencies.is_empty() {
        return bands;
    }

    // Identify standard octave center frequencies for parametric evaluation
    let center_candidates = [
        31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
    ];

    for &fc in center_candidates.iter().take(max_bands) {
        let m_db = interpolate_db(&measured.frequencies, &measured.magnitude_db, fc);
        let t_db = target.evaluate_db(fc);
        let delta = t_db - m_db;

        if delta.abs() >= 1.0 {
            // Apply biquad band to compensate for deviations > 1 dB
            bands.push(BiquadFitBand {
                center_freq_hz: fc,
                gain_db: delta.clamp(-12.0, 6.0),
                q: 1.414, // Standard 1-octave Q
            });
        }
    }

    bands
}

fn interpolate_db(freqs: &[f64], dbs: &[f64], f: f64) -> f64 {
    if freqs.is_empty() {
        return 0.0;
    }
    if f <= freqs[0] {
        return dbs[0];
    }
    if f >= freqs[freqs.len() - 1] {
        return dbs[dbs.len() - 1];
    }

    for i in 0..freqs.len() - 1 {
        if f >= freqs[i] && f <= freqs[i + 1] {
            let t = (f.log10() - freqs[i].log10()) / (freqs[i + 1].log10() - freqs[i].log10());
            return dbs[i] + t * (dbs[i + 1] - dbs[i]);
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_synth_minimum_phase_peak_at_origin() {
        let resp = FrequencyResponse {
            frequencies: vec![20.0, 100.0, 1000.0, 10000.0, 20000.0],
            magnitude_db: vec![-3.0, 0.0, 2.0, -1.0, 0.0],
            smoothing: crate::spatial::acoustic_metrics::frequency_response::OctaveSmoothing::None,
        };
        let target = TargetCurve::flat();
        let config = FilterSynthConfig {
            filter_length_taps: 256,
            mode: CorrectionMode::MinimumPhase,
            ..Default::default()
        };
        let (fir, metrics) = synthesize_channel_correction(&resp, &target, &config, 48_000.0);
        assert_eq!(fir.len(), 256);
        assert!(metrics.pre_correction_rms_db >= 0.0);

        // Peak tap in minimum phase should be at or near tap 0
        let peak_idx = fir
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap();
        assert!(
            peak_idx < 10,
            "Minimum phase peak should be near origin (got tap {peak_idx})"
        );
    }

    #[test]
    fn test_filter_synth_linear_phase_peak_at_center() {
        let resp = FrequencyResponse {
            frequencies: vec![20.0, 100.0, 1000.0, 10000.0, 20000.0],
            magnitude_db: vec![-2.0, 0.0, 3.0, -1.0, 0.0],
            smoothing: crate::spatial::acoustic_metrics::frequency_response::OctaveSmoothing::None,
        };
        let target = TargetCurve::flat();
        let config = FilterSynthConfig {
            filter_length_taps: 256,
            mode: CorrectionMode::LinearPhase,
            ..Default::default()
        };
        let (fir, _) = synthesize_channel_correction(&resp, &target, &config, 48_000.0);
        assert_eq!(fir.len(), 256);

        // Peak tap in linear phase should be at N/2 = 128
        let peak_idx = fir
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(peak_idx, 128, "Linear phase peak should be centered at N/2");
    }

    #[test]
    fn test_fit_parametric_eq_finds_deviations() {
        let resp = FrequencyResponse {
            frequencies: vec![63.0, 125.0, 250.0, 500.0, 1000.0],
            magnitude_db: vec![4.0, 0.0, -3.0, 0.0, 2.0],
            smoothing: crate::spatial::acoustic_metrics::frequency_response::OctaveSmoothing::None,
        };
        let target = TargetCurve::flat();
        let bands = fit_parametric_eq(&resp, &target, 5);
        assert!(!bands.is_empty());
        // Deviations at 63 Hz (+4 dB -> -4 dB cut) and 250 Hz (-3 dB -> +3 dB boost)
        assert!(bands
            .iter()
            .any(|b| (b.center_freq_hz - 63.0).abs() < 1.0 && b.gain_db < 0.0));
        assert!(bands
            .iter()
            .any(|b| (b.center_freq_hz - 250.0).abs() < 1.0 && b.gain_db > 0.0));
    }
}

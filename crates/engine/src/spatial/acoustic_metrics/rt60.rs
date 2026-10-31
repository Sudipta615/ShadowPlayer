//! Reverberation time (RT60, T20, T30) estimation via backward Schroeder integration (§11.1, Item 34).
//!
//! Implements ISO 3382-1 backward integration energy decay curves and linear regression analysis.

use serde::{Deserialize, Serialize};

/// RT60 reverberation analysis metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rt60Metrics {
    /// Early T10 decay extrapolated to 60 dB (-5 dB to -15 dB, in seconds).
    pub t10_seconds: f64,
    /// Early T20 decay extrapolated to 60 dB (-5 dB to -25 dB, in seconds).
    pub t20_seconds: f64,
    /// Full T30 decay extrapolated to 60 dB (-5 dB to -35 dB, in seconds).
    pub t30_seconds: f64,
    /// Regression correlation coefficient ($R^2$) for T10.
    pub t10_r_squared: f64,
    /// Regression correlation coefficient ($R^2$) for T20.
    pub t20_r_squared: f64,
    /// Regression correlation coefficient ($R^2$) for T30.
    pub t30_r_squared: f64,
}

/// Truncation point estimation according to ISO 3382-1 (Lundeby method).
/// Estimates the background noise floor and returns the sample index where decay meets noise.
pub fn lundeby_truncation_point(ir: &[f32]) -> usize {
    let n = ir.len();
    if n < 200 {
        return n;
    }

    // Estimate noise floor from the last 10% of samples
    let tail_start = (n * 9) / 10;
    let tail_slice = &ir[tail_start..];
    let noise_energy: f64 = tail_slice.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let noise_floor_pwr = (noise_energy / tail_slice.len() as f64).max(1e-15);

    // Find direct sound peak
    let mut peak_pwr = 0.0f64;
    let mut peak_idx = 0;
    for (i, &s) in ir.iter().enumerate() {
        let p = (s as f64) * (s as f64);
        if p > peak_pwr {
            peak_pwr = p;
            peak_idx = i;
        }
    }

    if peak_pwr <= noise_floor_pwr * 10.0 {
        return n; // Insufficient dynamic range
    }

    // Block-average energy from peak to end (blocks of ~20-50 samples)
    let block_size = 32.min(n / 10).max(4);
    let mut curr = peak_idx;
    let threshold_pwr = noise_floor_pwr * 3.162; // ~5 dB above noise floor

    while curr + block_size <= n {
        let b_pwr: f64 = ir[curr..curr + block_size]
            .iter()
            .map(|&x| (x as f64) * (x as f64))
            .sum::<f64>()
            / block_size as f64;
        if b_pwr <= threshold_pwr {
            return curr;
        }
        curr += block_size;
    }

    n
}

/// Computes the backward Schroeder integration Energy Decay Curve (EDC) in decibels.
/// Uses Lundeby noise truncation to avoid noise floor curvature.
pub fn schroeder_decay_curve(ir: &[f32]) -> Vec<f64> {
    let n = ir.len();
    if n == 0 {
        return Vec::new();
    }

    let trunc_idx = lundeby_truncation_point(ir);
    let mut edc = vec![0.0f64; n];
    let mut accum = 0.0f64;

    // Backward cumulative sum of squared samples up to truncation point
    for i in (0..trunc_idx).rev() {
        let s = ir[i] as f64;
        accum += s * s;
        edc[i] = accum;
    }

    let total_energy = edc[0];
    if total_energy <= 1e-12 {
        return vec![-120.0; n];
    }

    let inv_total = 1.0 / total_energy;
    for val in edc[..trunc_idx].iter_mut() {
        let normalized = *val * inv_total;
        *val = if normalized > 1e-12 {
            10.0 * normalized.log10()
        } else {
            -120.0
        };
    }

    // Beyond truncation, fill with floor
    for val in edc[trunc_idx..].iter_mut() {
        *val = -120.0;
    }

    edc
}

/// Performs linear least-squares regression on (t, edc_db) within a dB evaluation range.
/// Returns (slope_db_per_sec, r_squared).
fn linear_regression(edc_db: &[f64], sample_rate: f64, db_start: f64, db_end: f64) -> (f64, f64) {
    let mut t_vals = Vec::new();
    let mut y_vals = Vec::new();

    for (i, &db) in edc_db.iter().enumerate() {
        if db <= db_start && db >= db_end {
            let t = i as f64 / sample_rate;
            t_vals.push(t);
            y_vals.push(db);
        }
    }

    let n = t_vals.len() as f64;
    if n < 2.0 {
        return (0.0, 0.0);
    }

    let sum_t: f64 = t_vals.iter().sum();
    let sum_y: f64 = y_vals.iter().sum();
    let sum_tt: f64 = t_vals.iter().map(|&t| t * t).sum();
    let sum_ty: f64 = t_vals.iter().zip(y_vals.iter()).map(|(&t, &y)| t * y).sum();

    let denom = n * sum_tt - sum_t * sum_t;
    if denom.abs() < 1e-12 {
        return (0.0, 0.0);
    }

    let slope = (n * sum_ty - sum_t * sum_y) / denom;
    let intercept = (sum_y - slope * sum_t) / n;

    // Calculate R-squared
    let mean_y = sum_y / n;
    let ss_tot: f64 = y_vals.iter().map(|&y| (y - mean_y).powi(2)).sum();
    let ss_res: f64 = t_vals
        .iter()
        .zip(y_vals.iter())
        .map(|(&t, &y)| (y - (slope * t + intercept)).powi(2))
        .sum();

    let r_squared = if ss_tot > 1e-12 {
        (1.0 - (ss_res / ss_tot)).clamp(0.0, 1.0)
    } else {
        1.0
    };

    (slope, r_squared)
}

/// Calculates T10, T20 and T30 reverberation time metrics from an impulse response.
pub fn compute_rt60(ir: &[f32], sample_rate: f64) -> Rt60Metrics {
    let edc = schroeder_decay_curve(ir);

    // T10: evaluated between -5 dB and -15 dB (extrapolated * 6)
    let (slope_10, r2_10) = linear_regression(&edc, sample_rate, -5.0, -15.0);
    let t10 = if slope_10 < -1e-6 {
        -60.0 / slope_10
    } else {
        0.0
    };

    // T20: evaluated between -5 dB and -25 dB (extrapolated * 3)
    let (slope_20, r2_20) = linear_regression(&edc, sample_rate, -5.0, -25.0);
    let t20 = if slope_20 < -1e-6 {
        -60.0 / slope_20
    } else {
        0.0
    };

    // T30: evaluated between -5 dB and -35 dB (extrapolated * 2)
    let (slope_30, r2_30) = linear_regression(&edc, sample_rate, -5.0, -35.0);
    let t30 = if slope_30 < -1e-6 {
        -60.0 / slope_30
    } else {
        0.0
    };

    Rt60Metrics {
        t10_seconds: t10.max(0.0),
        t20_seconds: t20.max(0.0),
        t30_seconds: t30.max(0.0),
        t10_r_squared: r2_10,
        t20_r_squared: r2_20,
        t30_r_squared: r2_30,
    }
}

/// Standard octave-band RT60 estimation for 125, 250, 500, 1000, 2000, 4000 Hz.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MultibandRt60 {
    pub bands_hz: [f32; 6],
    pub t30_seconds: [f32; 6],
}

/// Estimates RT60 across standard octave bands (125 Hz to 4 kHz) using 2nd-order bandpass filtering.
pub fn compute_multiband_rt60(ir: &[f32], sample_rate: f64) -> MultibandRt60 {
    use crate::dsp::biquad::{BiquadCoeffsF32, BiquadStateF32};

    let bands_hz = [125.0f32, 250.0, 500.0, 1000.0, 2000.0, 4000.0];
    let mut t30_seconds = [0.0f32; 6];
    let q = std::f32::consts::SQRT_2; // 1-octave bandwidth Q ≈ 1.414

    for (idx, &fc) in bands_hz.iter().enumerate() {
        if fc >= sample_rate as f32 * 0.45 {
            continue;
        }
        let coeffs = BiquadCoeffsF32::bandpass(sample_rate as f32, fc, q);
        let mut state = BiquadStateF32::default();
        let mut filtered_ir = Vec::with_capacity(ir.len());
        for &s in ir {
            filtered_ir.push(state.process(s, &coeffs));
        }
        let metrics = compute_rt60(&filtered_ir, sample_rate);
        t30_seconds[idx] = metrics.t30_seconds as f32;
    }

    MultibandRt60 {
        bands_hz,
        t30_seconds,
    }
}

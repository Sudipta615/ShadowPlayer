//! Log-spaced frequency response analysis and fractional octave smoothing (§11.1, Item 34).
//!
//! Provides magnitude spectrum calculation, logarithmic frequency binning, and
//! standard 1/1, 1/3, 1/6, 1/12, and 1/24 octave smoothing filters.

use realfft::num_complex::Complex32;
use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

/// Fractional octave smoothing bandwidth setting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OctaveSmoothing {
    None,
    Octave1_1,
    Octave1_3,
    Octave1_6,
    Octave1_12,
    Octave1_24,
    /// Psychoacoustic smoothing: 1/3 octave in low bass, transitioning to 1/6 octave above 1 kHz.
    Psychoacoustic,
    /// Equivalent Rectangular Bandwidth (ERB) matching human auditory critical bands (Moore & Glasberg).
    Erb,
}

impl OctaveSmoothing {
    /// Nominal fractional bandwidth $Q = 1 / N$ in octaves.
    pub fn bandwidth_octaves(&self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Octave1_1 => 1.0,
            Self::Octave1_3 => 1.0 / 3.0,
            Self::Octave1_6 => 1.0 / 6.0,
            Self::Octave1_12 => 1.0 / 12.0,
            Self::Octave1_24 => 1.0 / 24.0,
            Self::Psychoacoustic => 1.0 / 6.0,
            Self::Erb => 1.0 / 6.0,
        }
    }

    /// Frequency-dependent fractional bandwidth in octaves at `freq_hz`.
    pub fn bandwidth_octaves_at(&self, freq_hz: f64) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Octave1_1 => 1.0,
            Self::Octave1_3 => 1.0 / 3.0,
            Self::Octave1_6 => 1.0 / 6.0,
            Self::Octave1_12 => 1.0 / 12.0,
            Self::Octave1_24 => 1.0 / 24.0,
            Self::Psychoacoustic => {
                if freq_hz <= 100.0 {
                    1.0 / 3.0
                } else if freq_hz < 1000.0 {
                    let t = (freq_hz - 100.0) / 900.0;
                    (1.0 / 3.0) * (1.0 - t) + (1.0 / 6.0) * t
                } else {
                    1.0 / 6.0
                }
            }
            Self::Erb => {
                let f = freq_hz.max(20.0);
                // Moore & Glasberg (1983): ERB(f) = 24.7 * (4.37 * f / 1000 + 1)
                let erb = 24.7 * (4.37 * (f / 1000.0) + 1.0);
                (1.0 + erb / f).log2()
            }
        }
    }
}

/// Frequency response curve representation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrequencyResponse {
    pub frequencies: Vec<f64>,
    pub magnitude_db: Vec<f64>,
    pub smoothing: OctaveSmoothing,
}

/// Computes the raw FFT magnitude frequency response of an impulse response.
pub fn compute_magnitude_response(ir: &[f32], sample_rate: f64) -> FrequencyResponse {
    let mut fft_size = 1024;
    while fft_size < ir.len() {
        fft_size <<= 1;
    }

    let mut in_buf = vec![0.0f32; fft_size];
    in_buf[..ir.len()].copy_from_slice(ir);

    let num_bins = fft_size / 2 + 1;
    let mut planner = RealFftPlanner::<f32>::new();
    let rfft = planner.plan_fft_forward(fft_size);
    let mut spec = vec![Complex32::new(0.0, 0.0); num_bins];
    let _ = rfft.process(&mut in_buf, &mut spec);

    let bin_step = sample_rate / fft_size as f64;
    let mut frequencies = Vec::with_capacity(num_bins);
    let mut magnitude_db = Vec::with_capacity(num_bins);

    for (bin, s) in spec.iter().enumerate() {
        frequencies.push(bin as f64 * bin_step);
        let mag = (s.re * s.re + s.im * s.im).sqrt() as f64;
        let db = if mag > 1e-9 {
            20.0 * mag.log10()
        } else {
            -180.0
        };
        magnitude_db.push(db);
    }

    FrequencyResponse {
        frequencies,
        magnitude_db,
        smoothing: OctaveSmoothing::None,
    }
}

/// Applies Gaussian-weighted fractional octave smoothing to a frequency response curve.
#[allow(clippy::needless_range_loop)]
pub fn smooth_frequency_response(
    response: &FrequencyResponse,
    smoothing: OctaveSmoothing,
) -> FrequencyResponse {
    if smoothing == OctaveSmoothing::None || response.frequencies.len() < 2 {
        return response.clone();
    }

    let n = response.frequencies.len();
    let mut smoothed_db = Vec::with_capacity(n);
    let ln2 = std::f64::consts::LN_2;
    let sqrt_8_ln2 = (8.0 * ln2).sqrt();

    // Precompute linear power
    let linear_power: Vec<f64> = response
        .magnitude_db
        .iter()
        .map(|&db| 10.0f64.powf(db / 10.0))
        .collect();

    for (i, &fc) in response.frequencies.iter().enumerate() {
        if fc <= 1e-3 {
            smoothed_db.push(response.magnitude_db[i]);
            continue;
        }

        let oct_bw = smoothing.bandwidth_octaves_at(fc);
        if oct_bw <= 0.0 {
            smoothed_db.push(response.magnitude_db[i]);
            continue;
        }

        // Sigma for Gaussian kernel: FWHM = oct_bw => sigma = oct_bw / (2 * sqrt(2 * ln 2))
        let sigma = oct_bw / sqrt_8_ln2;
        let two_sigma_sq = 2.0 * sigma * sigma;

        // Truncate at 2.5 sigma for efficiency (>98.7% energy)
        let max_oct_dist = 2.5 * sigma;
        let f_low = fc * 2.0f64.powf(-max_oct_dist);
        let f_high = fc * 2.0f64.powf(max_oct_dist);

        // Find binary search bounds
        let start_idx = response
            .frequencies
            .partition_point(|&f| f < f_low)
            .min(n - 1);
        let end_idx = response
            .frequencies
            .partition_point(|&f| f <= f_high)
            .min(n);

        let mut sum_weighted_power = 0.0;
        let mut sum_weights = 0.0;

        for j in start_idx..end_idx {
            let f = response.frequencies[j];
            if f <= 1e-6 {
                continue;
            }
            let oct_dist = (f / fc).log2();
            let weight = (-oct_dist * oct_dist / two_sigma_sq).exp();
            sum_weighted_power += linear_power[j] * weight;
            sum_weights += weight;
        }

        if sum_weights > 1e-9 {
            let avg_power = sum_weighted_power / sum_weights;
            let avg_db = if avg_power > 1e-18 {
                10.0 * avg_power.log10()
            } else {
                -180.0
            };
            smoothed_db.push(avg_db);
        } else {
            smoothed_db.push(response.magnitude_db[i]);
        }
    }

    FrequencyResponse {
        frequencies: response.frequencies.clone(),
        magnitude_db: smoothed_db,
        smoothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirac_magnitude_is_flat() {
        let mut ir = vec![0.0f32; 1024];
        ir[0] = 1.0;
        let resp = compute_magnitude_response(&ir, 48_000.0);
        assert_eq!(resp.frequencies.len(), 513);
        for &db in &resp.magnitude_db {
            assert!((db - 0.0).abs() < 1e-4, "Dirac magnitude must be 0 dBFS");
        }
    }

    #[test]
    fn test_fractional_octave_smoothing_preserves_flat() {
        let mut ir = vec![0.0f32; 1024];
        ir[0] = 1.0;
        let raw = compute_magnitude_response(&ir, 48_000.0);
        for smoothing in [
            OctaveSmoothing::Octave1_1,
            OctaveSmoothing::Octave1_3,
            OctaveSmoothing::Octave1_6,
            OctaveSmoothing::Psychoacoustic,
            OctaveSmoothing::Erb,
        ] {
            let smoothed = smooth_frequency_response(&raw, smoothing);
            assert_eq!(smoothed.smoothing, smoothing);
            for &db in &smoothed.magnitude_db {
                assert!(
                    (db - 0.0).abs() < 1e-3,
                    "Flat response should remain flat after smoothing {smoothing:?}"
                );
            }
        }
    }

    #[test]
    fn test_psychoacoustic_smoothing_bandwidth_varies() {
        let oct_50 = OctaveSmoothing::Psychoacoustic.bandwidth_octaves_at(50.0);
        let oct_2000 = OctaveSmoothing::Psychoacoustic.bandwidth_octaves_at(2000.0);
        assert!((oct_50 - 1.0 / 3.0).abs() < 1e-6);
        assert!((oct_2000 - 1.0 / 6.0).abs() < 1e-6);
    }
}

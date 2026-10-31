//! Measurement-stimulus adapter over the canonical sweep kit (§11.1, Item 34).
//!
//! Sweep generation and the matched (Farina) inverse filter have exactly one
//! implementation in the engine: [`crate::dsp::correction::sweep`]
//! ([`EssSweep`], [`farina_inverse_filter`], [`deconvolve_direct`]). This
//! module keeps the analysis-facing, `f32`, configuration-driven API the
//! spatial metrics use, and delegates every sample of DSP to that kit — a
//! second chirp law or a second inverse-filter envelope would be a bug, not a
//! variant.
//!
//! What it adds: the serializable [`LogSweepConfig`] the measurement workflow
//! stores, the `f32` conversion boundary, and the "invalid configuration"
//! policy for an API that cannot return a `Result` (log once, return no
//! samples — never a silently wrong sweep).
//!
//! Capture-path note: [`crate::spatial::room_correction::measurement`] still
//! owns its own `SweepConfig` wrapper for the room-correction wizard; moving
//! that wrapper onto this kit is the remaining consolidation step.

use serde::{Deserialize, Serialize};

use crate::dsp::correction::sweep::{
    deconvolve_direct, farina_inverse_filter, EssConfig, EssSweep,
};

/// Configuration for logarithmic sine sweep generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogSweepConfig {
    /// Start frequency in Hz (typically 20.0 Hz).
    pub start_freq_hz: f64,
    /// Stop frequency in Hz (typically 20000.0 Hz or Nyquist).
    pub stop_freq_hz: f64,
    /// Duration of the sweep in seconds.
    pub duration_secs: f64,
    /// Sample rate in Hz.
    pub sample_rate: f64,
    /// Fade in/out duration in seconds to prevent boundary clicks.
    pub fade_duration_secs: f64,
}

impl Default for LogSweepConfig {
    fn default() -> Self {
        Self {
            start_freq_hz: 20.0,
            stop_freq_hz: 20000.0,
            duration_secs: 5.0,
            sample_rate: 48000.0,
            fade_duration_secs: 0.05,
        }
    }
}

impl LogSweepConfig {
    /// The canonical generator configuration this adapter describes.
    ///
    /// The analysis stimulus is a full-scale, un-emphasised sweep: the old
    /// local generator emitted `sin(φ)` with no amplitude scaling and no
    /// pre-emphasis, and the canonical generator reproduces exactly that when
    /// asked for `amplitude = 1.0`.
    pub fn ess_config(&self) -> EssConfig {
        EssConfig {
            sample_rate: self.sample_rate,
            duration_secs: self.duration_secs,
            f_start: self.start_freq_hz,
            f_end: self.stop_freq_hz,
            amplitude: 1.0,
            fade_secs: self.fade_duration_secs,
            pre_emphasis: false,
        }
    }
}

/// Generates a logarithmic sine sweep according to Farina's method.
///
/// Delegates to [`EssSweep::new`]. A configuration outside the canonical
/// generator's documented ranges yields no samples and one warning: this
/// signature cannot report an error, and a truncated or clamped sweep would
/// quietly invalidate every measurement made with it.
pub fn generate_log_sweep(config: &LogSweepConfig) -> Vec<f32> {
    match EssSweep::new(config.ess_config()) {
        Ok(sweep) => sweep.samples().iter().map(|&x| x as f32).collect(),
        Err(e) => {
            log::warn!("acoustic_metrics::generate_log_sweep: unusable config: {e}");
            Vec::new()
        }
    }
}

/// Generates the matched inverse filter for a logarithmic sine sweep.
///
/// Time-reverses the sweep and applies the 6 dB/octave (`1/f`) envelope so
/// that convolving the sweep with the inverse filter yields an ideal impulse.
/// Delegates to [`farina_inverse_filter`].
pub fn generate_inverse_filter(sweep: &[f32], config: &LogSweepConfig) -> Vec<f32> {
    if sweep.is_empty() {
        return Vec::new();
    }
    let sweep_f64: Vec<f64> = sweep.iter().map(|&x| x as f64).collect();
    farina_inverse_filter(&sweep_f64, &config.ess_config())
        .into_iter()
        .map(|x| x as f32)
        .collect()
}

/// Deconvolves a recorded response with the inverse filter to extract the
/// impulse response, normalized to a unit peak.
///
/// Delegates to [`deconvolve_direct`]. The frequency-domain path with
/// pre-delay/harmonic/noise reporting is
/// [`crate::dsp::correction::sweep::deconvolve`]; this one exists for short
/// captures and for the analysis metrics that only need the time-domain IR.
pub fn deconvolve_sweep(recording: &[f32], inverse_filter: &[f32]) -> Vec<f32> {
    if recording.is_empty() || inverse_filter.is_empty() {
        return Vec::new();
    }
    let recorded: Vec<f64> = recording.iter().map(|&x| x as f64).collect();
    let inverse: Vec<f64> = inverse_filter.iter().map(|&x| x as f64).collect();
    deconvolve_direct(&recorded, &inverse)
        .into_iter()
        .map(|x| x as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Short enough that the O(n·m) reference deconvolution is fast, and
    /// inside the canonical generator's documented ranges.
    fn short_config() -> LogSweepConfig {
        LogSweepConfig {
            start_freq_hz: 20.0,
            stop_freq_hz: 3_000.0,
            duration_secs: 0.25,
            sample_rate: 8_000.0,
            fade_duration_secs: 0.01,
        }
    }

    #[test]
    fn generation_is_the_canonical_sweep_converted_to_f32() {
        let config = short_config();
        let sweep = generate_log_sweep(&config);
        assert_eq!(sweep.len(), 2_000);

        let canonical = EssSweep::new(config.ess_config()).expect("canonical sweep");
        let expected: Vec<f32> = canonical.samples().iter().map(|&x| x as f32).collect();
        assert_eq!(sweep, expected, "the adapter must not reshape the sweep");

        let peak = sweep.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            (peak - 1.0).abs() < 0.05,
            "peak {peak} should be full scale"
        );
    }

    #[test]
    fn inverse_filter_matches_the_sweep_length() {
        let config = short_config();
        let sweep = generate_log_sweep(&config);
        let inverse = generate_inverse_filter(&sweep, &config);
        assert_eq!(inverse.len(), sweep.len());
        assert!(inverse.iter().any(|x| x.abs() > 0.0));
    }

    #[test]
    fn deconvolution_returns_a_unit_peak_reference_ir() {
        let config = short_config();
        let sweep = generate_log_sweep(&config);
        let inverse = generate_inverse_filter(&sweep, &config);
        let ir = deconvolve_sweep(&sweep, &inverse);
        assert_eq!(ir.len(), 2 * sweep.len() - 1);

        let peak = ir.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 1.0).abs() < 1e-4, "peak {peak}");
    }

    #[test]
    fn an_unusable_config_yields_no_samples_instead_of_a_wrong_sweep() {
        let mut config = short_config();
        // Narrower than the two octaves the canonical generator requires.
        config.stop_freq_hz = 40.0;
        assert!(generate_log_sweep(&config).is_empty());
    }

    #[test]
    fn empty_inputs_stay_empty() {
        let config = short_config();
        assert!(generate_inverse_filter(&[], &config).is_empty());
        assert!(deconvolve_sweep(&[], &[1.0]).is_empty());
        assert!(deconvolve_sweep(&[1.0], &[]).is_empty());
    }
}

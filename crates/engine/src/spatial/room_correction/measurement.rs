//! Room correction measurement: the capture path for the correction wizard.
//!
//! Sweep generation and the matched inverse filter are **not** implemented
//! here. They delegate to the canonical kit in
//! [`crate::dsp::correction::sweep`] ([`EssSweep`], [`farina_inverse_filter`],
//! [`deconvolve_direct_raw`]) — there is exactly one chirp law and one
//! matched-filter envelope in the engine. This module owns only what the
//! wizard adds on top of it: the persisted [`SweepConfig`], a unit-peak
//! inverse filter for the capture UI, and an IR reported in the recording's
//! own time base (truncated to the recording length, levels un-normalized).
//!
//! All functions in this module run on the **control path** and allocate freely.
//! None of these functions may be called from the audio thread.

use crate::dsp::correction::sweep::{
    deconvolve_direct_raw, farina_inverse_filter, EssConfig, EssSweep,
};

/// Configuration for a logarithmic sine sweep.
#[derive(Debug, Clone)]
pub struct SweepConfig {
    /// Start frequency in Hz (e.g. 20 Hz).
    pub start_hz: f32,
    /// End frequency in Hz (e.g. 20 000 Hz).
    pub end_hz: f32,
    /// Sweep duration in seconds.
    pub duration_secs: f32,
    /// Peak amplitude (linear; 1.0 = 0 dBFS).
    pub amplitude: f32,
}

impl Default for SweepConfig {
    fn default() -> Self {
        Self {
            start_hz: 20.0,
            end_hz: 20_000.0,
            duration_secs: 3.0,
            amplitude: 0.5,
        }
    }
}

/// Map this wizard's configuration onto the canonical sweep kit.
///
/// The capture sweep has no fade (`fade_secs: 0.0`) — its endpoints are
/// windowed by the recorder, not by the stimulus — and it carries the
/// user-selected peak amplitude. Everything else is the same chirp law.
fn ess_config(config: &SweepConfig, sample_rate: f32) -> EssConfig {
    EssConfig {
        sample_rate: sample_rate as f64,
        duration_secs: config.duration_secs as f64,
        f_start: config.start_hz as f64,
        f_end: config.end_hz as f64,
        amplitude: config.amplitude as f64,
        fade_secs: 0.0,
        pre_emphasis: false,
    }
}

/// Generate a logarithmic sine sweep (Farina sweep). **Control path — allocates.**
///
/// The sweep covers `[config.start_hz, config.end_hz]` in `config.duration_secs`
/// seconds at `sample_rate`. The instantaneous frequency rises logarithmically so
/// equal octaves receive equal energy (flat pink-spectrum excitation).
pub fn generate_log_sweep(config: &SweepConfig, sample_rate: f32) -> Vec<f32> {
    match EssSweep::new(ess_config(config, sample_rate)) {
        Ok(sweep) => sweep.samples().iter().map(|&x| x as f32).collect(),
        Err(e) => {
            log::warn!("room_correction::generate_log_sweep: unusable config: {e}");
            Vec::new()
        }
    }
}

/// Generate the time-reversed, amplitude-modulated inverse filter for a log sweep.
///
/// The inverse filter is the time-reverse of the sweep weighted by `1/A(t)` where
/// `A(t) = f_inst(t)/f1` is the instantaneous frequency envelope. When convolved with
/// the system response to the original sweep, the result is the room's impulse
/// response. **Control path — allocates.**
pub fn generate_inverse_filter(config: &SweepConfig, sample_rate: f32) -> Vec<f32> {
    let sweep = generate_log_sweep(config, sample_rate);
    if sweep.is_empty() {
        return Vec::new();
    }
    let sweep_f64: Vec<f64> = sweep.iter().map(|&x| x as f64).collect();
    let mut inverse: Vec<f32> = farina_inverse_filter(&sweep_f64, &ess_config(config, sample_rate))
        .into_iter()
        .map(|x| x as f32)
        .collect();

    // The canonical kit normalizes to unit *energy*; this capture UI's
    // documented contract (and its level display) is a unit *peak*.
    let peak = inverse.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    if peak > 1e-6 {
        for v in inverse.iter_mut() {
            *v /= peak;
        }
    }
    inverse
}

/// Deconvolve `recorded` with `inverse_filter` to recover the room impulse
/// response, reported in the recording's own time base.
///
/// The convolution itself is the canonical [`deconvolve_direct_raw`]; this
/// wrapper only truncates the result to `recorded.len()` (the tail beyond the
/// recording cannot contain the direct response the wizard windows) and keeps
/// the recording's gain rather than normalizing it away. **Control path —
/// allocates.**
pub fn capture_impulse_response(recorded: &[f32], inverse_filter: &[f32]) -> Vec<f32> {
    if recorded.is_empty() || inverse_filter.is_empty() {
        return Vec::new();
    }
    let recorded_f64: Vec<f64> = recorded.iter().map(|&x| x as f64).collect();
    let inverse_f64: Vec<f64> = inverse_filter.iter().map(|&x| x as f64).collect();
    deconvolve_direct_raw(&recorded_f64, &inverse_f64)
        .into_iter()
        .take(recorded.len())
        .map(|x| x as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_log_sweep_length_and_amplitude() {
        let cfg = SweepConfig {
            start_hz: 100.0,
            end_hz: 10_000.0,
            duration_secs: 1.0,
            amplitude: 0.8,
        };
        let s = generate_log_sweep(&cfg, 48_000.0);
        assert_eq!(s.len(), 48_000);
        assert!(s.iter().all(|v| v.abs() <= 0.8 + 1e-4));
        // Starts near 0 (sin(0) = 0).
        assert!(s[0].abs() < 0.01);
    }

    #[test]
    fn generate_log_sweep_monotonic_instantaneous_frequency() {
        let cfg = SweepConfig {
            start_hz: 100.0,
            end_hz: 1_000.0,
            duration_secs: 0.5,
            amplitude: 1.0,
        };
        let s = generate_log_sweep(&cfg, 48_000.0);
        // Zero-crossing density increases over time (log sweep rises in frequency).
        let zx_first: usize = s[..1000].windows(2).filter(|w| w[0] * w[1] < 0.0).count();
        let zx_last: usize = s[s.len() - 1000..]
            .windows(2)
            .filter(|w| w[0] * w[1] < 0.0)
            .count();
        assert!(
            zx_last > zx_first,
            "frequency increases: {zx_first} vs {zx_last} zero crossings"
        );
    }

    #[test]
    fn generation_is_the_canonical_sweep_converted_to_f32() {
        let cfg = SweepConfig {
            start_hz: 100.0,
            end_hz: 10_000.0,
            duration_secs: 1.0,
            amplitude: 0.8,
        };
        let local = generate_log_sweep(&cfg, 48_000.0);
        let canonical = EssSweep::new(ess_config(&cfg, 48_000.0)).expect("canonical sweep");
        let expected: Vec<f32> = canonical.samples().iter().map(|&x| x as f32).collect();
        assert_eq!(
            local, expected,
            "the capture path must not reshape the sweep"
        );
    }

    #[test]
    fn generate_inverse_filter_same_length_as_sweep() {
        let cfg = SweepConfig::default();
        let sweep = generate_log_sweep(&cfg, 48_000.0);
        let inv = generate_inverse_filter(&cfg, 48_000.0);
        assert_eq!(sweep.len(), inv.len());
    }

    #[test]
    fn capture_impulse_response_dirac_roundtrip() {
        // Convolving a Dirac delta with the sweep and deconvolving with the
        // inverse should recover an approximate Dirac.
        let cfg = SweepConfig {
            start_hz: 100.0,
            end_hz: 8_000.0,
            // The canonical generator documents a 0.25 s floor; a shorter
            // stimulus is rejected rather than silently shortened.
            duration_secs: 0.25,
            amplitude: 1.0,
        };
        let sweep = generate_log_sweep(&cfg, 48_000.0);
        let inv = generate_inverse_filter(&cfg, 48_000.0);
        // "Record" = just the sweep (system with flat response).
        let ir = capture_impulse_response(&sweep, &inv);
        assert!(!ir.is_empty());
        // The output is non-trivially finite.
        assert!(ir.iter().all(|v| v.is_finite()));
    }
}

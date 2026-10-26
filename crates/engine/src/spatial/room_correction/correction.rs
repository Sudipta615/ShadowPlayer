//! Room correction filter computation and realtime processor.
//!
//! Control-path: [`compute_correction_filter`] derives a correction FIR from a
//! [`RoomIrAnalysis`] and a target curve — all computation is on the control path
//! and allocates freely.
//!
//! Realtime-safe: [`RoomCorrectionProcessor`] wraps the engine's
//! [`ConvolutionEngine`] and applies the correction filter with zero allocation
//! during `process_block`.
#![allow(clippy::needless_range_loop)]

use super::analysis::{RoomCorrectionTarget, RoomIrAnalysis};
use crate::dsp::convolution::{ConvolutionEngine, ConvolutionError};
use crate::dsp::correction::phase::{PhaseMode, RenderParams};

use serde::{Deserialize, Serialize};

/// Correction filter phase mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CorrectionMode {
    /// Minimum-phase filter: minimum latency, cannot correct group delay.
    MinimumPhase,
    /// Linear-phase FIR: corrects both magnitude and group delay; adds
    /// `latency_samples / 2` latency.
    #[default]
    LinearPhase,
    /// Mixed-phase: magnitude correction is minimum-phase; group delay is
    /// corrected separately with a linear-phase component.
    MixedPhase,
}

/// A computed room correction FIR filter.
#[derive(Debug, Clone)]
pub struct RoomCorrectionFilter {
    /// The FIR impulse response (mono). Apply this to each channel independently.
    pub ir: Vec<f32>,
    /// Phase mode used to compute this filter.
    pub mode: CorrectionMode,
    /// Latency in samples introduced by this filter (typically 0 for
    /// `MinimumPhase`, `ir.len() / 2` for `LinearPhase`).
    pub latency_samples: usize,
}

/// Compute a room correction FIR filter. **Control path — allocates.**
///
/// Derives the inverse filter from `analysis` to match `target`, constrained to
/// `max_gain_db` boost and `max_latency_samples` latency. Returns a
/// [`RoomCorrectionFilter`] ready for loading into a [`RoomCorrectionProcessor`].
///
/// The current implementation uses a regularized inverse spectrum approach for
/// `LinearPhase` mode (the most common setting). The `MinimumPhase` and
/// `MixedPhase` modes produce the minimum-phase equivalent via cepstrum liftering.
pub fn compute_correction_filter(
    analysis: &RoomIrAnalysis,
    target: &RoomCorrectionTarget,
    mode: CorrectionMode,
    max_gain_db: f32,
    max_latency_samples: usize,
) -> RoomCorrectionFilter {
    if analysis.freq_response.is_empty() {
        return RoomCorrectionFilter {
            ir: vec![1.0], // passthrough
            mode,
            latency_samples: 0,
        };
    }

    let n_freqs = analysis.freq_response.len();
    let max_gain_lin = 10.0f32.powf(max_gain_db / 20.0);

    // Build the target magnitude at each analysis frequency.
    let target_at_freq = |freq_hz: f32| -> f32 {
        match target {
            RoomCorrectionTarget::Flat => 1.0,
            RoomCorrectionTarget::Custom(pts) => {
                // Linear interpolation on the dB curve.
                if pts.is_empty() {
                    return 1.0;
                }
                let db = if freq_hz <= pts[0].0 {
                    pts[0].1
                } else if freq_hz >= pts[pts.len() - 1].0 {
                    pts[pts.len() - 1].1
                } else {
                    let i = pts.partition_point(|p| p.0 < freq_hz).saturating_sub(1);
                    let (f0, d0) = pts[i];
                    let (f1, d1) = pts[i + 1];
                    let t = (freq_hz - f0) / (f1 - f0);
                    d0 + t * (d1 - d0)
                };
                10.0f32.powf(db / 20.0)
            }
            RoomCorrectionTarget::SpeakerCurve => {
                // Gentle high-shelf (+3 dB/decade above 1 kHz) typical for loudspeakers.
                if freq_hz > 1000.0 {
                    (freq_hz / 1000.0).log10() * 1.5 + 1.0
                } else {
                    1.0
                }
            }
            RoomCorrectionTarget::HeadphoneCurve => {
                // Diffuse-field EQ: gentle lift from 2–10 kHz.
                if (2000.0..=10_000.0).contains(&freq_hz) {
                    1.3
                } else {
                    1.0
                }
            }
        }
    };

    // Compute per-frequency correction gains (target / measured), clamped.
    let correction_gains: Vec<f32> = analysis
        .freq_response
        .iter()
        .map(|&(f, mag)| {
            let t = target_at_freq(f);
            if mag < 1e-6 {
                1.0 // no correction where measurement is effectively zero
            } else {
                (t / mag).clamp(1.0 / max_gain_lin, max_gain_lin)
            }
        })
        .collect();

    // Determine filter length from latency budget (power of two).
    let n = (max_latency_samples * 2)
        .clamp(16, 32768)
        .next_power_of_two();
    let num_bins = n / 2 + 1;
    let nyquist = analysis
        .freq_response
        .last()
        .map(|&(f, _)| f)
        .unwrap_or(24_000.0)
        .max(1000.0);
    let sample_rate = (nyquist * 2.0) as f64;
    let bin_step = nyquist / (num_bins - 1) as f32;

    let mut mag_db = vec![0.0f64; num_bins];
    for (j, db) in mag_db.iter_mut().enumerate() {
        let f = (j as f32 * bin_step).max(1.0);
        let gain = if f <= analysis.freq_response[0].0 {
            correction_gains[0]
        } else if f >= analysis.freq_response[n_freqs - 1].0 {
            correction_gains[n_freqs - 1]
        } else {
            let i = analysis
                .freq_response
                .partition_point(|p| p.0 < f)
                .saturating_sub(1);
            let f0 = analysis.freq_response[i].0;
            let f1 = analysis.freq_response[(i + 1).min(n_freqs - 1)].0;
            let g0 = correction_gains[i];
            let g1 = correction_gains[(i + 1).min(n_freqs - 1)];
            if (f1 - f0).abs() < 1e-4 {
                g0
            } else {
                let t = (f.log10() - f0.log10()) / (f1.log10() - f0.log10());
                g0 + t * (g1 - g0)
            }
        };
        *db = (gain.max(1e-12).log10() * 20.0) as f64;
    }

    let phase_mode = match mode {
        CorrectionMode::MinimumPhase => PhaseMode::Minimum,
        CorrectionMode::LinearPhase => PhaseMode::Linear,
        CorrectionMode::MixedPhase => PhaseMode::Hybrid,
    };
    let render_params = RenderParams {
        sample_rate,
        ir_len_samples: n,
        phase_mode,
        hybrid_crossover_hz: 300.0,
    };

    let (ir, latency_samples) = if let Ok(rendered) =
        crate::dsp::correction::phase::render_from_magnitude_db(&mag_db, &render_params)
    {
        let samples = rendered.samples.into_iter().map(|s| s as f32).collect();
        let latency = match mode {
            CorrectionMode::MinimumPhase => 0,
            CorrectionMode::LinearPhase => n / 2,
            CorrectionMode::MixedPhase => rendered.delay_samples.round() as usize,
        };
        (samples, latency)
    } else {
        // Fallback: simple Dirac with gain
        let mut fallback = vec![0.0f32; n];
        let half = match mode {
            CorrectionMode::MinimumPhase => 0,
            _ => n / 2,
        };
        fallback[half] = 1.0;
        (fallback, half)
    };

    RoomCorrectionFilter {
        ir,
        mode,
        latency_samples,
    }
}

/// Realtime-safe room correction processor.
///
/// Wraps `dsp::convolution::ConvolutionEngine`, which applies the
/// pre-computed impulse response using partitioned overlap-add convolution.
///
/// Pre-allocates all FFT buffers at [`prepare`](Self::prepare) time on the
/// control thread. The audio callback `process_block`
/// is strictly allocation-free and lock-free.
pub struct RoomCorrectionProcessor {
    engine: ConvolutionEngine,
    prepared: bool,
    latency_samples: usize,
}

impl std::fmt::Debug for RoomCorrectionProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomCorrectionProcessor")
            .field("prepared", &self.prepared)
            .field("latency_samples", &self.latency_samples)
            .finish()
    }
}

impl RoomCorrectionProcessor {
    /// Create a new (unprepared) processor.
    pub fn new() -> Self {
        Self {
            engine: ConvolutionEngine::new(48000.0, 65536),
            prepared: false,
            latency_samples: 0,
        }
    }

    /// Load a [`RoomCorrectionFilter`] and prepare the convolution engine.
    /// **Control path — may allocate.**
    pub fn prepare(&mut self, filter: &RoomCorrectionFilter) -> Result<(), ConvolutionError> {
        let samples: Vec<(f32, f32)> = filter.ir.iter().map(|&v| (v, v)).collect();
        self.engine.load_ir_from_samples(&samples)?;
        self.latency_samples = filter.latency_samples;
        self.prepared = true;
        Ok(())
    }

    /// True when the engine is ready to process audio.
    pub fn prepared(&self) -> bool {
        self.prepared
    }

    /// Latency introduced by the loaded filter in samples.
    pub fn latency_samples(&self) -> usize {
        self.latency_samples
    }

    /// Process a stereo interleaved block (`L R L R …`) in-place.
    ///
    /// Does nothing when not prepared. **Realtime-safe — zero allocation.**
    pub fn process_stereo_interleaved(&mut self, buf: &mut [f32], block_size: usize) {
        if !self.prepared || buf.len() < block_size * 2 {
            return;
        }
        // De-interleave to scratch (stack, not heap — block_size bounded by 1024).
        let mut left = [0.0f32; 1024];
        let mut right = [0.0f32; 1024];
        let n = block_size.min(1024);
        for i in 0..n {
            left[i] = buf[i * 2];
            right[i] = buf[i * 2 + 1];
        }
        self.engine.process_block(&mut left[..n], &mut right[..n]);
        for i in 0..n {
            buf[i * 2] = left[i];
            buf[i * 2 + 1] = right[i];
        }
    }
}

impl Default for RoomCorrectionProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial::room_correction::analysis::analyze_ir;

    #[test]
    fn compute_correction_filter_flat_target_returns_approx_dirac() {
        // For a Dirac IR (flat frequency response), the correction should be
        // approximately a Dirac too (no boost or cut needed).
        let mut ir = vec![0.0f32; 512];
        ir[0] = 1.0;
        let analysis = analyze_ir(&ir, 48_000.0);
        let filter = compute_correction_filter(
            &analysis,
            &RoomCorrectionTarget::Flat,
            CorrectionMode::LinearPhase,
            12.0,
            256,
        );
        assert!(!filter.ir.is_empty());
        // The correction gain at each frequency for a flat response should be
        // near 1.0, so the FIR is approximately a Dirac.
        let peak = filter.ir.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.1, "non-trivial filter: {peak}");
        assert_eq!(filter.latency_samples, filter.ir.len() / 2);
    }

    #[test]
    fn correction_processor_prepared_after_load() {
        let mut ir_data = vec![0.0f32; 64];
        ir_data[0] = 1.0;
        let analysis = analyze_ir(&ir_data, 48_000.0);
        let filter = compute_correction_filter(
            &analysis,
            &RoomCorrectionTarget::Flat,
            CorrectionMode::MinimumPhase,
            6.0,
            32,
        );
        let mut proc = RoomCorrectionProcessor::new();
        assert!(!proc.prepared());
        proc.prepare(&filter).expect("prepare succeeds");
        assert!(proc.prepared());
    }

    #[test]
    fn room_correction_processor_allocates_zero_bytes_stub() {
        // Stub test — the realtime-allocation fidelity test covers this
        // end-to-end. Here we just confirm the processor stays silent when unprepared.
        let mut proc = RoomCorrectionProcessor::new();
        let mut buf = vec![0.5f32; 256];
        proc.process_stereo_interleaved(&mut buf, 128);
        // Unprepared → buffer untouched.
        assert!(buf.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn test_compute_correction_filter_minimum_phase_has_zero_latency() {
        let mut ir_data = vec![0.0f32; 128];
        ir_data[0] = 1.0;
        let analysis = analyze_ir(&ir_data, 48_000.0);
        let filter = compute_correction_filter(
            &analysis,
            &RoomCorrectionTarget::Flat,
            CorrectionMode::MinimumPhase,
            6.0,
            64,
        );
        assert_eq!(filter.latency_samples, 0);
        assert!(!filter.ir.is_empty());
        // Peak of minimum phase should be at tap 0
        let peak_idx = filter
            .ir
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(peak_idx, 0);
    }
}

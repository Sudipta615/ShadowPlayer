//! PCM→SDM (delta-sigma) modulation and native-DSD packing.
//!
//! This module is the real modulator behind the "convert PCM to DSD" output
//! mode. It is deliberately independent of the engine's transport code so it
//! can also be exercised offline (tests, benchmarks, export tooling).
//!
//! The chain is:
//!
//! ```text
//! PCM (working rate) → polyphase interpolation (stage_factor)
//!                     → one-bit CRFB modulator (5th/7th order)
//!                     → per-channel bit accumulator
//!                     → NativeDsdPacker (U8/U16/U32, negotiated endianness)
//! ```
//!
//! The coefficient tables, the 50 % modulation-depth limit, and the rate plan
//! are documented in the submodules. Every numeric claim used in the engine
//! README comes from the tests in this module, which measure the generated
//! bitstream — none of the figures are asserted from theory.

pub mod interpolator;
pub mod modulator;
pub mod ntf;
pub mod output;
pub mod rates;

pub use modulator::SdmModulator;
pub use output::{SdmBlock, SdmOutput, SdmTelemetry, DEFAULT_TAPS};
pub use rates::{candidates, clock_family, SdmRatePlan};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::dsd::DsdWireFormat;

    const DSD64: f64 = 2_822_400.0;
    const WORKING: u32 = 352_800;

    #[derive(Clone, Copy)]
    struct Biquad {
        b0: f64,
        b1: f64,
        b2: f64,
        a1: f64,
        a2: f64,
        z1: f64,
        z2: f64,
    }

    impl Biquad {
        fn lowpass(sr: f64, f: f64, q: f64) -> Self {
            let w0 = 2.0 * std::f64::consts::PI * f / sr;
            let (sin, cos) = w0.sin_cos();
            let alpha = sin / (2.0 * q);
            let a0 = 1.0 + alpha;
            Self {
                b0: (1.0 - cos) / 2.0 / a0,
                b1: (1.0 - cos) / a0,
                b2: (1.0 - cos) / 2.0 / a0,
                a1: -2.0 * cos / a0,
                a2: (1.0 - alpha) / a0,
                z1: 0.0,
                z2: 0.0,
            }
        }

        fn process(&mut self, x: f64) -> f64 {
            let y = self.b0 * x + self.z1;
            self.z1 = self.b1 * x - self.a1 * y + self.z2;
            self.z2 = self.b2 * x - self.a2 * y;
            y
        }
    }

    /// 16th-order Butterworth band-limiter at 20 kHz, used only to measure
    /// the in-band noise of the generated bitstream.
    struct BandLimiter {
        sections: Vec<Biquad>,
    }

    impl BandLimiter {
        fn new(sr: f64, cutoff: f64) -> Self {
            const ORDER: usize = 16;
            let sections = (1..=ORDER / 2)
                .map(|k| {
                    let q = 1.0
                        / (2.0
                            * ((2 * k - 1) as f64 * std::f64::consts::PI / (2.0 * ORDER as f64))
                                .sin());
                    Biquad::lowpass(sr, cutoff, q)
                })
                .collect();
            Self { sections }
        }

        fn process(&mut self, x: f64) -> f64 {
            self.sections
                .iter_mut()
                .fold(x, |acc, section| section.process(acc))
        }
    }

    fn dbfs(rms: f64) -> f64 {
        20.0 * rms.max(1e-18).log10()
    }

    fn in_band_rms(bits: &[f64], sr: f64) -> f64 {
        let mut filter = BandLimiter::new(sr, 20_000.0);
        let filtered: Vec<f64> = bits.iter().map(|&b| filter.process(b)).collect();
        let skip = 10_000.min(filtered.len());
        let slice = &filtered[skip..];
        let mean = slice.iter().sum::<f64>() / slice.len() as f64;
        (slice.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / slice.len() as f64).sqrt()
    }

    #[test]
    fn measured_idle_noise_floor() {
        // The engine README quotes these measured numbers.
        for (order, limit_db) in [(5usize, -110.0f64), (7, -125.0)] {
            let mut m = SdmModulator::new(order, 64).expect("modulator");
            let mut bits = Vec::with_capacity(180_000);
            for _ in 0..180_000 {
                bits.push(m.process_sample(0.0));
            }
            let rms = in_band_rms(&bits, DSD64);
            let level = dbfs(rms);
            assert!(
                level < limit_db,
                "order {order}: idle noise {level:.1} dBFS (limit {limit_db})"
            );
            assert_eq!(m.state_recoveries(), 0);
        }
    }

    #[test]
    fn measured_signal_transfer_is_linear() {
        // The 50 % modulation depth means a digital amplitude A becomes an
        // analog RMS of A/2/sqrt(2). Check three levels.
        for amplitude in [0.1, 0.25, 0.5] {
            let mut m = SdmModulator::new(5, 64).expect("modulator");
            let n = 180_000;
            let mut bits = Vec::with_capacity(n);
            for i in 0..n {
                let x = amplitude * (2.0 * std::f64::consts::PI * 1_000.0 * i as f64 / DSD64).sin();
                bits.push(m.process_sample(x));
            }
            let rms = in_band_rms(&bits, DSD64);
            let expected = amplitude * 0.5 / std::f64::consts::SQRT_2;
            let error_db = 20.0 * (rms / expected).log10();
            assert!(
                error_db.abs() < 0.5,
                "amp {amplitude}: rms {rms:.6} expected {expected:.6} ({error_db:.2} dB)"
            );
        }
    }

    #[test]
    fn measured_idle_noise_improves_with_rate() {
        let mut previous = 0.0;
        for multiple in [64, 128, 256, 512] {
            let rate = 44_100.0 * multiple as f64;
            let mut m = SdmModulator::new(5, multiple).expect("modulator");
            let mut bits = Vec::with_capacity(180_000);
            for _ in 0..180_000 {
                bits.push(m.process_sample(0.0));
            }
            let level = dbfs(in_band_rms(&bits, rate));
            if previous != 0.0 {
                assert!(
                    level < previous + 1.0,
                    "DSD{multiple}: {level:.1} dBFS vs previous {previous:.1}"
                );
            }
            previous = level;
        }
    }

    #[test]
    fn block_stage_packs_whole_words() {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: WORKING,
            dsd_rate: 2_822_400,
            multiple: 64,
            stage_factor: 8,
            family: 44_100,
        };
        for (wire, word_bits) in [
            (DsdWireFormat::U8, 8usize),
            (DsdWireFormat::U16Le, 16),
            (DsdWireFormat::U32Le, 32),
        ] {
            let mut stage = SdmOutput::new(plan, 2, wire, 5).expect("stage");
            let frames = 100;
            let input: Vec<f32> = (0..frames * 2)
                .map(|i| if i % 2 == 0 { 0.25 } else { -0.25 })
                .collect();
            let mut out = Vec::new();
            let block = stage.process_block(&input, &mut out);
            let expected_words = frames * 8 / word_bits;
            assert_eq!(block.input_frames, frames);
            assert_eq!(block.words, expected_words, "{wire:?}");
            assert_eq!(
                block.output_bytes,
                expected_words * 2 * wire.bytes_per_word(),
                "{wire:?}"
            );
            assert_eq!(out.len(), block.output_bytes);
            assert_eq!(stage.bytes_per_input_frame(), 2);
        }
    }

    #[test]
    fn block_stage_accumulates_partial_words_across_blocks() {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: WORKING,
            dsd_rate: 2_822_400,
            multiple: 64,
            stage_factor: 8,
            family: 44_100,
        };
        let mut stage = SdmOutput::new(plan, 1, DsdWireFormat::U16Le, 5).expect("stage");
        let mut out = Vec::new();
        // One input frame = 8 bits; a U16 word needs two frames.
        let first = stage.process_block(&[0.0], &mut out);
        assert_eq!(first.words, 0);
        assert_eq!(first.output_bytes, 0);
        let second = stage.process_block(&[0.0], &mut out);
        assert_eq!(second.words, 1);
        assert_eq!(second.output_bytes, 2);
    }

    #[test]
    fn reset_clears_the_accumulator() {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: WORKING,
            dsd_rate: 2_822_400,
            multiple: 64,
            stage_factor: 8,
            family: 44_100,
        };
        let mut stage = SdmOutput::new(plan, 1, DsdWireFormat::U16Le, 5).expect("stage");
        let mut out = Vec::new();
        stage.process_block(&[0.0], &mut out);
        stage.reset();
        // After reset the first block is again half a word.
        let block = stage.process_block(&[0.0], &mut out);
        assert_eq!(block.words, 0);
    }

    #[test]
    fn block_stage_reports_telemetry() {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: WORKING,
            dsd_rate: 2_822_400,
            multiple: 64,
            stage_factor: 8,
            family: 44_100,
        };
        let mut stage = SdmOutput::new(plan, 2, DsdWireFormat::U8, 5).expect("stage");
        let input = vec![1.5f32; 64 * 2];
        let mut out = Vec::new();
        stage.process_block(&input, &mut out);
        let t = stage.telemetry();
        assert_eq!(t.input_frames, 64);
        assert!(t.clips > 0);
        // The measurement is taken after interpolation, so it includes the
        // filter's step overshoot; it can only be at least the input level.
        assert!(t.input_peak >= 1.5 - 1e-6, "peak {}", t.input_peak);
        assert_eq!(t.state_recoveries, 0);
        assert!((t.modulation_depth - 0.5).abs() < 1e-12);
        assert!(t.interpolator_delay_ms > 0.0);
    }
}

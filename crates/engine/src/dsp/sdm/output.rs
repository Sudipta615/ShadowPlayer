//! PCM→SDM output stage: interpolate, modulate, pack.
//!
//! This is the last stage of the playback path when `sdm.enabled` and native
//! DSD output is negotiated. It consumes interleaved PCM at the **working
//! rate** (the rate the engine's resampler produced, 352.8/384 kHz), runs one
//! [`SdmModulator`] per channel, and emits the exact interleaved word layout
//! the native DSD transport expects.
//!
//! Everything that can allocate is allocated in [`SdmOutput::new`] on the
//! control path. The block API only writes into caller-owned or preallocated
//! buffers, so a warmed-up instance is allocation-free on the audio thread.

use crate::buffer::MAX_CHANNELS;
use crate::decode::dsd::{DsdWireFormat, NativeDsdPacker};

use super::interpolator::PolyphaseInterpolator;
use super::modulator::{SdmModulator, MODULATION_DEPTH};
use super::rates::SdmRatePlan;

/// Taps per polyphase branch. The working rate is already band-limited, so
/// this only has to remove zero-stuffing images; 24 taps gives more than
/// 90 dB of image rejection without a long filter.
pub const DEFAULT_TAPS: usize = 17;

/// Upper bound used to size the block buffers; the engine validates audio
/// blocks against the same limit.
const MAX_BLOCK_FRAMES: usize = crate::buffer::MAX_AUDIO_BLOCK_FRAMES;

/// Result of one [`SdmOutput::process_block`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdmBlock {
    /// PCM frames consumed.
    pub input_frames: usize,
    /// Whole DSD words produced per channel.
    pub words: usize,
    /// Bytes written to the caller's packed buffer.
    pub output_bytes: usize,
}

/// Snapshot of the stage's measured behaviour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SdmTelemetry {
    pub input_frames: u64,
    pub clips: u64,
    pub state_recoveries: u64,
    pub input_peak: f64,
    pub ones_ratio: f64,
    pub interpolator_delay_ms: f64,
    pub modulation_depth: f64,
}

/// Append one modulated bit to a channel's wire-word accumulator.
#[inline]
fn push_bit(
    y: f64,
    acc: &mut u32,
    count: &mut u8,
    bytes: &mut Vec<u8>,
    word_bits: u32,
    bytes_per_word: usize,
) {
    let bit = u32::from(y > 0.0);
    *acc |= bit << *count;
    *count += 1;
    if *count as u32 == word_bits {
        bytes.extend_from_slice(&acc.to_le_bytes()[..bytes_per_word]);
        *acc = 0;
        *count = 0;
    }
}

/// Per-channel PCM→SDM conversion stage.
pub struct SdmOutput {
    plan: SdmRatePlan,
    channels: usize,
    wire: DsdWireFormat,
    interpolators: Vec<PolyphaseInterpolator>,
    modulators: Vec<SdmModulator>,
    bit_acc: Vec<u32>,
    bit_count: Vec<u8>,
    word_bits: u8,
    channel_bytes: Vec<Vec<u8>>,
    interp_scratch: Vec<f64>,
    input_frames: u64,
}

impl SdmOutput {
    /// Build a stage for `plan`, `channels`, and the negotiated wire format.
    pub fn new(
        plan: SdmRatePlan,
        channels: usize,
        wire: DsdWireFormat,
        modulator_order: usize,
    ) -> Option<Self> {
        if channels == 0 || channels > MAX_CHANNELS {
            return None;
        }
        let factor = plan.stage_factor as usize;
        let interpolator = PolyphaseInterpolator::new(factor, DEFAULT_TAPS)?;
        let modulator = SdmModulator::new(modulator_order, plan.multiple)?;
        let bytes_per_frame = factor / 8;
        let per_channel_capacity = MAX_BLOCK_FRAMES * bytes_per_frame;

        Some(Self {
            plan,
            channels,
            wire,
            interpolators: vec![interpolator; channels],
            modulators: vec![modulator; channels],
            bit_acc: vec![0; channels],
            bit_count: vec![0; channels],
            word_bits: wire.samples_per_word() as u8,
            channel_bytes: (0..channels)
                .map(|_| Vec::with_capacity(per_channel_capacity))
                .collect(),
            interp_scratch: vec![0.0; factor],
            input_frames: 0,
        })
    }

    /// Convert one interleaved PCM block into packed native-DSD words.
    ///
    /// `out` is cleared and refilled. The caller must have checked that the
    /// transport has room for `output_bytes`; this stage does not drop
    /// samples silently. Bits that do not yet fill a whole wire word stay in
    /// the per-channel accumulator for the next block.
    pub fn process_block(&mut self, input: &[f32], out: &mut Vec<u8>) -> SdmBlock {
        let frames = input.len() / self.channels;
        for bytes in &mut self.channel_bytes {
            bytes.clear();
        }
        let factor = self.plan.stage_factor as usize;
        let word_bits = self.word_bits as u32;
        let bytes_per_word = self.wire.bytes_per_word();

        for frame in 0..frames {
            let base = frame * self.channels;
            for ch in 0..self.channels {
                let sample = input[base + ch] as f64;
                self.interpolators[ch].process_sample(sample, &mut self.interp_scratch);
                for i in 0..factor {
                    let y = self.modulators[ch].process_sample(self.interp_scratch[i]);
                    push_bit(
                        y,
                        &mut self.bit_acc[ch],
                        &mut self.bit_count[ch],
                        &mut self.channel_bytes[ch],
                        word_bits,
                        bytes_per_word,
                    );
                }
            }
        }

        let refs: [&[u8]; MAX_CHANNELS] = std::array::from_fn(|i| {
            if i < self.channels {
                self.channel_bytes[i].as_slice()
            } else {
                &[]
            }
        });
        let words = NativeDsdPacker::pack(self.wire, &refs[..self.channels], out);
        self.input_frames += frames as u64;
        SdmBlock {
            input_frames: frames,
            words,
            output_bytes: out.len(),
        }
    }

    /// Bytes of native-DSD transport the stage can produce for one PCM frame
    /// (all channels, whole bytes only).
    #[inline]
    pub fn bytes_per_input_frame(&self) -> usize {
        self.channels * (self.plan.stage_factor as usize / 8)
    }

    /// Drop signal state and per-track counters after a seek or track change.
    pub fn reset(&mut self) {
        for m in &mut self.modulators {
            m.reset();
        }
        for i in &mut self.interpolators {
            i.reset();
        }
        self.bit_acc.fill(0);
        self.bit_count.fill(0);
        for bytes in &mut self.channel_bytes {
            bytes.clear();
        }
        self.input_frames = 0;
    }

    /// Measured telemetry, never asserted values.
    pub fn telemetry(&self) -> SdmTelemetry {
        let mut clips = 0;
        let mut recoveries = 0;
        let mut ones = 0u64;
        let mut samples = 0u64;
        let mut peak = 0.0f64;
        for m in &self.modulators {
            clips += m.clip_count();
            recoveries += m.state_recoveries();
            ones += m.ones();
            samples += m.samples();
            peak = peak.max(m.input_peak());
        }
        SdmTelemetry {
            input_frames: self.input_frames,
            clips,
            state_recoveries: recoveries,
            input_peak: peak,
            ones_ratio: if samples == 0 {
                0.0
            } else {
                ones as f64 / samples as f64
            },
            interpolator_delay_ms: self.plan.interpolator_delay_ms(DEFAULT_TAPS / 2),
            modulation_depth: MODULATION_DEPTH,
        }
    }

    #[inline]
    pub fn plan(&self) -> SdmRatePlan {
        self.plan
    }

    #[inline]
    pub fn channels(&self) -> usize {
        self.channels
    }

    #[inline]
    pub fn wire(&self) -> DsdWireFormat {
        self.wire
    }
}

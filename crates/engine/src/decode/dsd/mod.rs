//! Direct Stream Digital (DSD) bitstream decoder, DoP framing, and DSD-to-PCM decimation.
//!
//! Supports DSF and DFF container parsing, native DSD bitstream delivery,
//! DoP (DSD over PCM v1.1) packaging for USB Audio Class 2 DACs, and high-quality
//! multi-stage FIR decimation down to 24/32-bit PCM.
//!
//! # Payload layout notes (verified against reference implementations)
//!
//! Both containers store the 1-bit DSD bitstream as packed bytes, **per block of
//! `block_size` bytes per channel**, with the channels stored consecutively inside
//! each block (channel 0 first). A stereo file therefore alternates
//! `[ch0 block][ch1 block][ch0 block][ch1 block]…`. Each byte holds 8 DSD
//! samples; the samples inside a byte are **LSB-first** (bit 0 is the earliest
//! sample) unless the file explicitly says otherwise (DSF `bits_per_sample == 8`).
//!
//! - **DSF**: audio bytes = `data` chunk size − 12. The block size and bit order
//!   come from the `fmt ` chunk; the final block is zero-padded to a whole block.
//! - **DFF (DSDIFF)**: the `DSD ` chunk holds exactly the audio bytes, and the
//!   block size is fixed at 4096 samples per channel by the format. Bit order is
//!   always LSB-first. (DST-compressed files are rejected.)

mod decimator;
pub mod decoder;
mod packer;
mod reader;
#[cfg(test)]
mod tests;

use thiserror::Error;

pub use decimator::DsdToPcmDecimator;
pub use decoder::DsdDecoder;
pub use packer::{DopPacker, DsdWireFormat, NativeDsdPacker};
pub use reader::DsdReader;

#[derive(Debug, Error)]
pub enum DsdError {
    #[error("Invalid DSD header: {0}")]
    InvalidHeader(String),
    #[error("Unsupported DSD channel count: {0}")]
    UnsupportedChannels(usize),
    #[error("Unsupported DSD sample rate: {0}")]
    UnsupportedRate(u32),
    #[error("DST-compressed DSD is not supported: {0}")]
    UnsupportedCompression(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// DSD standard sampling rates (multiples of 44.1 kHz × 64 or 48 kHz × 64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsdRate {
    /// DSD64: 2.8224 MHz (64 × 44.1 kHz)
    Dsd64,
    /// DSD128: 5.6448 MHz (128 × 44.1 kHz)
    Dsd128,
    /// DSD256: 11.2896 MHz (256 × 44.1 kHz)
    Dsd256,
    /// DSD512: 22.5792 MHz (512 × 44.1 kHz)
    Dsd512,
    /// DSD1024: 45.1584 MHz (1024 × 44.1 kHz)
    Dsd1024,
    /// DSD64 (48k): 3.072 MHz (64 × 48 kHz)
    Dsd64_48k,
    /// DSD128 (48k): 6.144 MHz (128 × 48 kHz)
    Dsd128_48k,
    /// DSD256 (48k): 12.288 MHz (256 × 48 kHz)
    Dsd256_48k,
    /// DSD512 (48k): 24.576 MHz (512 × 48 kHz)
    Dsd512_48k,
    /// DSD1024 (48k): 49.152 MHz (1024 × 48 kHz)
    Dsd1024_48k,
}

impl DsdRate {
    pub fn sample_rate_hz(&self) -> u32 {
        match self {
            Self::Dsd64 => 2_822_400,
            Self::Dsd128 => 5_644_800,
            Self::Dsd256 => 11_289_600,
            Self::Dsd512 => 22_579_200,
            Self::Dsd1024 => 45_158_400,
            Self::Dsd64_48k => 3_072_000,
            Self::Dsd128_48k => 6_144_000,
            Self::Dsd256_48k => 12_288_000,
            Self::Dsd512_48k => 24_576_000,
            Self::Dsd1024_48k => 49_152_000,
        }
    }

    /// Base PCM rate family: 44,100 Hz or 48,000 Hz.
    pub fn base_frequency_hz(&self) -> u32 {
        match self {
            Self::Dsd64 | Self::Dsd128 | Self::Dsd256 | Self::Dsd512 | Self::Dsd1024 => 44_100,
            Self::Dsd64_48k
            | Self::Dsd128_48k
            | Self::Dsd256_48k
            | Self::Dsd512_48k
            | Self::Dsd1024_48k => 48_000,
        }
    }

    /// Oversampling multiplier over the base PCM rate (64..1024).
    pub fn multiplier(&self) -> u32 {
        match self {
            Self::Dsd64 | Self::Dsd64_48k => 64,
            Self::Dsd128 | Self::Dsd128_48k => 128,
            Self::Dsd256 | Self::Dsd256_48k => 256,
            Self::Dsd512 | Self::Dsd512_48k => 512,
            Self::Dsd1024 | Self::Dsd1024_48k => 1024,
        }
    }

    pub fn from_hz(hz: u32) -> Option<Self> {
        match hz {
            2_822_400 => Some(Self::Dsd64),
            5_644_800 => Some(Self::Dsd128),
            11_289_600 => Some(Self::Dsd256),
            22_579_200 => Some(Self::Dsd512),
            45_158_400 => Some(Self::Dsd1024),
            3_072_000 => Some(Self::Dsd64_48k),
            6_144_000 => Some(Self::Dsd128_48k),
            12_288_000 => Some(Self::Dsd256_48k),
            24_576_000 => Some(Self::Dsd512_48k),
            49_152_000 => Some(Self::Dsd1024_48k),
            _ => None,
        }
    }
}

/// A block of raw (still bit-packed) DSD payload, de-interleaved per channel.
///
/// `channels` holds one payload byte-slice per source channel in file order
/// (channel 0 = front left). For multichannel DSF sources this exposes the
/// full layout (center / LFE / surrounds), not just the front pair.
#[derive(Debug, Clone)]
pub struct DsdBlock {
    /// Number of DSD frames (per channel) in this block.
    pub frames: u32,
    /// De-interleaved payload: one byte-slice per channel.
    pub channels: Vec<Vec<u8>>,
}

impl DsdBlock {
    /// Channel 0 payload (front left).
    pub fn left(&self) -> &[u8] {
        &self.channels[0]
    }

    /// Channel 1 payload (front right); `None` for mono sources.
    pub fn right(&self) -> Option<&[u8]> {
        self.channels.get(1).map(|c| c.as_slice())
    }
}

/// A block of decoded PCM produced by [`DsdReader::decode_block`].
#[derive(Debug, Clone)]
pub struct DsdPcmBlock {
    /// Number of DSD frames consumed to produce this PCM block.
    pub frames: u32,
    /// De-interleaved PCM: one f32-slice per channel.
    pub channels: Vec<Vec<f32>>,
}

impl DsdPcmBlock {
    /// De-interleaved left channel PCM.
    pub fn left(&self) -> &[f32] {
        &self.channels[0]
    }

    /// De-interleaved right channel PCM; mirrors `left` for mono sources.
    pub fn right(&self) -> &[f32] {
        self.channels
            .get(1)
            .map(|c| c.as_slice())
            .unwrap_or(&self.channels[0])
    }
}

/// Extract editorial tags and duration from a DSD (`.dsf` or `.dff`) file.
pub fn extract_track_metadata(path: &std::path::Path) -> crate::decode::ExtractedTags {
    let mut tags = crate::decode::ExtractedTags::unknown_for(path);
    if let Ok(reader) = DsdReader::open(path) {
        let fmt = reader.format_info();
        if let Some(dur) = fmt.duration_secs {
            tags.duration_secs = dur;
        }
    }
    tags
}

/// Read all metadata and stream properties for a DSD (`.dsf` or `.dff`) file.
pub fn probe_metadata(path: &std::path::Path) -> crate::decode::ProbedMetadata {
    let tags = extract_track_metadata(path);
    let loudness = crate::dsp::LoudnessMetadata::default();
    let mut probed = crate::decode::ProbedMetadata::from_tags_and_loudness(tags, loudness);

    if let Ok(reader) = DsdReader::open(path) {
        let fmt = reader.format_info();
        probed.sample_rate = Some(fmt.sample_rate);
        probed.channels = Some(fmt.channels);
        probed.bits_per_sample = fmt.bit_depth;
    }

    probed
}

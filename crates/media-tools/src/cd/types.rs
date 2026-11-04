//! Audio-CD extraction through `libcdio`, `libcdio-paranoia` and (optionally)
//! `libcddb`.
//!
//! Drives are opened by path, so the same code works for `/dev/sr0`, a macOS
//! device node, and a Windows drive letter. Every native library is loaded at
//! runtime: a system without CD support still builds and still plays files.
//!
//! Extraction is track-at-a-time and checks the sector count the drive reports.
//! Output is a 16-bit 44.1 kHz WAV, optionally encoded through
//! [`crate::transcode`]. CDDB is best-effort; a failed lookup never loses audio.

use crate::transcode::OutputFormat;
use std::ffi::c_int;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub(crate) const CD_FRAMES_PER_SEC: u32 = 75;
pub(crate) const CD_SAMPLES_PER_SECTOR: usize = 2352 / 2;
pub(crate) const SEEK_SET: c_int = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RipFormat {
    Wav,
    Flac16,
    Flac24,
    Opus,
}

impl RipFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac16 | Self::Flac24 => "flac",
            Self::Opus => "opus",
        }
    }

    pub fn export_format(self) -> Option<OutputFormat> {
        match self {
            Self::Wav => None,
            Self::Flac16 => Some(OutputFormat::Flac { bit_depth: 16 }),
            Self::Flac24 => Some(OutputFormat::Flac { bit_depth: 24 }),
            Self::Opus => Some(OutputFormat::Opus),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdTrack {
    pub number: u32,
    pub title: String,
    pub artist: String,
    pub first_sector: u32,
    pub last_sector: u32,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub year: Option<u32>,
    pub genre: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdDisc {
    pub device: String,
    pub tracks: Vec<CdTrack>,
    pub metadata: DiscMetadata,
    pub warnings: Vec<String>,
}

impl CdDisc {
    pub fn total_duration_secs(&self) -> f64 {
        self.tracks.iter().map(|track| track.duration_secs).sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RipProgress {
    pub track_number: u32,
    pub track_count: u32,
    pub sector: u32,
    pub total_sectors: u32,
    pub track_title: String,
    pub completed: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RipOptions {
    pub output_dir: PathBuf,
    pub format: RipFormat,
    pub fetch_metadata: bool,
    pub overwrite: bool,
}

impl RipOptions {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            format: RipFormat::Flac24,
            fetch_metadata: true,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RipReport {
    pub disc: CdDisc,
    pub files: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CdRipper {
    pub(crate) device: PathBuf,
}

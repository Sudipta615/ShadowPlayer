//! Decoder adapter for the recommendation analyser.
//!
//! The recommendation core is deliberately dependency-free: it asks the host
//! for three short PCM windows and does the DSP itself. This module is the
//! host side of that boundary. It opens the same [`engine::decode::Decoder`]
//! playback uses, seeks to the requested interval, and decodes only that
//! interval — it never decodes a whole album image to analyse one CUE track.

use std::path::{Path, PathBuf};

use engine::decode::Decoder;
use recommendation::{AudioReadError, AudioWindowSource, PcmWindow};

use crate::{MediaError, Result};

/// One track (or one CUE-sheet track inside an album image) opened for
/// analysis.
pub struct DecoderWindowSource {
    path: PathBuf,
    decoder: Decoder,
    /// Offset of the track inside the file, for CUE virtual paths.
    offset_secs: f64,
    /// Length of the track, not the whole file.
    duration_secs: f64,
    sample_rate: u32,
    channels: u16,
}

impl DecoderWindowSource {
    /// Open a real file or a `audio#cue:N#sheet` virtual path.
    pub fn open(path: &Path) -> Result<Self> {
        let path_text = path.to_string_lossy().to_string();
        if let Some(cue) = domain::cue::parse_cue_virtual_path(&path_text) {
            return Self::open_cue(path, cue);
        }

        let decoder = Decoder::open(path).map_err(|error| MediaError::Decode {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
        let (sample_rate, channel_count) = {
            let info = decoder.info();
            (info.sample_rate, info.channels)
        };
        if sample_rate == 0 || channel_count == 0 {
            return Err(MediaError::InvalidInput(format!(
                "{} reported no usable audio format",
                path.display()
            )));
        }
        let duration_secs = f64::from(decoder.duration_secs());
        if !duration_secs.is_finite() || duration_secs <= 0.0 {
            return Err(MediaError::InvalidInput(format!(
                "{} has no known duration",
                path.display()
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
            decoder,
            offset_secs: 0.0,
            duration_secs,
            sample_rate,
            channels: u16::try_from(channel_count).unwrap_or(u16::MAX),
        })
    }

    fn open_cue(virtual_path: &Path, cue: domain::cue::CueVirtualPath<'_>) -> Result<Self> {
        let audio_path = cue.audio_path();
        let sheet_path = cue.sheet_path().ok_or_else(|| {
            MediaError::InvalidInput(format!(
                "{} is a CUE track without the sheet path",
                virtual_path.display()
            ))
        })?;
        let sheet = engine::decode::CueSheet::parse_file(&sheet_path).map_err(|error| {
            MediaError::InvalidInput(format!(
                "could not read CUE sheet {}: {error}",
                sheet_path.display()
            ))
        })?;

        let decoder = Decoder::open(&audio_path).map_err(|error| MediaError::Decode {
            path: audio_path.clone(),
            message: error.to_string(),
        })?;
        let (sample_rate, channel_count) = {
            let info = decoder.info();
            (info.sample_rate, info.channels)
        };
        if sample_rate == 0 || channel_count == 0 {
            return Err(MediaError::InvalidInput(format!(
                "{} reported no usable audio format",
                audio_path.display()
            )));
        }
        let file_duration = f64::from(decoder.duration_secs());
        if !file_duration.is_finite() || file_duration <= 0.0 {
            return Err(MediaError::InvalidInput(format!(
                "{} has no known duration",
                audio_path.display()
            )));
        }

        let durations = sheet.calculate_track_durations(Some(file_duration));
        let (_, start_secs, track_duration) = durations
            .iter()
            .find(|(number, _, _)| *number == cue.track_number)
            .copied()
            .ok_or_else(|| {
                MediaError::InvalidInput(format!(
                    "CUE sheet {} has no track {}",
                    sheet_path.display(),
                    cue.track_number
                ))
            })?;
        let duration_secs = track_duration.unwrap_or(file_duration - start_secs);
        if !duration_secs.is_finite() || duration_secs <= 0.0 {
            return Err(MediaError::InvalidInput(format!(
                "CUE track {} of {} is empty",
                cue.track_number,
                sheet_path.display()
            )));
        }

        Ok(Self {
            path: virtual_path.to_path_buf(),
            decoder,
            offset_secs: start_secs,
            duration_secs,
            sample_rate,
            channels: u16::try_from(channel_count).unwrap_or(u16::MAX),
        })
    }

    fn decode_error(&self, message: String) -> AudioReadError {
        AudioReadError::new(format!(
            "could not decode {}: {message}",
            self.path.display()
        ))
    }
}

impl AudioWindowSource for DecoderWindowSource {
    fn duration_seconds(&self) -> f64 {
        self.duration_secs
    }

    fn read_window(
        &mut self,
        start_seconds: f64,
        duration_seconds: f64,
    ) -> std::result::Result<PcmWindow, AudioReadError> {
        let start = start_seconds.max(0.0).min(self.duration_secs);
        let end = (start + duration_seconds.max(0.0)).min(self.duration_secs);
        if end <= start {
            return Ok(PcmWindow {
                samples: Vec::new(),
                sample_rate: self.sample_rate,
                channels: self.channels,
            });
        }

        let absolute_start = self.offset_secs + start;
        self.decoder
            .seek(absolute_start as f32)
            .map_err(|error| self.decode_error(error.to_string()))?;

        let channels = self.channels.max(1) as usize;
        let wanted_frames = ((end - start) * f64::from(self.sample_rate))
            .ceil()
            .max(1.0) as usize;
        let mut samples = Vec::with_capacity(wanted_frames.saturating_mul(channels));

        while samples.len() / channels < wanted_frames {
            match self.decoder.decode_next(4096) {
                Ok(chunk) => {
                    if chunk.samples.is_empty() || chunk.channels == 0 {
                        break;
                    }
                    let chunk_frames = chunk.samples.len() / chunk.channels;
                    let remaining = wanted_frames - samples.len() / channels;
                    let take = chunk_frames.min(remaining);
                    if take == 0 {
                        break;
                    }
                    let end_sample = take * chunk.channels;
                    samples.extend_from_slice(&chunk.samples[..end_sample]);
                    if take < chunk_frames {
                        break;
                    }
                }
                Err(engine::decode::DecodeError::EndOfStream) => break,
                Err(error) => return Err(self.decode_error(error.to_string())),
            }
        }

        // A short final chunk is normal at end-of-stream; the analyser treats
        // a window with no samples as a failed region, so return it as-is.
        Ok(PcmWindow {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_dir() -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "shadowplayer-analysis-{}-{stamp}",
            std::process::id()
        ))
    }

    /// A 2-second, 44.1 kHz, stereo 16-bit PCM WAV with a quiet sine tone.
    fn write_wav(path: &Path) {
        let sample_rate = 44_100u32;
        let seconds = 2u32;
        let frames = sample_rate * seconds;
        let data_len = frames * 2 * 2;
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16u32.to_le_bytes()).unwrap();
        file.write_all(&1u16.to_le_bytes()).unwrap(); // PCM
        file.write_all(&2u16.to_le_bytes()).unwrap(); // stereo
        file.write_all(&sample_rate.to_le_bytes()).unwrap();
        file.write_all(&(sample_rate * 4).to_le_bytes()).unwrap();
        file.write_all(&4u16.to_le_bytes()).unwrap();
        file.write_all(&16u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_len.to_le_bytes()).unwrap();
        for frame in 0..frames {
            let value = ((frame as f32 * 440.0 * std::f32::consts::TAU / sample_rate as f32).sin()
                * 3_000.0) as i16;
            file.write_all(&value.to_le_bytes()).unwrap();
            file.write_all(&value.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn decodes_only_the_requested_window() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("tone.wav");
        write_wav(&wav);

        let mut source = DecoderWindowSource::open(&wav).unwrap();
        assert!((source.duration_seconds() - 2.0).abs() < 0.05);
        let window = source.read_window(0.5, 0.25).unwrap();
        assert_eq!(window.sample_rate, 44_100);
        assert_eq!(window.channels, 2);
        let frames = window.frames();
        assert!(
            (frames as i64 - 11_025).abs() <= 2_048,
            "expected roughly a quarter second, got {frames} frames"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cue_track_uses_the_sheet_offset_and_length() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("album.wav");
        write_wav(&wav);
        let cue_path = dir.join("album.cue");
        std::fs::write(
            &cue_path,
            "PERFORMER \"Test\"\n\
             FILE \"album.wav\" WAVE\n\
             TRACK 01 AUDIO\n\
             TITLE \"One\"\n\
             INDEX 01 00:00:00\n\
             TRACK 02 AUDIO\n\
             TITLE \"Two\"\n\
             INDEX 01 00:01:00\n",
        )
        .unwrap();

        let virtual_path = domain::cue::format_cue_virtual_path(&wav, 2, &cue_path);
        let mut source = DecoderWindowSource::open(Path::new(&virtual_path)).unwrap();
        assert!(
            (source.duration_seconds() - 1.0).abs() < 0.05,
            "track two should be the second half, got {}",
            source.duration_seconds()
        );
        let window = source.read_window(0.0, 0.2).unwrap();
        assert!(!window.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

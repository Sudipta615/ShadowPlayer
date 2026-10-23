use domain::ids::TrackId;
use domain::models::Track;
use std::fs::File;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MetadataError {
    #[error("I/O error reading file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to extract metadata")]
    ExtractionFailed,
}

pub struct ExtractedMetadata {
    pub track: Track,
    pub embedded_artwork: Option<(Vec<u8>, String)>,
}

pub fn extract_metadata(path: &Path) -> Result<ExtractedMetadata, MetadataError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_size_bytes = metadata.len();
    // Both parts of the modification time, from the handle that is already
    // open: the scanner compares them to decide whether a file changed, and
    // whole seconds cannot tell an unchanged file from one rewritten within the
    // same second as the last scan.
    let (mtime_unix_secs, mtime_nanos) = metadata
        .modified()
        .map(|t| match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(since_epoch) => (since_epoch.as_secs() as i64, since_epoch.subsec_nanos()),
            // A file dated in the future has no meaningful timestamp.
            Err(_) => (0, 0),
        })
        .unwrap_or((0, 0));

    // One probe for everything cheap: editorial tags, loudness tags, the
    // stream format and the cover art. This used to be two engine calls plus a
    // third `symphonia` probe of the same file in this crate — three opens and
    // three tag parses per scanned file.
    let engine::decode::ProbedMetadata {
        tags,
        loudness,
        sample_rate,
        channels,
        bits_per_sample,
        artwork,
    } = engine::decode::probe_metadata(path);

    let format_name = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "AUDIO".to_string());

    let is_lossless = matches!(
        format_name.as_str(),
        "FLAC" | "WAV" | "AIFF" | "ALAC" | "APE" | "WV" | "TTA" | "DSF" | "DFF"
    );

    let title = tags.title;
    let artist = tags.artist;
    let album = tags.album;
    let duration_secs = tags.duration_secs as f32;
    let genre: Option<String> = (!tags.genre.is_empty()).then_some(tags.genre);
    // A date tag is `1973` or `1973-05-01`; the year is its first four digits.
    let year: Option<u32> = tags.date.chars().take(4).collect::<String>().parse().ok();
    let track_number: Option<u32> = (tags.track_number > 0).then_some(tags.track_number);
    let disc_number: Option<u32> = (tags.disc_number > 0).then_some(tags.disc_number);
    // A container that says nothing about the stream keeps the house defaults.
    let sample_rate = sample_rate.unwrap_or(44100);
    let channels = channels.unwrap_or(2);
    let bitrate_kbps: Option<u32> = bits_per_sample.map(|bits| {
        if duration_secs > 0.0 {
            // Computed in `f64`: a sub-second file keeps its fractional
            // duration (the old integer form truncated it to 0 and divided by
            // zero).
            (file_size_bytes as f64 * 8.0 / (duration_secs as f64 * 1000.0)) as u32
        } else {
            (sample_rate * channels as u32 * bits) / 1000
        }
    });
    let embedded_artwork = artwork;

    // Index the lyric flag alongside the tags. The lyrics panel reads the same
    // two sources, so "Has Lyrics" and the panel agree.
    let has_lyrics = crate::has_lyrics(path);

    // BPM is a first-class radio feature but not part of the engine's
    // symphonia probe (which doesn't expose a typed tempo field). One lofty
    // read here, alongside the lyric-flag check, keeps the extraction in the
    // scanner where every other tag is read.
    let bpm = extract_bpm(path);

    let track_id = TrackId::from_canonical_path(path);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let track = Track {
        id: track_id,
        path: path.to_path_buf(),
        title,
        artist,
        album,
        genre,
        year,
        track_number,
        disc_number,
        duration_secs,
        bpm,
        sample_rate,
        channels,
        format: format_name,
        bitrate_kbps,
        bit_depth: bits_per_sample,
        is_lossless,
        replaygain_track_db: loudness.replaygain_track_db,
        replaygain_track_peak: loudness.replaygain_track_peak,
        // The engine's probe already reads `REPLAYGAIN_ALBUM_GAIN` and the
        // album peak from the file's own tags.
        replaygain_album_db: loudness.replaygain_album_db,
        replaygain_album_peak: loudness.replaygain_album_peak,
        r128_loudness_lufs: loudness.ebu_r128_loudness,
        file_size_bytes,
        mtime_unix_secs,
        mtime_nanos,
        // Filled in by the scanner, which knows the on-disk file.
        content_hash: None,
        // The filesystem's own identity for the file. The handle is already
        // open here, so this costs one `stat` and nothing more.
        file_id: domain::file_identity::file_identity(path).map(|identity| identity.to_key()),
        is_favorite: false,
        rating: 0,
        has_lyrics,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        added_at: now,
    };

    // NOTE: lyrics are *not* read here. Scanning touches every changed file, and
    // nothing consumes them at index time; `crate::extract_lyrics` reads them on
    // demand for the track being played.
    Ok(ExtractedMetadata {
        track,
        embedded_artwork,
    })
}

/// Read a track's tempo from its tags.
///
/// `TBPM`, `tmpo` and the MP4 `tmpo` atom all arrive as `IntegerBpm`; a few
/// taggers write the free-form `BPM` key instead. Both are accepted, and a
/// decimal value (`"128.5"`) is rounded because [`domain::models::Track`]
/// stores a whole number of beats per minute.
fn extract_bpm(path: &Path) -> Option<u16> {
    use lofty::prelude::*;
    use lofty::tag::ItemKey;

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let raw = tag
        .get(ItemKey::IntegerBpm)
        .and_then(|item| item.value().text())
        .or_else(|| tag.get(ItemKey::Bpm).and_then(|item| item.value().text()))?;
    parse_bpm(raw)
}

/// Parse the tag's textual tempo, tolerating `"128"`, `"128.0"` and `"128 BPM"`.
fn parse_bpm(raw: &str) -> Option<u16> {
    let number: String = raw
        .chars()
        .skip_while(|c| !c.is_ascii_digit() && *c != '.')
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f32 = number.parse().ok()?;
    if !value.is_finite() || value <= 0.0 || value > u16::MAX as f32 {
        return None;
    }
    Some(value.round() as u16)
}

/// The audio file a standalone `.cue` sheet indexes, if one can be resolved.
///
/// Prefers the sheet's own `FILE` reference and falls back to a sibling that
/// shares the sheet's stem. The scanner uses this to index an album image
/// *either* as CUE tracks *or* as one file, never as both.
pub fn cue_referenced_audio(cue_path: &Path) -> Option<PathBuf> {
    let sheet = engine::decode::cue::CueSheet::parse_file(cue_path).ok()?;
    let parent_dir = cue_path.parent().unwrap_or(Path::new("."));

    if let Some(name) = sheet.files.first() {
        let candidate = parent_dir.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let stem = cue_path.file_stem()?.to_str()?;
    for ext in &["flac", "wav", "ape", "wv", "mp3", "m4a", "dsf", "dff"] {
        let candidate = parent_dir.join(format!("{}.{}", stem, ext));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Extract tracks from a standalone .cue sheet and resolve boundaries to audio files
pub fn extract_cue_metadata(cue_path: &Path) -> Result<Vec<ExtractedMetadata>, MetadataError> {
    let sheet = engine::decode::cue::CueSheet::parse_file(cue_path)
        .map_err(|_| MetadataError::ExtractionFailed)?;

    let audio_path = cue_referenced_audio(cue_path).ok_or(MetadataError::ExtractionFailed)?;

    // Extract underlying audio file specs
    let audio_meta = extract_metadata(&audio_path)?;
    let total_audio_duration = audio_meta.track.duration_secs;

    let mut results = Vec::new();
    let track_count = sheet.tracks.len();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for (idx, cue_track) in sheet.tracks.iter().enumerate() {
        let start_sec = cue_track.start_time_seconds();
        let end_sec = if idx + 1 < track_count {
            sheet.tracks[idx + 1].start_time_seconds()
        } else {
            total_audio_duration as f64
        };
        let duration = (end_sec - start_sec).max(0.0) as f32;

        // The sheet is part of the identity. Without it, playback can only
        // guess at the `.cue` filename and silently falls back to playing the
        // whole album image when the guess is wrong.
        let virtual_path =
            domain::cue::format_cue_virtual_path(&audio_path, cue_track.number, cue_path);
        let track_id = TrackId::from_canonical_path(Path::new(&virtual_path));

        let title = cue_track
            .title
            .clone()
            .unwrap_or_else(|| format!("Track {:02}", cue_track.number));
        let artist = cue_track
            .performer
            .clone()
            .or_else(|| sheet.performer.clone())
            .unwrap_or_else(|| audio_meta.track.artist.clone());
        let album = sheet
            .title
            .clone()
            .unwrap_or_else(|| audio_meta.track.album.clone());
        let year = sheet
            .date
            .as_ref()
            .and_then(|d| d.parse().ok())
            .or(audio_meta.track.year);

        let track = Track {
            id: track_id,
            path: PathBuf::from(virtual_path),
            title,
            artist,
            album,
            genre: sheet
                .genre
                .clone()
                .or_else(|| audio_meta.track.genre.clone()),
            year,
            track_number: Some(cue_track.number),
            disc_number: Some(1),
            duration_secs: duration,
            bpm: None,
            sample_rate: audio_meta.track.sample_rate,
            channels: audio_meta.track.channels,
            format: format!("CUE+{}", audio_meta.track.format),
            bitrate_kbps: audio_meta.track.bitrate_kbps,
            bit_depth: audio_meta.track.bit_depth,
            is_lossless: audio_meta.track.is_lossless,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: audio_meta.track.file_size_bytes,
            mtime_unix_secs: audio_meta.track.mtime_unix_secs,
            mtime_nanos: audio_meta.track.mtime_nanos,
            // CUE virtual tracks share one file, so they are never fingerprinted.
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: audio_meta.track.has_lyrics,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: now,
        };

        results.push(ExtractedMetadata {
            track,
            embedded_artwork: audio_meta.embedded_artwork.clone(),
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_meta_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A minimal 16-bit stereo PCM WAV, 0.1 s at 44.1 kHz.
    fn write_wav(path: &Path) {
        let sr = 44_100u32;
        let frames = sr / 10;
        let mut data = Vec::with_capacity(frames as usize * 4);
        for i in 0..frames {
            let t = i as f32 / sr as f32;
            let sample =
                ((2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.3 * i16::MAX as f32) as i16;
            data.extend_from_slice(&sample.to_le_bytes());
            data.extend_from_slice(&sample.to_le_bytes());
        }
        let data_len = data.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&sr.to_le_bytes());
        out.extend_from_slice(&(sr * 4).to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&data);
        std::fs::write(path, out).expect("write wav");
    }

    /// The stream facts come from the container, the title from the file name,
    /// and the bitrate is derived — all from one probe.
    #[test]
    fn a_tagless_wav_is_described_by_its_container() {
        let dir = temp_dir("wav");
        let path = dir.join("test_tone.wav");
        write_wav(&path);

        let extracted = extract_metadata(&path).expect("extract");

        assert_eq!(extracted.track.sample_rate, 44_100);
        assert_eq!(extracted.track.channels, 2);
        assert_eq!(extracted.track.title, "test_tone", "falls back to the stem");
        assert!(
            (extracted.track.duration_secs - 0.1).abs() < 0.01,
            "duration came from the header: {}",
            extracted.track.duration_secs
        );
        assert!(extracted.track.bitrate_kbps.is_some());
        assert!(extracted.track.is_lossless);
        assert!(extracted.embedded_artwork.is_none());
        assert!(!extracted.track.id.to_string().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The size and both mtime parts are recorded for the scanner's
    /// unchanged-file check, and every scan says the same thing twice.
    #[test]
    fn file_facts_are_stable_across_probes() {
        let dir = temp_dir("stable");
        let path = dir.join("song.wav");
        write_wav(&path);

        let first = extract_metadata(&path).expect("extract");
        let second = extract_metadata(&path).expect("extract again");

        assert_eq!(first.track.file_size_bytes, second.track.file_size_bytes);
        assert_eq!(first.track.mtime_unix_secs, second.track.mtime_unix_secs);
        assert_eq!(first.track.mtime_nanos, second.track.mtime_nanos);
        assert!(first.track.mtime_nanos > 0, "sub-second component recorded");
        assert_eq!(
            first.track.id, second.track.id,
            "id is a function of the path"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_fabrication() {
        let error = extract_metadata(Path::new("/nonexistent/definitely-not-here.flac"));
        assert!(matches!(error, Err(MetadataError::Io(_))));
    }
}

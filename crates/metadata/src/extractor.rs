use domain::ids::TrackId;
use domain::models::Track;
use engine::decode::{extract_loudness_metadata, extract_track_metadata};
use std::fs::File;
use std::path::{Path, PathBuf};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTag};
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
    let file_size_bytes = file.metadata()?.len();
    let mtime_unix_secs = file
        .metadata()?
        .modified()
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
        .unwrap_or(0);

    // 1. Leverage engine's built-in extraction (handles all codecs + Opus routing)
    let tags = extract_track_metadata(path);
    let loudness = extract_loudness_metadata(path);

    let format_name = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "AUDIO".to_string());

    let is_lossless = matches!(
        format_name.as_str(),
        "FLAC" | "WAV" | "AIFF" | "ALAC" | "APE" | "WV" | "TTA" | "DSF" | "DFF"
    );

    let mut title = tags.title;
    let mut artist = tags.artist;
    let mut album = tags.album;
    let duration_secs = tags.duration_secs as f32;
    let mut genre: Option<String> = if !tags.genre.is_empty() { Some(tags.genre) } else { None };
    let mut year: Option<u32> = tags.date.parse().ok();
    let mut track_number: Option<u32> = if tags.track_number > 0 { Some(tags.track_number) } else { None };
    let mut disc_number: Option<u32> = if tags.disc_number > 0 { Some(tags.disc_number) } else { None };
    let mut sample_rate = 44100u32;
    let mut channels = 2usize;
    let mut bitrate_kbps: Option<u32> = None;
    let mut embedded_artwork: Option<(Vec<u8>, String)> = None;

    // 2. Extra tag and artwork pass using Symphonia probe
    if let Ok(file) = File::open(path) {
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let meta_opts = MetadataOptions::default();
        let fmt_opts = FormatOptions::default();

        if let Ok(mut format_reader) =
            symphonia::default::get_probe().probe(&hint, mss, fmt_opts, meta_opts)
        {
            if let Some(t) = format_reader.tracks().first() {
                if let Some(audio) = t.codec_params.as_ref().and_then(|cp| cp.audio()) {
                    if let Some(sr) = audio.sample_rate {
                        sample_rate = sr;
                    }
                    if let Some(ch) = &audio.channels {
                        channels = ch.count();
                    }
                    if let Some(bits) = audio.bits_per_sample {
                        // Computed in `f64`: a sub-second file keeps its
                        // fractional duration (the old integer form truncated it
                        // to 0 and divided by zero).
                        bitrate_kbps = if duration_secs > 0.0 {
                            Some(
                                (file_size_bytes as f64 * 8.0
                                    / (duration_secs as f64 * 1000.0))
                                    as u32,
                            )
                        } else {
                            Some((sample_rate * channels as u32 * bits) / 1000)
                        };
                    }
                }
            }

            if let Some(current) = format_reader.metadata().current() {
                for tag in &current.media.tags {
                    if let Some(std) = &tag.std {
                        match std {
                            StandardTag::TrackTitle(v) if !v.is_empty() => title = v.to_string(),
                            StandardTag::Artist(v) if !v.is_empty() => artist = v.to_string(),
                            StandardTag::Album(v) if !v.is_empty() => album = v.to_string(),
                            StandardTag::Genre(v) if !v.is_empty() => genre = Some(v.to_string()),
                            StandardTag::TrackNumber(v) => track_number = Some(*v as u32),
                            StandardTag::DiscNumber(v) => disc_number = Some(*v as u32),
                            _ => {}
                        }
                    } else {
                        let key = tag.raw.key.to_lowercase();
                        let val = tag.raw.value.to_string();
                        if !val.is_empty() {
                            if key == "genre" && genre.is_none() {
                                genre = Some(val);
                            } else if (key == "date" || key == "year") && year.is_none() {
                                if let Ok(y) =
                                    val.chars().take(4).collect::<String>().parse::<u32>()
                                {
                                    year = Some(y);
                                }
                            }
                        }
                    }
                }

                if let Some(visual) = current.media.visuals.first() {
                    let mime = visual
                        .media_type
                        .clone()
                        .unwrap_or_else(|| "image/jpeg".to_string());
                    embedded_artwork = Some((visual.data.to_vec(), mime));
                }
            }
        }
    }

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
        sample_rate,
        channels,
        format: format_name,
        bitrate_kbps,
        is_lossless,
        replaygain_track_db: loudness.replaygain_track_db,
        replaygain_track_peak: loudness.replaygain_track_peak,
        r128_loudness_lufs: loudness.ebu_r128_loudness,
        file_size_bytes,
        mtime_unix_secs,
        // Filled in by the scanner, which knows the on-disk file.
        content_hash: None,
        is_favorite: false,
        play_count: 0,
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
        let virtual_path = domain::cue::format_cue_virtual_path(&audio_path, cue_track.number, cue_path);
        let track_id = TrackId::from_canonical_path(Path::new(&virtual_path));

        let title = cue_track.title.clone()
            .unwrap_or_else(|| format!("Track {:02}", cue_track.number));
        let artist = cue_track.performer.clone()
            .or_else(|| sheet.performer.clone())
            .unwrap_or_else(|| audio_meta.track.artist.clone());
        let album = sheet.title.clone()
            .unwrap_or_else(|| audio_meta.track.album.clone());
        let year = sheet.date.as_ref().and_then(|d| d.parse().ok()).or(audio_meta.track.year);

        let track = Track {
            id: track_id,
            path: PathBuf::from(virtual_path),
            title,
            artist,
            album,
            genre: sheet.genre.clone().or_else(|| audio_meta.track.genre.clone()),
            year,
            track_number: Some(cue_track.number),
            disc_number: Some(1),
            duration_secs: duration,
            sample_rate: audio_meta.track.sample_rate,
            channels: audio_meta.track.channels,
            format: format!("CUE+{}", audio_meta.track.format),
            bitrate_kbps: audio_meta.track.bitrate_kbps,
            is_lossless: audio_meta.track.is_lossless,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: audio_meta.track.file_size_bytes,
            mtime_unix_secs: audio_meta.track.mtime_unix_secs,
            // CUE virtual tracks share one file, so they are never fingerprinted.
            content_hash: None,
            is_favorite: false,
            play_count: 0,
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

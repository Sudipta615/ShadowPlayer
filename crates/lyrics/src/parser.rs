use domain::models::SyncedLyricLine;
use std::fs;
use std::path::{Path, PathBuf};

/// Parse an LRC-formatted string into sorted SyncedLyricLine items.
pub fn parse_lrc(content: &str) -> Vec<SyncedLyricLine> {
    let mut lines = Vec::new();

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Handle lines with one or more timestamps, e.g. [01:23.45]Lyrics text
        let mut timestamps = Vec::new();
        let mut text_start = 0;

        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'[' {
                if let Some(close) = trimmed[i..].find(']') {
                    let tag = &trimmed[i + 1..i + close];
                    if let Some(secs) = parse_timestamp(tag) {
                        timestamps.push(secs);
                        i += close + 1;
                        text_start = i;
                        continue;
                    }
                }
            }
            break;
        }

        let lyric_text = trimmed[text_start..].trim().to_string();
        for ts in timestamps {
            lines.push(SyncedLyricLine {
                start_time_secs: ts,
                text: lyric_text.clone(),
            });
        }
    }

    lines.sort_by(|a, b| a.start_time_secs.total_cmp(&b.start_time_secs));
    lines
}

/// Parses timestamp strings such as "01:23.45", "01:23.456", "01:23"
fn parse_timestamp(s: &str) -> Option<f32> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: f32 = parts[0].trim().parse().ok()?;
    let seconds: f32 = parts[1].trim().parse().ok()?;

    if minutes.is_finite() && seconds.is_finite() && minutes >= 0.0 && seconds >= 0.0 {
        Some(minutes * 60.0 + seconds)
    } else {
        None
    }
}

/// Binary search to find the active lyric line at `position_secs`.
pub fn find_active_line(lines: &[SyncedLyricLine], position_secs: f32) -> Option<usize> {
    if lines.is_empty() || position_secs < lines[0].start_time_secs {
        return None;
    }

    match lines.binary_search_by(|line| line.start_time_secs.total_cmp(&position_secs)) {
        Ok(idx) => Some(idx),
        Err(idx) => {
            if idx > 0 {
                Some(idx - 1)
            } else {
                None
            }
        }
    }
}

/// Interpret arbitrary lyric text (a tag value or a plain file).
///
/// Returns `(is_synced, lines)`. Text carrying LRC timestamps is synced;
/// anything else is plain prose the caller should render line by line.
pub fn interpret_lyrics_text(content: &str) -> (bool, Vec<SyncedLyricLine>) {
    let lines = parse_lrc(content);
    if lines.is_empty() {
        (false, Vec::new())
    } else {
        (true, lines)
    }
}

/// Find sidecar `.lrc` or `.txt` lyrics for a song path.
///
/// `.lrc` wins over `.txt`, and both the extension and the file stem are
/// matched case-insensitively: a `song.flac` is satisfied by `song.LRC`,
/// `Song.lrc` or `song.lrc`. Lyric text without timestamps is still returned,
/// as plain (unsynced) lyrics.
pub fn find_sidecar_lyrics(
    song_path: &Path,
) -> Option<(bool, Vec<SyncedLyricLine>, Option<String>)> {
    let content = read_sidecar(song_path)?;
    let (is_synced, lines) = interpret_lyrics_text(&content);
    if is_synced {
        Some((true, lines, Some(content)))
    } else {
        Some((false, Vec::new(), Some(content)))
    }
}

/// Locate and read the best sidecar lyric file beside `song_path`.
fn read_sidecar(song_path: &Path) -> Option<String> {
    // Fast path: the conventional lower-case name needs no directory walk.
    let exact = song_path.with_extension("lrc");
    if let Ok(content) = fs::read_to_string(&exact) {
        return Some(content);
    }

    let stem = song_path.file_stem()?.to_str()?;
    let dir = song_path.parent()?;

    let mut lrc: Option<PathBuf> = None;
    let mut txt: Option<PathBuf> = None;

    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let Some(entry_stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !entry_stem.eq_ignore_ascii_case(stem) {
            continue;
        }
        if ext.eq_ignore_ascii_case("lrc") {
            lrc = lrc.or(Some(path));
        } else if ext.eq_ignore_ascii_case("txt") {
            txt = txt.or(Some(path));
        }
    }

    fs::read_to_string(lrc.or(txt)?).ok()
}

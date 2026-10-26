use domain::models::SyncedLyricLine;
use std::fs;
use std::path::{Path, PathBuf};

/// Parse an LRC-formatted string into sorted SyncedLyricLine items.
///
/// Handles the three things real `.lrc` files contain besides timestamped text:
///
/// * **Several timestamps on one line** (`[00:10.00][00:40.00]same words`) —
///   one lyric line per timestamp.
/// * **Metadata tags** (`[ar:]`, `[ti:]`, `[al:]`, `[by:]`, `[offset:]`, …).
///   These are consumed, not sung: the old parser stopped at the first
///   non-timestamp bracket, so `[00:12.00][ar:Artist]text` produced the lyric
///   body `"[ar:Artist]text"` and showed the artist in the middle of the song.
/// * **`[offset:…]`**, in milliseconds. Following the format's convention, a
///   positive value shifts every line *earlier* (the lyrics were written for a
///   copy of the track that ran late).
///
/// Returns an empty vector when the text holds no timestamps at all.
pub fn parse_lrc(content: &str) -> Vec<SyncedLyricLine> {
    let mut lines = Vec::new();
    let mut offset_ms: f32 = 0.0;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Consume every leading bracket group: timestamps and metadata alike.
        let mut timestamps = Vec::new();
        let mut text_start = 0;
        let bytes = trimmed.as_bytes();
        let mut i = 0;
        while i < bytes.len() && bytes[i] == b'[' {
            let Some(rel_close) = trimmed[i..].find(']') else {
                break;
            };
            let close = i + rel_close;
            let tag = &trimmed[i + 1..close];

            if let Some(secs) = parse_timestamp(tag) {
                timestamps.push(secs);
            } else if let Some(value) = parse_metadata(tag) {
                if tag
                    .get(..6)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("offset"))
                {
                    if let Ok(ms) = value.trim().parse::<f32>() {
                        if ms.is_finite() {
                            offset_ms = ms;
                        }
                    }
                }
            } else {
                // An unknown bracket (`[Chorus]`) is part of the words, not a
                // tag: leave it in the text.
                break;
            }

            i = close + 1;
            text_start = i;
        }

        if timestamps.is_empty() {
            // A pure metadata or prose line contributes no lyric.
            continue;
        }

        let lyric_text = trimmed[text_start..].trim().to_string();
        for ts in timestamps {
            lines.push(SyncedLyricLine {
                start_time_secs: ts,
                text: lyric_text.clone(),
            });
        }
    }

    if offset_ms != 0.0 {
        let shift = offset_ms / 1000.0;
        for line in &mut lines {
            // Never let a wild offset produce a negative or non-finite time:
            // the position clock starts at zero.
            line.start_time_secs = (line.start_time_secs - shift).max(0.0);
        }
    }

    lines.sort_by(|a, b| a.start_time_secs.total_cmp(&b.start_time_secs));
    lines
}

/// Recognise an LRC metadata tag (`ar:Artist`, `offset:+500`) and return its
/// value. The key must be alphabetic, which is what keeps `[Chorus]` and
/// `[repeat]` out of this branch.
fn parse_metadata(tag: &str) -> Option<&str> {
    let (key, value) = tag.split_once(':')?;
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(value)
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
    if let Some(content) = read_text_file(&exact) {
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

    read_text_file(&lrc.or(txt)?)
}

/// Read a text file, tolerating the encodings a real library contains.
///
/// `fs::read_to_string` rejects the whole file on a single non-UTF-8 byte, so a
/// lyric sheet written by a Windows tag editor or a Japanese player used to be
/// silently skipped. The bytes are decoded instead, and a guess is logged.
fn read_text_file(path: &Path) -> Option<String> {
    // Security check: cap lyric file size to 1MB to prevent memory exhaustion / DoS attacks.
    const MAX_LYRIC_FILE_SIZE: u64 = 1024 * 1024;
    let meta = fs::metadata(path).ok()?;
    if meta.len() > MAX_LYRIC_FILE_SIZE {
        log::warn!(
            "Lyric file {} exceeds 1MB limit; skipping for safety",
            path.display()
        );
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let (text, guessed) = domain::text::decode_text_bytes(&bytes);
    if guessed {
        log::warn!(
            "{} is not UTF-8; decoded it as Windows-1252",
            path.display()
        );
    }
    Some(text)
}

//! Embedded lyric read-back.
//!
//! Lyrics live in a different tag item depending on the container, and the
//! tag reader that ships with the decoder only understands Vorbis comments:
//! a FLAC with a `LYRICS` field came through, while an MP3 carrying the very
//! common `USLT` frame came back empty. [`lofty`] maps them all onto
//! [`ItemKey::Lyrics`] (`USLT`, `LYRICS`, `©lyr`, …), so it is the one reader
//! used here.
//!
//! Timed lyrics are a second, separate item: ID3v2 stores them in `SYLT`
//! (a list of `(timestamp, text)` pairs) rather than in `USLT` prose. Those are
//! rendered back out as LRC, so a file with synchronized lyrics scrolls with the
//! music instead of showing nothing at all.
//!
//! Reading tags is a header-only operation: no audio is decoded.

use std::path::Path;

use lofty::id3::v2::{FrameFlags, SyncTextContentType, SynchronizedTextFrame, TimestampFormat};
use lofty::prelude::*;
use lofty::tag::{ItemKey, Tag};

/// Read lyrics embedded in a file's tags, if any.
///
/// Timed (`SYLT`) lyrics are converted to LRC and are preferred over unsynced
/// prose when both are present: a file that carries timed lyrics is a file whose
/// author wanted them to scroll. The value is returned verbatim otherwise;
/// callers decide whether it is timed (LRC) or plain prose.
pub fn extract_lyrics(path: &Path) -> Option<String> {
    let tagged = match lofty::read_from_path(path) {
        Ok(tagged) => tagged,
        Err(e) => {
            // Not silent any more: an unreadable header is worth knowing about
            // when a track turns up without lyrics.
            log::debug!("Could not read tags from {}: {e}", path.display());
            return None;
        }
    };

    // Timed lyrics win, then unsynced prose from the primary or any other tag.
    if let Some(lrc) = synced_lyrics(path) {
        return Some(lrc);
    }
    if let Some(tag) = tagged.primary_tag() {
        if let Some(text) = lyrics_in(tag) {
            return Some(text);
        }
    }
    tagged.tags().iter().find_map(lyrics_in)
}

/// Whether a track has any lyrics the player would show: embedded in its tags
/// or in a sidecar file beside it.
///
/// The scanner stores this as `tracks.has_lyrics`, because evaluating a smart
/// playlist must not open every matching file. The check is deliberately the
/// same two sources the lyrics panel reads, so the rule and the panel cannot
/// disagree about what "has lyrics" means.
pub fn has_lyrics(path: &Path) -> bool {
    extract_lyrics(path).is_some() || sidecar_lyrics_exists(path)
}

/// Whether a `.lrc` or `.txt` lyric file with this song's stem exists beside it.
///
/// Case-insensitive, matching `lyrics::find_sidecar_lyrics`: `Song.FLAC` may
/// be accompanied by `song.LRC`.
fn sidecar_lyrics_exists(song_path: &Path) -> bool {
    if song_path.with_extension("lrc").is_file() {
        return true;
    }
    let (Some(stem), Some(dir)) = (
        song_path.file_stem().and_then(|s| s.to_str()),
        song_path.parent(),
    ) else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
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
        if entry_stem.eq_ignore_ascii_case(stem)
            && (ext.eq_ignore_ascii_case("lrc") || ext.eq_ignore_ascii_case("txt"))
        {
            return true;
        }
    }
    false
}

/// Read an ID3v2 `SYLT` frame and render it as LRC text.
///
/// `lofty` keeps `SYLT` as an opaque binary frame and does not expose the
/// containing ID3v2 tag, so the frame is located here with a small ID3v2 header
/// walk — and then handed to `lofty`'s own `SynchronizedTextFrame::parse` for the
/// encoding/UTF-16/terminator work, rather than re-implementing that.
fn synced_lyrics(path: &Path) -> Option<String> {
    let data = read_sylt_frame(path)?;
    let frame = match SynchronizedTextFrame::parse(&data, FrameFlags::default()) {
        Ok(frame) => frame,
        Err(e) => {
            log::warn!("{} has an unreadable SYLT frame: {e}", path.display());
            return None;
        }
    };

    // Only lyrics are lyrics: the same frame type also carries chords and
    // trivia, which should not be rendered as the song's words.
    if frame.content_type != SyncTextContentType::Lyrics || frame.content.is_empty() {
        return None;
    }
    // MPEG-frame timestamps would need the file's frame rate to convert; treat
    // them as unavailable rather than inventing a timeline.
    if frame.timestamp_format != TimestampFormat::MS {
        log::debug!(
            "{} has SYLT lyrics in MPEG-frame units, which cannot be converted to a timeline",
            path.display()
        );
        return None;
    }

    let mut out = String::new();
    for (ms, text) in &frame.content {
        let total_secs = *ms as f64 / 1000.0;
        let minutes = (total_secs / 60.0).floor() as u64;
        let seconds = total_secs - (minutes as f64 * 60.0);
        // LRC's `mm:ss.xx` form.
        out.push_str(&format!("[{minutes:02}:{seconds:05.2}]{text}\n"));
    }
    Some(out)
}

/// Pull the raw bytes of the first `SYLT` frame out of a file's ID3v2 tag.
///
/// ID3v2.3 and ID3v2.4 frames only (ID3v2.2's three-character frames predate
/// `SYLT` in practice). Whole-tag unsynchronisation is undone, because the frame
/// bodies are stored unsynchronised in that case.
fn read_sylt_frame(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 10];
    file.read_exact(&mut header).ok()?;
    if &header[..3] != b"ID3" {
        return None;
    }
    let major = header[3];
    if major != 3 && major != 4 {
        return None;
    }
    let unsynchronised = header[5] & 0x80 != 0;
    let size = synchsafe_u32(&header[6..10])? as usize;

    let mut body = vec![0u8; size];
    file.read_exact(&mut body).ok()?;
    if unsynchronised {
        // 0xFF followed by 0x00 is an escaped 0xFF (or 0x00) in the stream.
        let mut plain = Vec::with_capacity(body.len());
        let mut i = 0;
        while i < body.len() {
            plain.push(body[i]);
            if body[i] == 0xFF && body.get(i + 1) == Some(&0x00) {
                i += 2;
            } else {
                i += 1;
            }
        }
        body = plain;
    }

    let mut pos = 0usize;
    while pos + 10 <= body.len() {
        let id = &body[pos..pos + 4];
        // A zero id marks the padding at the end of the tag.
        if id == [0, 0, 0, 0] {
            return None;
        }
        let frame_size = if major == 4 {
            synchsafe_u32(&body[pos + 4..pos + 8])? as usize
        } else {
            u32::from_be_bytes([body[pos + 4], body[pos + 5], body[pos + 6], body[pos + 7]])
                as usize
        };
        let start = pos + 10;
        let end = start.checked_add(frame_size)?;
        if end > body.len() {
            return None;
        }
        if id == b"SYLT" {
            return Some(body[start..end].to_vec());
        }
        pos = end;
    }

    None
}

/// Decode an ID3v2 synchsafe integer (7 bits per byte).
fn synchsafe_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.len() != 4 || bytes.iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    Some(
        ((bytes[0] as u32) << 21)
            | ((bytes[1] as u32) << 14)
            | ((bytes[2] as u32) << 7)
            | (bytes[3] as u32),
    )
}

/// Pull a non-empty `Lyrics` item out of one tag.
fn lyrics_in(tag: &Tag) -> Option<String> {
    let text = tag
        .get_string(ItemKey::Lyrics)
        // ID3v2 stores unsynchronised lyrics as USLT; lofty maps that to the
        // distinct UnsyncLyrics key rather than Lyrics.
        .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-build a small but valid MP3 carrying a USLT lyrics frame, so the
    /// embedded-tag path can be tested without shipping a binary fixture.
    fn write_mp3(name: &str, tag_id: &[u8; 4], body: &[u8]) -> std::path::PathBuf {
        let mut tag = Vec::new();
        tag.extend_from_slice(tag_id);
        tag.extend_from_slice(&(body.len() as u32).to_be_bytes());
        tag.extend_from_slice(&[0x00, 0x00]); // frame flags
        tag.extend_from_slice(body);

        let mut out = Vec::new();
        out.extend_from_slice(b"ID3");
        out.extend_from_slice(&[0x03, 0x00, 0x00]); // v2.3, no flags
        let size = tag.len() as u32;
        out.extend_from_slice(&[
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&tag);

        // A handful of MPEG-1 Layer III frames (128 kbps, 44.1 kHz) so the file
        // looks like real audio to any other reader.
        for _ in 0..8 {
            out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            out.extend_from_slice(&[0u8; 413]);
        }

        let dir = std::env::temp_dir().join(format!("sp_meta_lyrics_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.mp3"));
        std::fs::write(&path, out).unwrap();
        path
    }

    #[test]
    fn reads_uslt_lyrics_from_an_mp3() {
        let mut body = vec![0x00]; // ISO-8859-1 encoding byte
        body.extend_from_slice(b"eng");
        body.push(0x00); // empty content descriptor
        body.extend_from_slice(b"[00:04.00]Tagged line one\n[00:09.00]Tagged line two");
        let path = write_mp3("uslt", b"USLT", &body);

        let lyrics = extract_lyrics(&path).expect("USLT lyrics should be found");
        assert!(lyrics.contains("Tagged line one"));
    }

    #[test]
    fn a_file_without_lyrics_returns_none() {
        let mut body = vec![0x00];
        body.extend_from_slice(b"Just a title");
        let path = write_mp3("title_only", b"TIT2", &body);
        assert_eq!(extract_lyrics(&path), None);
    }

    #[test]
    fn missing_file_returns_none() {
        assert_eq!(extract_lyrics(Path::new("/nonexistent/nope.flac")), None);
    }

    /// `SYLT` is where MP3s keep *timed* lyrics. It arrived as an opaque binary
    /// frame and was never read, so those files showed no lyrics at all.
    #[test]
    fn reads_sylt_timed_lyrics_as_lrc() {
        let mut body = vec![0x00]; // ISO-8859-1 encoding byte
        body.extend_from_slice(b"eng");
        body.push(0x02); // timestamp unit: milliseconds
        body.push(0x01); // content type: lyrics
        body.push(0x00); // empty description
        for (ms, text) in [(4_000u32, "First line"), (9_500, "Second line")] {
            body.extend_from_slice(text.as_bytes());
            body.push(0x00);
            body.extend_from_slice(&ms.to_be_bytes());
        }
        let path = write_mp3("sylt", b"SYLT", &body);

        let lyrics = extract_lyrics(&path).expect("SYLT lyrics should be found");
        assert!(lyrics.contains("[00:04.00]First line"), "got {lyrics}");
        assert!(lyrics.contains("[00:09.50]Second line"), "got {lyrics}");
    }

    /// The same frame type also carries chords and trivia; those are not words.
    #[test]
    fn non_lyric_sylt_content_is_ignored() {
        let mut body = vec![0x00];
        body.extend_from_slice(b"eng");
        body.push(0x02); // milliseconds
        body.push(0x05); // content type: chord
        body.push(0x00);
        body.extend_from_slice(&1_000u32.to_be_bytes());
        body.extend_from_slice(b"Cmaj7");
        body.push(0x00);
        let path = write_mp3("sylt_chords", b"SYLT", &body);

        assert_eq!(extract_lyrics(&path), None);
    }
}

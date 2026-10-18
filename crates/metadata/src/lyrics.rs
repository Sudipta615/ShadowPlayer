//! Embedded lyric read-back.
//!
//! Lyrics live in a different tag item depending on the container, and the
//! tag reader that ships with the decoder only understands Vorbis comments:
//! a FLAC with a `LYRICS` field came through, while an MP3 carrying the very
//! common `USLT` frame came back empty. [`lofty`] maps them all onto
//! [`ItemKey::Lyrics`] (`USLT`, `LYRICS`, `©lyr`, …), so it is the one reader
//! used here.
//!
//! Reading tags is a header-only operation: no audio is decoded.

use std::path::Path;

use lofty::prelude::*;
use lofty::tag::{ItemKey, Tag};

/// Read unsynced lyrics embedded in a file's tags, if any.
///
/// The value is returned verbatim; callers decide whether it is timed (LRC) or
/// plain prose.
pub fn extract_lyrics(path: &Path) -> Option<String> {
    let tagged = lofty::read_from_path(path).ok()?;

    // Prefer the container's primary tag, then try any other tag present.
    if let Some(tag) = tagged.primary_tag() {
        if let Some(text) = lyrics_in(tag) {
            return Some(text);
        }
    }
    tagged.tags().iter().find_map(lyrics_in)
}

/// Pull a non-empty `Lyrics` item out of one tag.
fn lyrics_in(tag: &Tag) -> Option<String> {
    let text = tag
        .get_string(&ItemKey::Lyrics)
        // Some taggers write the field without registering it as a known key.
        .or_else(|| tag.get_string(&ItemKey::Unknown("UNSYNCEDLYRICS".to_string())))?;

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
}

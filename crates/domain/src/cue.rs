//! The CUE "virtual path" convention.
//!
//! A standalone `.cue` sheet splits one album image into several logical
//! tracks, so the library stores each of them under a synthetic path:
//!
//! ```text
//! /music/album/image.flac#cue:3#/music/album/album.cue
//!  └──── audio file ────┘ └track┘ └───── sheet ─────┘
//! ```
//!
//! Both the indexer (`metadata`) and the player (`playback`) need to agree on
//! this shape, so it lives here rather than in either of them. The sheet
//! component is optional: rows written before it was recorded only carry
//! `#cue:<n>`, and callers must keep working for them.

use std::path::{Path, PathBuf};

/// The marker separating the audio file from the CUE track number.
pub const CUE_MARKER: &str = "#cue:";

/// A decomposed CUE virtual path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueVirtualPath<'a> {
    /// The album image the sheet indexes.
    pub audio: &'a str,
    /// 1-based track number within the sheet.
    pub track_number: u32,
    /// The `.cue` sheet, when the row recorded it.
    pub sheet: Option<&'a str>,
}

impl CueVirtualPath<'_> {
    /// The album image as a path.
    pub fn audio_path(&self) -> PathBuf {
        PathBuf::from(self.audio)
    }

    /// The sheet as a path, when recorded.
    pub fn sheet_path(&self) -> Option<PathBuf> {
        self.sheet.map(PathBuf::from)
    }
}

/// Build the virtual path for one track of a CUE sheet.
pub fn format_cue_virtual_path(audio: &Path, track_number: u32, sheet: &Path) -> String {
    format!(
        "{}#cue:{}#{}",
        audio.display(),
        track_number,
        sheet.display()
    )
}

/// Split a path into its CUE components, or `None` when it is a real file.
pub fn parse_cue_virtual_path(path: &str) -> Option<CueVirtualPath<'_>> {
    let (audio, rest) = path.split_once(CUE_MARKER)?;
    let (number, sheet) = match rest.split_once('#') {
        Some((number, sheet)) if !sheet.is_empty() => (number, Some(sheet)),
        Some((number, _)) => (number, None),
        None => (rest, None),
    };
    let track_number = number.parse().ok()?;
    if audio.is_empty() {
        return None;
    }
    Some(CueVirtualPath {
        audio,
        track_number,
        sheet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn round_trips_a_virtual_path() {
        let text = format_cue_virtual_path(
            Path::new("/music/album/image.flac"),
            3,
            Path::new("/music/album/album.cue"),
        );
        assert_eq!(
            text,
            "/music/album/image.flac#cue:3#/music/album/album.cue"
        );
        let parsed = parse_cue_virtual_path(&text).expect("parses");
        assert_eq!(parsed.audio, "/music/album/image.flac");
        assert_eq!(parsed.track_number, 3);
        assert_eq!(parsed.sheet, Some("/music/album/album.cue"));
    }

    #[test]
    fn legacy_rows_without_a_sheet_still_parse() {
        let parsed = parse_cue_virtual_path("/music/image.flac#cue:12").expect("parses");
        assert_eq!(parsed.audio, "/music/image.flac");
        assert_eq!(parsed.track_number, 12);
        assert_eq!(parsed.sheet, None);
    }

    #[test]
    fn ordinary_paths_are_not_cue_paths() {
        assert!(parse_cue_virtual_path("/music/track.flac").is_none());
        assert!(parse_cue_virtual_path("/music/#cue:.flac").is_none());
        assert!(parse_cue_virtual_path("/music/a.flac#cue:notanumber").is_none());
    }
}

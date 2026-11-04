use super::{
    parse_asx, parse_pls, parse_wpl, parse_xspf, write_asx, write_pls, write_wpl, write_xspf,
};
use super::{parse_m3u, write_m3u};
use super::{M3uEntry, ParsedPlaylist, PlaylistFormat};

use std::path::{Path, PathBuf};

fn parse_playlist_text(text: &str, path: &Path, format: PlaylistFormat) -> ParsedPlaylist {
    let base = path.parent();
    match format {
        PlaylistFormat::M3u | PlaylistFormat::M3u8 => ParsedPlaylist {
            title: None,
            entries: parse_m3u(text, base),
        },
        PlaylistFormat::Pls => parse_pls(text, base),
        PlaylistFormat::Xspf => parse_xspf(text, base),
        PlaylistFormat::Asx => parse_asx(text, base),
        PlaylistFormat::Wpl => parse_wpl(text, base),
    }
}

/// Serialize a playlist in `format`.
///
/// `base_dir` is the directory the playlist is being written into. Formats
/// that accept relative paths use one where possible, so a playlist moved
/// together with its music keeps working.
fn write_playlist(format: PlaylistFormat, entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    match format {
        PlaylistFormat::M3u => write_m3u(entries, base_dir),
        PlaylistFormat::M3u8 => {
            let body = write_m3u(entries, base_dir);
            // A BOM marks the file as UTF-8 for players that otherwise guess a
            // legacy code page. ASCII-only playlists need no marker and stay
            // readable by the oldest readers.
            if body.is_ascii() {
                body
            } else {
                format!("\u{feff}{body}")
            }
        }
        PlaylistFormat::Pls => write_pls(entries, base_dir),
        PlaylistFormat::Xspf => write_xspf(entries),
        PlaylistFormat::Asx => write_asx(entries, base_dir),
        PlaylistFormat::Wpl => write_wpl(entries, base_dir),
    }
}

// ── PLS ─────────────────────────────────────────────────────────────────────

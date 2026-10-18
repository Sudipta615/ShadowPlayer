//! Extended M3U (`.m3u` / `.m3u8`) playlist reading and writing.
//!
//! This is deliberately a small, dependency-free parser: M3U is a line-oriented
//! format with one optional `#EXTINF:<seconds>,<Artist> - <Title>` line before
//! each path. Real-world files are sloppy — BOMs, CRLF, Windows separators,
//! bare comments, blank lines, absolute and relative paths — so the parser
//! accepts all of those and never fails: an unreadable file simply yields no
//! entries, and unreadable *lines* are skipped.

use std::path::{Path, PathBuf};

/// One entry in a playlist: a file path plus whatever tagging the
/// `#EXTINF` line carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uEntry {
    pub path: PathBuf,
    pub duration_secs: Option<u64>,
    pub artist: Option<String>,
    pub title: Option<String>,
}

impl M3uEntry {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            duration_secs: None,
            artist: None,
            title: None,
        }
    }

    /// `"Artist - Title"`, `"Title"`, or `None` when neither is known.
    pub fn display_name(&self) -> Option<String> {
        match (&self.artist, &self.title) {
            (Some(a), Some(t)) => Some(format!("{a} - {t}")),
            (None, Some(t)) => Some(t.clone()),
            (Some(a), None) => Some(a.clone()),
            (None, None) => None,
        }
    }
}

/// Strip a UTF-8 BOM, if present.
fn strip_bom(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
}

/// Resolve a playlist path token against the playlist's directory. Handles
/// Windows separators and `file://` URIs so playlists exported elsewhere still
/// resolve. Absolute paths are returned as-is.
fn resolve(raw: &str, base_dir: Option<&Path>) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"');
    let unescaped = trimmed.replace('\\', "/");

    // file:///home/me/song.flac -> /home/me/song.flac
    if let Some(rest) = unescaped.strip_prefix("file://") {
        return PathBuf::from(rest);
    }

    let p = PathBuf::from(&unescaped);
    if p.is_absolute() {
        return p;
    }
    match base_dir {
        Some(base) => base.join(p),
        None => p,
    }
}

/// Parse `#EXTINF:<len>,<name>` into a duration and an artist/title split.
fn parse_extinf(line: &str) -> (Option<u64>, Option<String>, Option<String>) {
    let rest = match line.strip_prefix("#EXTINF:") {
        Some(r) => r,
        None => return (None, None, None),
    };

    // Everything before the first comma is the (possibly fractional) length;
    // everything after is the display name.
    let (len_part, name_part) = match rest.split_once(',') {
        Some((l, n)) => (l.trim(), n.trim()),
        None => (rest.trim(), ""),
    };

    let duration = len_part
        .parse::<f64>()
        .ok()
        .map(|f| if f.is_finite() && f > 0.0 { f.round() as u64 } else { 0 });

    if name_part.is_empty() {
        return (duration, None, None);
    }
    // Split on " - " (the conventional separator) at most once.
    match name_part.split_once(" - ") {
        Some((artist, title)) => (
            duration,
            Some(artist.trim().to_string()),
            Some(title.trim().to_string()),
        ),
        None => (duration, None, Some(name_part.to_string())),
    }
}

/// Parse M3U text. `base_dir` resolves relative paths (pass the playlist's
/// directory; `None` keeps them relative to the process CWD).
pub fn parse_m3u(text: &str, base_dir: Option<&Path>) -> Vec<M3uEntry> {
    let text = strip_bom(text);
    let mut entries: Vec<M3uEntry> = Vec::new();
    // Pending #EXTINF fields, applied to the next non-comment line.
    let mut pending: Option<(Option<u64>, Option<String>, Option<String>)> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(ext) = line.strip_prefix("#EXTINF") {
            // Re-add the prefix stripped above so parse_extinf sees it.
            pending = Some(parse_extinf(&format!("#EXTINF{ext}")));
            continue;
        }
        if line.starts_with('#') {
            // Header or arbitrary comment.
            continue;
        }

        let (duration, artist, title) = pending.take().unwrap_or((None, None, None));
        entries.push(M3uEntry {
            path: resolve(line, base_dir),
            duration_secs: duration,
            artist,
            title,
        });
    }

    entries
}

/// Read and parse an M3U file, resolving relative paths against its directory.
pub fn parse_m3u_file(path: &Path) -> std::io::Result<Vec<M3uEntry>> {
    let text = std::fs::read_to_string(path)?;
    let base = path.parent();
    Ok(parse_m3u(&text, base))
}

/// Serialize entries as extended M3U. When `base_dir` is given, paths inside it
/// are written relative (with `/` separators) so the file stays portable; paths
/// outside it stay absolute.
pub fn write_m3u(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from("#EXTM3U\n");
    for e in entries {
        let len = e.duration_secs.unwrap_or(0);
        let name = e.display_name().unwrap_or_default();
        out.push_str(&format!("#EXTINF:{len},{name}\n"));

        let path = match base_dir.and_then(|b| e.path.strip_prefix(b).ok()) {
            Some(rel) => rel.to_path_buf(),
            None => e.path.clone(),
        };
        let mut text = path.to_string_lossy().replace('\\', "/");
        // M3U has no escaping; a path that itself begins with '#' would be
        // parsed back as a comment, so fall back to the absolute form.
        if text.starts_with('#') {
            text = e.path.to_string_lossy().replace('\\', "/");
        }
        out.push_str(&text);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extinf_and_paths() {
        let text = "#EXTM3U\n\
                    #EXTINF:213,Radiohead - Everything In Its Right Place\n\
                    Kid A/01 Everything.flac\n\
                    \n\
                    # a comment\n\
                    #EXTINF:-1,Live Set\n\
                    /music/live.flac\n";
        let dir = Path::new("/music");
        let entries = parse_m3u(text, Some(dir));

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, Path::new("/music/Kid A/01 Everything.flac"));
        assert_eq!(entries[0].duration_secs, Some(213));
        assert_eq!(entries[0].artist.as_deref(), Some("Radiohead"));
        assert_eq!(
            entries[0].title.as_deref(),
            Some("Everything In Its Right Place")
        );
        // Absolute paths are kept as-is; a negative/unknown length becomes 0.
        assert_eq!(entries[1].path, Path::new("/music/live.flac"));
        assert_eq!(entries[1].duration_secs, Some(0));
        assert_eq!(entries[1].title.as_deref(), Some("Live Set"));
    }

    #[test]
    fn tolerates_bom_crlf_and_windows_separators() {
        let text = "\u{feff}#EXTM3U\r\n#EXTINF:60,A - B\r\nAlbums\\Disc 1\\01.flac\r\n";
        let entries = parse_m3u(text, Some(Path::new("/lib")));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, Path::new("/lib/Albums/Disc 1/01.flac"));
    }

    #[test]
    fn accepts_a_bare_list_without_headers() {
        let entries = parse_m3u("/a.flac\n/b.flac\n", None);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, None);
    }

    #[test]
    fn write_then_parse_round_trips() {
        let dir = Path::new("/music");
        let entries = vec![
            M3uEntry {
                path: PathBuf::from("/music/Artist/Album/01 Song.flac"),
                duration_secs: Some(215),
                artist: Some("Artist".into()),
                title: Some("Song".into()),
            },
            M3uEntry::new("/outside/other.mp3"),
        ];

        let text = write_m3u(&entries, Some(dir));
        assert!(text.starts_with("#EXTM3U\n"));
        assert!(text.contains("#EXTINF:215,Artist - Song\n"));
        assert!(text.contains("Artist/Album/01 Song.flac\n"));
        // Outside the base dir → absolute.
        assert!(text.contains("/outside/other.mp3\n"));

        let parsed = parse_m3u(&text, Some(dir));
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].path, PathBuf::from("/music/Artist/Album/01 Song.flac"));
        assert_eq!(parsed[0].duration_secs, Some(215));
        assert_eq!(parsed[0].artist.as_deref(), Some("Artist"));
        assert_eq!(parsed[0].title.as_deref(), Some("Song"));
        assert_eq!(parsed[1].path, PathBuf::from("/outside/other.mp3"));
    }
}

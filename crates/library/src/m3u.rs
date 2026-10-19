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

/// Decode `%xx` escapes, as used in `file://` URIs.
///
/// `file:///music/My%20Song.flac` is what every player exports when a name has
/// a space in it, and the literal `%20` used to be handed to the filesystem as
/// part of the name — the track was simply "not found". A malformed escape
/// (`%zz`, or a `%` at the end) is left alone rather than dropped.
fn percent_decode(s: &str) -> String {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                decoded.push((high << 4) | low);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }

    // Percent-encoded paths hold UTF-8 in practice; the shared decoder keeps
    // the odd legacy encoding readable instead of failing on it.
    domain::text::decode_text_bytes(&decoded).0
}

/// Resolve a playlist path token against the playlist's directory. Handles
/// Windows separators, `file://` URIs and their `%xx` escapes so playlists
/// exported elsewhere still resolve. Absolute paths are returned as-is.
fn resolve(raw: &str, base_dir: Option<&Path>) -> PathBuf {
    let trimmed = raw.trim().trim_matches('"');
    let unescaped = trimmed.replace('\\', "/");

    // file:///home/me/song.flac -> /home/me/song.flac
    if let Some(rest) = unescaped.strip_prefix("file://") {
        // RFC 8089 allows a host, but only localhost is meaningful for a
        // playlist: `file://localhost/music/x.flac`.
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode(rest);
        // `file:///C:/Music/x.flac` carries the slash the URI form requires;
        // the filesystem path on Windows does not.
        let decoded = match decoded.as_bytes() {
            [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => decoded[1..].to_string(),
            _ => decoded,
        };
        return PathBuf::from(decoded);
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
    // Tolerate the encodings a playlist exported elsewhere may use.
    let bytes = std::fs::read(path)?;
    let (text, guessed) = domain::text::decode_text_bytes(&bytes);
    if guessed {
        log::warn!(
            "{} is not UTF-8; decoded it as Windows-1252",
            path.display()
        );
    }
    let base = path.parent();
    Ok(parse_m3u(&text, base))
}

/// `path` relative to `base_dir`, compared on real paths.
///
/// A plain `strip_prefix` compares the text of the two paths, so a library root
/// reached through a symlink (or spelled with a trailing `..`) never matched and
/// every entry was written as an absolute path — a playlist that silently only
/// worked on the machine that wrote it. Both sides are canonicalised first, and
/// the plain comparison is kept as the fallback for paths that no longer exist.
fn relative_to(path: &Path, base_dir: &Path) -> Option<PathBuf> {
    if let (Ok(real_path), Ok(real_base)) = (
        std::fs::canonicalize(path),
        std::fs::canonicalize(base_dir),
    ) {
        if let Ok(rel) = real_path.strip_prefix(&real_base) {
            return Some(rel.to_path_buf());
        }
    }
    path.strip_prefix(base_dir).ok().map(Path::to_path_buf)
}

/// Serialize entries as extended M3U. When `base_dir` is given, paths inside it
/// are written relative (with `/` separators) so the file stays portable; paths
/// outside it stay absolute.
///
/// An entry whose path contains a line break cannot be represented — M3U has no
/// escaping, so writing it would silently split into two lines, one of them a
/// bogus path. Such entries are skipped and noted in a comment rather than
/// corrupting the file.
pub fn write_m3u(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from("#EXTM3U\n");
    for e in entries {
        let mut text = match base_dir.and_then(|b| relative_to(&e.path, b)) {
            Some(rel) => rel.to_string_lossy().replace('\\', "/"),
            None => e.path.to_string_lossy().replace('\\', "/"),
        };
        // M3U has no escaping; a path that itself begins with '#' would be
        // parsed back as a comment, so fall back to the absolute form.
        if text.starts_with('#') {
            text = e.path.to_string_lossy().replace('\\', "/");
        }
        if text.contains(['\n', '\r']) {
            log::warn!(
                "Skipping a playlist entry whose path contains a line break: {:?}",
                e.path
            );
            out.push_str("# ShadowPlayer: skipped an entry with a line break in its path\n");
            continue;
        }

        let len = e.duration_secs.unwrap_or(0);
        // A line break in the display name would break the record just as
        // badly; the name is cosmetic, so a space is a faithful-enough repair.
        let name = e
            .display_name()
            .unwrap_or_default()
            .replace(['\n', '\r'], " ");
        out.push_str(&format!("#EXTINF:{len},{name}\n"));
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

    /// `file://` URIs carry `%xx` escapes; a literal `%20` is a file that does
    /// not exist.
    #[test]
    fn percent_escapes_in_file_uris_are_decoded() {
        let entries = parse_m3u(
            "file:///music/Artist/My%20Song.flac\n\
             file://localhost/music/Don%27t%20Stop.flac\n\
             file:///music/caf%C3%A9.flac\n\
             file:///C:/Users/me/Music/01.flac\n",
            None,
        );
        assert_eq!(entries[0].path, PathBuf::from("/music/Artist/My Song.flac"));
        assert_eq!(entries[1].path, PathBuf::from("/music/Don't Stop.flac"));
        assert_eq!(entries[2].path, PathBuf::from("/music/café.flac"));
        // A malformed escape is left alone instead of eating the character.
        let broken = parse_m3u("/music/100%zz.flac", None);
        assert_eq!(broken[0].path, PathBuf::from("/music/100%zz.flac"));
    }

    /// A library root reached through a symlink must still produce relative
    /// entries, or the playlist only works on the machine that wrote it.
    #[test]
    fn relative_paths_survive_a_symlinked_base_dir() {
        let root = std::env::temp_dir().join(format!("sp_m3u_rel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("real/Album")).unwrap();
        std::fs::write(root.join("real/Album/01.flac"), b"x").unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        let entry = M3uEntry::new(root.join("real/Album/01.flac"));
        // Base dir spelled the other way round: same directory, different text.
        let text = write_m3u(&[entry], Some(&root.join("link")));
        assert!(
            text.contains("\nAlbum/01.flac\n"),
            "expected a relative path, got:\n{text}"
        );
    }

    /// A filename with a line break cannot be written faithfully; it must not
    /// produce a playlist that parses back as two bogus entries.
    #[test]
    fn a_path_with_a_line_break_is_skipped_whole() {
        let entries = vec![
            M3uEntry::new("/music/fine.flac"),
            M3uEntry::new("/music/two\nlines.flac"),
        ];
        let text = write_m3u(&entries, None);
        assert!(text.contains("/music/fine.flac\n"));
        assert!(!text.contains("two\nlines.flac"));

        let parsed = parse_m3u(&text, None);
        assert_eq!(parsed.len(), 1, "the broken entry must not reappear");
        assert_eq!(parsed[0].path, PathBuf::from("/music/fine.flac"));
    }

    /// The same guard on the display name, which is cosmetic.
    #[test]
    fn a_display_name_with_a_line_break_is_flattened() {
        let entry = M3uEntry {
            path: PathBuf::from("/music/x.flac"),
            duration_secs: Some(3),
            artist: Some("A".into()),
            title: Some("B\nC".into()),
        };
        let text = write_m3u(&[entry], None);
        assert!(text.contains("#EXTINF:3,A - B C\n"), "got:\n{text}");
    }

    /// Playlists written by non-UTF-8 tools still load.
    #[test]
    fn a_non_utf8_playlist_file_is_decoded() {
        let dir = std::env::temp_dir().join(format!("sp_m3u_enc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("latin1.m3u");
        let mut bytes = b"#EXTM3U\n".to_vec();
        bytes.extend_from_slice(&[b'S', b'o', b'n', b'g', b' ', 0xE9, b'\n']);
        std::fs::write(&file, bytes).unwrap();

        let entries = parse_m3u_file(&file).unwrap();
        assert_eq!(entries[0].path, dir.join("Song é"));
    }
}

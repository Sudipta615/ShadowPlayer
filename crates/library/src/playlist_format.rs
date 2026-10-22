//! Playlist format sniffing, parsing and writing.
//!
//! M3U is the lingua franca, but every desktop player has its own dialect and
//! users arrive with playlists from all of them. A `.pls` file exported by
//! foobar2000, an XSPF from Clementine, an ASX left behind by Windows Media
//! Player and a WPL from MusicBee all describe the same thing: an ordered list
//! of paths. This module is the one place that knows how each spells it.
//!
//! Sniffing is by content first, extension second. Exporters rename files, and
//! a PLS file called `mix.m3u` is still a PLS file; refusing it because of its
//! suffix would be the kind of bug that makes an import feature look broken.
//!
//! Parsing is deliberately forgiving and dependency-free. Real playlists carry
//! BOMs, CRLF endings, Windows paths, relative paths and hand-edited XML. An
//! entry that cannot be read is skipped; one bad line must not lose the other
//! hundred tracks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::m3u::{parse_m3u, relative_to, resolve, write_m3u, M3uEntry};

/// The playlist dialects ShadowPlayer can read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlaylistFormat {
    /// Legacy M3U, written as UTF-8.
    M3u,
    /// Extended M3U with a UTF-8 BOM when the contents need one.
    ///
    /// New exports default to the UTF-8 dialect: it is the one that carries
    /// non-ASCII paths without guessing a legacy code page.
    #[default]
    M3u8,
    /// Winamp / foobar2000 PLS.
    Pls,
    /// XML Shareable Playlist Format (XSPF).
    Xspf,
    /// Microsoft ASX.
    Asx,
    /// Windows Media Player playlist (WPL).
    Wpl,
}

impl PlaylistFormat {
    pub const ALL: [PlaylistFormat; 6] = [
        PlaylistFormat::M3u8,
        PlaylistFormat::M3u,
        PlaylistFormat::Pls,
        PlaylistFormat::Xspf,
        PlaylistFormat::Asx,
        PlaylistFormat::Wpl,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            PlaylistFormat::M3u => "m3u",
            PlaylistFormat::M3u8 => "m3u8",
            PlaylistFormat::Pls => "pls",
            PlaylistFormat::Xspf => "xspf",
            PlaylistFormat::Asx => "asx",
            PlaylistFormat::Wpl => "wpl",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlaylistFormat::M3u => "M3U",
            PlaylistFormat::M3u8 => "M3U8 (UTF-8)",
            PlaylistFormat::Pls => "PLS",
            PlaylistFormat::Xspf => "XSPF",
            PlaylistFormat::Asx => "ASX",
            PlaylistFormat::Wpl => "WPL",
        }
    }

    /// The extension shown in a save dialog's file-type filter.
    pub fn filter_name(self) -> String {
        format!("{} playlist (*.{})", self.label(), self.extension())
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "m3u" => Some(PlaylistFormat::M3u),
            "m3u8" => Some(PlaylistFormat::M3u8),
            "pls" => Some(PlaylistFormat::Pls),
            "xspf" => Some(PlaylistFormat::Xspf),
            "asx" | "wax" => Some(PlaylistFormat::Asx),
            "wpl" => Some(PlaylistFormat::Wpl),
            _ => None,
        }
    }

    /// Guess from a path's extension, falling back to M3U8 (the format we
    /// prefer for new exports).
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
            .unwrap_or(PlaylistFormat::M3u8)
    }
}

/// A playlist parsed from one file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedPlaylist {
    /// The playlist's own title, where the format carries one.
    pub title: Option<String>,
    pub entries: Vec<M3uEntry>,
}

/// Guess a playlist's format from its bytes, using the extension only when the
/// content is silent.
///
/// A mislabelled file is common: players export PLS data with an `.m3u` name,
/// and download services sometimes serve XSPF as text. Content wins.
pub fn sniff_format(bytes: &[u8], path: Option<&Path>) -> PlaylistFormat {
    let (text, _) = domain::text::decode_text_bytes(bytes);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("[playlist]") || lower.contains("\n[playlist]") {
        return PlaylistFormat::Pls;
    }
    if lower.starts_with("<?wpl") || lower.contains("<wpl") {
        return PlaylistFormat::Wpl;
    }
    if lower.starts_with("<asx") || lower.contains("<asx ") || lower.contains("<asx>") {
        return PlaylistFormat::Asx;
    }
    if lower.contains("xspf.org/ns/0") || lower.starts_with("<playlist") {
        return PlaylistFormat::Xspf;
    }
    if lower.starts_with("#extm3u") {
        // A BOM and non-ASCII content are the reason M3U8 exists; without a
        // path, a BOM is the only evidence that this was the UTF-8 dialect.
        return match path.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("m3u8") => PlaylistFormat::M3u8,
            _ => PlaylistFormat::M3u,
        };
    }

    path.and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .and_then(PlaylistFormat::from_extension)
        .unwrap_or(PlaylistFormat::M3u)
}

/// Read a playlist file, whatever its format.
pub fn parse_playlist(path: &Path) -> std::io::Result<Vec<ParsedPlaylist>> {
    let bytes = std::fs::read(path)?;
    let (text, guessed) = domain::text::decode_text_bytes(&bytes);
    if guessed {
        log::warn!(
            "{} is not UTF-8; decoded it as Windows-1252",
            path.display()
        );
    }
    let format = sniff_format(&bytes, Some(path));
    Ok(vec![parse_playlist_text(&text, path, format)])
}

/// Parse playlist text as a known format.
pub fn parse_playlist_text(text: &str, path: &Path, format: PlaylistFormat) -> ParsedPlaylist {
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
pub fn write_playlist(
    format: PlaylistFormat,
    entries: &[M3uEntry],
    base_dir: Option<&Path>,
) -> String {
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

fn parse_pls(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    // PLS keys are unordered and indices are not guaranteed contiguous. Keep
    // them in index order so the playlist plays in the order it was exported.
    let mut files: BTreeMap<u32, PathBuf> = BTreeMap::new();
    let mut titles: BTreeMap<u32, String> = BTreeMap::new();
    let mut lengths: BTreeMap<u32, u64> = BTreeMap::new();

    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let lower = key.to_ascii_lowercase();
        let Some((kind, index)) = split_indexed_key(&lower) else {
            continue;
        };
        match kind {
            "file" => {
                if !value.is_empty() {
                    files.insert(index, resolve(value, base_dir));
                }
            }
            "title" => {
                if !value.is_empty() {
                    titles.insert(index, value.to_string());
                }
            }
            "length" => {
                if let Ok(seconds) = value.parse::<i64>() {
                    if seconds >= 0 {
                        lengths.insert(index, seconds as u64);
                    }
                }
            }
            _ => {}
        }
    }

    let mut entries = Vec::with_capacity(files.len());
    for (index, path) in files {
        let title = titles.remove(&index);
        let (artist, title) = split_artist_title(title);
        entries.push(M3uEntry {
            path,
            duration_secs: lengths.remove(&index),
            artist,
            title,
        });
    }
    ParsedPlaylist {
        title: None,
        entries,
    }
}

fn split_indexed_key(key: &str) -> Option<(&str, u32)> {
    let digit_start = key.find(|c: char| c.is_ascii_digit())?;
    if digit_start == 0 {
        return None;
    }
    let (kind, digits) = key.split_at(digit_start);
    let index = digits.parse().ok()?;
    Some((kind, index))
}

fn write_pls(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from("[playlist]\n");
    let mut number = 1usize;
    for entry in entries {
        let path = path_for_playlist(&entry.path, base_dir);
        // Everything after the first `=` is the value, so `=` itself is a
        // legal filename character; only a line break cannot be represented.
        if path.contains(['\n', '\r']) {
            log::warn!(
                "Skipping PLS entry with an unrepresentable path: {:?}",
                entry.path
            );
            continue;
        }
        out.push_str(&format!("File{number}={path}\n"));
        if let Some(name) = entry.display_name() {
            if !name.contains(['\n', '\r']) {
                out.push_str(&format!("Title{number}={name}\n"));
            }
        }
        let length = entry.duration_secs.unwrap_or(0);
        out.push_str(&format!("Length{number}={length}\n"));
        number += 1;
    }
    out.push_str(&format!("NumberOfEntries={}\n", number - 1));
    out.push_str("Version=2\n");
    out
}

// ── XSPF ────────────────────────────────────────────────────────────────────

fn parse_xspf(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let title = tag_contents(text, "title")
        .into_iter()
        .next()
        .map(decode_xml_entities)
        .filter(|s| !s.trim().is_empty());

    let mut entries = Vec::new();
    for track in tag_contents(text, "track") {
        let location = tag_contents(track, "location")
            .into_iter()
            .next()
            .map(decode_xml_entities);
        let Some(location) = location else {
            continue;
        };
        let location = location.trim();
        if location.is_empty() {
            continue;
        }
        let path = resolve(location, base_dir);
        let track_title = tag_contents(track, "title")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        let artist = tag_contents(track, "creator")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        // XSPF durations are milliseconds; M3UEntry carries seconds.
        let duration_secs = tag_contents(track, "duration")
            .into_iter()
            .next()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|ms| ms / 1000);
        entries.push(M3uEntry {
            path,
            duration_secs,
            artist,
            title: track_title,
        });
    }

    ParsedPlaylist { title, entries }
}

fn write_xspf(entries: &[M3uEntry]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n\
         \x20 <title>ShadowPlayer playlist</title>\n\
         \x20 <trackList>\n",
    );
    for entry in entries {
        out.push_str("    <track>\n");
        out.push_str(&format!(
            "      <location>{}</location>\n",
            escape_xml(&file_uri(&entry.path))
        ));
        if let Some(title) = entry.title.as_deref() {
            out.push_str(&format!("      <title>{}</title>\n", escape_xml(title)));
        }
        if let Some(artist) = entry.artist.as_deref() {
            out.push_str(&format!(
                "      <creator>{}</creator>\n",
                escape_xml(artist)
            ));
        }
        if let Some(seconds) = entry.duration_secs {
            out.push_str(&format!("      <duration>{}</duration>\n", seconds * 1000));
        }
        out.push_str("    </track>\n");
    }
    out.push_str("  </trackList>\n</playlist>\n");
    out
}

// ── ASX ─────────────────────────────────────────────────────────────────────

fn parse_asx(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let title = tag_contents(text, "title")
        .into_iter()
        .next()
        .map(decode_xml_entities)
        .filter(|s| !s.trim().is_empty());

    let mut entries = Vec::new();
    for entry in tag_contents(text, "entry") {
        let Some(path) = first_attr(entry, "ref", "href") else {
            continue;
        };
        let path = resolve(&path, base_dir);
        let entry_title = tag_contents(entry, "title")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        entries.push(M3uEntry {
            path,
            duration_secs: None,
            artist: None,
            title: entry_title,
        });
    }

    // Some ASX files put `<ref>` straight under `<asx>` with no `<entry>`.
    if entries.is_empty() {
        for tag in all_tags(text, "ref") {
            if let Some(href) = attr(tag, "href") {
                entries.push(M3uEntry::new(resolve(&href, base_dir)));
            }
        }
    }

    ParsedPlaylist { title, entries }
}

fn write_asx(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from("<asx version=\"3.0\">\n  <title>ShadowPlayer playlist</title>\n");
    for entry in entries {
        out.push_str("  <entry>\n");
        if let Some(name) = entry.display_name() {
            out.push_str(&format!("    <title>{}</title>\n", escape_xml(&name)));
        }
        out.push_str(&format!(
            "    <ref href=\"{}\" />\n",
            escape_xml(&path_for_playlist(&entry.path, base_dir))
        ));
        out.push_str("  </entry>\n");
    }
    out.push_str("</asx>\n");
    out
}

// ── WPL ─────────────────────────────────────────────────────────────────────

fn parse_wpl(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let mut entries = Vec::new();
    for tag in all_tags(text, "media") {
        if let Some(src) = attr(tag, "src") {
            if !src.trim().is_empty() {
                entries.push(M3uEntry::new(resolve(&src, base_dir)));
            }
        }
    }
    ParsedPlaylist {
        title: None,
        entries,
    }
}

fn write_wpl(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from(
        "<?wpl version=\"1.0\"?>\n\
         <smil>\n\
         \x20 <head>\n\
         \x20   <meta name=\"Generator\" content=\"ShadowPlayer\"/>\n\
         \x20 </head>\n\
         \x20 <body>\n\
         \x20   <seq>\n",
    );
    for entry in entries {
        out.push_str(&format!(
            "      <media src=\"{}\"/>\n",
            escape_xml(&path_for_playlist(&entry.path, base_dir))
        ));
    }
    out.push_str("    </seq>\n  </body>\n</smil>\n");
    out
}

// ── Shared XML helpers ──────────────────────────────────────────────────────

/// The text between every `<tag ...>...</tag>` pair.
///
/// A deliberately small scanner rather than an XML dependency: playlists are
/// flat, and the parser must not fail the whole import because one file has an
/// unclosed cosmetic element.
fn tag_contents<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&open) {
        let start = from + rel;
        let after_name = start + open.len();
        // `<trackList>` must not match `<track>`.
        if lower[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            from = after_name;
            continue;
        }
        let Some(gt) = lower[after_name..].find('>') else {
            break;
        };
        let content_start = after_name + gt + 1;
        // Self-closing elements have no contents.
        if lower[after_name..content_start].trim_end().ends_with('/') {
            from = content_start;
            continue;
        }
        let Some(close_rel) = lower[content_start..].find(&close) else {
            break;
        };
        let content_end = content_start + close_rel;
        out.push(&xml[content_start..content_end]);
        from = content_end + close.len();
    }
    out
}

/// Every opening tag named `tag`, as the text from `<` through `>`.
fn all_tags<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let lower = xml.to_ascii_lowercase();
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&open) {
        let start = from + rel;
        let after_name = start + open.len();
        if lower[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            from = after_name;
            continue;
        }
        let Some(gt) = lower[after_name..].find('>') else {
            break;
        };
        let end = after_name + gt + 1;
        out.push(&xml[start..end]);
        from = end;
    }
    out
}

fn first_attr(tag: &str, element: &str, attribute: &str) -> Option<String> {
    all_tags(tag, element)
        .into_iter()
        .find_map(|t| attr(t, attribute))
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = name.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find(&needle) {
        let start = from + rel;
        // A window function must match the whole attribute name.
        let before_ok = start == 0
            || lower[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_whitespace());
        let after = start + needle.len();
        let after_ok = lower[after..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '=');
        if !before_ok || !after_ok {
            from = after;
            continue;
        }
        let mut cursor = after;
        while lower[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            cursor += lower[cursor..].chars().next()?.len_utf8();
        }
        if !lower[cursor..].starts_with('=') {
            from = after;
            continue;
        }
        cursor += 1;
        while lower[cursor..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            cursor += lower[cursor..].chars().next()?.len_utf8();
        }
        let quote = tag[cursor..].chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_start = cursor + quote.len_utf8();
        let value_rest = &tag[value_start..];
        let value_end = value_rest.find(quote)?;
        return Some(decode_xml_entities(&value_rest[..value_end]));
    }
    None
}

pub(crate) fn decode_xml_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let Some(semi) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let entity = &after[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&after[..=semi]),
        }
        rest = &after[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// A path as it should appear in a text/XML playlist.
fn path_for_playlist(path: &Path, base_dir: Option<&Path>) -> String {
    let chosen = base_dir
        .and_then(|base| relative_to(path, base))
        .unwrap_or_else(|| path.to_path_buf());
    chosen.to_string_lossy().replace('\\', "/")
}

/// A `file://` URI for XSPF. The URI form is the portable one across Windows,
/// macOS and Linux; XSPF's `<location>` is explicitly a URI.
fn file_uri(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    // A Windows drive or UNC path is not absolute according to a Unix host's
    // `Path`, but its URI form is still known. Detect the shape before
    // resolving against the current directory, or an exported XSPF from a
    // mixed-platform library would point into the exporter's working tree.
    let windows_absolute = raw.starts_with("//")
        || (raw.len() >= 2 && raw.as_bytes()[0].is_ascii_alphabetic() && raw.as_bytes()[1] == b':');
    let text = if path.is_absolute() || windows_absolute {
        raw
    } else {
        let absolute = std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf());
        absolute.to_string_lossy().replace('\\', "/")
    };
    let encoded = percent_encode_path(&text);
    if let Some(unc) = encoded.strip_prefix("//") {
        format!("file://{unc}")
    } else if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn percent_encode_path(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(*byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~');
        if keep {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn split_artist_title(value: Option<String>) -> (Option<String>, Option<String>) {
    match value {
        Some(text) => match text.split_once(" - ") {
            Some((artist, title)) if !artist.is_empty() && !title.is_empty() => (
                Some(artist.trim().to_string()),
                Some(title.trim().to_string()),
            ),
            _ => (None, Some(text)),
        },
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, title: &str) -> M3uEntry {
        M3uEntry {
            path: PathBuf::from(path),
            duration_secs: Some(123),
            artist: Some("Artist".to_string()),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn content_beats_extension() {
        let pls = b"[playlist]\nFile1=/music/a.flac\nNumberOfEntries=1\n";
        assert_eq!(
            sniff_format(pls, Some(Path::new("mix.m3u"))),
            PlaylistFormat::Pls
        );
        let m3u = b"#EXTM3U\n/music/a.flac\n";
        assert_eq!(
            sniff_format(m3u, Some(Path::new("mix.pls"))),
            PlaylistFormat::M3u
        );
    }

    #[test]
    fn pls_round_trip() {
        let entries = vec![entry("/music/one=two.flac", "One = Two")];
        let text = write_playlist(PlaylistFormat::Pls, &entries, None);
        let parsed = parse_playlist_text(&text, Path::new("/tmp/mix.pls"), PlaylistFormat::Pls);
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].path, PathBuf::from("/music/one=two.flac"));
        assert_eq!(parsed.entries[0].title.as_deref(), Some("One = Two"));
        assert_eq!(parsed.entries[0].duration_secs, Some(123));
    }

    #[test]
    fn xspf_round_trip_escapes_and_percent_encodes() {
        let entries = vec![entry("/music/Björk - Jóga.flac", "Jóga & <friends>")];
        let text = write_playlist(PlaylistFormat::Xspf, &entries, None);
        assert!(text.contains("Bj%C3%B6rk"));
        assert!(text.contains("Jóga &amp; &lt;friends&gt;"));
        let parsed = parse_playlist_text(&text, Path::new("/tmp/mix.xspf"), PlaylistFormat::Xspf);
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].title.as_deref(), Some("Jóga & <friends>"));
        assert_eq!(parsed.entries[0].duration_secs, Some(123));
        assert!(parsed.entries[0].path.to_string_lossy().contains("Björk"));
    }

    #[test]
    fn asx_and_wpl_parse_their_refs() {
        let asx = r#"<asx version="3.0"><entry><title>A</title><ref href="/music/a.flac"/></entry></asx>"#;
        let parsed = parse_playlist_text(asx, Path::new("/tmp/mix.asx"), PlaylistFormat::Asx);
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].path, PathBuf::from("/music/a.flac"));

        let wpl = r#"<?wpl version="1.0"?><smil><body><seq><media src="/music/b.flac"/></seq></body></smil>"#;
        let parsed = parse_playlist_text(wpl, Path::new("/tmp/mix.wpl"), PlaylistFormat::Wpl);
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].path, PathBuf::from("/music/b.flac"));
    }

    #[test]
    fn m3u8_marks_non_ascii_output_with_a_bom() {
        let ascii = vec![entry("/music/a.flac", "A")];
        assert!(!write_playlist(PlaylistFormat::M3u8, &ascii, None).starts_with('\u{feff}'));
        let unicode = vec![entry("/music/björk.flac", "Jóga")];
        assert!(write_playlist(PlaylistFormat::M3u8, &unicode, None).starts_with('\u{feff}'));
    }

    #[test]
    fn every_format_round_trips_the_same_paths() {
        let entries = vec![
            entry("/music/one.flac", "Artist - One"),
            entry("/music/two with space.flac", "Two"),
        ];
        for format in PlaylistFormat::ALL {
            let text = write_playlist(format, &entries, None);
            let parsed = parse_playlist_text(&text, Path::new("/tmp/mix.m3u"), format);
            let paths: Vec<_> = parsed.entries.into_iter().map(|e| e.path).collect();
            assert_eq!(
                paths,
                entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>(),
                "{format:?} did not round-trip its paths"
            );
        }
    }

    #[test]
    fn fixtures_from_real_players_parse() {
        let base = Path::new("/fixtures");
        let cases: [(&str, PlaylistFormat, &str, usize); 5] = [
            (
                include_str!("../tests/fixtures/mix.pls"),
                PlaylistFormat::Pls,
                "",
                2,
            ),
            (
                include_str!("../tests/fixtures/mix.xspf"),
                PlaylistFormat::Xspf,
                "Fixture Road",
                2,
            ),
            (
                include_str!("../tests/fixtures/mix.m3u8"),
                PlaylistFormat::M3u8,
                "",
                2,
            ),
            (
                include_str!("../tests/fixtures/mix.asx"),
                PlaylistFormat::Asx,
                "Fixture Radio",
                2,
            ),
            (
                include_str!("../tests/fixtures/mix.wpl"),
                PlaylistFormat::Wpl,
                "",
                2,
            ),
        ];

        for (text, format, title, count) in cases {
            let parsed = parse_playlist_text(text, Path::new("/fixtures/mix"), format);
            assert_eq!(
                parsed.entries.len(),
                count,
                "{format:?} fixture entry count"
            );
            assert_eq!(
                parsed.title.as_deref(),
                (!title.is_empty()).then_some(title)
            );
            for entry in &parsed.entries {
                assert!(
                    entry.path.starts_with(base),
                    "{format:?} did not resolve {:?} against the fixture directory",
                    entry.path
                );
            }
        }
    }

    #[test]
    fn unc_xspf_locations_are_authorities() {
        let entries = vec![entry(r"\\server\share\Björk - Jóga.flac", "Jóga")];
        let text = write_playlist(PlaylistFormat::Xspf, &entries, None);
        assert!(
            text.contains("file://server/share/Bj%C3%B6rk%20-%20J%C3%B3ga.flac"),
            "{text}"
        );
        let parsed = parse_playlist_text(&text, Path::new("/tmp/mix.xspf"), PlaylistFormat::Xspf);
        assert_eq!(
            parsed.entries[0].path,
            PathBuf::from("//server/share/Björk - Jóga.flac")
        );
    }

    #[test]
    fn windows_relative_paths_are_not_rebased_on_a_unix_host() {
        // A playlist imported from Windows with drive and UNC entries must
        // keep those shapes even when the import runs on Linux.
        let text = "#EXTM3U\nC:\\Music\\Local.flac\n\\\\server\\share\\Remote.flac\n";
        let parsed = parse_playlist_text(text, Path::new("/playlists/mix"), PlaylistFormat::M3u);
        assert_eq!(parsed.entries[0].path, PathBuf::from("C:/Music/Local.flac"));
        assert_eq!(
            parsed.entries[1].path,
            PathBuf::from("//server/share/Remote.flac")
        );
    }
}

//! Importing playlists from other players.
//!
//! Everyone arrives with a library built somewhere else. These are the
//! playlist formats the common players leave behind, read from the places
//! they put them, so a move to ShadowPlayer does not mean re-typing every
//! playlist by hand.

use std::path::{Path, PathBuf};

use crate::playlist_format::{parse_playlist, ParsedPlaylist};

/// A playlist found in another player's library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundPlaylist {
    /// Where it came from: "Rhythmbox", "iTunes" or "Audacious".
    pub source: &'static str,
    /// The playlist's name.
    pub name: String,
    /// The files it points at.
    pub paths: Vec<PathBuf>,
}

/// How a player's discovery candidate should be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateKind {
    /// A file in one of the supported playlist formats.
    Playlist,
    /// A directory of playlist files.
    Directory,
    /// An INI/text file that points at playlist files or audio paths.
    Pointer(&'static str),
}

/// The well-known places other players keep their playlists.
///
/// The paths are the ones a normal install uses. A player that keeps its
/// playlists somewhere else simply is not found, which is why every lookup
/// is optional rather than fatal.
fn candidate_files() -> Vec<(&'static str, PathBuf, CandidateKind)> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let appdata = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData/Roaming"));
    let mut candidates = vec![
        (
            "Rhythmbox",
            home.join(".local/share/rhythmbox/playlists.xml"),
            CandidateKind::Pointer("rhythmbox"),
        ),
        (
            "iTunes",
            home.join("Music/iTunes/iTunes Library.xspf"),
            CandidateKind::Playlist,
        ),
        (
            "iTunes",
            home.join("Music/iTunes/iTunes Playlist.xspf"),
            CandidateKind::Playlist,
        ),
        (
            "Audacious",
            home.join(".config/audacious/playlists.txt"),
            CandidateKind::Pointer("audacious"),
        ),
        (
            "Clementine",
            home.join(".config/Clementine/playlists.ini"),
            CandidateKind::Pointer("clementine"),
        ),
        (
            "Clementine",
            home.join(".config/clementine/playlists.ini"),
            CandidateKind::Pointer("clementine"),
        ),
        (
            "Winamp",
            appdata.join("Winamp/Winamp.ini"),
            CandidateKind::Pointer("winamp"),
        ),
        (
            "foobar2000",
            appdata.join("foobar2000/foo_playlists.txt"),
            CandidateKind::Pointer("foobar"),
        ),
        (
            "iTunes",
            appdata.join("Apple Computer/iTunes/iTunes Playlist.xspf"),
            CandidateKind::Playlist,
        ),
    ];

    for (source, directory) in [
        ("Winamp", appdata.join("Winamp/Plugins/ml/playlists")),
        ("Winamp", appdata.join("Winamp/playlists")),
        ("foobar2000", appdata.join("foobar2000/playlists")),
        ("MusicBee", appdata.join("MusicBee/Playlists")),
        ("Clementine", home.join(".config/Clementine/playlists")),
        ("Strawberry", home.join(".config/strawberry/playlists")),
        (
            "Strawberry",
            home.join(".config/StrawberryMusicPlayer/playlists"),
        ),
    ] {
        candidates.push((source, directory, CandidateKind::Directory));
    }
    candidates
}

/// Find every playlist the other players left behind.
///
/// A file that cannot be read or parsed is skipped rather than fatal: one
/// broken playlist must not stop the rest from being imported.
pub fn find_playlists() -> Vec<FoundPlaylist> {
    find_playlists_in(candidate_files())
}

/// The discovery walk, separated from the host-specific candidate list so a
/// test can point it at a temporary directory.
fn find_playlists_in(
    candidates: impl IntoIterator<Item = (&'static str, PathBuf, CandidateKind)>,
) -> Vec<FoundPlaylist> {
    let mut out = Vec::new();
    for (source, path, kind) in candidates {
        if !path.exists() {
            continue;
        }
        match kind {
            CandidateKind::Directory => scan_playlist_dir(source, &path, &mut out),
            CandidateKind::Playlist => {
                // iTunes' own XSPF export is read by the older parser, which
                // knows to describe multiple `<playlist>` blocks. Everything
                // else goes through the shared format dispatcher.
                if source == "iTunes"
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("xspf"))
                {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        out.extend(parse_xspf(&text));
                    }
                } else if let Some(parsed) = parse_one_playlist(&path) {
                    push_parsed(source, &path, parsed, &mut out);
                }
            }
            CandidateKind::Pointer(kind) => {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                match kind {
                    "rhythmbox" => out.extend(parse_rhythmbox(&text)),
                    "audacious" => out.extend(parse_audacious(&text)),
                    "clementine" => out.extend(parse_ini_paths(source, &text, &path)),
                    "winamp" | "foobar" => out.extend(parse_pointer_file(source, &text, &path)),
                    _ => {}
                }
            }
        }
    }

    // A directory and a pointer file can name the same playlist. Deduplicate
    // without making the user choose between two identical rows.
    let mut unique: Vec<FoundPlaylist> = Vec::with_capacity(out.len());
    for found in out {
        if !unique.iter().any(|existing| {
            existing.source == found.source
                && existing.name == found.name
                && existing.paths == found.paths
        }) {
            unique.push(found);
        }
    }
    unique
}

fn parse_one_playlist(path: &Path) -> Option<ParsedPlaylist> {
    parse_playlist(path)
        .ok()
        .and_then(|mut parsed| parsed.drain(..).next())
        .filter(|playlist| !playlist.entries.is_empty())
}

fn push_parsed(
    source: &'static str,
    path: &Path,
    parsed: ParsedPlaylist,
    out: &mut Vec<FoundPlaylist>,
) {
    if parsed.entries.is_empty() {
        return;
    }
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported playlist")
        .to_string();
    out.push(FoundPlaylist {
        source,
        name: parsed.title.unwrap_or(fallback),
        paths: parsed.entries.into_iter().map(|entry| entry.path).collect(),
    });
}

fn scan_playlist_dir(source: &'static str, directory: &Path, out: &mut Vec<FoundPlaylist>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    // Directory enumeration order is not specified. Sorting makes the import
    // list stable between launches, which matters when two playlists have the
    // same name and the user has to choose one.
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|e| e.to_str())
                .and_then(crate::playlist_format::PlaylistFormat::from_extension)
                .is_some()
        })
        .collect();
    files.sort();
    for path in files {
        if let Some(parsed) = parse_one_playlist(&path) {
            push_parsed(source, &path, parsed, out);
        }
    }
}

/// Parse an INI-style playlist store: `[Playlist name]` sections with indexed
/// paths or URLs. Clementine and Strawberry both use this shape.
fn parse_ini_paths(source: &'static str, text: &str, path: &Path) -> Vec<FoundPlaylist> {
    let base = path.parent();
    let mut out = Vec::new();
    let mut name = String::new();
    let mut paths = Vec::new();

    let flush = |out: &mut Vec<FoundPlaylist>, name: &mut String, paths: &mut Vec<PathBuf>| {
        if !paths.is_empty() {
            out.push(FoundPlaylist {
                source,
                name: if name.is_empty() {
                    "Imported playlist".to_string()
                } else {
                    name.clone()
                },
                paths: std::mem::take(paths),
            });
        }
        name.clear();
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(&mut out, &mut name, &mut paths);
            name = header.trim().to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        if !(key.contains("url")
            || key.contains("file")
            || key.contains("path")
            || key.chars().all(|c| c.is_ascii_digit()))
        {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if let Some(p) = file_url_to_path(value) {
            paths.push(p);
        } else if !value.contains("://") {
            let candidate = PathBuf::from(value.replace('\\', "/"));
            paths.push(if candidate.is_absolute() {
                candidate
            } else {
                base.map(|b| b.join(&candidate)).unwrap_or(candidate)
            });
        }
    }
    flush(&mut out, &mut name, &mut paths);
    out
}

/// Parse Winamp's `Winamp.ini` and foobar2000's `foo_playlists.txt`.
///
/// These are pointer files: a section/list can name playlist files (which are
/// then parsed in their real format) or contain audio paths directly.
fn parse_pointer_file(source: &'static str, text: &str, path: &Path) -> Vec<FoundPlaylist> {
    let base = path.parent();
    let mut out = Vec::new();
    let mut loose_name = String::new();
    let mut loose_paths = Vec::new();

    let flush_loose =
        |out: &mut Vec<FoundPlaylist>, name: &mut String, paths: &mut Vec<PathBuf>| {
            if !paths.is_empty() {
                out.push(FoundPlaylist {
                    source,
                    name: if name.is_empty() {
                        "Imported playlist".to_string()
                    } else {
                        name.clone()
                    },
                    paths: std::mem::take(paths),
                });
            }
            name.clear();
        };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush_loose(&mut out, &mut loose_name, &mut loose_paths);
            loose_name = header.trim().to_string();
            continue;
        }
        let value = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or(line);
        if value.is_empty() {
            continue;
        }
        // Winamp and foobar2000 both store some entries as `file://` URLs.
        // Treating the URL as a filename meant the playlist they pointed at
        // was never found.
        let candidate = file_url_to_path(value).unwrap_or_else(|| {
            let candidate = PathBuf::from(value.replace('\\', "/"));
            if candidate.is_absolute() {
                candidate
            } else {
                base.map(|b| b.join(&candidate)).unwrap_or(candidate)
            }
        });
        if candidate.is_file()
            && candidate
                .extension()
                .and_then(|e| e.to_str())
                .and_then(crate::playlist_format::PlaylistFormat::from_extension)
                .is_some()
        {
            if let Some(parsed) = parse_one_playlist(&candidate) {
                push_parsed(source, &candidate, parsed, &mut out);
            }
        } else if candidate.is_dir() {
            scan_playlist_dir(source, &candidate, &mut out);
        } else if looks_like_audio(&candidate) {
            loose_paths.push(candidate);
        }
    }
    flush_loose(&mut out, &mut loose_name, &mut loose_paths);
    out
}

/// Parse Rhythmbox's `playlists.xml`.
///
/// The shape is a list of `<playlist>` elements, each with a `name` and a
/// `<location>` per track.
fn parse_rhythmbox(text: &str) -> Vec<FoundPlaylist> {
    let mut out: Vec<FoundPlaylist> = Vec::new();
    for block in xml_blocks(text, "playlist") {
        let name = xml_attr(&block, "name").unwrap_or_else(|| "Playlist".to_string());
        let paths: Vec<PathBuf> = xml_values(&block, "location")
            .iter()
            .filter_map(|v| file_url_to_path(v))
            .collect();
        if !paths.is_empty() {
            out.push(FoundPlaylist {
                source: "Rhythmbox",
                name,
                paths,
            });
        }
    }
    out
}

/// Parse an XSPF playlist, which is what iTunes writes.
///
/// The shape is a `<playlist>` with a `<title>` and a `<trackList>` of
/// `<track>` elements, each with a `<location>`.
fn parse_xspf(text: &str) -> Vec<FoundPlaylist> {
    let mut out: Vec<FoundPlaylist> = Vec::new();
    for block in xml_blocks(text, "playlist") {
        let name = xml_values(&block, "title")
            .first()
            .cloned()
            .unwrap_or_else(|| "Imported playlist".to_string());
        let paths: Vec<PathBuf> = xml_values(&block, "location")
            .iter()
            .filter_map(|v| file_url_to_path(v))
            .collect();
        if !paths.is_empty() {
            out.push(FoundPlaylist {
                source: "iTunes",
                name,
                paths,
            });
        }
    }
    out
}

/// Parse Audacious's `playlists.txt`.
///
/// The format is a plain list: a `[name]` header line, then one file per
/// line, then a blank line before the next one.
fn parse_audacious(text: &str) -> Vec<FoundPlaylist> {
    let mut out: Vec<FoundPlaylist> = Vec::new();
    let mut name = String::new();
    let mut paths: Vec<PathBuf> = Vec::new();

    let flush = |out: &mut Vec<FoundPlaylist>, name: &mut String, paths: &mut Vec<PathBuf>| {
        if !paths.is_empty() {
            out.push(FoundPlaylist {
                source: "Audacious",
                name: if name.is_empty() {
                    "Imported playlist".to_string()
                } else {
                    name.clone()
                },
                paths: std::mem::take(paths),
            });
        }
        name.clear();
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(&mut out, &mut name, &mut paths);
        } else if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(&mut out, &mut name, &mut paths);
            name = header.to_string();
        } else {
            paths.push(PathBuf::from(line));
        }
    }
    flush(&mut out, &mut name, &mut paths);
    out
}

/// The contents of every `<tag>…</tag>` element.
fn xml_blocks(text: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        // Only a real tag, not a prefix of a longer name.
        let after = &rest[start + open.len()..];
        let is_tag = after.starts_with('>') || after.starts_with(' ') || after.starts_with('\t');
        let Some(body_start) = rest[start..].find('>').map(|i| start + i + 1) else {
            break;
        };
        let Some(end) = rest[body_start..].find(&close) else {
            break;
        };
        if is_tag {
            // The opening tag is included, so an attribute on it — the
            // playlist's name, for instance — can be read.
            out.push(rest[start..body_start + end].to_string());
        }
        rest = &rest[body_start + end..];
    }
    out
}

/// The value of an attribute on the first tag of a block.
fn xml_attr(block: &str, name: &str) -> Option<String> {
    let open = block.find('<')?;
    let tag = &block[open..];
    let end = tag.find('>')?;
    let tag = &tag[..end];
    let key = format!("{name}=");
    let at = tag.find(&key)?;
    let rest = &tag[at + key.len()..];
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(crate::playlist_format::decode_xml_entities(&rest[..end]))
}

/// The text of every `<tag>…</tag>` element in a block.
fn xml_values(block: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = block;
    while let Some(start) = rest.find(&open) {
        let body = &rest[start + open.len()..];
        let Some(end) = body.find(&close) else {
            break;
        };
        out.push(crate::playlist_format::decode_xml_entities(
            body[..end].trim(),
        ));
        rest = &body[end + close.len()..];
    }
    out
}

/// Turn a `file://` URL into a path.
///
/// A URL-encoded space (`%20`) and the other escapes have to be undone, or
/// the path will not match anything in the library.
pub fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let url = url.trim();
    if let Some(path) = crate::m3u::file_uri_to_path(url) {
        return Some(path);
    }
    if !url.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(url);
    if decoded.is_empty() {
        return None;
    }
    Some(PathBuf::from(decoded))
}

/// Undo percent-encoding.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether a path looks like a music file the library would know.
pub fn looks_like_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "flac"
            | "wav"
            | "aiff"
            | "aif"
            | "mp3"
            | "m4a"
            | "mp4"
            | "aac"
            | "alac"
            | "ogg"
            | "opus"
            | "ape"
            | "wv"
            | "dsf"
            | "dff"
            | "wma"
            | "mpc"
            | "tta"
            | "tak"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rhythmbox_playlists() {
        let xml = r#"<?xml version="1.0"?>
        <rhythmbox-playlists>
          <playlist name="Chill &amp; Rain" type="location">
            <location>file:///home/u/Music/a%20song&amp;mix.flac</location>
            <location>file:///home/u/Music/b.flac</location>
          </playlist>
          <playlist name="Empty" type="location"/>
        </rhythmbox-playlists>"#;
        let found = parse_rhythmbox(xml);
        assert_eq!(found.len(), 1, "a playlist with no tracks is not offered");
        assert_eq!(found[0].name, "Chill & Rain");
        assert_eq!(found[0].paths.len(), 2);
        assert_eq!(
            found[0].paths[0],
            PathBuf::from("/home/u/Music/a song&mix.flac")
        );
    }

    #[test]
    fn parses_xspf_playlists() {
        let xml = r#"<?xml version="1.0"?>
        <playlist version="1" xmlns="http://xspf.org/ns/0/">
          <title>Road trip</title>
          <trackList>
            <track><location>file:///music/one.flac</location></track>
            <track><location>file:///music/two.flac</location></track>
          </trackList>
        </playlist>"#;
        let found = parse_xspf(xml);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Road trip");
        assert_eq!(found[0].paths.len(), 2);
    }

    #[test]
    fn parses_audacious_playlists() {
        let text = "[Favourites]\n/music/one.flac\n/music/two.flac\n\n[Later]\n/music/three.flac\n";
        let found = parse_audacious(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Favourites");
        assert_eq!(found[0].paths.len(), 2);
        assert_eq!(found[1].name, "Later");
        assert_eq!(found[1].paths.len(), 1);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(
            file_url_to_path("file:///a%20b/c%2Bd.flac"),
            Some(PathBuf::from("/a b/c+d.flac"))
        );
        assert_eq!(file_url_to_path("http://example.com/x.mp3"), None);
    }

    #[test]
    fn unc_file_urls_keep_their_share_authority() {
        assert_eq!(
            file_url_to_path("file://server/share/Music/My%20Song.flac"),
            Some(PathBuf::from("//server/share/Music/My Song.flac"))
        );
        assert_eq!(
            file_url_to_path("file://LOCALHOST/music/a.flac"),
            Some(PathBuf::from("/music/a.flac"))
        );
        assert_eq!(
            file_url_to_path("file:///C:/Users/me/Music/a.flac"),
            Some(PathBuf::from("C:/Users/me/Music/a.flac"))
        );
    }

    #[test]
    fn discovery_reads_a_directory_and_deduplicates_a_pointer_to_it() {
        let root = std::env::temp_dir().join(format!(
            "sp_discovery_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let playlists = root.join("Playlists");
        std::fs::create_dir_all(&playlists).unwrap();
        std::fs::write(
            playlists.join("mix.pls"),
            "[playlist]\nFile1=/music/a.flac\nNumberOfEntries=1\n",
        )
        .unwrap();
        std::fs::write(
            playlists.join("other.wpl"),
            "<?wpl version=\"1.0\"?><smil><body><seq><media src=\"/music/b.flac\"/></seq></body></smil>",
        )
        .unwrap();
        // A non-playlist file is not offered.
        std::fs::write(playlists.join("notes.txt"), "not a playlist").unwrap();

        let pointer = root.join("playlists.txt");
        // The pointer names the same PLS by absolute path so deduplication is
        // exercised against the directory scan.
        let pls_url = format!(
            "file://{}",
            playlists
                .join("mix.pls")
                .to_string_lossy()
                .replace(' ', "%20")
        );
        std::fs::write(&pointer, format!("[Saved]\n{pls_url}\n")).unwrap();

        let found = find_playlists_in([
            ("TestPlayer", playlists.clone(), CandidateKind::Directory),
            ("TestPlayer", pointer, CandidateKind::Pointer("winamp")),
            // The same directory can be reached by two candidate paths.
            ("TestPlayer", playlists, CandidateKind::Directory),
        ]);
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["mix", "other"], "{found:?}");
        assert!(found
            .iter()
            .all(|p| p.source == "TestPlayer" && !p.paths.is_empty()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn recognises_audio_extensions() {
        assert!(looks_like_audio(Path::new("/a/b.flac")));
        assert!(looks_like_audio(Path::new("/a/b.MP3")));
        assert!(!looks_like_audio(Path::new("/a/b.txt")));
        assert!(!looks_like_audio(Path::new("/a/b")));
    }
}

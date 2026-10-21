//! Importing playlists from other players.
//!
//! Everyone arrives with a library built somewhere else. These are the
//! playlist formats the common players leave behind, read from the places
//! they put them, so a move to ShadowPlayer does not mean re-typing every
//! playlist by hand.

use std::path::{Path, PathBuf};

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

/// The well-known places other players keep their playlists.
///
/// The paths are the ones a normal install uses. A player that keeps its
/// playlists somewhere else simply is not found, which is why every lookup
/// is optional rather than fatal.
fn candidate_files() -> Vec<(&'static str, PathBuf)> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    vec![
        (
            "Rhythmbox",
            home.join(".local/share/rhythmbox/playlists.xml"),
        ),
        ("iTunes", home.join("Music/iTunes/iTunes Library.xspf")),
        ("Audacious", home.join(".config/audacious/playlists.txt")),
    ]
}

/// Find every playlist the other players left behind.
///
/// A file that cannot be read or parsed is skipped rather than fatal: one
/// broken playlist must not stop the rest from being imported.
pub fn find_playlists() -> Vec<FoundPlaylist> {
    let mut out = Vec::new();
    for (source, path) in candidate_files() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        match source {
            "Rhythmbox" => out.extend(parse_rhythmbox(&text)),
            "iTunes" => out.extend(parse_xspf(&text)),
            "Audacious" => out.extend(parse_audacious(&text)),
            _ => {}
        }
    }
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
    Some(rest[..end].to_string())
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
        out.push(body[..end].trim().to_string());
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
    let rest = if let Some(rest) = url.strip_prefix("file://") {
        rest
    } else if url.starts_with('/') {
        url
    } else {
        return None;
    };
    // `file:///home/x` leaves a leading slash; `file://host/path` is rare and
    // is not supported.
    let decoded = percent_decode(rest);
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
          <playlist name="Chill" type="location">
            <location>file:///home/u/Music/a%20song.flac</location>
            <location>file:///home/u/Music/b.flac</location>
          </playlist>
          <playlist name="Empty" type="location"/>
        </rhythmbox-playlists>"#;
        let found = parse_rhythmbox(xml);
        assert_eq!(found.len(), 1, "a playlist with no tracks is not offered");
        assert_eq!(found[0].name, "Chill");
        assert_eq!(found[0].paths.len(), 2);
        assert_eq!(
            found[0].paths[0],
            PathBuf::from("/home/u/Music/a song.flac")
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
    fn recognises_audio_extensions() {
        assert!(looks_like_audio(Path::new("/a/b.flac")));
        assert!(looks_like_audio(Path::new("/a/b.MP3")));
        assert!(!looks_like_audio(Path::new("/a/b.txt")));
        assert!(!looks_like_audio(Path::new("/a/b")));
    }
}

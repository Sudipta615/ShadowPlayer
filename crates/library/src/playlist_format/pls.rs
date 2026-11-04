use super::{path_for_playlist, resolve, split_artist_title, M3uEntry, ParsedPlaylist};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(crate) fn parse_pls(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
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

pub(crate) fn write_pls(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
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

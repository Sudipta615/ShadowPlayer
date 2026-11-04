use super::{OrganizeEntry, OrganizeIssue, OrganizePlan};
use domain::models::Track;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn build_plan(tracks: &[Track], pattern: &str, destination: Option<&Path>) -> OrganizePlan {
    let mut entries: Vec<OrganizeEntry> = Vec::with_capacity(tracks.len());
    // First entry claiming a destination, so a within-batch collision can be
    // reported against the later claimant.
    let mut claimed: HashMap<PathBuf, usize> = HashMap::new();

    for track in tracks {
        let mut entry = OrganizeEntry {
            track_id: track.id,
            title: track.title.clone(),
            content_hash: track.content_hash.clone(),
            file_id: track.file_id.clone(),
            from: track.path.clone(),
            to: PathBuf::new(),
            issue: None,
            unchanged: false,
        };

        match target_path(track, pattern, destination) {
            Ok(to) => {
                entry.unchanged = to == track.path;
                if !entry.unchanged {
                    entry.issue = if !entry.from.exists() {
                        // Reported before the disk is touched: a plan that claims
                        // to move files that are gone is a lie.
                        Some(OrganizeIssue::SourceMissing)
                    } else if to.exists() {
                        Some(OrganizeIssue::DestinationExists)
                    } else if to.parent().map(|p| p.is_file()).unwrap_or(false) {
                        Some(OrganizeIssue::ParentNotDirectory)
                    } else {
                        None
                    };

                    if entry.issue.is_none() {
                        match claimed.get(&to) {
                            Some(&first) if entries[first].from != track.path => {
                                entry.issue = Some(OrganizeIssue::DuplicateDestination);
                            }
                            Some(_) => {}
                            None => {
                                claimed.insert(to.clone(), entries.len());
                            }
                        }
                    }
                }
                entry.to = to;
            }
            Err(msg) => entry.issue = Some(OrganizeIssue::BadPattern(msg)),
        }

        entries.push(entry);
    }

    OrganizePlan { entries }
}

/// Resolve one track's destination path.
pub(crate) fn target_path(
    track: &Track,
    pattern: &str,
    destination: Option<&Path>,
) -> Result<PathBuf, String> {
    let rendered = render(pattern, track)?;
    let ext = track
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_string();

    // Split on '/' before sanitising: '/' is the only path separator we honour,
    // and everything hostile inside a component becomes '_'.
    let mut components: Vec<String> = rendered
        .split('/')
        .map(sanitize_component)
        .filter(|c| !c.is_empty() && c != "." && c != "..")
        .collect();

    if components.is_empty() {
        return Err("the pattern produced an empty name".to_string());
    }

    // Carry the original extension unless the pattern asked for it explicitly.
    if !pattern.contains("{ext}") && !ext.is_empty() {
        let last = components.last_mut().expect("non-empty");
        if !last
            .to_ascii_lowercase()
            .ends_with(&format!(".{}", ext.to_ascii_lowercase()))
        {
            last.push('.');
            last.push_str(&ext);
        }
    }

    let file_name = components.pop().expect("non-empty");
    let parent = match destination {
        Some(root) => {
            let mut p = root.to_path_buf();
            for dir in &components {
                p.push(dir);
            }
            p
        }
        None => {
            if !components.is_empty() {
                return Err(
                    "the pattern contains folders — choose a destination folder, or use a name-only pattern"
                        .to_string(),
                );
            }
            track
                .path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        }
    };

    Ok(parent.join(file_name))
}

/// Substitute `{token}` placeholders, leaving all other text alone.
pub(crate) fn render(pattern: &str, track: &Track) -> Result<String, String> {
    let mut out = String::with_capacity(pattern.len() + 32);
    let mut rest = pattern;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find('}') else {
            return Err("unclosed '{' in the pattern".to_string());
        };
        let token = &after[..=end];
        match token_value(token, track) {
            Some(value) => out.push_str(&value),
            None => return Err(format!("unknown token {token}")),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

pub(crate) fn token_value(token: &str, track: &Track) -> Option<String> {
    let value = match token {
        "{artist}" => track.artist.clone(),
        "{album}" => track.album.clone(),
        "{title}" => track.title.clone(),
        "{track}" => track
            .track_number
            .map(|n| format!("{n:02}"))
            .unwrap_or_default(),
        "{disc}" => track.disc_number.map(|n| n.to_string()).unwrap_or_default(),
        "{year}" => track.year.map(|y| y.to_string()).unwrap_or_default(),
        "{genre}" => track.genre.clone().unwrap_or_default(),
        "{stem}" => track
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string(),
        "{ext}" => track
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(&track.format)
            .to_string(),
        "{format}" => track.format.to_uppercase(),
        "{samplerate}" => track.sample_rate.to_string(),
        "{bitdepth}" => track.bit_depth.map(|b| b.to_string()).unwrap_or_default(),
        "{bitrate}" => track
            .bitrate_kbps
            .map(|b| b.to_string())
            .unwrap_or_default(),
        "{channels}" => track.channels.to_string(),
        _ => return None,
    };
    Some(value)
}

/// Name Windows refuses outright, whatever the extension.
const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Make one path component safe for every filesystem we care about.
///
/// Reserved characters become `_`, whitespace runs collapse to a single space,
/// and leading/trailing dots and spaces are dropped (they hide files on Unix
/// and are rejected outright on Windows). A device name (`CON`, `NUL`, `COM1`,
/// … — reserved with or without an extension) gets a leading underscore, which
/// is all Windows needs to treat it as an ordinary name.
pub(crate) fn sanitize_component(component: &str) -> String {
    let mut cleaned = String::with_capacity(component.len());
    for ch in component.chars() {
        // Whitespace is normalised below; only the other control characters
        // (and the filesystem-reserved set) need replacing first.
        let illegal = matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            || (ch.is_control() && !ch.is_whitespace());
        cleaned.push(if illegal { '_' } else { ch });
    }

    let mut cleaned = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();

    if is_windows_reserved(&cleaned) {
        cleaned.insert(0, '_');
    }
    cleaned
}

/// `true` for names Windows reserves for devices (`CON`, `NUL`, `COM4`, …).
///
/// The check is on the part before the first dot: `NUL.tar.gz` is just as
/// reserved as `NUL`, because Windows resolves the device before the extension.
pub(crate) fn is_windows_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    WINDOWS_RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

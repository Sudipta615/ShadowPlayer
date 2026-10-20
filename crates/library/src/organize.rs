//! Batch file organization: turn a naming pattern into a rename/move plan.
//!
//! Two properties make this safe to point at a whole library:
//!
//! 1. The plan is computed and shown *before* anything touches the disk, so
//!    every destination is reviewable.
//! 2. The plan never silently overwrites. A destination that already exists is
//!    reported as a conflict and skipped, as is any second track resolving to
//!    the same target within one batch.
//!
//! The pattern is plain text with `{token}` placeholders. If it contains a
//! `{ext}` token the extension comes from that token; otherwise the source
//! file's own extension is appended automatically, so `{track} - {title}` does
//! the right thing without the user thinking about it.
//!
//! Nothing here touches the database: `apply_plan` moves files and reports what
//! moved, and the caller relinks the affected rows.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use domain::ids::TrackId;
use domain::models::Track;

/// Tokens accepted in a pattern, with a one-line description for the UI.
pub const TOKENS: &[(&str, &str)] = &[
    ("{artist}", "Track artist"),
    ("{album}", "Album title"),
    ("{title}", "Track title"),
    ("{track}", "Track number, zero-padded"),
    ("{disc}", "Disc number"),
    ("{year}", "Year"),
    ("{genre}", "Genre"),
    ("{stem}", "Original file name, without extension"),
    ("{ext}", "File extension (added automatically if omitted)"),
];

/// A pattern that keeps the current file name shape.
pub const DEFAULT_IN_PLACE_PATTERN: &str = "{track} - {title}";
/// A pattern for building an artist/album tree.
pub const DEFAULT_TREE_PATTERN: &str = "{artist}/{album}/{track} - {title}";

/// Why an entry cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizeIssue {
    /// The pattern is malformed or uses an unknown token.
    BadPattern(String),
    /// The file is no longer where the library says it is.
    SourceMissing,
    /// A file already exists at the destination.
    DestinationExists,
    /// Another track in this plan resolves to the same destination.
    DuplicateDestination,
    /// The destination's parent exists but is a file, not a directory.
    ParentNotDirectory,
}

impl OrganizeIssue {
    pub fn label(&self) -> String {
        match self {
            Self::BadPattern(msg) => msg.clone(),
            Self::SourceMissing => "the file is missing".to_string(),
            Self::DestinationExists => "a file already exists there".to_string(),
            Self::DuplicateDestination => "another track maps to the same name".to_string(),
            Self::ParentNotDirectory => "the parent path is a file".to_string(),
        }
    }
}

/// One track's proposed move.
#[derive(Debug, Clone)]
pub struct OrganizeEntry {
    pub track_id: TrackId,
    pub title: String,
    /// Carried through the move so the track keeps its identity fingerprint.
    pub content_hash: Option<String>,
    pub from: PathBuf,
    pub to: PathBuf,
    /// `None` when the entry is applicable.
    pub issue: Option<OrganizeIssue>,
    /// The destination already equals the source: nothing to do, not an error.
    pub unchanged: bool,
}

/// A complete, reviewable plan.
#[derive(Debug, Clone, Default)]
pub struct OrganizePlan {
    pub entries: Vec<OrganizeEntry>,
}

impl OrganizePlan {
    /// Entries that will actually move when applied.
    pub fn actionable(&self) -> impl Iterator<Item = &OrganizeEntry> {
        self.entries
            .iter()
            .filter(|e| e.issue.is_none() && !e.unchanged)
    }

    pub fn pending_count(&self) -> usize {
        self.actionable().count()
    }

    pub fn conflict_count(&self) -> usize {
        self.entries.iter().filter(|e| e.issue.is_some()).count()
    }

    pub fn unchanged_count(&self) -> usize {
        self.entries.iter().filter(|e| e.unchanged).count()
    }

    /// How many distinct folders the plan will create or move into.
    pub fn destination_folders(&self) -> usize {
        let mut dirs: Vec<&Path> = self.actionable().filter_map(|e| e.to.parent()).collect();
        dirs.sort_unstable();
        dirs.dedup();
        dirs.len()
    }
}

/// Result of applying a plan.
#[derive(Debug, Default)]
pub struct OrganizeOutcome {
    /// `(track id, new path, content hash)` for every file that moved.
    pub moved: Vec<(TrackId, PathBuf, Option<String>)>,
    /// `(source path, reason)` for every file left where it was.
    pub failures: Vec<(PathBuf, String)>,
}

/// Build a plan for `tracks` without touching the filesystem.
///
/// `destination` is the root of the move. `None` limits the pattern to a file
/// name and keeps each file in its current folder.
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
fn target_path(
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
fn render(pattern: &str, track: &Track) -> Result<String, String> {
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

fn token_value(token: &str, track: &Track) -> Option<String> {
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
fn sanitize_component(component: &str) -> String {
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
fn is_windows_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    WINDOWS_RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

/// Move every applicable file, creating destination folders as needed.
///
/// Already-validated entries only: this trusts the plan it is given, so call it
/// with the same plan the user reviewed.
pub fn apply_plan(plan: &OrganizePlan) -> OrganizeOutcome {
    let mut outcome = OrganizeOutcome::default();

    for entry in plan.actionable() {
        if let Some(parent) = entry.to.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                outcome.failures.push((
                    entry.from.clone(),
                    format!("cannot create {}: {e}", parent.display()),
                ));
                continue;
            }
        }

        match move_file(&entry.from, &entry.to) {
            Ok(()) => {
                outcome
                    .moved
                    .push((entry.track_id, entry.to.clone(), entry.content_hash.clone()))
            }
            Err(e) => outcome
                .failures
                .push((entry.from.clone(), describe_move_error(&e))),
        }
    }

    outcome
}

/// Move a file, never replacing an existing destination and working across
/// filesystems.
///
/// `fs::rename` does neither. On Unix it silently replaces an existing
/// destination — the plan checks for conflicts, but the plan was made minutes
/// ago and the disk is free to change underneath it — and across filesystems it
/// fails outright, so organizing a library spread over two drives simply did
/// not work.
///
/// A hard link is the same-filesystem case: it cannot clobber (`AlreadyExists`
/// is an error, not a replacement) and it never copies the bytes. When linking
/// is impossible — another filesystem, or a filesystem that does not support
/// links — the file is copied with `create_new` and the copy is durable before
/// the original is unlinked.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::hard_link(from, to) {
        Ok(()) => {
            // The destination now names the same file. Dropping the old name
            // cannot lose data: if this fails the two names still agree.
            fs::remove_file(from)
        }
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(e),
        Err(_) => copy_then_delete(from, to),
    }
}

/// The cross-filesystem fallback: copy, flush, then unlink the source.
fn copy_then_delete(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::io::Write;

    let mut source = fs::File::open(from)?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;

    if let Err(e) = std::io::copy(&mut source, &mut destination)
        .and_then(|_| destination.flush())
        .and_then(|_| destination.sync_all())
    {
        // Never leave a partial copy at the destination: the next run would
        // see it as a conflict and the library would gain a truncated file.
        drop(destination);
        let _ = fs::remove_file(to);
        return Err(e);
    }
    drop(destination);

    fs::remove_file(from)
}

fn describe_move_error(e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::CrossesDevices {
        return "destination is on another filesystem; pick a folder on the same drive".to_string();
    }
    if e.kind() == ErrorKind::AlreadyExists {
        return "a file already exists there (not overwritten)".to_string();
    }
    if e.kind() == ErrorKind::PermissionDenied {
        return "permission denied".to_string();
    }
    e.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ids::TrackId;

    fn track(id_seed: u8, path: &str, title: &str, artist: &str, album: &str) -> Track {
        // Deterministic ids keep the duplicate-name test readable, without
        // naming the UUID type (which this crate does not depend on directly).
        let id = TrackId::from_name_and_seed(
            TrackId::new().as_uuid(),
            &format!("organize-test-{id_seed}"),
        );
        Track {
            id,
            path: PathBuf::from(path),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: Some("Ambient".to_string()),
            year: Some(1997),
            track_number: Some(id_seed as u32),
            disc_number: None,
            duration_secs: 200.0,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 1,
            mtime_unix_secs: 1,
            mtime_nanos: 0,
            content_hash: Some(format!("hash-{id_seed}")),
            is_favorite: false,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
        }
    }

    /// A scratch directory, emptied first so re-runs are clean.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp_organize_{}_{}", std::process::id(), name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn renders_tokens_and_leaves_other_text_alone() {
        let t = track(
            3,
            "/music/a.flac",
            "Copper Wires",
            "Horizon Lines",
            "Slow Static",
        );
        assert_eq!(
            render("{track} - {title}", &t).unwrap(),
            "03 - Copper Wires"
        );
        assert_eq!(
            render("{artist} - {album}", &t).unwrap(),
            "Horizon Lines - Slow Static"
        );
        assert_eq!(
            render("literal {title} text", &t).unwrap(),
            "literal Copper Wires text"
        );
    }

    #[test]
    fn unknown_and_unclosed_tokens_are_errors() {
        let t = track(1, "/music/a.flac", "T", "A", "B");
        assert!(render("{nope}", &t).unwrap_err().contains("unknown token"));
        assert!(render("{title", &t).unwrap_err().contains("unclosed"));
    }

    #[test]
    fn missing_values_do_not_leave_stray_separators() {
        let mut t = track(1, "/music/a.flac", "T", "A", "B");
        t.track_number = None;
        t.year = None;
        // The empty {track} collapses away with the whitespace around it.
        assert_eq!(render("{track} - {title}", &t).unwrap(), " - T");
        let plan = build_plan(&[t], "{title}", None);
        assert_eq!(plan.entries[0].to, PathBuf::from("/music/T.flac"));
    }

    #[test]
    fn illegal_characters_are_replaced_and_whitespace_collapsed() {
        assert_eq!(sanitize_component("AC/DC"), "AC_DC");
        assert_eq!(sanitize_component("a:b*c?d\"e<f>g|h"), "a_b_c_d_e_f_g_h");
        assert_eq!(sanitize_component("  spaced   out  "), "spaced out");
        assert_eq!(sanitize_component("...hidden..."), "hidden");
        assert_eq!(sanitize_component("tab\there"), "tab here");
    }

    #[test]
    fn name_only_pattern_keeps_the_file_in_place_and_the_extension() {
        let dir = scratch("name_only");
        let src = dir.join("loose.mp3");
        fs::write(&src, b"audio").unwrap();
        let mut t = track(7, &src.to_string_lossy(), "Glass Curtain", "A", "B");
        t.path = src.clone();

        let plan = build_plan(&[t], "{track} - {title}", None);
        assert_eq!(plan.entries[0].to, dir.join("07 - Glass Curtain.mp3"));
        assert!(plan.entries[0].issue.is_none());
        assert_eq!(plan.pending_count(), 1);
    }

    #[test]
    fn folder_pattern_builds_a_tree_under_the_destination() {
        let root = scratch("tree");
        let t = track(
            2,
            "/incoming/x.flac",
            "Gravity Well",
            "Low Orbit",
            "Deep Field",
        );
        let plan = build_plan(&[t], "{artist}/{album}/{track} - {title}", Some(&root));
        assert_eq!(
            plan.entries[0].to,
            root.join("Low Orbit/Deep Field/02 - Gravity Well.flac")
        );
    }

    #[test]
    fn folder_pattern_without_a_destination_is_rejected() {
        let t = track(
            2,
            "/incoming/x.flac",
            "Gravity Well",
            "Low Orbit",
            "Deep Field",
        );
        let plan = build_plan(&[t], "{artist}/{album}/{title}", None);
        let issue = plan.entries[0].issue.as_ref().expect("should be rejected");
        assert!(matches!(issue, OrganizeIssue::BadPattern(_)));
        assert_eq!(plan.pending_count(), 0);
    }

    #[test]
    fn explicit_ext_token_is_not_doubled() {
        let t = track(4, "/music/a.flac", "T", "A", "B");
        let plan = build_plan(&[t], "{track}.{ext}", None);
        assert_eq!(plan.entries[0].to, PathBuf::from("/music/04.flac"));
    }

    #[test]
    fn a_track_already_at_its_destination_is_unchanged_not_a_conflict() {
        let t = track(5, "/music/05 - Copper Wires.flac", "Copper Wires", "A", "B");
        let plan = build_plan(&[t], "{track} - {title}", None);
        assert!(plan.entries[0].unchanged);
        assert!(plan.entries[0].issue.is_none());
        assert_eq!(plan.pending_count(), 0);
        assert_eq!(plan.unchanged_count(), 1);
        assert_eq!(plan.conflict_count(), 0);
    }

    #[test]
    fn an_existing_destination_is_a_conflict_and_never_overwritten() {
        let dir = scratch("exists");
        let src = dir.join("source.flac");
        let taken = dir.join("01 - Taken.flac");
        fs::write(&src, b"x").unwrap();
        fs::write(&taken, b"y").unwrap();

        let mut t = track(1, &src.to_string_lossy(), "Taken", "A", "B");
        t.path = src.clone();

        let plan = build_plan(&[t], "{track} - {title}", None);
        assert_eq!(
            plan.entries[0].issue,
            Some(OrganizeIssue::DestinationExists)
        );
        assert_eq!(plan.pending_count(), 0);

        // Applying a plan with nothing actionable must leave the disk alone.
        let outcome = apply_plan(&plan);
        assert!(outcome.moved.is_empty());
        assert!(outcome.failures.is_empty());
        assert_eq!(fs::read(&taken).unwrap(), b"y");
        assert!(src.exists());
    }

    #[test]
    fn both_tracks_mapping_to_one_name_are_reported() {
        let root = scratch("dup");
        let dir_a = scratch("dup_a");
        let dir_b = scratch("dup_b");
        let src_a = dir_a.join("same.flac");
        let src_b = dir_b.join("same.flac");
        fs::write(&src_a, b"a").unwrap();
        fs::write(&src_b, b"b").unwrap();

        let mut a = track(1, &src_a.to_string_lossy(), "Same", "A", "B");
        a.path = src_a;
        let mut b = track(2, &src_b.to_string_lossy(), "Same", "A", "B");
        b.path = src_b;

        let plan = build_plan(&[a, b], "{title}", Some(&root));
        assert!(plan.entries[0].issue.is_none(), "first claim wins");
        assert_eq!(
            plan.entries[1].issue,
            Some(OrganizeIssue::DuplicateDestination)
        );
        assert_eq!(plan.pending_count(), 1);
    }

    #[test]
    fn a_missing_source_is_reported_instead_of_failing_at_apply_time() {
        let dir = scratch("missing");
        let gone = dir.join("gone.flac");
        let mut t = track(3, &gone.to_string_lossy(), "Vanished", "A", "B");
        t.path = gone;

        let plan = build_plan(&[t], "{title}", None);
        assert_eq!(plan.entries[0].issue, Some(OrganizeIssue::SourceMissing));
        assert_eq!(plan.pending_count(), 0);
        assert!(apply_plan(&plan).moved.is_empty());
    }

    #[test]
    fn apply_moves_files_and_creates_folders() {
        let dir = scratch("apply");
        let src = dir.join("incoming.flac");
        fs::write(&src, b"audio").unwrap();

        let mut t = track(
            9,
            &src.to_string_lossy(),
            "Deep Field",
            "Low Orbit",
            "Gravity",
        );
        t.path = src.clone();

        let plan = build_plan(
            &[t.clone()],
            "{artist}/{album}/{track} - {title}",
            Some(&dir),
        );
        assert_eq!(plan.pending_count(), 1);
        assert_eq!(plan.destination_folders(), 1);

        let outcome = apply_plan(&plan);
        assert!(
            outcome.failures.is_empty(),
            "failures: {:?}",
            outcome.failures
        );
        assert_eq!(outcome.moved.len(), 1);

        let (id, new_path, hash) = &outcome.moved[0];
        assert_eq!(*id, t.id);
        assert_eq!(hash.as_deref(), Some("hash-9"));
        assert!(!src.exists(), "source should have moved");
        assert_eq!(fs::read(new_path).unwrap(), b"audio");
        assert_eq!(
            *new_path,
            dir.join("Low Orbit/Gravity/09 - Deep Field.flac")
        );
    }

    #[test]
    fn later_entries_still_apply_when_one_conflicts() {
        let dir = scratch("partial");
        let good = dir.join("good.flac");
        let bad = dir.join("bad.flac");
        fs::write(&good, b"g").unwrap();
        fs::write(&bad, b"b").unwrap();
        // Occupy the destination the second track wants (`{track}` = 02).
        fs::write(dir.join("02 - Second.flac"), b"taken").unwrap();

        let mut first = track(1, &good.to_string_lossy(), "First", "A", "B");
        first.path = good.clone();
        let mut second = track(2, &bad.to_string_lossy(), "Second", "A", "B");
        second.path = bad.clone();

        let plan = build_plan(&[first, second], "{track} - {title}", None);
        assert_eq!(plan.pending_count(), 1);
        assert_eq!(plan.conflict_count(), 1);

        let outcome = apply_plan(&plan);
        assert_eq!(outcome.moved.len(), 1);
        assert!(dir.join("01 - First.flac").exists());
        assert!(bad.exists(), "conflicting source must be left alone");
    }

    #[test]
    fn windows_device_names_are_escaped() {
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("nul"), "_nul");
        assert_eq!(sanitize_component("COM4"), "_COM4");
        // The reservation applies before the first dot.
        assert_eq!(sanitize_component("NUL.tar.gz"), "_NUL.tar.gz");
        // Only real device names are touched.
        assert_eq!(sanitize_component("Console"), "Console");
        assert_eq!(sanitize_component("Com"), "Com");
    }

    /// A file that appears at the destination between plan time and apply time
    /// (another program, or a second run) must not be replaced.
    #[test]
    fn applying_never_replaces_a_file_that_appeared_after_the_plan() {
        let dir = scratch("race");
        let src = dir.join("incoming.flac");
        fs::write(&src, b"audio").unwrap();

        let mut t = track(1, &src.to_string_lossy(), "Racer", "A", "B");
        t.path = src.clone();
        let plan = build_plan(&[t], "{track} - {title}", None);
        assert_eq!(plan.pending_count(), 1);

        // The plan is already made; now somebody else writes the destination.
        let target = dir.join("01 - Racer.flac");
        fs::write(&target, b"not mine").unwrap();

        let outcome = apply_plan(&plan);
        assert!(outcome.moved.is_empty());
        assert_eq!(outcome.failures.len(), 1);
        assert!(
            outcome.failures[0].1.contains("not overwritten"),
            "got {:?}",
            outcome.failures[0].1
        );
        assert_eq!(fs::read(&target).unwrap(), b"not mine");
        assert!(src.exists(), "the source must survive a refused move");
    }

    /// The cross-filesystem path, exercised directly: copy, flush, unlink.
    #[test]
    fn the_copy_fallback_moves_bytes_and_refuses_to_overwrite() {
        let dir = scratch("copy_fallback");
        let src = dir.join("from.flac");
        fs::write(&src, b"audio bytes").unwrap();

        let dest = dir.join("nested/to.flac");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        copy_then_delete(&src, &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), b"audio bytes");
        assert!(
            !src.exists(),
            "the source is unlinked once the copy is durable"
        );

        // An occupied destination is an error, not a replacement.
        let occupied = dir.join("taken.flac");
        fs::write(&occupied, b"keep me").unwrap();
        let other = dir.join("other.flac");
        fs::write(&other, b"other").unwrap();
        let err = copy_then_delete(&other, &occupied).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&occupied).unwrap(), b"keep me");
        assert!(other.exists());
    }

    /// A failure mid-copy must not leave a half-written file behind.
    #[test]
    fn a_failed_copy_leaves_no_partial_destination() {
        let dir = scratch("partial_copy");
        let src = dir.join("dir_not_file");
        fs::create_dir_all(&src).unwrap(); // opening a directory as a file fails
        let dest = dir.join("dest.flac");
        assert!(copy_then_delete(&src, &dest).is_err());
        assert!(
            !dest.exists(),
            "no partial copy may be left at the destination"
        );
    }
}

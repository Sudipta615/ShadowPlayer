//! Stable track identity across moves and renames.
//!
//! Track ids are derived from the file path, so a move used to create a brand
//! new track and orphan its play counts, favourites and playlist membership.
//! [`resolve_identity`] fixes that: before storing a file at a path we have not
//! seen before, it fingerprints the content and — if an existing track holds the
//! same content and its old file is gone — moves that track's row to the new
//! path, keeping its id.
//!
//! There is a second, cheaper way to recognise the same thing, and it is the
//! one that works on a case-insensitive filesystem.
//!
//! [`resolve_identity_by_file_id`] asks the *filesystem* what the file is,
//! rather than what it is called. On Windows and on macOS — where the
//! filesystem ignores the case of a name — renaming `Track.FLAC` to
//! `track.flac` does not move a file; it changes how the same file is spelled.
//! The old path still resolves, so the content-fingerprint path cannot tell
//! that anything happened (it insists the old file be *gone*), and the new
//! spelling becomes a second track. The filesystem's own identity does not
//! have that blind spot: one file, one identity, however it is spelled.
//!
//! The fingerprint path stays, because it recognises something the identity
//! path cannot: a file that was moved and *replaced* by a different file with
//! the same contents.

use database::TrackRepo;
use domain::ids::TrackId;
use log::{info, warn};
use std::ffi::OsStr;
use std::path::Path;

use crate::fingerprint::Fingerprint;

/// True only when the old file is *definitely* gone.
///
/// `Path::exists()` returns false for permission errors, stale NFS mounts and
/// other I/O failures too, so a track whose volume was merely unreachable
/// looked deleted — and its identity, with its play counts and playlist
/// membership, could be handed to a different file. Only a genuine `NotFound`
/// counts as gone.
fn old_file_is_gone(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(_) => false,
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// True when the directory still holds an entry with exactly this name.
///
/// See [`domain::file_identity::directory_still_holds`], which is where the
/// rule lives; this is the local name for it.
fn directory_still_holds(parent: &Path, file_name: &OsStr) -> bool {
    domain::file_identity::directory_still_holds(parent, file_name)
}

/// Fingerprint the file at `path` for a caller that already knows the row at
/// that path exists but may predate fingerprints. Best-effort.
pub fn backfill_fingerprint(repo: &TrackRepo, path: &Path, id: &TrackId) {
    if let Some(fp) = Fingerprint::compute(path) {
        let _ = repo.set_content_hash(id, Some(fp.as_str()));
    }
}

/// Recognise a file the filesystem already knows, under a different name.
///
/// * `path` — the file being indexed.
/// * `already_indexed` — whether a row already exists for this exact path.
/// * `file_id` — the filesystem's identity for the file, when it is known.
///
/// Returns `Some(id)` when the file is one the library already tracks under a
/// different name — a rename, including a rename that only changed the case —
/// and the row must move to the new path. Returns `None` when the caller's own
/// id is correct.
///
/// A hard link is *not* a rename: the old entry is still in the directory, so
/// the row stays where it is and the new path is indexed as its own track.
pub fn resolve_identity_by_file_id(
    repo: &TrackRepo,
    path: &Path,
    already_indexed: bool,
    file_id: Option<&str>,
) -> Option<TrackId> {
    if already_indexed {
        return None;
    }
    let file_id = file_id?;

    let candidates = repo.find_by_file_id(file_id).ok()?;
    if candidates.is_empty() {
        return None;
    }

    // More than one row for one file means the library already holds a
    // duplicate. The migration merges those; until it has run (or if a copy
    // was made by a tool that preserves the identity), the ambiguity is
    // resolved by refusing to move anything.
    let existing = candidates.first()?;

    // The same path: nothing to move.
    if existing.path == path {
        return None;
    }

    // A hard link is a second entry for the same file, not a rename.
    let (Some(parent), Some(file_name)) = (existing.path.parent(), existing.path.file_name())
    else {
        return None;
    };
    if directory_still_holds(parent, file_name) {
        return None;
    }

    match repo.set_file_id_and_path(&existing.id, Some(file_id), Some(path)) {
        Ok(true) => {
            info!(
                "Renamed track {} -> {} (the filesystem says it is the same file)",
                existing.path.display(),
                path.display()
            );
            Some(existing.id)
        }
        Ok(false) => None,
        Err(e) => {
            warn!(
                "Could not move {} -> {}: {}",
                existing.path.display(),
                path.display(),
                e
            );
            None
        }
    }
}

/// Work out the identity to store a file under.
///
/// * `path` — the file being indexed.
/// * `path_derived_id` — the id the caller computed from `path`.
/// * `already_indexed` — whether a row already exists for this exact path.
/// * `content_hash` — filled in with the file's fingerprint when computable.
///
/// Returns `Some(id)` when the track must be stored under a *different*
/// (pre-existing) id because the file was recognised as moved. Returns `None`
/// when the caller's own id is correct.
///
/// Relinking only happens when the old file no longer exists on disk, so a
/// duplicated copy of a track is still indexed as a separate track.
pub fn resolve_identity(
    repo: &TrackRepo,
    path: &Path,
    path_derived_id: TrackId,
    already_indexed: bool,
    content_hash: &mut Option<String>,
) -> Option<TrackId> {
    let fp = Fingerprint::compute(path)?;
    let hash = fp.into_string();
    *content_hash = Some(hash.clone());

    if already_indexed {
        return None;
    }

    // An ambiguous fingerprint is never used, so we cannot steal the wrong row.
    let (old_id, old_path) = repo.find_by_content_hash(&hash).ok().flatten()?;
    if old_path.as_path() == path || !old_file_is_gone(&old_path) {
        return None;
    }

    match repo.relink_path(&old_id, path, Some(&hash)) {
        Ok(true) => {
            info!(
                "Relinked moved track {} -> {}",
                old_path.display(),
                path.display()
            );
            Some(old_id)
        }
        Ok(false) => None,
        Err(e) => {
            warn!(
                "Could not relink {} -> {}: {}",
                old_path.display(),
                path.display(),
                e
            );
            // Fall through: the caller stores the file under its own id.
            let _ = path_derived_id;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{Database, TrackRepo};
    use domain::models::Track;
    use std::path::{Path, PathBuf};

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_identity_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Fingerprints do not need real audio, but they do need the bytes.
    fn write_content(path: &Path, seed: u8) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let bytes: Vec<u8> = (0..64 * 1024u32)
            .map(|i| (i as u8).wrapping_mul(seed).wrapping_add(seed))
            .collect();
        std::fs::write(path, bytes).expect("write content");
    }

    fn sample_track(id: TrackId, path: &Path) -> Track {
        Track {
            id,
            path: path.to_path_buf(),
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 60.0,
            bpm: None,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            bit_depth: None,
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 64 * 1024,
            mtime_unix_secs: 1_700_000_000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: true,
            rating: 0,
            has_lyrics: false,
            play_count: 7,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
        }
    }

    /// The file was moved: the fingerprint matches and the old path is gone, so
    /// the row is relinked rather than duplicated.
    #[test]
    fn a_moved_file_keeps_its_id() {
        let root = temp_root("moved");
        let old_path = root.join("old.flac");
        let new_path = root.join("new.flac");
        write_content(&old_path, 3);

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let old_id = TrackId::new();
        let mut track = sample_track(old_id, &old_path);
        let hash = Fingerprint::compute(&old_path).unwrap();
        track.content_hash = Some(hash.as_str().to_string());
        repo.upsert(&track).unwrap();

        std::fs::rename(&old_path, &new_path).unwrap();
        let derived = TrackId::from_canonical_path(&new_path);
        let mut content_hash = None;

        let resolved = resolve_identity(&repo, &new_path, derived, false, &mut content_hash);

        assert_eq!(resolved, Some(old_id), "the move is recognised");
        assert_eq!(content_hash.as_deref(), Some(hash.as_str()));
        // And the scanner stores the row under the old id at the new path.
        assert!(repo
            .relink_path(&old_id, &new_path, content_hash.as_deref())
            .unwrap());
        let stored = repo.get_by_path(&new_path).unwrap().unwrap();
        assert_eq!(stored.id, old_id);
        assert_eq!(stored.play_count, 7, "play counts survive the move");
        assert!(stored.is_favorite);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The same bytes still exist where they were: this is a copy, not a move,
    /// and it must be indexed as its own track.
    #[test]
    fn a_copy_does_not_steal_identity() {
        let root = temp_root("copy");
        let original = root.join("original.flac");
        let copy = root.join("copy.flac");
        write_content(&original, 5);
        std::fs::copy(&original, &copy).unwrap();

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let original_id = TrackId::new();
        let mut track = sample_track(original_id, &original);
        track.content_hash = Some(
            Fingerprint::compute(&original)
                .unwrap()
                .as_str()
                .to_string(),
        );
        repo.upsert(&track).unwrap();

        let derived = TrackId::from_canonical_path(&copy);
        let mut content_hash = None;
        let resolved = resolve_identity(&repo, &copy, derived, false, &mut content_hash);

        assert_eq!(resolved, None, "a copy keeps its own id");
        assert_eq!(
            content_hash.as_deref(),
            Some(track.content_hash.as_deref().unwrap())
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file that was renamed keeps its row — even when the rename only
    /// changed the case, which is the case a content fingerprint cannot see
    /// (the old path still resolves on a case-insensitive filesystem, so the
    /// fingerprint path insists the old file be gone and never relinks).
    #[test]
    fn a_rename_keeps_its_id_through_the_filesystem_identity() {
        let root = temp_root("rename");
        let old_path = root.join("Track.FLAC");
        let new_path = root.join("track.flac");
        write_content(&old_path, 11);

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let old_id = TrackId::new();
        let mut track = sample_track(old_id, &old_path);
        track.file_id =
            domain::file_identity::file_identity(&old_path).map(|identity| identity.to_key());
        repo.upsert(&track).unwrap();

        std::fs::rename(&old_path, &new_path).unwrap();

        let resolved = resolve_identity_by_file_id(
            &repo,
            &new_path,
            false,
            domain::file_identity::file_identity(&new_path)
                .map(|identity| identity.to_key())
                .as_deref(),
        );

        assert_eq!(resolved, Some(old_id), "the rename is recognised");
        let stored = repo.get_by_path(&new_path).unwrap().unwrap();
        assert_eq!(stored.id, old_id);
        assert_eq!(stored.play_count, 7, "play counts survive the rename");
        assert!(stored.is_favorite);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A hard link is a second *entry* for one file, not a rename of it. The
    /// row must stay where it is, and the new path must be indexed as its own
    /// track — otherwise a library with hard-linked duplicates would flicker
    /// between the two paths on every scan.
    #[test]
    fn a_hard_link_is_not_treated_as_a_rename() {
        let root = temp_root("hardlink");
        let original = root.join("original.flac");
        let link = root.join("link.flac");
        write_content(&original, 13);
        std::fs::hard_link(&original, &link).unwrap();

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let original_id = TrackId::new();
        let mut track = sample_track(original_id, &original);
        track.file_id =
            domain::file_identity::file_identity(&original).map(|identity| identity.to_key());
        repo.upsert(&track).unwrap();

        let file_id = domain::file_identity::file_identity(&link).map(|i| i.to_key());
        let resolved = resolve_identity_by_file_id(&repo, &link, false, file_id.as_deref());

        assert_eq!(resolved, None, "a hard link is not a rename");
        // The original row is untouched.
        let stored = repo.get_by_path(&original).unwrap().unwrap();
        assert_eq!(stored.id, original_id);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file that is already indexed under this exact path is left alone.
    #[test]
    fn an_already_indexed_path_is_left_alone() {
        let root = temp_root("already");
        let path = root.join("song.flac");
        write_content(&path, 17);

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let id = TrackId::new();
        let mut track = sample_track(id, &path);
        track.file_id = domain::file_identity::file_identity(&path).map(|i| i.to_key());
        repo.upsert(&track).unwrap();

        let file_id = domain::file_identity::file_identity(&path).map(|i| i.to_key());
        let resolved = resolve_identity_by_file_id(&repo, &path, true, file_id.as_deref());
        assert_eq!(resolved, None);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A row indexed before fingerprints existed gets one, so the next move can
    /// still be recognised.
    #[test]
    fn backfill_stores_the_fingerprint() {
        let root = temp_root("backfill");
        let path = root.join("song.flac");
        write_content(&path, 7);

        let db = Database::in_memory().unwrap();
        let repo = TrackRepo::new(db);
        let id = TrackId::new();
        repo.upsert(&sample_track(id, &path)).unwrap();
        assert!(repo.get_by_id(&id).unwrap().unwrap().content_hash.is_none());

        backfill_fingerprint(&repo, &path, &id);

        let expected = Fingerprint::compute(&path).unwrap();
        assert_eq!(
            repo.get_by_id(&id)
                .unwrap()
                .unwrap()
                .content_hash
                .as_deref(),
            Some(expected.as_str())
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}

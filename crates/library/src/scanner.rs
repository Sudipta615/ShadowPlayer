use crate::artwork::ArtworkManager;
use crossbeam::channel::Sender;
use database::{Database, TrackRepo};
use domain::events::LibraryScanEvent;
use log::{info, warn};
use metadata::extract_metadata;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use walkdir::WalkDir;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "ogg", "wav", "m4a", "aac", "opus", "alac", "aiff", "aif", "ape", "wv",
    "tta", "dsf", "dff", "cue",
];

/// True if `path` carries a supported audio (or CUE) extension.
pub fn is_supported_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS.iter().any(|s| *s == e)
        })
        .unwrap_or(false)
}

pub struct LibraryScanner {
    db: Database,
    artwork_mgr: Arc<ArtworkManager>,
}

impl LibraryScanner {
    pub fn new(db: Database, artwork_mgr: Arc<ArtworkManager>) -> Self {
        Self { db, artwork_mgr }
    }

    /// Perform an incremental scan on the given directory.
    pub fn scan_directory(
        &self,
        root: &Path,
        cancel_flag: Arc<AtomicBool>,
        event_sender: Option<Sender<LibraryScanEvent>>,
    ) {
        let start_time = Instant::now();
        if let Some(ref tx) = event_sender {
            let _ = tx.send(LibraryScanEvent::Started {
                root: root.to_path_buf(),
            });
        }

        let repo = TrackRepo::new(self.db.clone());
        let mut scanned_count = 0;
        let mut added_count = 0;
        let mut updated_count = 0;

        let supported_ext_set: HashSet<&str> = SUPPORTED_EXTENSIONS.iter().copied().collect();

        info!("Starting incremental library scan for: {:?}", root);

        // Album images referenced by a `.cue` sheet are indexed as that sheet's
        // tracks, so they must not *also* be indexed as one long track.
        let cue_covered = self.collect_cue_covered_audio(root, &cancel_flag);

        // Every path the walk reached, so a file that vanished can be pruned
        // afterwards. Pruning is skipped if the walk hit errors, because a
        // permission problem must not look like a deletion.
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut walk_errors = 0usize;
        let mut skipped_count = 0usize;

        // `follow_links(false)`: following symlinks let a scan index files
        // outside the chosen root (and index them twice through an alias).
        let walker = WalkDir::new(root).follow_links(false).into_iter();

        for entry in walker {
            if cancel_flag.load(Ordering::Relaxed) {
                info!("Library scan cancelled");
                return;
            }

            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    walk_errors += 1;
                    warn!("Skipping unreadable entry under {}: {}", root.display(), e);
                    continue;
                }
            };

            // Directories are not descended into when they are symlinks, but a
            // symlink to a file is still a file worth indexing.
            let is_file = entry.file_type().is_file()
                || (entry.file_type().is_symlink()
                    && std::fs::metadata(entry.path())
                        .map(|m| m.is_file())
                        .unwrap_or(false));
            if !is_file {
                continue;
            }

            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());

            if let Some(ref e) = ext {
                if !supported_ext_set.contains(e.as_str()) {
                    continue;
                }
            } else {
                continue;
            }

            seen.insert(path.to_path_buf());

            if cue_covered.contains(path) {
                continue;
            }

            scanned_count += 1;

            // Everything from here needs the file's metadata. Failing to get
            // it used to skip the file *silently*: not indexed, not counted,
            // not logged.
            let metadata = match path.metadata() {
                Ok(metadata) => metadata,
                Err(e) => {
                    skipped_count += 1;
                    warn!("Skipping {}: cannot read its metadata: {e}", path.display());
                    continue;
                }
            };

            {
                let (mtime_secs, mtime_nanos) = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| (d.as_secs() as i64, d.subsec_nanos()))
                    .unwrap_or((0, 0));
                let size = metadata.len();

                if let Ok(Some(existing)) = repo.get_by_path(path) {
                    // A row written before `mtime_nanos` existed carries no
                    // sub-second part; accept the second-granularity match once
                    // (and record the finer value) instead of forcing a full
                    // re-decode of the library after the upgrade.
                    let same_subsecond =
                        existing.mtime_nanos == mtime_nanos || existing.mtime_nanos == 0;
                    if existing.mtime_unix_secs == mtime_secs
                        && existing.file_size_bytes == size
                        && same_subsecond
                    {
                        if existing.mtime_nanos == 0 && mtime_nanos != 0 {
                            if let Err(e) = repo.set_mtime_nanos(&existing.id, mtime_nanos) {
                                warn!(
                                    "Could not record the finer mtime for {}: {e}",
                                    path.display()
                                );
                            }
                        }
                        // File unchanged, skip decoding — but backfill the
                        // content fingerprint for rows indexed before V3 so a
                        // later move can still be recognised.
                        if existing.content_hash.is_none() {
                            crate::identity::backfill_fingerprint(&repo, path, &existing.id);
                        }
                        continue;
                    }
                }

                // Check if file is a CUE sheet to split
                if ext.as_deref() == Some("cue") {
                    match metadata::extract_cue_metadata(path) {
                        Ok(cue_tracks) => {
                            for extracted in cue_tracks {
                                let is_new = repo.get_by_path(&extracted.track.path).ok().flatten().is_none();
                                if is_new {
                                    added_count += 1;
                                } else {
                                    updated_count += 1;
                                }

                                if let Some((bytes, _mime)) = extracted.embedded_artwork {
                                    self.artwork_mgr.save_thumbnail(&extracted.track.id, &bytes);
                                } else if let Some(folder_art) = self.artwork_mgr.find_folder_artwork(path) {
                                    if let Ok(bytes) = std::fs::read(&folder_art) {
                                        self.artwork_mgr.save_thumbnail(&extracted.track.id, &bytes);
                                    }
                                }

                                if let Err(e) = repo.upsert(&extracted.track) {
                                    warn!("Failed to save CUE track {}: {}", extracted.track.path.display(), e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Skipping unreadable CUE sheet {}: {:?}", path.display(), e);
                        }
                    }
                    continue;
                }

                // File is new or changed - extract metadata
                match extract_metadata(path) {
                    Ok(mut extracted) => {
                        let already_indexed = repo.get_by_path(path).ok().flatten().is_some();

                        // A file we have never seen at this path may simply have
                        // been moved or renamed; adopt the old identity when the
                        // content matches a track whose file is gone, so play
                        // counts, favourites and playlist membership survive.
                        let path_derived_id = extracted.track.id;
                        if let Some(stable_id) = crate::identity::resolve_identity(
                            &repo,
                            path,
                            path_derived_id,
                            already_indexed,
                            &mut extracted.track.content_hash,
                        ) {
                            extracted.track.id = stable_id;
                        }
                        let is_new = !already_indexed
                            && extracted.track.id == path_derived_id;

                        if is_new {
                            added_count += 1;
                        } else {
                            updated_count += 1;
                        }

                        // Save artwork thumbnail if embedded
                        if let Some((bytes, _mime)) = extracted.embedded_artwork {
                            self.artwork_mgr.save_thumbnail(&extracted.track.id, &bytes);
                        } else if let Some(folder_art) = self.artwork_mgr.find_folder_artwork(path) {
                            if let Ok(bytes) = std::fs::read(&folder_art) {
                                self.artwork_mgr.save_thumbnail(&extracted.track.id, &bytes);
                            }
                        }

                        if let Err(e) = repo.upsert(&extracted.track) {
                            warn!("Failed to save track {}: {}", path.display(), e);
                        }
                    }
                    Err(e) => {
                        // Corrupt or unsupported file does not abort scan
                        skipped_count += 1;
                        warn!("Skipping unreadable audio file {}: {}", path.display(), e);
                    }
                }
            }

            if scanned_count % 25 == 0 {
                if let Some(ref tx) = event_sender {
                    let _ = tx.send(LibraryScanEvent::Progress {
                        scanned: scanned_count,
                        added: added_count,
                        updated: updated_count,
                    });
                }
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            info!("Library scan cancelled");
            return;
        }

        // Reconcile: anything under this root that the walk no longer sees is
        // gone from disk. Without this, deleted files stayed in the library
        // forever (the watcher only ever adds and updates).
        let mut removed_count = 0usize;
        if walk_errors == 0 {
            match repo.list_paths_under_prefix(root) {
                Ok(rows) => {
                    for (id, path) in rows {
                        if !seen.contains(&cue_base_path(&path)) {
                            match repo.delete(&id) {
                                Ok(()) => removed_count += 1,
                                Err(e) => warn!("Could not remove missing track {}: {}", path.display(), e),
                            }
                        }
                    }
                }
                Err(e) => warn!("Could not reconcile removed files under {}: {}", root.display(), e),
            }
        } else {
            warn!(
                "Skipping removal reconciliation for {}: the walk reported {} error(s)",
                root.display(),
                walk_errors
            );
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let total_tracks = repo.count().unwrap_or(0);

        info!(
            "Scan completed: {} scanned, {} added, {} updated, {} removed, {} skipped in {}ms. \
             Total library: {} tracks",
            scanned_count,
            added_count,
            updated_count,
            removed_count,
            skipped_count,
            elapsed,
            total_tracks
        );

        if let Some(ref tx) = event_sender {
            let _ = tx.send(LibraryScanEvent::Completed {
                total_tracks,
                elapsed_ms: elapsed,
                skipped: skipped_count,
            });
        }
    }

    /// The album images that a `.cue` sheet in `root` claims, so the walk can
    /// index the sheet instead of the image.
    fn collect_cue_covered_audio(
        &self,
        root: &Path,
        cancel_flag: &AtomicBool,
    ) -> HashSet<PathBuf> {
        let mut covered = HashSet::new();
        let walker = WalkDir::new(root).follow_links(false).into_iter();
        for entry in walker.filter_map(|e| e.ok()) {
            if cancel_flag.load(Ordering::Relaxed) {
                return covered;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let is_cue = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("cue"))
                .unwrap_or(false);
            if !is_cue {
                continue;
            }
            if let Some(audio) = metadata::cue_referenced_audio(path) {
                covered.insert(audio);
            }
        }
        covered
    }
}

/// The real file behind a path, undoing the CUE virtual-path suffix when there
/// is one. Used to decide whether the file a row was indexed from still exists.
fn cue_base_path(path: &Path) -> PathBuf {
    match path.to_str().and_then(domain::cue::parse_cue_virtual_path) {
        Some(virtual_path) => PathBuf::from(virtual_path.audio),
        None => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{FavoritesRepo, TrackRepo};
    use domain::models::{SortField, SortOrder};
    use std::path::PathBuf;

    /// Minimal 16-bit mono PCM WAV carrying a sine tone.
    fn write_wav(path: &Path, freq: f32, secs: f32) {
        let sr = 44_100u32;
        let n = (sr as f32 * secs) as u32;
        let mut data = Vec::with_capacity(n as usize * 2);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin();
            data.extend_from_slice(&((s * 0.4 * i16::MAX as f32) as i16).to_le_bytes());
        }
        let data_len = data.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&sr.to_le_bytes());
        out.extend_from_slice(&(sr * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&data);
        std::fs::write(path, out).expect("write wav");
    }

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp_scan_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn scan(scanner: &LibraryScanner, root: &Path) {
        scanner.scan_directory(root, Arc::new(AtomicBool::new(false)), None);
    }

    /// End-to-end: moving a file to a new folder must not orphan its identity.
    #[test]
    fn moved_file_keeps_its_identity() {
        let root = temp_root("relink");
        let dir_a = root.join("a");
        let dir_b = root.join("b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        let original = dir_a.join("track.wav");
        write_wav(&original, 440.0, 1.0);

        let db = Database::in_memory().expect("in-memory db");
        let scanner = LibraryScanner::new(db.clone(), Arc::new(ArtworkManager::new(root.join("art"))));
        let repo = TrackRepo::new(db.clone());

        scan(&scanner, &root);
        assert_eq!(repo.count().unwrap(), 1);
        let before = repo
            .list(SortField::Title, SortOrder::Ascending)
            .unwrap()
            .remove(0);
        assert!(
            before.content_hash.is_some(),
            "the scan must fingerprint the file"
        );
        FavoritesRepo::new(db.clone()).add(&before.id).unwrap();

        // Rows indexed before fingerprints existed are backfilled by a plain
        // re-scan even though the file itself is unchanged.
        repo.set_content_hash(&before.id, None).unwrap();
        scan(&scanner, &root);
        assert!(
            repo.get_by_id(&before.id)
                .unwrap()
                .unwrap()
                .content_hash
                .is_some(),
            "an unchanged file must have its fingerprint backfilled"
        );

        // Move the file into another folder under the same root.
        let moved = dir_b.join("renamed.wav");
        std::fs::rename(&original, &moved).unwrap();

        scan(&scanner, &root);

        assert_eq!(
            repo.count().unwrap(),
            1,
            "a moved file must not be indexed twice"
        );
        let after = repo
            .get_by_id(&before.id)
            .expect("the original id must survive")
            .expect("row still present");
        assert_eq!(after.path, moved, "the row follows the file");
        assert!(
            FavoritesRepo::new(db.clone())
                .list()
                .unwrap()
                .iter()
                .any(|t| t.id == before.id),
            "favourite must survive the move"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// A file shorter than a second used to divide by zero while deriving its
    /// bitrate, which aborted the whole scan.
    #[test]
    fn sub_second_files_do_not_abort_the_scan() {
        let root = temp_root("short");
        write_wav(&root.join("jingle.wav"), 440.0, 0.5);

        let db = Database::in_memory().expect("in-memory db");
        let scanner = LibraryScanner::new(db.clone(), Arc::new(ArtworkManager::new(root.join("art"))));
        let repo = TrackRepo::new(db.clone());

        scan(&scanner, &root);

        assert_eq!(repo.count().unwrap(), 1);
        assert!(repo.list_paths_under_prefix(&root).unwrap()[0].1.ends_with("jingle.wav"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// One album image plus its `.cue` sheet must produce the sheet's tracks
    /// only — not the tracks *and* a full-length copy of the image.
    #[test]
    fn a_cue_album_is_indexed_once() {
        let root = temp_root("cue");
        write_wav(&root.join("image.wav"), 440.0, 2.0);
        std::fs::write(
            root.join("album.cue"),
            "PERFORMER \"Someone\"\nTITLE \"An Album\"\nFILE \"image.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:01:00\n",
        )
        .unwrap();

        let db = Database::in_memory().expect("in-memory db");
        let scanner = LibraryScanner::new(db.clone(), Arc::new(ArtworkManager::new(root.join("art"))));
        let repo = TrackRepo::new(db.clone());

        scan(&scanner, &root);

        let rows = repo.list_paths_under_prefix(&root).unwrap();
        assert_eq!(rows.len(), 2, "two CUE tracks, no separate album image");
        for (_, path) in &rows {
            let parsed = domain::cue::parse_cue_virtual_path(path.to_str().unwrap())
                .expect("a CUE virtual path");
            assert!(
                parsed.sheet.is_some(),
                "the sheet must be recorded so playback does not have to guess"
            );
        }

        // A second scan must be stable (nothing new, nothing pruned).
        scan(&scanner, &root);
        assert_eq!(repo.count().unwrap(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Files removed from disk must leave the library on the next scan.
    #[test]
    fn files_deleted_from_disk_are_pruned() {
        let root = temp_root("prune");
        write_wav(&root.join("keep.wav"), 440.0, 1.0);
        write_wav(&root.join("drop.wav"), 660.0, 1.0);

        let db = Database::in_memory().expect("in-memory db");
        let scanner = LibraryScanner::new(db.clone(), Arc::new(ArtworkManager::new(root.join("art"))));
        let repo = TrackRepo::new(db.clone());

        scan(&scanner, &root);
        assert_eq!(repo.count().unwrap(), 2);

        std::fs::remove_file(root.join("drop.wav")).unwrap();
        scan(&scanner, &root);

        assert_eq!(repo.count().unwrap(), 1, "the missing file is reconciled away");
        assert!(repo.list_paths_under_prefix(&root).unwrap()[0].1.ends_with("keep.wav"));

        let _ = std::fs::remove_dir_all(&root);
    }
}

use crate::artwork::ArtworkManager;
use crossbeam::channel::Sender;
use database::{Database, TrackRepo};
use domain::events::LibraryScanEvent;
use log::{info, warn};
use metadata::extract_metadata;
use std::collections::HashSet;
use std::path::Path;
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

        let walker = WalkDir::new(root).follow_links(true).into_iter();

        for entry in walker.filter_map(|e| e.ok()) {
            if cancel_flag.load(Ordering::Relaxed) {
                info!("Library scan cancelled");
                return;
            }

            if !entry.file_type().is_file() {
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

            scanned_count += 1;

            // Check if file is already indexed and unchanged
            if let Ok(metadata) = path.metadata() {
                let mtime = metadata
                    .modified()
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    })
                    .unwrap_or(0);
                let size = metadata.len();

                if let Ok(Some(existing)) = repo.get_by_path(path) {
                    if existing.mtime_unix_secs == mtime && existing.file_size_bytes == size {
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

        let elapsed = start_time.elapsed().as_millis() as u64;
        let total_tracks = repo.count().unwrap_or(0);

        info!(
            "Scan completed: {} scanned, {} added, {} updated in {}ms. Total library: {} tracks",
            scanned_count, added_count, updated_count, elapsed, total_tracks
        );

        if let Some(ref tx) = event_sender {
            let _ = tx.send(LibraryScanEvent::Completed {
                total_tracks,
                elapsed_ms: elapsed,
            });
        }
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
}

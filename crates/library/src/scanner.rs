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
                        // File unchanged, skip decoding!
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
                    Ok(extracted) => {
                        let is_new = repo.get_by_path(path).ok().flatten().is_none();
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

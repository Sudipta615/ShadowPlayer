use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::channel::Receiver;
use database::Database;
use domain::events::LibraryScanEvent;
use library::{ArtworkManager, LibraryScanner};

/// Service managing background library directory scanning and progress telemetry
pub struct ScannerService {
    scanner: Arc<LibraryScanner>,
    is_scanning: bool,
    scan_cancel: Arc<AtomicBool>,
    scan_progress_text: String,
    scan_rx: Option<Receiver<LibraryScanEvent>>,
}

impl ScannerService {
    pub fn new(db: Database, artwork_mgr: Arc<ArtworkManager>) -> Self {
        let scanner = Arc::new(LibraryScanner::new(db, artwork_mgr));
        Self {
            scanner,
            is_scanning: false,
            scan_cancel: Arc::new(AtomicBool::new(false)),
            scan_progress_text: String::new(),
            scan_rx: None,
        }
    }

    pub fn is_scanning(&self) -> bool {
        self.is_scanning
    }

    pub fn progress_text(&self) -> &str {
        &self.scan_progress_text
    }

    /// Launch background directory scan
    pub fn start_scan(&mut self, path: PathBuf) {
        if self.is_scanning {
            return;
        }

        // Bounded channel to prevent queue bloating while ensuring smooth event bursts
        let (tx, rx) = crossbeam::channel::bounded(128);
        self.scan_rx = Some(rx);
        self.is_scanning = true;
        self.scan_progress_text = format!("Scanning: {}...", path.display());
        self.scan_cancel.store(false, Ordering::Relaxed);

        let scanner = self.scanner.clone();
        let cancel = self.scan_cancel.clone();
        let scan_path = path;

        std::thread::Builder::new()
            .name("shadowplayer-scanner".into())
            .spawn(move || {
                scanner.scan_directory(&scan_path, cancel, Some(tx));
            })
            .expect("spawn scanner worker");
    }

    /// Poll scan events; returns true if scan completed and library should reload
    pub fn poll(&mut self) -> bool {
        let mut scan_completed = false;
        let mut clear_scan_rx = false;

        if let Some(ref rx) = self.scan_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    LibraryScanEvent::Started { root } => {
                        self.scan_progress_text = format!("Scanning: {}...", root.display());
                    }
                    LibraryScanEvent::Progress {
                        scanned,
                        added,
                        updated,
                    } => {
                        self.scan_progress_text =
                            format!("Scanning: {} files ({}+ / {}~)", scanned, added, updated);
                    }
                    LibraryScanEvent::Completed {
                        total_tracks,
                        elapsed_ms,
                        skipped,
                    } => {
                        self.is_scanning = false;
                        self.scan_progress_text = if skipped == 0 {
                            format!("Scan complete: {} tracks ({}ms)", total_tracks, elapsed_ms)
                        } else {
                            // A file that cannot be indexed is still a fact the
                            // user should hear about; the log has the details.
                            format!(
                                "Scan complete: {} tracks ({}ms), {} file(s) skipped — see the log",
                                total_tracks, elapsed_ms, skipped
                            )
                        };
                        scan_completed = true;
                    }
                    LibraryScanEvent::Failed { error } => {
                        self.is_scanning = false;
                        self.scan_progress_text = format!("Scan failed: {}", error);
                    }
                }
            }
            if !self.is_scanning {
                clear_scan_rx = true;
            }
        }

        if clear_scan_rx {
            self.scan_rx = None;
        }

        scan_completed
    }
}

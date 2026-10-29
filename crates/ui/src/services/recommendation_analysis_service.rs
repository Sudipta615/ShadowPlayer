//! Background library analysis for the recommendation engine.
//!
//! Feature extraction is CPU-bound and decodes audio, so it runs on a worker
//! thread and reports progress through a bounded channel. The decoder adapter
//! lives in `media-tools`; this service only schedules work and persists the
//! resulting vectors.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::channel::{bounded, Receiver};
use database::{Database, RecommendationRepo};
use media_tools::analysis::DecoderWindowSource;
use recommendation::{analyze_track, AnalysisConfig, AnalysisProfile, FEATURE_SCHEMA_VERSION};

/// Progress of one analysis run, or the summary of the last one.
#[derive(Debug, Clone, Default)]
pub struct RecommendationAnalysisProgress {
    pub running: bool,
    pub done: usize,
    pub total: usize,
    pub failed: usize,
    /// File currently being analysed, or the last status line.
    pub current: String,
}

/// Events the analysis worker sends to the UI thread.
#[derive(Debug, Clone)]
pub enum RecommendationAnalysisEvent {
    Progress {
        done: usize,
        total: usize,
        failed: usize,
        current: String,
    },
    Finished {
        done: usize,
        total: usize,
        failed: usize,
        cancelled: bool,
    },
    Error(String),
}

/// Owns the one background analysis worker.
pub struct RecommendationAnalysisService {
    db: Database,
    rx: Option<Receiver<RecommendationAnalysisEvent>>,
    cancel: Arc<AtomicBool>,
    running: bool,
    progress: RecommendationAnalysisProgress,
}

impl RecommendationAnalysisService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            running: false,
            progress: RecommendationAnalysisProgress::default(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn progress(&self) -> &RecommendationAnalysisProgress {
        &self.progress
    }

    /// Analyse every track that has no current-schema feature vector.
    ///
    /// Returns `false` when a run is already in flight; the existing run is
    /// never silently replaced.
    pub fn start(&mut self, profile: AnalysisProfile) -> bool {
        if self.running {
            return false;
        }
        let db = self.db.clone();
        let (tx, rx) = bounded(256);
        self.rx = Some(rx);
        self.running = true;
        self.cancel.store(false, Ordering::Relaxed);
        self.progress = RecommendationAnalysisProgress {
            running: true,
            ..RecommendationAnalysisProgress::default()
        };

        let cancel = self.cancel.clone();
        std::thread::Builder::new()
            .name("shadowplayer-recommend-analysis".into())
            .spawn(move || {
                let repo = RecommendationRepo::new(db);
                let targets = match repo.list_tracks_missing_features(FEATURE_SCHEMA_VERSION) {
                    Ok(targets) => targets,
                    Err(error) => {
                        let _ = tx.send(RecommendationAnalysisEvent::Error(error.to_string()));
                        return;
                    }
                };
                let total = targets.len();
                let config = AnalysisConfig::for_profile(profile);
                let mut done = 0;
                let mut failed = 0;
                let _ = tx.send(RecommendationAnalysisEvent::Progress {
                    done,
                    total,
                    failed,
                    current: String::new(),
                });

                for target in targets {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let current = display_name(&target.path);
                    let result = DecoderWindowSource::open(&target.path).and_then(|mut source| {
                        analyze_track(&mut source, &config).map_err(|error| {
                            media_tools::MediaError::InvalidInput(error.to_string())
                        })
                    });
                    match result {
                        Ok(analysis) => {
                            match repo.put_feature(
                                &target.track_id,
                                &analysis.features,
                                FEATURE_SCHEMA_VERSION,
                            ) {
                                Ok(()) => done += 1,
                                Err(error) => {
                                    failed += 1;
                                    log::warn!(
                                        "Could not store recommendation features for {}: {error}",
                                        target.path.display()
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            failed += 1;
                            log::debug!(
                                "Recommendation analysis skipped {}: {error}",
                                target.path.display()
                            );
                        }
                    }
                    let _ = tx.send(RecommendationAnalysisEvent::Progress {
                        done,
                        total,
                        failed,
                        current,
                    });
                    // Analysis is deliberately low priority: let the audio
                    // thread and the UI run before the next file.
                    std::thread::yield_now();
                }

                let _ = tx.send(RecommendationAnalysisEvent::Finished {
                    done,
                    total,
                    failed,
                    cancelled: cancel.load(Ordering::Relaxed),
                });
            })
            .expect("spawn recommendation analysis worker");
        true
    }

    /// Stop after the current file. The worker still reports `Finished` with
    /// `cancelled = true`.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Non-blocking poll for worker events.
    pub fn poll(&mut self) -> Option<RecommendationAnalysisEvent> {
        let rx = self.rx.as_ref()?;
        let event = rx.try_recv().ok()?;
        match &event {
            RecommendationAnalysisEvent::Progress {
                done,
                total,
                failed,
                current,
            } => {
                self.progress.running = true;
                self.progress.done = *done;
                self.progress.total = *total;
                self.progress.failed = *failed;
                self.progress.current.clone_from(current);
            }
            RecommendationAnalysisEvent::Finished {
                done,
                total,
                failed,
                ..
            } => {
                self.running = false;
                self.progress.running = false;
                self.progress.done = *done;
                self.progress.total = *total;
                self.progress.failed = *failed;
                self.rx = None;
            }
            RecommendationAnalysisEvent::Error(_) => {
                self.running = false;
                self.progress.running = false;
                self.rx = None;
            }
        }
        Some(event)
    }
}

impl Default for RecommendationAnalysisService {
    fn default() -> Self {
        Self::new(Database::in_memory().expect("in-memory database"))
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_prefers_the_file_name() {
        assert_eq!(display_name(Path::new("/music/a/b.flac")), "b.flac");
        assert_eq!(display_name(Path::new("/")), "/");
    }

    #[test]
    fn a_second_start_is_rejected_while_running() {
        let db = Database::in_memory().expect("in-memory database");
        let mut service = RecommendationAnalysisService::new(db.clone());
        assert!(service.start(AnalysisProfile::LowResource));
        assert!(!service.start(AnalysisProfile::Balanced));
        service.cancel();
    }
}

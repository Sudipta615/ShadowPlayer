//! Filesystem work that used to run on the frame thread.
//!
//! Tag write-back, deleting duplicate files, applying a batch organizer plan,
//! exporting a playlist or the listening history and copying a measurement WAV
//! all touch the disk. `AGENTS.md` §2.4 puts file I/O on a worker: a network
//! share, a spun-down drive or a large batch can hold a frame for seconds, and
//! the player must keep drawing while it happens.
//!
//! The worker is a single long-lived thread with one request channel and one
//! result channel. Tasks run one at a time, in the order the user issued them,
//! which is what keeps two file operations from racing over the same rows. The
//! UI polls [`FileTaskService::poll`] once per frame; the worker also calls
//! `request_repaint` when a result lands, so an idle window still shows the
//! toast for a finished job.
//!
//! Database work that belongs to the same action (row updates after a move,
//! rows for deleted files) happens on this thread too. A repository call from
//! the frame is supposed to be a bounded single-row lookup, and a batch is not.

use std::path::PathBuf;

use crossbeam::channel::{unbounded, Receiver, Sender};
use database::{Database, ScrobbleRepo, TrackRepo};
use domain::ids::TrackId;
use library::organize::{OrganizeOutcome, OrganizePlan};

use crate::modals::listening_modal::ExportFormat;

/// One unit of work for the file worker.
pub enum FileTask {
    /// Write editorial tags into one file.
    WriteTags {
        path: PathBuf,
        tags: audio_session::engine::decode::EditorialTags,
        /// Shown as a success toast when the write lands, built on the UI
        /// thread while the file name is still at hand.
        success_message: Option<String>,
    },
    /// Write editorial tags into several files and report one summary.
    WriteTagsBatch {
        items: Vec<(PathBuf, audio_session::engine::decode::EditorialTags)>,
        success_message: String,
    },
    /// Delete library rows, optionally unlinking the files first.
    DeleteTracks {
        ids: Vec<TrackId>,
        delete_from_disk: bool,
    },
    /// Move the files of an already-built organizer plan and relink their rows.
    ApplyOrganize(OrganizePlan),
    /// Copy one file (a measurement capture, for instance).
    CopyFile {
        source: PathBuf,
        destination: PathBuf,
        success_message: String,
    },
    /// Write a small text file (a playlist export).
    WriteText {
        path: PathBuf,
        contents: String,
        success_message: String,
    },
    /// Ask for a destination and write the listening history there.
    ExportHistory(ExportFormat),
}

/// What the worker finished.
pub enum FileTaskEvent {
    TagsWritten {
        path: PathBuf,
        success_message: Option<String>,
        error: Option<String>,
    },
    TagsBatchWritten {
        written: usize,
        failures: Vec<String>,
        success_message: String,
    },
    TracksDeleted {
        removed: usize,
        delete_from_disk: bool,
        failures: Vec<String>,
    },
    OrganizeApplied {
        outcome: OrganizeOutcome,
        relinked: usize,
        relink_failures: Vec<String>,
    },
    FileCopied {
        destination: PathBuf,
        success_message: String,
        error: Option<String>,
    },
    TextWritten {
        path: PathBuf,
        success_message: String,
        error: Option<String>,
    },
    /// `Ok(None)` means the user dismissed the save dialog.
    HistoryExported(Result<Option<PathBuf>, String>),
}

/// Owns the worker thread and both ends of its channels.
pub struct FileTaskService {
    tx: Option<Sender<FileTask>>,
    rx: Receiver<FileTaskEvent>,
    db: Database,
    wake: eframe::egui::Context,
    in_flight: usize,
}

impl FileTaskService {
    pub fn new(ctx: &eframe::egui::Context, db: Database) -> Self {
        let (_, rx) = unbounded();
        Self {
            tx: None,
            rx,
            db,
            wake: ctx.clone(),
            in_flight: 0,
        }
    }

    /// True while at least one task is queued or running.
    pub fn is_busy(&self) -> bool {
        self.in_flight > 0
    }

    /// Queue `task`, starting the worker if it is not running.
    ///
    /// Returns `false` when the worker could not be started; the caller keeps
    /// its state and shows the failure rather than pretending the work began.
    pub fn submit(&mut self, task: FileTask) -> bool {
        if !self.ensure_worker() {
            return false;
        }
        let Some(tx) = self.tx.as_ref() else {
            return false;
        };
        match tx.send(task) {
            Ok(()) => {
                self.in_flight += 1;
                true
            }
            Err(e) => {
                log::warn!("Could not hand a file task to the worker: {e}");
                self.tx = None;
                false
            }
        }
    }

    /// Take every finished task. Never blocks.
    pub fn poll(&mut self) -> Vec<FileTaskEvent> {
        let mut events = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(event) => {
                    self.in_flight = self.in_flight.saturating_sub(1);
                    events.push(event);
                }
                Err(crossbeam::channel::TryRecvError::Empty) => break,
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    // A panic in a task takes the worker with it. The queue is
                    // then unwinnable: without this reset `is_busy` would stay
                    // true for the rest of the session and every later file
                    // action would be refused. Drop the handle so the next
                    // submit starts a fresh worker.
                    if self.in_flight > 0 {
                        log::warn!(
                            "The file worker stopped with {} task(s) unfinished",
                            self.in_flight
                        );
                    }
                    self.in_flight = 0;
                    self.tx = None;
                    break;
                }
            }
        }
        events
    }

    /// Start the worker unless one is already connected.
    ///
    /// A panicking task takes the thread with it; the channel then disconnects,
    /// and the next submit starts a fresh worker instead of leaving the app
    /// unable to write tags for the rest of the session.
    fn ensure_worker(&mut self) -> bool {
        if self.tx.is_some() {
            return true;
        }
        let (task_tx, task_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let ctx = self.wake.clone();
        let track_repo = TrackRepo::new(self.db.clone());
        let scrobble_repo = ScrobbleRepo::new(self.db.clone());
        let spawned = std::thread::Builder::new()
            .name("shadow-file-tasks".into())
            .spawn(move || run(task_rx, event_tx, ctx, track_repo, scrobble_repo));
        match spawned {
            Ok(_) => {
                self.rx = event_rx;
                self.tx = Some(task_tx);
                true
            }
            Err(e) => {
                log::warn!("Could not start the file worker: {e}");
                false
            }
        }
    }
}

fn run(
    rx: Receiver<FileTask>,
    tx: Sender<FileTaskEvent>,
    ctx: eframe::egui::Context,
    track_repo: TrackRepo,
    scrobble_repo: ScrobbleRepo,
) {
    // `recv` returns Err once every sender is gone, which is how the thread
    // learns the app is shutting down.
    while let Ok(task) = rx.recv() {
        let event = execute(task, &track_repo, &scrobble_repo);
        if tx.send(event).is_err() {
            break;
        }
        // An idle window has no frame pending; the result must draw its toast.
        ctx.request_repaint();
    }
}

fn execute(task: FileTask, track_repo: &TrackRepo, scrobble_repo: &ScrobbleRepo) -> FileTaskEvent {
    match task {
        FileTask::WriteTags {
            path,
            tags,
            success_message,
        } => {
            let error = audio_session::engine::decode::write_editorial_tags(&path, &tags)
                .err()
                .map(|e| e.to_string());
            FileTaskEvent::TagsWritten {
                path,
                success_message,
                error,
            }
        }
        FileTask::WriteTagsBatch {
            items,
            success_message,
        } => {
            let mut written = 0usize;
            let mut failures = Vec::new();
            for (path, tags) in items {
                match audio_session::engine::decode::write_editorial_tags(&path, &tags) {
                    Ok(()) => written += 1,
                    Err(e) => failures.push(format!("{}: {e}", path.display())),
                }
            }
            FileTaskEvent::TagsBatchWritten {
                written,
                failures,
                success_message,
            }
        }
        FileTask::DeleteTracks {
            ids,
            delete_from_disk,
        } => {
            let mut removed = 0usize;
            let mut failures = Vec::new();
            for id in ids {
                // Deleting the file first means a failed unlink leaves the
                // library row intact rather than orphaning a file on disk.
                if delete_from_disk {
                    match track_repo.get_by_id(&id) {
                        Ok(Some(track)) if track.path.exists() => {
                            if let Err(e) = std::fs::remove_file(&track.path) {
                                failures.push(format!("{}: {e}", track.path.display()));
                                continue;
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            failures.push(format!("could not read the row: {e}"));
                            continue;
                        }
                    }
                }
                match track_repo.delete(&id) {
                    Ok(()) => removed += 1,
                    Err(e) => failures.push(format!("could not delete the row: {e}")),
                }
            }
            FileTaskEvent::TracksDeleted {
                removed,
                delete_from_disk,
                failures,
            }
        }
        FileTask::ApplyOrganize(plan) => {
            let outcome = library::organize::apply_plan(&plan);
            // Relinking is one row write per moved file. On a library-sized
            // batch that is thousands of statements, so it happens here with
            // the moves rather than on the frame that receives them.
            let mut relinked = 0usize;
            let mut relink_failures = Vec::new();
            for (id, new_path, content_hash) in &outcome.moved {
                match track_repo.relink_path(id, new_path, content_hash.as_deref()) {
                    Ok(true) => relinked += 1,
                    Ok(false) => relink_failures.push(format!(
                        "moved {} but found no library row",
                        new_path.display()
                    )),
                    Err(e) => relink_failures
                        .push(format!("could not relink {}: {e}", new_path.display())),
                }
            }
            FileTaskEvent::OrganizeApplied {
                outcome,
                relinked,
                relink_failures,
            }
        }
        FileTask::CopyFile {
            source,
            destination,
            success_message,
        } => {
            let error = std::fs::copy(&source, &destination)
                .err()
                .map(|e| e.to_string());
            FileTaskEvent::FileCopied {
                destination,
                success_message,
                error,
            }
        }
        FileTask::WriteText {
            path,
            contents,
            success_message,
        } => {
            let error = std::fs::write(&path, contents).err().map(|e| e.to_string());
            FileTaskEvent::TextWritten {
                path,
                success_message,
                error,
            }
        }
        FileTask::ExportHistory(format) => FileTaskEvent::HistoryExported(
            crate::modals::listening_modal::export_with_dialog(scrobble_repo, format),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::models::Track;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sp_file_tasks_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    fn track(id_seed: u8, path: &std::path::Path) -> Track {
        Track {
            id: TrackId::from_name_and_seed(
                TrackId::new().as_uuid(),
                &format!("file-task-test-{id_seed}"),
            ),
            path: path.to_path_buf(),
            title: "Task".to_string(),
            artist: "A".to_string(),
            album: "B".to_string(),
            genre: None,
            year: None,
            track_number: Some(u32::from(id_seed)),
            disc_number: None,
            duration_secs: 1.0,
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
            file_size_bytes: 1,
            mtime_unix_secs: 1,
            mtime_nanos: 0,
            content_hash: Some(format!("hash-{id_seed}")),
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 0,
        }
    }

    #[test]
    fn deleting_a_track_removes_the_row_and_the_file() {
        let dir = scratch("delete");
        let path = dir.join("gone.flac");
        std::fs::write(&path, b"audio").expect("fixture file");
        let db = Database::in_memory().expect("in-memory database");
        let repo = TrackRepo::new(db.clone());
        let track = track(1, &path);
        repo.upsert(&track).expect("row");

        let event = execute(
            FileTask::DeleteTracks {
                ids: vec![track.id],
                delete_from_disk: true,
            },
            &repo,
            &ScrobbleRepo::new(db.clone()),
        );

        match event {
            FileTaskEvent::TracksDeleted {
                removed,
                failures,
                delete_from_disk,
            } => {
                assert_eq!(removed, 1);
                assert!(failures.is_empty(), "failures: {failures:?}");
                assert!(delete_from_disk);
            }
            _ => panic!("wrong event"),
        }
        assert!(!path.exists(), "the file must be unlinked");
        assert!(repo.get_by_id(&track.id).expect("query").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_unlink_keeps_the_row() {
        let dir = scratch("keep_row");
        // A directory cannot be removed with `remove_file`, so the unlink
        // fails the way a permissions error would.
        let path = dir.join("not-a-file");
        std::fs::create_dir_all(&path).expect("directory");
        let db = Database::in_memory().expect("in-memory database");
        let repo = TrackRepo::new(db.clone());
        let track = track(2, &path);
        repo.upsert(&track).expect("row");
        let id = track.id;

        let event = execute(
            FileTask::DeleteTracks {
                ids: vec![id],
                delete_from_disk: true,
            },
            &repo,
            &ScrobbleRepo::new(db.clone()),
        );

        match event {
            FileTaskEvent::TracksDeleted {
                removed, failures, ..
            } => {
                assert_eq!(removed, 0);
                assert_eq!(failures.len(), 1);
            }
            _ => panic!("wrong event"),
        }
        assert!(repo.get_by_id(&id).expect("query").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn applying_a_plan_moves_the_file_and_relinks_the_row() {
        let dir = scratch("organize");
        let source = dir.join("incoming.flac");
        std::fs::write(&source, b"audio").expect("fixture file");
        let db = Database::in_memory().expect("in-memory database");
        let repo = TrackRepo::new(db.clone());
        let moved = track(3, &source);
        repo.upsert(&moved).expect("row");
        let id = moved.id;
        let plan = library::organize::build_plan(&[moved], "{title}", None);
        assert_eq!(plan.pending_count(), 1);

        let event = execute(
            FileTask::ApplyOrganize(plan),
            &repo,
            &ScrobbleRepo::new(db.clone()),
        );

        let new_path = dir.join("Task.flac");
        match event {
            FileTaskEvent::OrganizeApplied {
                outcome,
                relinked,
                relink_failures,
            } => {
                assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);
                assert_eq!(relinked, 1);
                assert!(relink_failures.is_empty());
            }
            _ => panic!("wrong event"),
        }
        assert!(new_path.exists());
        assert!(!source.exists());
        let row = repo.get_by_id(&id).expect("row").expect("still there");
        assert_eq!(row.path, new_path);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_text_write_reports_the_destination() {
        let dir = scratch("text");
        let path = dir.join("playlist.m3u8");
        let event = execute(
            FileTask::WriteText {
                path: path.clone(),
                contents: "#EXTM3U\n".to_string(),
                success_message: "Exported".to_string(),
            },
            &TrackRepo::new(Database::in_memory().expect("db")),
            &ScrobbleRepo::new(Database::in_memory().expect("db")),
        );
        match event {
            FileTaskEvent::TextWritten {
                success_message,
                error,
                ..
            } => {
                assert!(error.is_none(), "{error:?}");
                assert_eq!(success_message, "Exported");
            }
            _ => panic!("wrong event"),
        }
        assert_eq!(
            std::fs::read_to_string(&path).expect("written"),
            "#EXTM3U\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

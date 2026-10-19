//! Filesystem watching for library roots.
//!
//! One watcher per root; the handle owns the OS watch, so dropping it stops
//! watching. Events are coalesced before they reach the caller: a single album
//! copy used to fire the callback for every one of the hundreds of raw events
//! inotify reports, and the library was rescan-ready on each one. The watcher
//! now waits for the filesystem to go quiet and reports the distinct paths once.
//!
//! Backend errors are logged rather than dropped: when a watched root becomes
//! unreadable (an unmounted drive, a deleted directory) `notify` hands back an
//! error, and the only trace used to be that the library quietly stopped
//! updating.

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long the filesystem must be quiet before changes are reported.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Upper bound on that wait, so a continuously-written directory (a long copy)
/// still reports progress instead of staying silent until it finishes.
const MAX_DEBOUNCE: Duration = Duration::from_secs(2);

/// Add newly reported paths to the pending set, ignoring duplicates.
///
/// One logical change arrives as several events (a write per buffer, a rename
/// pair, a metadata write), so the same path is reported many times within one
/// burst.
fn merge_paths(pending: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, paths: Vec<PathBuf>) {
    for path in paths {
        if seen.insert(path.clone()) {
            pending.push(path);
        }
    }
}

pub struct LibraryWatcher {
    _watcher: RecommendedWatcher,
}

impl LibraryWatcher {
    /// Watch a directory path and invoke callback on changes.
    ///
    /// The callback runs on the watcher's own thread and receives the distinct
    /// paths touched by one burst of activity.
    pub fn watch<F>(path: impl AsRef<Path>, on_change: F) -> Result<Self, notify::Error>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    let _ = tx.send(event);
                }
                Err(e) => {
                    // An OS-level failure on a watched root (unmounted drive,
                    // permissions revoked). Worth a warning: it means the
                    // library has silently stopped tracking changes.
                    log::warn!("Filesystem watcher error: {e}");
                }
            },
            // The poll interval only applies to the polling backend; the
            // inotify/FSEvents/ReadDirectoryChangesW backends are event-driven
            // and ignore it. It is kept as a sane fallback interval for systems
            // where `notify` picks the polling watcher.
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

        std::thread::Builder::new()
            .name("shadow-library-watcher".into())
            .spawn(move || {
                let mut pending: Vec<PathBuf> = Vec::new();
                let mut seen: HashSet<PathBuf> = HashSet::new();
                let mut flush_at: Option<Instant> = None;

                loop {
                    // Idle: block until something happens (and exit when the
                    // watcher handle is dropped and the channel closes).
                    // Busy: wait for quiet, or until the maximum window closes.
                    let received = match flush_at {
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        Some(deadline) => {
                            let wait = deadline.saturating_duration_since(Instant::now());
                            rx.recv_timeout(wait.min(DEBOUNCE))
                        }
                    };

                    match received {
                        Ok(event) => {
                            if is_relevant(&event) && !event.paths.is_empty() {
                                merge_paths(&mut pending, &mut seen, event.paths);
                                flush_at.get_or_insert_with(|| Instant::now() + MAX_DEBOUNCE);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if !pending.is_empty() {
                                // Distinct paths, in a stable order, so the
                                // caller can rescan deterministically.
                                pending.sort();
                                on_change(std::mem::take(&mut pending));
                                seen.clear();
                                flush_at = None;
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;

        log::info!("Filesystem watcher active for: {:?}", path.as_ref());
        Ok(Self { _watcher: watcher })
    }
}

/// Whether an event describes a change to the library's contents.
fn is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_paths_from_one_burst_are_reported_once() {
        let mut pending = Vec::new();
        let mut seen = HashSet::new();

        merge_paths(
            &mut pending,
            &mut seen,
            vec![PathBuf::from("/music/a.flac"), PathBuf::from("/music/b.flac")],
        );
        merge_paths(
            &mut pending,
            &mut seen,
            vec![PathBuf::from("/music/a.flac"), PathBuf::from("/music/c.flac")],
        );

        assert_eq!(pending.len(), 3);
        assert!(pending.contains(&PathBuf::from("/music/a.flac")));
    }

    #[test]
    fn only_content_events_are_relevant() {
        let event = |kind| Event {
            kind,
            paths: vec![PathBuf::from("/music/a.flac")],
            attrs: Default::default(),
        };
        use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind};
        assert!(is_relevant(&event(EventKind::Create(CreateKind::File))));
        assert!(is_relevant(&event(EventKind::Modify(ModifyKind::Any))));
        assert!(is_relevant(&event(EventKind::Remove(RemoveKind::File))));
        // Opening a file to play it must not queue a rescan.
        assert!(!is_relevant(&event(EventKind::Access(AccessKind::Open(
            AccessMode::Any
        )))));
    }

    /// End to end: a burst of writes reaches the callback once, not once per
    /// raw event. Skips itself on systems where directories cannot be watched.
    #[test]
    fn a_burst_of_writes_is_reported_as_one_batch() {
        let dir = std::env::temp_dir().join(format!("sp_watch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let Ok(_watcher) = LibraryWatcher::watch(&dir, move |paths| {
            let _ = tx.send(paths);
        }) else {
            eprintln!("skipping: this system cannot watch directories");
            return;
        };

        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.flac")), b"x").unwrap();
        }

        let first = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the burst should have been reported");
        assert!(!first.is_empty(), "a report must carry the changed paths");
        assert!(
            first.windows(2).all(|w| w[0] < w[1]),
            "paths are sorted and distinct: {first:?}"
        );

        // Collect anything else the same burst produced, for a little longer
        // than the debounce window. Five writes must not arrive as five
        // separate rescans (nor as ten: create plus modify per file).
        let mut batches = 1;
        while let Ok(_more) = rx.recv_timeout(DEBOUNCE * 3) {
            batches += 1;
        }
        assert!(batches <= 2, "expected one coalesced batch, got {batches}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}

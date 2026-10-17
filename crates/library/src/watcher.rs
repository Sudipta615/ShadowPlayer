use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use log::info;

pub struct LibraryWatcher {
    _watcher: RecommendedWatcher,
}

impl LibraryWatcher {
    /// Watch a directory path and invoke callback on changes.
    pub fn watch<F>(path: impl AsRef<Path>, on_change: F) -> Result<Self, notify::Error>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        )?;

        watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

        std::thread::Builder::new()
            .name("shadow-library-watcher".into())
            .spawn(move || {
                while let Ok(event) = rx.recv() {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            let paths = event.paths;
                            if !paths.is_empty() {
                                on_change(paths);
                            }
                        }
                        _ => {}
                    }
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;

        info!("Filesystem watcher active for: {:?}", path.as_ref());
        Ok(Self { _watcher: watcher })
    }
}

use std::path::PathBuf;

use crossbeam::channel::{unbounded, Receiver};

/// Audio file extensions offered to the native picker filter. Kept in sync with
/// `library::scanner::SUPPORTED_EXTENSIONS`.
const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "ogg", "wav", "m4a", "aac", "opus", "alac", "aiff", "aif", "ape", "wv", "tta",
    "dsf", "dff", "cue",
];

/// What the user asked the picker to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// Pick one or more individual audio files.
    Files,
    /// Pick a directory to scan and monitor.
    Folder,
}

/// Outcome of a native picker interaction, delivered off the UI thread.
#[derive(Debug, Clone)]
pub enum ImportResult {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
    /// The picker was dismissed with no selection.
    Cancelled,
}

/// Runs the blocking native file dialog on a worker thread so the egui frame
/// loop is never blocked while the OS picker is open. The UI polls
/// [`ImportService::poll`] each frame and reacts when a result arrives.
#[derive(Default)]
pub struct ImportService {
    rx: Option<Receiver<ImportResult>>,
    active: Option<ImportKind>,
}

impl ImportService {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while a picker is open (used to disable the import buttons).
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn active_kind(&self) -> Option<ImportKind> {
        self.active
    }

    /// Open the native "choose audio files" dialog. No-op if one is already open.
    pub fn request_files(&mut self) {
        self.request(ImportKind::Files);
    }

    /// Open the native "choose a folder" dialog. No-op if one is already open.
    pub fn request_folder(&mut self) {
        self.request(ImportKind::Folder);
    }

    fn request(&mut self, kind: ImportKind) {
        if self.active.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        let spawned = std::thread::Builder::new()
            .name("shadow-import-dialog".into())
            .spawn(move || {
                let result = run_picker(kind);
                let _ = tx.send(result);
            });

        match spawned {
            Ok(_) => {
                self.rx = Some(rx);
                self.active = Some(kind);
            }
            Err(e) => log::warn!("Could not open import dialog: {}", e),
        }
    }

    /// Returns the picker result once, then resets to idle.
    pub fn poll(&mut self) -> Option<ImportResult> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(result) => {
                self.rx = None;
                self.active = None;
                Some(result)
            }
            Err(crossbeam::channel::TryRecvError::Empty) => None,
            Err(crossbeam::channel::TryRecvError::Disconnected) => {
                self.rx = None;
                self.active = None;
                Some(ImportResult::Cancelled)
            }
        }
    }
}

fn run_picker(kind: ImportKind) -> ImportResult {
    let mut dialog = rfd::FileDialog::new();
    if let Some(dir) = dirs::audio_dir() {
        if dir.exists() {
            dialog = dialog.set_directory(dir);
        }
    }

    match kind {
        ImportKind::Folder => match dialog.set_title("Import Music Folder").pick_folder() {
            Some(path) => ImportResult::Folder(path),
            None => ImportResult::Cancelled,
        },
        ImportKind::Files => {
            let dialog = dialog
                .set_title("Import Songs")
                .add_filter("Audio Files", AUDIO_EXTENSIONS)
                .add_filter("All Files", &["*"]);
            match dialog.pick_files() {
                Some(paths) if !paths.is_empty() => ImportResult::Files(paths),
                _ => ImportResult::Cancelled,
            }
        }
    }
}

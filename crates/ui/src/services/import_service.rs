use std::path::PathBuf;

use crossbeam::channel::{unbounded, Receiver};

/// Audio file extensions offered to the native picker filter. Kept in sync with
/// `library::scanner::SUPPORTED_EXTENSIONS`.
const AUDIO_EXTENSIONS: &[&str] = &[
    "flac", "mp3", "ogg", "wav", "m4a", "aac", "opus", "alac", "aiff", "aif", "ape", "wv", "tta",
    "dsf", "dff", "cue",
];

/// Extensions recognised as playlists.
const PLAYLIST_EXTENSIONS: &[&str] = &["m3u", "m3u8"];

/// What the user asked the picker to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// Pick one or more individual audio files.
    Files,
    /// Pick a directory to scan and monitor.
    Folder,
    /// Pick an `.m3u` / `.m3u8` playlist to open into the library.
    Playlist,
    /// Pick a destination file for an M3U export.
    SaveM3u,
    /// Pick the root folder for a batch organize.
    OrganizeDestination,
}

/// Outcome of a native picker interaction, delivered off the UI thread.
#[derive(Debug, Clone)]
pub enum ImportResult {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
    /// An M3U file to read and turn into a playlist.
    Playlist(PathBuf),
    /// A destination chosen for an M3U export.
    SavePath(PathBuf),
    /// A root folder chosen for a batch organize.
    OrganizeDestination(PathBuf),
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
    /// Stem suggested to the "save as" dialog (playlist name).
    suggested_name: String,
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

    /// Open the native "choose an M3U playlist" dialog.
    pub fn request_playlist(&mut self) {
        self.request(ImportKind::Playlist);
    }

    /// Open the native "save playlist as" dialog, suggesting `suggested_name`.
    pub fn request_save_m3u(&mut self, suggested_name: String) {
        if self.active.is_some() {
            return;
        }
        self.suggested_name = suggested_name;
        self.request(ImportKind::SaveM3u);
    }

    /// Open the native "choose a destination folder" dialog for the organizer.
    pub fn request_organize_destination(&mut self) {
        self.request(ImportKind::OrganizeDestination);
    }

    fn request(&mut self, kind: ImportKind) {
        if self.active.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        let suggested = std::mem::take(&mut self.suggested_name);
        let spawned = std::thread::Builder::new()
            .name("shadow-import-dialog".into())
            .spawn(move || {
                let result = run_picker(kind, suggested);
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

fn run_picker(kind: ImportKind, suggested_name: String) -> ImportResult {
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
        ImportKind::Playlist => {
            let dialog = dialog
                .set_title("Open M3U Playlist")
                .add_filter("Playlists", PLAYLIST_EXTENSIONS);
            match dialog.pick_file() {
                Some(path) => ImportResult::Playlist(path),
                None => ImportResult::Cancelled,
            }
        }
        ImportKind::SaveM3u => {
            let name = if suggested_name.trim().is_empty() {
                "playlist.m3u8".to_string()
            } else {
                format!("{}.m3u8", suggested_name.trim())
            };
            let dialog = dialog
                .set_title("Export Playlist")
                .set_file_name(name)
                .add_filter("Playlists", PLAYLIST_EXTENSIONS);
            match dialog.save_file() {
                Some(path) => ImportResult::SavePath(path),
                None => ImportResult::Cancelled,
            }
        }
        ImportKind::OrganizeDestination => {
            match dialog.set_title("Organize Into Folder").pick_folder() {
                Some(path) => ImportResult::OrganizeDestination(path),
                None => ImportResult::Cancelled,
            }
        }
    }
}

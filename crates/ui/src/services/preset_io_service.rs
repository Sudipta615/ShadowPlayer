//! Import and export of text EQ curves.
//!
//! The dialogs and the file I/O run on a worker thread, because both can
//! block: the native picker waits on the user, and a network share can take
//! seconds to open. The UI only polls [`PresetIoService::poll`] once per frame
//! and never touches the filesystem itself.
//!
//! Parsing lives in `config::parse_curve`, so the same grammar is used by the
//! tests, by the AutoEQ database importer and by this service.

use std::path::PathBuf;

use config::{parse_curve, ImportedCurve};
use crossbeam::channel::{unbounded, Receiver};

/// Refuse files larger than this. A curve is a few kilobytes; anything larger
/// is a mis-selected audio file or an archive, and reading it on a worker just
/// to reject it wastes memory.
const MAX_CURVE_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// What the worker produced.
#[derive(Debug, Clone)]
pub enum PresetIoEvent {
    /// A file was read and parsed.
    Imported { path: PathBuf, curve: ImportedCurve },
    /// A curve was written to `path`.
    Exported(PathBuf),
    /// The picker was dismissed.
    Cancelled,
    /// The file could not be read, parsed or written.
    Error(String),
}

/// Owns the picker thread and the channel back to the UI.
#[derive(Default)]
pub struct PresetIoService {
    rx: Option<Receiver<PresetIoEvent>>,
    active: bool,
}

impl PresetIoService {
    pub fn new() -> Self {
        Self::default()
    }

    /// True while a dialog or a file operation is in flight.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Ask for a text curve file and parse it on a worker.
    pub fn request_import(&mut self) {
        if self.active {
            return;
        }
        let (tx, rx) = unbounded();
        let spawned = std::thread::Builder::new()
            .name("shadow-eq-import".into())
            .spawn(move || {
                let _ = tx.send(import_curve());
            });
        match spawned {
            Ok(_) => {
                self.rx = Some(rx);
                self.active = true;
            }
            Err(e) => log::warn!("Could not start the EQ import worker: {e}"),
        }
    }

    /// Ask for a destination and write `contents` there on a worker.
    pub fn request_export(&mut self, suggested_name: String, contents: String) {
        if self.active {
            return;
        }
        let (tx, rx) = unbounded();
        let spawned = std::thread::Builder::new()
            .name("shadow-eq-export".into())
            .spawn(move || {
                let _ = tx.send(export_curve(suggested_name, contents));
            });
        match spawned {
            Ok(_) => {
                self.rx = Some(rx);
                self.active = true;
            }
            Err(e) => log::warn!("Could not start the EQ export worker: {e}"),
        }
    }

    /// Return the worker's result once, then go idle.
    pub fn poll(&mut self) -> Option<PresetIoEvent> {
        let rx = self.rx.as_ref()?;
        match rx.try_recv() {
            Ok(event) => {
                self.rx = None;
                self.active = false;
                Some(event)
            }
            Err(crossbeam::channel::TryRecvError::Empty) => None,
            Err(crossbeam::channel::TryRecvError::Disconnected) => {
                self.rx = None;
                self.active = false;
                Some(PresetIoEvent::Error(
                    "the EQ file worker stopped unexpectedly".into(),
                ))
            }
        }
    }
}

fn import_curve() -> PresetIoEvent {
    let picked = rfd::FileDialog::new()
        .set_title("Import EQ Curve")
        .add_filter("EQ curve", &["txt", "peq", "csv", "apo"])
        .add_filter("All files", &["*"])
        .pick_file();

    let Some(path) = picked else {
        return PresetIoEvent::Cancelled;
    };

    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > MAX_CURVE_FILE_BYTES => {
            return PresetIoEvent::Error(format!(
                "{} is {:.1} MB; an EQ curve should be a few kilobytes",
                path.display(),
                meta.len() as f64 / (1024.0 * 1024.0)
            ));
        }
        Ok(_) => {}
        Err(e) => return PresetIoEvent::Error(format!("Could not inspect the file: {e}")),
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return PresetIoEvent::Error(format!("Could not read the file: {e}")),
    };

    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "Imported curve".to_string());

    match parse_curve(&name, &text) {
        Ok(curve) => PresetIoEvent::Imported { path, curve },
        Err(e) => PresetIoEvent::Error(format!("{}: {e}", path.display())),
    }
}

fn export_curve(suggested_name: String, contents: String) -> PresetIoEvent {
    let file_name = sanitize_file_name(&suggested_name);
    let picked = rfd::FileDialog::new()
        .set_title("Export EQ Curve")
        .set_file_name(&file_name)
        .add_filter("EQ curve text", &["txt"])
        .save_file();

    let Some(mut path) = picked else {
        return PresetIoEvent::Cancelled;
    };
    if path.extension().is_none() {
        path.set_extension("txt");
    }

    match std::fs::write(&path, contents) {
        Ok(()) => PresetIoEvent::Exported(path),
        Err(e) => PresetIoEvent::Error(format!("Could not write {}: {e}", path.display())),
    }
}

/// Turn a preset name into a usable default filename.
fn sanitize_file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let stem = stem.trim().trim_end_matches('.');
    if stem.is_empty() {
        "eq-curve.txt".to_string()
    } else if stem.to_ascii_lowercase().ends_with(".txt") {
        stem.to_string()
    } else {
        format!("{stem}.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_lose_path_separators_and_gain_an_extension() {
        assert_eq!(sanitize_file_name("Room / Desk"), "Room _ Desk.txt");
        assert_eq!(sanitize_file_name("already.txt"), "already.txt");
        assert_eq!(sanitize_file_name("   "), "eq-curve.txt");
        assert_eq!(sanitize_file_name("bad*.txt"), "bad_.txt");
    }
}

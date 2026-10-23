//! Background jobs for the Tools page: CD probing and ripping, batch
//! transcoding, and the opt-in update check/download.
//!
//! Every job runs on its own thread and reports through a channel. The UI
//! never blocks on a disc, an encoder, or the network, and a job that is
//! cancelled stops before writing the next file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::channel::{unbounded, Receiver};
use media_tools::cd::{CdDisc, CdRipper, RipFormat, RipOptions, RipProgress, RipReport};
use media_tools::transcode::{transcode, OutputFormat, TranscodeOptions, TranscodeReport};
use media_tools::update::{ReleaseAsset, ReleaseInfo, UpdateCheck, UpdateClient, VerifiedDownload};

/// One message from a media job.
#[derive(Debug)]
pub enum MediaToolEvent {
    DiscProbed(Result<CdDisc, String>),
    RipProgress(RipProgress),
    RipFinished(Result<RipReport, String>),
    TranscodeProgress {
        done: usize,
        total: usize,
        current: String,
    },
    TranscodeFinished(Result<Vec<TranscodeReport>, String>),
    UpdateChecked(Result<UpdateCheck, String>),
    UpdateDownloaded(Result<VerifiedDownload, String>),
}

/// Owns the current worker and its cancel flag.
pub struct MediaToolsService {
    rx: Option<Receiver<MediaToolEvent>>,
    busy: bool,
    cancel: Arc<AtomicBool>,
    /// What the running job is, for the modal's status line.
    activity: String,
}

impl Default for MediaToolsService {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaToolsService {
    pub fn new() -> Self {
        Self {
            rx: None,
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            activity: String::new(),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn activity(&self) -> &str {
        &self.activity
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain everything the worker has said since the last frame.
    pub fn poll(&mut self) -> Vec<MediaToolEvent> {
        let Some(rx) = &self.rx else {
            return Vec::new();
        };
        let mut events = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(event) => {
                    if matches!(
                        event,
                        MediaToolEvent::DiscProbed(_)
                            | MediaToolEvent::RipFinished(_)
                            | MediaToolEvent::TranscodeFinished(_)
                            | MediaToolEvent::UpdateChecked(_)
                            | MediaToolEvent::UpdateDownloaded(_)
                    ) {
                        self.busy = false;
                        self.activity.clear();
                    }
                    events.push(event);
                }
                Err(crossbeam::channel::TryRecvError::Empty) => break,
                Err(crossbeam::channel::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.busy = false;
                    self.activity.clear();
                    break;
                }
            }
        }
        events
    }

    fn start(&mut self, activity: impl Into<String>) -> bool {
        if self.busy {
            return false;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.busy = true;
        self.activity = activity.into();
        true
    }

    /// Probe a disc for its table of contents and (optionally) CDDB metadata.
    pub fn probe_disc(&mut self, device: String, fetch_metadata: bool) -> bool {
        if !self.start("Reading disc…") {
            return false;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        let cancel = Arc::clone(&self.cancel);
        std::thread::Builder::new()
            .name("cd-probe".to_string())
            .spawn(move || {
                let ripper = if device.trim().is_empty() {
                    CdRipper::default_device()
                } else {
                    Ok(CdRipper::new(device))
                };
                let result = if cancel.load(Ordering::Relaxed) {
                    Err("cancelled".to_string())
                } else {
                    match ripper {
                        Ok(ripper) => ripper
                            .probe(fetch_metadata)
                            .map_err(|error| error.to_string()),
                        Err(error) => Err(error.to_string()),
                    }
                };
                let _ = tx.send(MediaToolEvent::DiscProbed(result));
            })
            .ok();
        true
    }

    /// Rip the selected track numbers. An empty selection means every track.
    pub fn rip_disc(
        &mut self,
        device: String,
        output_dir: PathBuf,
        format: RipFormat,
        track_numbers: Vec<u32>,
        fetch_metadata: bool,
    ) -> bool {
        if !self.start("Ripping CD…") {
            return false;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        let cancel = Arc::clone(&self.cancel);
        std::thread::Builder::new()
            .name("cd-rip".to_string())
            .spawn(move || {
                let ripper = if device.trim().is_empty() {
                    CdRipper::default_device()
                } else {
                    Ok(CdRipper::new(device))
                };
                let ripper = match ripper {
                    Ok(ripper) => ripper,
                    Err(error) => {
                        let _ = tx.send(MediaToolEvent::RipFinished(Err(error.to_string())));
                        return;
                    }
                };
                let options = RipOptions {
                    output_dir,
                    format,
                    fetch_metadata,
                    overwrite: false,
                };
                let numbers = if track_numbers.is_empty() {
                    match ripper.probe(false) {
                        Ok(disc) => disc.tracks.iter().map(|t| t.number).collect(),
                        Err(error) => {
                            let _ = tx.send(MediaToolEvent::RipFinished(Err(error.to_string())));
                            return;
                        }
                    }
                } else {
                    track_numbers
                };

                let mut all_files = Vec::new();
                let mut all_warnings = Vec::new();
                let mut disc = None;
                for number in numbers {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(MediaToolEvent::RipFinished(Err("cancelled".to_string())));
                        return;
                    }
                    let progress_tx = tx.clone();
                    let progress_cancel = Arc::clone(&cancel);
                    let result = ripper.rip_track(number, &options, move |progress| {
                        let _ = progress_tx.send(MediaToolEvent::RipProgress(progress));
                        !progress_cancel.load(Ordering::Relaxed)
                    });
                    match result {
                        Ok(report) => {
                            disc.get_or_insert(report.disc);
                            all_files.extend(report.files);
                            all_warnings.extend(report.warnings);
                        }
                        Err(error) => {
                            let _ = tx.send(MediaToolEvent::RipFinished(Err(error.to_string())));
                            return;
                        }
                    }
                }
                let report = RipReport {
                    disc: disc.unwrap_or(CdDisc {
                        device: String::new(),
                        tracks: Vec::new(),
                        metadata: Default::default(),
                        warnings: Vec::new(),
                    }),
                    files: all_files,
                    warnings: all_warnings,
                };
                let _ = tx.send(MediaToolEvent::RipFinished(Ok(report)));
            })
            .ok();
        true
    }

    /// Transcode every input into `output_dir`, keeping the file stem.
    pub fn transcode_files(
        &mut self,
        inputs: Vec<PathBuf>,
        output_dir: PathBuf,
        format: OutputFormat,
        copy_tags: bool,
        overwrite: bool,
    ) -> bool {
        if inputs.is_empty() {
            return false;
        }
        if !self.start("Exporting…") {
            return false;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        let cancel = Arc::clone(&self.cancel);
        std::thread::Builder::new()
            .name("transcode-export".to_string())
            .spawn(move || {
                if let Err(error) = std::fs::create_dir_all(&output_dir) {
                    let _ = tx.send(MediaToolEvent::TranscodeFinished(Err(error.to_string())));
                    return;
                }
                let options = TranscodeOptions { format, copy_tags };
                let total = inputs.len();
                let mut reports = Vec::new();
                for (done, input) in inputs.into_iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(MediaToolEvent::TranscodeFinished(Err(
                            "cancelled".to_string()
                        )));
                        return;
                    }
                    let stem = input
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "track".to_string());
                    let mut output = output_dir.join(format!("{stem}.{}", format.extension()));
                    if output == input || (output.exists() && !overwrite) {
                        let mut n = 1u32;
                        loop {
                            let candidate =
                                output_dir.join(format!("{stem} ({n}).{}", format.extension()));
                            if candidate != input && !candidate.exists() {
                                output = candidate;
                                break;
                            }
                            n += 1;
                        }
                    }
                    let _ = tx.send(MediaToolEvent::TranscodeProgress {
                        done,
                        total,
                        current: input
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    });
                    match transcode(&input, &output, &options) {
                        Ok(report) => reports.push(report),
                        Err(error) => {
                            let _ = tx.send(MediaToolEvent::TranscodeFinished(Err(format!(
                                "{}: {error}",
                                input.display()
                            ))));
                            return;
                        }
                    }
                }
                let _ = tx.send(MediaToolEvent::TranscodeFinished(Ok(reports)));
            })
            .ok();
        true
    }

    /// Check GitHub releases. `enabled` is the user's explicit consent.
    pub fn check_for_update(&mut self, enabled: bool, current_version: String) -> bool {
        if !self.start("Checking for updates…") {
            return false;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        std::thread::Builder::new()
            .name("update-check".to_string())
            .spawn(move || {
                let client = UpdateClient::new(enabled);
                let result = client
                    .check(&current_version)
                    .map_err(|error| error.to_string());
                let _ = tx.send(MediaToolEvent::UpdateChecked(result));
            })
            .ok();
        true
    }

    /// Download and verify a release asset into `directory`.
    pub fn download_update(
        &mut self,
        enabled: bool,
        release: ReleaseInfo,
        asset: ReleaseAsset,
        directory: PathBuf,
        overwrite: bool,
    ) -> bool {
        if !self.start("Downloading update…") {
            return false;
        }
        let (tx, rx) = unbounded();
        self.rx = Some(rx);
        let cancel = Arc::clone(&self.cancel);
        std::thread::Builder::new()
            .name("update-download".to_string())
            .spawn(move || {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(MediaToolEvent::UpdateDownloaded(Err(
                        "cancelled".to_string()
                    )));
                    return;
                }
                let client = UpdateClient::new(enabled);
                let result = client
                    .download_verified(&release, &asset, &directory, overwrite)
                    .map_err(|error| error.to_string());
                let _ = tx.send(MediaToolEvent::UpdateDownloaded(result));
            })
            .ok();
        true
    }
}

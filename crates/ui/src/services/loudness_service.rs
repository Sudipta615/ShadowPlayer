//! Background, library-wide ReplayGain / EBU R128 loudness scan.
//!
//! For every track that has no measured loudness yet we run the engine's
//! offline analyser, persist the result into the engine's on-disk loudness
//! cache (which playback consults) and record it in the library database for
//! display and smart playlists. The user's files are never modified.
//!
//! The scan fully decodes each file, so it is CPU-bound; on modest hardware it
//! deliberately yields between tracks to avoid starving the audio thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam::channel::Receiver;
use database::{Database, TrackRepo};

/// A snapshot of scan progress for the settings UI.
#[derive(Debug, Clone, Default)]
pub struct LoudnessProgress {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    /// File name currently being analysed, or the last status message.
    pub current: String,
    /// `true` once at least one scan has completed (so the UI can show a summary).
    pub has_run: bool,
}

enum LoudnessEvent {
    Progress { done: usize, failed: usize, current: String },
    Finished { done: usize, failed: usize },
    Cancelled { done: usize },
}

/// Owns the loudness scan worker and its progress channel.
pub struct LoudnessService {
    db: Database,
    running: bool,
    cancel: Arc<AtomicBool>,
    rx: Option<Receiver<LoudnessEvent>>,
    progress: LoudnessProgress,
    /// Set when results were written and the caller should reload the library.
    dirty: bool,
}

impl LoudnessService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            progress: LoudnessProgress::default(),
            dirty: false,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn progress(&self) -> &LoudnessProgress {
        &self.progress
    }

    /// Begin a scan of every track that still lacks measured loudness.
    pub fn start(&mut self) {
        if self.running {
            return;
        }

        let items = match TrackRepo::new(self.db.clone()).list_missing_replaygain() {
            Ok(items) => items,
            Err(e) => {
                log::warn!("Could not list tracks for loudness scan: {e}");
                return;
            }
        };

        if items.is_empty() {
            self.progress = LoudnessProgress {
                running: false,
                current: "Nothing to scan — every track already has loudness data.".to_string(),
                has_run: true,
                ..Default::default()
            };
            return;
        }

        let (tx, rx) = crossbeam::channel::unbounded();
        self.rx = Some(rx);
        self.running = true;
        self.cancel.store(false, Ordering::Relaxed);
        self.progress = LoudnessProgress {
            running: true,
            total: items.len(),
            current: "Starting…".to_string(),
            has_run: true,
            ..Default::default()
        };

        let cancel = Arc::clone(&self.cancel);
        let db = self.db.clone();

        let spawned = std::thread::Builder::new()
            .name("loudness-scan".to_string())
            .spawn(move || {
                let repo = TrackRepo::new(db);
                let mut done = 0usize;
                let mut failed = 0usize;

                for (id, path) in items {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(LoudnessEvent::Cancelled { done });
                        return;
                    }

                    let current = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    let _ = tx.send(LoudnessEvent::Progress {
                        done,
                        failed,
                        current,
                    });

                    match engine::decode::scan_track_loudness(&path) {
                        Some(result) => {
                            // Feed the engine's own cache so normalization works
                            // on playback without touching the source file.
                            engine::decode::loudness_cache::store(&path, &result);
                            if let Err(e) = repo.set_loudness(
                                &id,
                                result.replaygain_track_db,
                                result.replaygain_track_peak,
                                result.ebu_r128_loudness,
                            ) {
                                log::warn!("Could not record loudness for {}: {e}", path.display());
                            }
                        }
                        None => failed += 1,
                    }

                    done += 1;

                    // Cooperative yield: a full decode is heavy and this must not
                    // starve audio rendering on a dual-core machine.
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }

                let _ = tx.send(LoudnessEvent::Finished { done, failed });
            });

        if spawned.is_err() {
            log::warn!("Could not start loudness scan thread");
            self.running = false;
            self.rx = None;
        }
    }

    /// Ask the running scan to stop after the current track.
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain progress events. Returns `true` when a run finished or was
    /// cancelled, so the caller can reload the library.
    pub fn poll(&mut self) -> bool {
        let mut finished = false;
        let mut clear = false;

        if let Some(ref rx) = self.rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    LoudnessEvent::Progress {
                        done,
                        failed,
                        current,
                    } => {
                        self.progress.done = done;
                        self.progress.failed = failed;
                        self.progress.current = current;
                    }
                    LoudnessEvent::Finished { done, failed } => {
                        self.progress.done = done;
                        self.progress.failed = failed;
                        self.progress.running = false;
                        self.progress.current =
                            format!("Done: {done} analysed, {failed} skipped");
                        self.running = false;
                        self.dirty = true;
                        finished = true;
                    }
                    LoudnessEvent::Cancelled { done } => {
                        self.progress.done = done;
                        self.progress.running = false;
                        self.progress.current = format!("Cancelled after {done} track(s)");
                        self.running = false;
                        self.dirty = true;
                        finished = true;
                    }
                }
            }
            if !self.running {
                clear = true;
            }
        }

        if clear {
            self.rx = None;
        }
        finished
    }

    /// Consume the "results changed" flag.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Write a minimal 16-bit mono PCM WAV containing a 1 kHz sine tone.
    fn write_tone_wav(path: &Path, secs: f32) {
        let sr = 44_100u32;
        let n = (sr as f32 * secs) as u32;
        let mut data = Vec::with_capacity(n as usize * 2);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
            let v = (s * 0.5 * i16::MAX as f32) as i16;
            data.extend_from_slice(&v.to_le_bytes());
        }
        let data_len = data.len() as u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&sr.to_le_bytes());
        out.extend_from_slice(&(sr * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.extend_from_slice(&data);
        std::fs::write(path, out).expect("write wav");
    }

    /// The engine's offline analyser is the heart of the scan; make sure the
    /// pipeline the service depends on actually produces ReplayGain values.
    #[test]
    fn scan_produces_replaygain_for_a_tone() {
        let dir = std::env::temp_dir().join(format!("sp_loudness_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let wav = dir.join("tone.wav");
        write_tone_wav(&wav, 2.0);

        let result = engine::decode::scan_track_loudness(&wav).expect("scan should succeed");
        assert!(
            result.ebu_r128_loudness.is_some(),
            "expected an integrated LUFS measurement"
        );
        assert!(
            result.replaygain_track_db.is_some(),
            "expected a ReplayGain track gain"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

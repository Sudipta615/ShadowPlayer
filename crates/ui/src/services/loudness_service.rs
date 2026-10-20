//! Background, library-wide ReplayGain / EBU R128 loudness scan.
//!
//! For every track that has no measured loudness yet we run the engine's
//! offline analyser, persist the result into the engine's on-disk loudness
//! cache (which playback consults) and record it in the library database for
//! display and smart playlists. The user's files are never modified.
//!
//! The scan fully decodes each file, so it is CPU-bound; on modest hardware it
//! deliberately yields between tracks to avoid starving the audio thread.

use std::collections::HashMap;
use std::path::PathBuf;
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
    /// Number of files whose tags were written back (0 when tag write-back is off).
    pub tagged: usize,
    /// Whether the current/last scan wrote ReplayGain tags into the files.
    pub write_tags: bool,
}

enum LoudnessEvent {
    Progress {
        done: usize,
        failed: usize,
        tagged: usize,
        current: String,
    },
    Finished {
        done: usize,
        failed: usize,
        tagged: usize,
    },
    Cancelled {
        done: usize,
        tagged: usize,
    },
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
    ///
    /// When `write_tags` is set the measured ReplayGain / EBU R128 values are
    /// also written back into each file's tags (the only place the app ever
    /// modifies the user's files). Callers must only pass `true` after explicit
    /// user consent.
    pub fn start(&mut self, write_tags: bool) {
        if self.running {
            return;
        }

        let items = match TrackRepo::new(self.db.clone()).list_missing_replaygain_with_album() {
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
                write_tags,
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
            write_tags,
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
                let mut tagged = 0usize;
                // The scan's results, grouped by album. ReplayGain's album
                // gain is a property of the album, so it can only be computed
                // once every track of that album has been measured.
                let mut by_album: HashMap<
                    (String, String),
                    Vec<(PathBuf, engine::decode::LoudnessScanResult)>,
                > = HashMap::new();

                for (id, path, album, artist) in items {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(LoudnessEvent::Cancelled { done, tagged });
                        return;
                    }

                    let current = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    let _ = tx.send(LoudnessEvent::Progress {
                        done,
                        failed,
                        tagged,
                        current,
                    });

                    match engine::decode::scan_track_loudness(&path) {
                        Some(result) => {
                            by_album
                                .entry((album.clone(), artist.clone()))
                                .or_default()
                                .push((path.clone(), result.clone()));

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

                            // Optional: persist the values into the file's own
                            // tags so other players/gear can normalise too.
                            if write_tags {
                                let meta = engine::dsp::LoudnessMetadata {
                                    replaygain_track_db: result.replaygain_track_db,
                                    replaygain_album_db: None,
                                    replaygain_track_peak: result.replaygain_track_peak,
                                    replaygain_album_peak: None,
                                    ebu_r128_loudness: result.ebu_r128_loudness,
                                    ebu_r128_peak: result.ebu_r128_peak_dbtp,
                                };
                                match engine::decode::write_loudness_tags(&path, &meta) {
                                    Ok(()) => tagged += 1,
                                    // Unsupported containers (PCM/DSD) are
                                    // expected; anything else is worth a log.
                                    Err(e) => log::debug!("Not tagging {}: {e:?}", path.display()),
                                }
                            }
                        }
                        None => failed += 1,
                    }

                    done += 1;

                    // Cooperative yield: a full decode is heavy and this must not
                    // starve audio rendering on a dual-core machine.
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }

                // ── Album gain ────────────────────────────────────────────
                // ReplayGain 2.0's album gain is not the mean of the track
                // gains: every track contributes with its loudness weight, so
                // a quiet track pulls the album gain down the way the
                // specification says it should.
                for ((album, artist), tracks) in by_album {
                    let Some(album_gain) =
                        engine::decode::accumulate_album_replaygain(tracks.iter().map(|(_, r)| r))
                    else {
                        continue;
                    };
                    if let Err(e) = repo.set_album_loudness(
                        &album,
                        &artist,
                        Some(album_gain.album_gain_db),
                        Some(album_gain.album_peak),
                    ) {
                        log::warn!("Could not record the album gain for {album}: {e}");
                        continue;
                    }

                    // When the values are being written into the files, the
                    // album gain is written too — but only now that it is
                    // known, which is why this is a second pass.
                    if write_tags {
                        for (path, result) in &tracks {
                            let meta = engine::dsp::LoudnessMetadata {
                                replaygain_track_db: result.replaygain_track_db,
                                replaygain_album_db: Some(album_gain.album_gain_db),
                                replaygain_track_peak: result.replaygain_track_peak,
                                replaygain_album_peak: Some(album_gain.album_peak),
                                ebu_r128_loudness: result.ebu_r128_loudness,
                                ebu_r128_peak: result.ebu_r128_peak_dbtp,
                            };
                            match engine::decode::write_loudness_tags(path, &meta) {
                                Ok(()) => {}
                                Err(e) => {
                                    log::debug!("Not tagging {}: {e:?}", path.display())
                                }
                            }
                        }
                    }
                }

                let _ = tx.send(LoudnessEvent::Finished {
                    done,
                    failed,
                    tagged,
                });
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
                        tagged,
                        current,
                    } => {
                        self.progress.done = done;
                        self.progress.failed = failed;
                        self.progress.tagged = tagged;
                        self.progress.current = current;
                    }
                    LoudnessEvent::Finished {
                        done,
                        failed,
                        tagged,
                    } => {
                        self.progress.done = done;
                        self.progress.failed = failed;
                        self.progress.tagged = tagged;
                        self.progress.running = false;
                        self.progress.current = if self.progress.write_tags {
                            format!("Done: {done} analysed, {failed} skipped, {tagged} tagged")
                        } else {
                            format!("Done: {done} analysed, {failed} skipped")
                        };
                        self.running = false;
                        self.dirty = true;
                        finished = true;
                    }
                    LoudnessEvent::Cancelled { done, tagged } => {
                        self.progress.done = done;
                        self.progress.tagged = tagged;
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

    /// Tag write-back is opt-in, but when enabled it must actually land in the
    /// file. A WAV carries an ID3v2 chunk, so the raw tag key is inspectable
    /// without a second metadata dependency.
    #[test]
    fn scan_can_write_replaygain_tags_into_the_file() {
        let dir = std::env::temp_dir().join(format!("sp_loudness_tags_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let wav = dir.join("tone.wav");
        write_tone_wav(&wav, 2.0);

        let before = std::fs::read(&wav).expect("read wav");

        let result = engine::decode::scan_track_loudness(&wav).expect("scan should succeed");
        let meta = engine::dsp::LoudnessMetadata {
            replaygain_track_db: result.replaygain_track_db,
            replaygain_album_db: None,
            replaygain_track_peak: result.replaygain_track_peak,
            replaygain_album_peak: None,
            ebu_r128_loudness: result.ebu_r128_loudness,
            ebu_r128_peak: result.ebu_r128_peak_dbtp,
        };
        engine::decode::write_loudness_tags(&wav, &meta).expect("tag write should succeed");

        let after = std::fs::read(&wav).expect("read wav");
        assert_ne!(before, after, "the file must have been modified");
        let text = String::from_utf8_lossy(&after);
        assert!(
            text.contains("REPLAYGAIN_TRACK_GAIN"),
            "expected a ReplayGain track gain tag in the file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

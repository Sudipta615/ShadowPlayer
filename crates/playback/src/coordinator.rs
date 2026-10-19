use audio_session::{AudioSession, EngineEvent, OutputEvent, PlaybackState};
use database::{Database, HistoryRepo, QueueRepo, TrackRepo};
use domain::ids::TrackId;
use domain::models::{RepeatModeChoice, Track};
use log::{info, warn};
use rand::seq::SliceRandom;
use std::path::PathBuf;
use std::sync::Arc;

/// A track counts as "played" once this many seconds have been listened to (or
/// the whole track, whichever is shorter). Skips before this do not register.
const PLAY_THRESHOLD_SECS: f32 = 20.0;

/// The largest position advance between two polls that still counts as
/// continuous playback. Anything bigger is a seek and must not inflate the
/// listening total; a normal frame is two orders of magnitude smaller.
const MAX_CREDITED_STEP_SECS: f32 = 2.0;

/// Pressing *previous* this far into a track restarts it instead of moving on.
const RESTART_SEEK_THRESHOLD_SECS: f32 = 3.0;

/// The `.cue` names to look for beside an album image, after one named for the
/// image itself. These are the two conventions the rippers actually use.
const SIBLING_CUE_NAMES: [&str; 2] = ["album.cue", "CDImage.cue"];

pub struct PlaybackCoordinator {
    session: Arc<AudioSession>,
    _db: Database,
    track_repo: TrackRepo,
    history_repo: HistoryRepo,
    queue_repo: QueueRepo,
    queue: Vec<Track>,
    /// Original insertion order, kept while shuffle is active so we can restore
    /// it when shuffle is turned off.
    unshuffled_queue: Option<Vec<Track>>,
    current_index: Option<usize>,
    current_track: Option<Track>,
    shuffle: bool,
    repeat_mode: RepeatModeChoice,

    // Listening-accounting for the recently/most played rules.
    listen_secs: f32,
    last_position: f32,
    play_registered: bool,

    /// Engine and audio-device problems worth showing the user, collected by
    /// [`PlaybackCoordinator::poll_events`] and drained by the shell. These
    /// used to be dropped on the floor (`_ => {}`), so a decode failure or a
    /// disconnected DAC was invisible.
    engine_notices: Vec<String>,
}

impl PlaybackCoordinator {
    pub fn new(session: Arc<AudioSession>, db: Database) -> Self {
        let track_repo = TrackRepo::new(db.clone());
        let history_repo = HistoryRepo::new(db.clone());
        let queue_repo = QueueRepo::new(db.clone());

        // Drop play-history rows that have aged out of the rolling window.
        let _ = history_repo.prune_older_than_window();

        let mut coordinator = Self {
            session,
            _db: db,
            track_repo,
            history_repo,
            queue_repo,
            queue: Vec::new(),
            unshuffled_queue: None,
            current_index: None,
            current_track: None,
            shuffle: false,
            repeat_mode: RepeatModeChoice::Off,
            listen_secs: 0.0,
            last_position: 0.0,
            play_registered: false,
            engine_notices: Vec::new(),
        };

        coordinator.restore_saved_queue();
        coordinator
    }

    pub fn restore_saved_queue(&mut self) {
        if let Ok(loaded) = self.queue_repo.load_queue() {
            if !loaded.is_empty() {
                info!("Restored {} tracks from saved queue", loaded.len());
                self.queue = loaded;
                self.current_index = Some(0);
                self.current_track = self.queue.first().cloned();
            }
        }
    }

    pub fn queue(&self) -> &[Track] {
        &self.queue
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_track.as_ref()
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    pub fn is_playing(&self) -> bool {
        self.session.is_playing()
    }

    pub fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat_mode(&self) -> RepeatModeChoice {
        self.repeat_mode
    }

    pub fn play_track(&mut self, track: Track, new_queue: Option<Vec<Track>>) {
        let queue_replaced = new_queue.is_some();
        if let Some(q) = new_queue {
            self.queue = q;
            self.persist_queue();
        }

        // Find index of track in current queue
        self.current_index = self.queue.iter().position(|t| t.id == track.id);
        if self.current_index.is_none() {
            self.queue.push(track.clone());
            self.current_index = Some(self.queue.len() - 1);
            self.persist_queue();
        }

        // If shuffle is on, permute everything after the chosen track and keep
        // the original order for when shuffle is turned back off.
        if self.shuffle {
            // Snapshot only when there is no snapshot yet. `self.queue` is
            // already a permutation when shuffle is on, so re-snapshotting on
            // every track change replaced the user's real order with a
            // shuffled one — and turning shuffle off could never restore it.
            // A brand new queue does need a fresh snapshot.
            if queue_replaced || self.unshuffled_queue.is_none() {
                self.unshuffled_queue = Some(self.queue.clone());
            }
            self.shuffle_upcoming();
            self.persist_queue();
        } else {
            self.unshuffled_queue = None;
        }

        self.current_track = Some(track.clone());
        info!("Playing track: {} - {}", track.title, track.artist);

        self.begin_tracking();
        self.open_and_play_track(&track);

        // Gapless preload of next track
        self.preload_next();
    }

    pub fn play_index(&mut self, index: usize) {
        if index < self.queue.len() {
            let track = self.queue[index].clone();
            self.current_index = Some(index);
            self.current_track = Some(track.clone());
            self.begin_tracking();
            self.open_and_play_track(&track);
            self.preload_next();
        }
    }

    fn open_and_play_track(&self, track: &Track) {
        let path_str = track.path.to_string_lossy();
        let Some(cue) = domain::cue::parse_cue_virtual_path(&path_str) else {
            self.session.open_file(&track.path);
            self.session.play();
            return;
        };

        let real_path = cue.audio_path();

        // Prefer the sheet the row was indexed from. Guessing at the filename
        // is only a fallback for rows written before it was recorded, and it
        // used to fail silently by playing the whole image from 0:00.
        let recorded = cue.sheet_path();
        let mut candidates: Vec<std::path::PathBuf> = Vec::with_capacity(4);
        if let Some(sheet) = &recorded {
            candidates.push(sheet.clone());
        }
        candidates.extend(sibling_cue_candidates(&real_path));

        for sheet in candidates {
            if sheet.exists() && self.session.open_cue_track(&real_path, &sheet, cue.track_number) {
                self.session.play();
                return;
            }
        }

        warn!(
            "No usable .cue sheet for {} (track {}); playing the whole album image",
            real_path.display(),
            cue.track_number
        );
        self.session.open_file(&real_path);
        self.session.play();
    }

    pub fn toggle_play_pause(&self) {
        if self.session.is_playing() {
            self.session.pause();
        } else if self.session.state() == PlaybackState::Paused {
            self.session.play();
        } else if let Some(ref track) = self.current_track {
            self.open_and_play_track(track);
        } else if let Some(first) = self.queue.first() {
            self.open_and_play_track(first);
        }
    }

    pub fn next(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        // The queue is already permuted when shuffle is on, so advancing is
        // simply "next in the list" in both modes.
        let next_idx = if let Some(curr) = self.current_index {
            if curr + 1 < self.queue.len() {
                curr + 1
            } else if self.repeat_mode == RepeatModeChoice::All {
                if self.shuffle {
                    // Start a fresh shuffled pass.
                    self.shuffle_upcoming_from(0);
                }
                0
            } else {
                return;
            }
        } else {
            0
        };

        self.play_index(next_idx);
    }

    pub fn previous(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        // Already into the track: restart it rather than moving to the
        // previous one.
        if self.session.position_secs_compensated() > RESTART_SEEK_THRESHOLD_SECS {
            self.session.seek(0.0);
            return;
        }

        let prev_idx = if let Some(curr) = self.current_index {
            if curr > 0 {
                curr - 1
            } else if self.repeat_mode == RepeatModeChoice::All {
                self.queue.len().saturating_sub(1)
            } else {
                0
            }
        } else {
            0
        };

        self.play_index(prev_idx);
    }

    /// Repoint queued tracks after their files were moved on disk.
    ///
    /// Keeps ids, order and the current index — so playback continues — while
    /// making sure the next track is opened from its new location.
    pub fn remap_paths(&mut self, moved: &[(TrackId, PathBuf)]) {
        if moved.is_empty() {
            return;
        }
        // A map, not a scan: this was an O(queue x moved) linear search, which
        // is tens of millions of comparisons when organizing a large library.
        let lookup: std::collections::HashMap<TrackId, &PathBuf> =
            moved.iter().map(|(id, path)| (*id, path)).collect();

        let mut changed = false;
        for track in self.queue.iter_mut() {
            if let Some(new_path) = lookup.get(&track.id) {
                track.path = (*new_path).clone();
                changed = true;
            }
        }
        if let Some(current) = self.current_track.as_mut() {
            if let Some(new_path) = lookup.get(&current.id) {
                current.path = (*new_path).clone();
                changed = true;
            }
        }
        if changed {
            self.persist_queue();
        }
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.current_index = None;
        // The current track used to survive: the queue was emptied but the
        // coordinator still reported a current track and kept crediting
        // listening time for it.
        self.current_track = None;
        self.unshuffled_queue = None;
        self.begin_tracking();
        if let Err(e) = self.queue_repo.clear() {
            warn!("Could not clear the saved queue: {e}");
        }
        self.session.clear_playlist();
    }

    pub fn add_to_queue(&mut self, track: Track) {
        self.queue.push(track);
        if self.shuffle {
            // Structural change: drop the saved order so disabling shuffle
            // can't resurrect a stale queue.
            self.unshuffled_queue = None;
        }
        self.persist_queue();
    }

    pub fn add_to_queue_next(&mut self, track: Track) {
        if let Some(curr) = self.current_index {
            self.queue.insert(curr + 1, track);
        } else {
            self.queue.push(track);
        }
        if self.shuffle {
            self.unshuffled_queue = None;
        }
        self.persist_queue();
        self.preload_next();
    }

    pub fn remove_from_queue(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
            if self.shuffle {
                self.unshuffled_queue = None;
            }
            if let Some(curr) = self.current_index {
                if index < curr {
                    self.current_index = Some(curr.saturating_sub(1));
                } else if index == curr {
                    if self.queue.is_empty() {
                        self.current_index = None;
                        self.current_track = None;
                        self.session.stop();
                    } else {
                        let new_idx = curr.min(self.queue.len() - 1);
                        self.play_index(new_idx);
                    }
                }
            }
            self.persist_queue();
            self.preload_next();
        }
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled == self.shuffle {
            self.session.set_shuffle(enabled);
            return;
        }

        if enabled {
            // Remember the original order, then shuffle everything from the
            // current track onwards (played items stay put).
            self.unshuffled_queue = Some(self.queue.clone());
            self.shuffle_upcoming();
        } else if let Some(original) = self.unshuffled_queue.take() {
            // Restore the original order and re-locate the current track.
            let current_id = self.current_track.as_ref().map(|t| t.id);
            self.queue = original;
            self.current_index =
                current_id.and_then(|id| self.queue.iter().position(|t| t.id == id));
        }

        self.shuffle = enabled;
        self.session.set_shuffle(enabled);
        self.persist_queue();
        self.preload_next();
    }

    /// Shuffle the queue from just after the current track to the end.
    fn shuffle_upcoming(&mut self) {
        let from = self.current_index.map(|i| i + 1).unwrap_or(0).min(self.queue.len());
        self.shuffle_upcoming_from(from);
    }

    fn shuffle_upcoming_from(&mut self, from: usize) {
        let from = from.min(self.queue.len());
        if self.queue.len() > from + 1 {
            let mut rng = rand::thread_rng();
            self.queue[from..].shuffle(&mut rng);
        }
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatModeChoice) {
        self.repeat_mode = mode;
        let engine_repeat = match mode {
            RepeatModeChoice::Off => audio_session::RepeatMode::Off,
            RepeatModeChoice::All => audio_session::RepeatMode::All,
            RepeatModeChoice::One => audio_session::RepeatMode::One,
        };
        self.session.set_repeat_mode(engine_repeat);
    }

    pub fn poll_events(&mut self) {
        // Accumulate listening time for the current track (drives the
        // recently/most played rules).
        self.sample_listening();

        while let Some(event) = self.session.try_recv_event() {
            match event {
                EngineEvent::SourceFinished { .. } => {
                    // A track that reaches its natural end always counts.
                    if !self.play_registered {
                        if let Some(id) = self.current_track.as_ref().map(|t| t.id) {
                            let threshold = self
                                .current_track
                                .as_ref()
                                .map(|t| PLAY_THRESHOLD_SECS.min(t.duration_secs))
                                .unwrap_or(PLAY_THRESHOLD_SECS);
                            self.register_play(id, threshold, true);
                        }
                    }

                    if self.repeat_mode == RepeatModeChoice::One {
                        // The engine already restarts the source at 0:00 for
                        // Repeat-One, so do not reopen the file: reopening is a
                        // second restart, and for a CUE virtual track it played
                        // the whole album image instead of the track. Only the
                        // listening accounting needs resetting.
                        self.begin_tracking();
                        continue;
                    }

                    self.next();
                }
                EngineEvent::Error(message) => {
                    warn!("Engine error: {message}");
                    self.push_notice(format!("Audio engine error: {message}"));
                }
                EngineEvent::PlaylistLoadFailed { path, message } => {
                    warn!("Playlist load failed for {}: {message}", path.display());
                    self.push_notice(format!(
                        "Could not load playlist {}: {message}",
                        path.display()
                    ));
                }
                // Opened/paused/stopped/seek/format/telemetry events carry no
                // action for the coordinator.
                _ => {}
            }
        }

        // Output-device events are not just decoration: a disconnected DAC
        // stops playback, and the user needs to be told rather than left
        // wondering why the transport is dead.
        while let Some(event) = self.session.try_recv_output_event() {
            match event {
                OutputEvent::DeviceDisconnected { device } => {
                    warn!("Audio device disconnected: {device}");
                    self.push_notice(format!("Audio device disconnected: {device}"));
                }
                OutputEvent::EndpointError { endpoint, message } => {
                    warn!("Audio endpoint {endpoint} failed: {message}");
                    self.push_notice(format!("Audio endpoint {endpoint} failed: {message}"));
                }
                // Arrivals, list changes and the negotiated device are already
                // reflected by the device picker.
                OutputEvent::DeviceConnected { .. }
                | OutputEvent::DeviceListChanged { .. }
                | OutputEvent::OutputDeviceChanged { .. } => {}
            }
        }
    }

    /// Record a user-facing engine problem, keeping the list bounded so a
    /// misbehaving engine cannot grow it without limit.
    fn push_notice(&mut self, message: String) {
        const MAX_NOTICES: usize = 16;
        if self.engine_notices.len() == MAX_NOTICES {
            self.engine_notices.remove(0);
        }
        self.engine_notices.push(message);
    }

    /// Take the engine/device problems seen since the last call, for the shell
    /// to present (toast, status line).
    pub fn take_engine_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.engine_notices)
    }

    /// Reset per-track listening accounting.
    fn begin_tracking(&mut self) {
        self.listen_secs = 0.0;
        self.last_position = 0.0;
        self.play_registered = false;
    }

    /// Add the delta in playback position to the listened total, and register
    /// the play once the threshold is crossed.
    fn sample_listening(&mut self) {
        if self.play_registered {
            return;
        }
        let Some(track_id) = self.current_track.as_ref().map(|t| t.id) else {
            return;
        };
        if !self.session.is_playing() {
            self.last_position = self.session.position_secs_compensated();
            return;
        }

        let pos = self.session.position_secs_compensated();
        self.listen_secs += credited_listen_secs(self.last_position, pos);
        self.last_position = pos;

        let duration = self
            .current_track
            .as_ref()
            .map(|t| t.duration_secs)
            .unwrap_or(0.0);
        let threshold = PLAY_THRESHOLD_SECS.min(duration);
        if self.listen_secs >= threshold {
            self.register_play(track_id, threshold, false);
        }
    }

    fn register_play(&mut self, track_id: TrackId, threshold: f32, completed: bool) {
        // Reported rather than swallowed: silently failing here means a track
        // that was heard never appears in "recently played" or the play counts.
        if let Err(e) = self.track_repo.increment_play_count(&track_id) {
            warn!("Could not increment play count for {track_id}: {e}");
        }
        if let Err(e) = self
            .history_repo
            .record_play(&track_id, self.listen_secs.max(threshold), completed)
        {
            warn!("Could not record play history for {track_id}: {e}");
        }
        self.play_registered = true;
    }

    fn preload_next(&self) {
        if let Some(curr) = self.current_index {
            let next_idx = curr + 1;
            if next_idx < self.queue.len() {
                let next_path = &self.queue[next_idx].path;
                self.session.prepare_next_file(next_path);
            }
        }
    }

    fn persist_queue(&self) {
        let ids: Vec<_> = self.queue.iter().map(|t| t.id).collect();
        // A failure here means the queue will not survive a restart, which is
        // worth a log line: it used to fail completely silently.
        if let Err(e) = self.queue_repo.save_queue(&ids) {
            warn!("Could not persist the queue ({} entries): {e}", ids.len());
        }
    }
}

/// Listening time to credit for a poll that moved the playhead from
/// `last_position` to `pos`.
///
/// Only *contiguous* playback counts. A forward jump is a seek too, so without
/// the guard, dragging the progress bar to the end registered a play for a
/// track the user never heard; a normal UI frame advances by a few milliseconds.
fn credited_listen_secs(last_position: f32, pos: f32) -> f32 {
    let delta = pos - last_position;
    if delta > 0.0 && delta <= MAX_CREDITED_STEP_SECS {
        delta
    } else {
        0.0
    }
}

#[cfg(test)]
mod coordinator_unit_tests {
    use super::credited_listen_secs;

    /// A forward jump in the playhead is a seek, not listening.
    #[test]
    fn only_contiguous_playback_counts_as_listening() {
        // A normal UI frame.
        assert!((credited_listen_secs(10.0, 10.016) - 0.016).abs() < 1e-6);
        // A stalled frame is still playback.
        assert!((credited_listen_secs(10.0, 11.5) - 1.5).abs() < 1e-6);
        // Dragging the progress bar near the end is not.
        assert_eq!(credited_listen_secs(10.0, 175.0), 0.0);
        // Neither is seeking backwards or restarting a track.
        assert_eq!(credited_listen_secs(100.0, 5.0), 0.0);
        assert_eq!(credited_listen_secs(0.0, 0.0), 0.0);
    }
}

/// The conventional `.cue` names to try beside an album image, for rows written
/// before the sheet's own path was recorded.
fn sibling_cue_candidates(real_path: &std::path::Path) -> [std::path::PathBuf; 3] {
    let stem = real_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let parent = real_path.parent().unwrap_or(std::path::Path::new("."));
    [
        parent.join(format!("{}.cue", stem)),
        parent.join(SIBLING_CUE_NAMES[0]),
        parent.join(SIBLING_CUE_NAMES[1]),
    ]
}

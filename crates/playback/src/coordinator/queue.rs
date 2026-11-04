use audio_session::{AudioSession, PlaybackState};
use database::{Database, HistoryRepo, QueueRepo, ScrobbleRepo, TrackRepo};
use domain::ids::TrackId;
use domain::models::{RepeatModeChoice, Track};
use log::{info, warn};
use std::path::PathBuf;
use std::sync::Arc;

use super::{sibling_cue_candidates, PlaybackCoordinator, RESTART_SEEK_THRESHOLD_SECS};

impl PlaybackCoordinator {
    pub fn new(session: Arc<AudioSession>, db: Database) -> Self {
        let track_repo = TrackRepo::new(db.clone());
        let history_repo = HistoryRepo::new(db.clone());
        let scrobble_repo = ScrobbleRepo::new(db.clone());
        let queue_repo = QueueRepo::new(db.clone());

        // Drop play-history rows that have aged out of the rolling window.
        let _ = history_repo.prune_older_than_window();

        let mut coordinator = Self {
            session,
            _db: db,
            track_repo,
            history_repo,
            scrobble_repo,
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
            room_measurement_events: Vec::new(),
            pending_output_device: None,
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

    /// Reflect a rating change in the in-memory queue and current track.
    ///
    /// Ratings are persisted by `TrackRepo`; the coordinator holds its own
    /// clones for playback, and the now-playing card reads those clones. This
    /// keeps the card in step without reloading the queue from disk.
    pub fn update_track_rating(&mut self, id: domain::ids::TrackId, rating: u8) {
        for track in &mut self.queue {
            if track.id == id {
                track.rating = rating;
            }
        }
        if let Some(track) = self.current_track.as_mut() {
            if track.id == id {
                track.rating = rating;
            }
        }
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
            if sheet.exists()
                && self
                    .session
                    .open_cue_track(&real_path, &sheet, cue.track_number)
            {
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

        // Leaving the current track before it was heard is a skip.
        self.register_skip();

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

        // Leaving the current track before it was heard is a skip.
        self.register_skip();

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
}

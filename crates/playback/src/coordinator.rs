use audio_session::{AudioSession, EngineEvent, PlaybackState};
use database::{Database, HistoryRepo, QueueRepo, TrackRepo};
use domain::models::{RepeatModeChoice, Track};
use log::info;
use rand::seq::SliceRandom;
use std::sync::Arc;

pub struct PlaybackCoordinator {
    session: Arc<AudioSession>,
    _db: Database,
    track_repo: TrackRepo,
    history_repo: HistoryRepo,
    queue_repo: QueueRepo,
    queue: Vec<Track>,
    current_index: Option<usize>,
    current_track: Option<Track>,
    shuffle: bool,
    repeat_mode: RepeatModeChoice,
}

impl PlaybackCoordinator {
    pub fn new(session: Arc<AudioSession>, db: Database) -> Self {
        let track_repo = TrackRepo::new(db.clone());
        let history_repo = HistoryRepo::new(db.clone());
        let queue_repo = QueueRepo::new(db.clone());

        let mut coordinator = Self {
            session,
            _db: db,
            track_repo,
            history_repo,
            queue_repo,
            queue: Vec::new(),
            current_index: None,
            current_track: None,
            shuffle: false,
            repeat_mode: RepeatModeChoice::Off,
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

        self.current_track = Some(track.clone());
        info!("Playing track: {} - {}", track.title, track.artist);

        self.open_and_play_track(&track);

        // Gapless preload of next track
        self.preload_next();
    }

    pub fn play_index(&mut self, index: usize) {
        if index < self.queue.len() {
            let track = self.queue[index].clone();
            self.current_index = Some(index);
            self.current_track = Some(track.clone());
            self.open_and_play_track(&track);
            self.preload_next();
        }
    }

    fn open_and_play_track(&self, track: &Track) {
        let path_str = track.path.to_string_lossy();
        if let Some((audio_path, cue_suffix)) = path_str.split_once("#cue:") {
            if let Ok(track_num) = cue_suffix.parse::<u32>() {
                let real_path = std::path::Path::new(audio_path);
                let stem = real_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let parent = real_path.parent().unwrap_or(std::path::Path::new("."));
                let cue_candidates = [
                    parent.join(format!("{}.cue", stem)),
                    parent.join("album.cue"),
                    parent.join("CDImage.cue"),
                ];
                let mut opened = false;
                for cue_file in cue_candidates {
                    if cue_file.exists() && self.session.open_cue_track(real_path, &cue_file, track_num) {
                        opened = true;
                        break;
                    }
                }
                if !opened {
                    self.session.open_file(real_path);
                }
            } else {
                self.session.open_file(&track.path);
            }
        } else {
            self.session.open_file(&track.path);
        }
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

        let next_idx = if self.shuffle {
            let mut rng = rand::thread_rng();
            (0..self.queue.len()).collect::<Vec<_>>().choose(&mut rng).copied().unwrap_or(0)
        } else if let Some(curr) = self.current_index {
            if curr + 1 < self.queue.len() {
                curr + 1
            } else if self.repeat_mode == RepeatModeChoice::All {
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

        // If played > 3 seconds into track, seek back to beginning
        if self.session.position_secs_compensated() > 3.0 {
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

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.current_index = None;
        let _ = self.queue_repo.clear();
        self.session.clear_playlist();
    }

    pub fn add_to_queue(&mut self, track: Track) {
        self.queue.push(track);
        self.persist_queue();
    }

    pub fn add_to_queue_next(&mut self, track: Track) {
        if let Some(curr) = self.current_index {
            self.queue.insert(curr + 1, track);
        } else {
            self.queue.push(track);
        }
        self.persist_queue();
        self.preload_next();
    }

    pub fn remove_from_queue(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
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
        self.shuffle = enabled;
        self.session.set_shuffle(enabled);
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
        while let Some(event) = self.session.try_recv_event() {
            match event {
                EngineEvent::SourceFinished { .. } => {
                    if let Some(ref track) = self.current_track {
                        let _ = self.track_repo.increment_play_count(&track.id);
                        let _ = self.history_repo.record_play(&track.id, track.duration_secs, true);
                    }

                    if self.repeat_mode == RepeatModeChoice::One {
                        if let Some(ref track) = self.current_track {
                            self.session.open_file(&track.path);
                            self.session.play();
                            return;
                        }
                    }

                    self.next();
                }
                _ => {}
            }
        }
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
        let entries: Vec<_> = self.queue.iter().map(|t| (t.id, t.path.clone())).collect();
        let _ = self.queue_repo.save_queue(&entries);
    }
}

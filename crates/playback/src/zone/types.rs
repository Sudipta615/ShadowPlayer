//! Autonomous playback state for an individual audio zone.

use domain::ids::ZoneId;
use domain::models::Track;
use domain::zone::{ClockDriftManager, ZoneConfig};

/// Active playback state and transport owned by an individual audio zone.
#[derive(Debug, Clone)]
pub struct ZonePlaybackState {
    pub id: ZoneId,
    pub config: ZoneConfig,
    pub queue: Vec<Track>,
    pub unshuffled_queue: Option<Vec<Track>>,
    pub current_index: Option<usize>,
    pub current_track: Option<Track>,
    pub is_playing: bool,
    pub position_secs: f32,
    pub duration_secs: f32,
    pub clock_drift: ClockDriftManager,
}

impl ZonePlaybackState {
    /// Construct a new idle zone from stored configuration.
    pub fn new(config: ZoneConfig) -> Self {
        let drift = ClockDriftManager::with_drift_pct(config.clock_drift_pct);
        Self {
            id: config.id,
            config,
            queue: Vec::new(),
            unshuffled_queue: None,
            current_index: None,
            current_track: None,
            is_playing: false,
            position_secs: 0.0,
            duration_secs: 0.0,
            clock_drift: drift,
        }
    }

    /// The effective output volume (0.0 if muted, otherwise configured volume).
    pub fn effective_volume(&self) -> f32 {
        if self.config.muted {
            0.0
        } else {
            self.config.volume
        }
    }

    /// Set volume for this zone [0.0, 1.0].
    pub fn set_volume(&mut self, volume: f32) {
        self.config.volume = volume.clamp(0.0, 1.0);
    }

    /// Toggle or set mute for this zone.
    pub fn set_muted(&mut self, muted: bool) {
        self.config.muted = muted;
    }

    /// Start playing a track in this zone's queue.
    pub fn play_track(&mut self, track: Track, new_queue: Option<Vec<Track>>) {
        if let Some(q) = new_queue {
            self.queue = q;
        }

        self.current_index = self.queue.iter().position(|t| t.id == track.id);
        if self.current_index.is_none() {
            self.queue.push(track.clone());
            self.current_index = Some(self.queue.len() - 1);
        }

        self.duration_secs = track.duration_secs;
        self.position_secs = 0.0;
        self.current_track = Some(track);
        self.is_playing = true;
    }

    /// Pause or resume playback in this zone.
    pub fn toggle_pause(&mut self) -> bool {
        self.is_playing = !self.is_playing;
        self.is_playing
    }

    /// Stop playback and reset current position.
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.position_secs = 0.0;
    }

    /// Seek to a specific timestamp in seconds.
    pub fn seek(&mut self, pos_secs: f32) {
        if self.duration_secs > 0.0 {
            self.position_secs = pos_secs.clamp(0.0, self.duration_secs);
        } else {
            self.position_secs = pos_secs.max(0.0);
        }
    }

    /// Advance to the next track in this zone's queue.
    pub fn next_track(&mut self) -> Option<Track> {
        let idx = self.current_index?;
        if idx + 1 < self.queue.len() {
            let next_idx = idx + 1;
            self.current_index = Some(next_idx);
            let track = self.queue[next_idx].clone();
            self.current_track = Some(track.clone());
            self.position_secs = 0.0;
            self.duration_secs = track.duration_secs;
            Some(track)
        } else {
            self.stop();
            None
        }
    }

    /// Go to the previous track in this zone's queue.
    pub fn prev_track(&mut self) -> Option<Track> {
        let idx = self.current_index?;
        if self.position_secs > 3.0 || idx == 0 {
            self.position_secs = 0.0;
            self.current_track.clone()
        } else {
            let prev_idx = idx - 1;
            self.current_index = Some(prev_idx);
            let track = self.queue[prev_idx].clone();
            self.current_track = Some(track.clone());
            self.position_secs = 0.0;
            self.duration_secs = track.duration_secs;
            Some(track)
        }
    }
}

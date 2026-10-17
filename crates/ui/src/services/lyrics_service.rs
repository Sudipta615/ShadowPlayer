use std::collections::HashMap;

use domain::ids::TrackId;
use domain::models::{SyncedLyricLine, Track};
use lyrics::{find_active_line, find_sidecar_lyrics};

/// Service managing sidecar and embedded lyrics loading, caching, and synchronization
pub struct LyricsService {
    cache: HashMap<TrackId, (Vec<SyncedLyricLine>, Option<String>)>,
}

impl LyricsService {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Ensure lyrics are parsed and cached for the track
    pub fn ensure_loaded(&mut self, track: &Track) {
        if !self.cache.contains_key(&track.id) {
            if let Some((_is_synced, lrc_lines, raw)) = find_sidecar_lyrics(&track.path) {
                self.cache.insert(track.id, (lrc_lines, raw));
            } else {
                self.cache.insert(track.id, (Vec::new(), None));
            }
        }
    }

    /// Retrieve active synced lines and plain text lyrics for track
    pub fn get_lyrics(&self, track_id: &TrackId) -> (Vec<SyncedLyricLine>, Option<String>) {
        if let Some((lines, raw)) = self.cache.get(track_id) {
            (lines.clone(), raw.clone())
        } else {
            (Vec::new(), None)
        }
    }

    /// Find current active synchronized lyric line index for time position
    pub fn active_index(lines: &[SyncedLyricLine], position_secs: f32) -> Option<usize> {
        find_active_line(lines, position_secs)
    }
}

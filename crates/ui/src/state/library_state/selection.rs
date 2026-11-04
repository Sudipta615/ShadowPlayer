//! Favourites, ratings, and multi-selection maintenance.

use super::*;

impl LibraryState {
    pub fn toggle_favorite(&mut self, favorites_repo: &FavoritesRepo, track_id: TrackId) {
        if self.favorite_ids.contains(&track_id) {
            let _ = favorites_repo.remove(&track_id);
            self.favorite_ids.remove(&track_id);
        } else {
            let _ = favorites_repo.add(&track_id);
            self.favorite_ids.insert(track_id);
        }
        // Favorites membership changed; force the visible list to rebuild.
        self.displayed_key = None;
    }

    /// Mark a track as favorite (idempotent; used by bulk actions).
    pub fn add_favorite(&mut self, favorites_repo: &FavoritesRepo, track_id: TrackId) {
        if !self.favorite_ids.contains(&track_id) {
            let _ = favorites_repo.add(&track_id);
            self.favorite_ids.insert(track_id);
            self.displayed_key = None;
        }
    }

    /// Update a track's rating in memory across all_tracks and displayed_tracks.
    pub fn update_track_rating(&mut self, track_id: TrackId, rating: u8) {
        let clamped = rating.min(5);
        if let Some(track) = self.all_tracks.iter_mut().find(|t| t.id == track_id) {
            track.rating = clamped;
        }
        if let Some(track) = self.displayed_tracks.iter_mut().find(|t| t.id == track_id) {
            track.rating = clamped;
        }
        if self.quick_filter == crate::components::songs_table::QuickFilter::Rating {
            self.displayed_key = None;
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_track_ids.clear();
        self.last_clicked_index = None;
    }

    /// The selected tracks in a stable, user-visible order: the current view's
    /// order first, then any remaining ids (e.g. carried over from another tab)
    /// resolved from the full library.
    pub fn selected_tracks_in_order(&self) -> Vec<Track> {
        if self.selected_track_ids.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Track> = self
            .displayed_tracks
            .iter()
            .filter(|t| self.selected_track_ids.contains(&t.id))
            .cloned()
            .collect();
        let mut seen: HashSet<TrackId> = out.iter().map(|t| t.id).collect();
        for t in &self.all_tracks {
            if self.selected_track_ids.contains(&t.id) && seen.insert(t.id) {
                out.push(t.clone());
            }
        }
        out
    }

    /// Drop selected ids that no longer exist in the library.
    pub fn prune_selection(&mut self) {
        if self.selected_track_ids.is_empty() {
            return;
        }
        let alive: HashSet<TrackId> = self.all_tracks.iter().map(|t| t.id).collect();
        self.selected_track_ids.retain(|id| alive.contains(id));
    }
}

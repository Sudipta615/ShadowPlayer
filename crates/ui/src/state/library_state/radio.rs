//! Radio seeds and recommendation-id projection.

use super::*;

impl LibraryState {
    /// Replace the current local radio station.
    ///
    /// The running order is stored here rather than rebuilt from
    /// `displayed_tracks`, so leaving Radio and coming back does not re-query
    /// the library or reshuffle the station.
    pub fn set_radio_station(&mut self, seed: TrackId, label: String, tracks: Vec<Track>) {
        self.radio_seed = Some(seed);
        self.radio_label = label;
        self.radio_tracks = tracks;
        self.displayed_key = None;
    }

    /// Replace the recommendation feed with a new pending list.
    ///
    /// Played tracks drop out and fresh ones append, so the visible For You
    /// list changes without the user touching a control. The rebuild is only
    /// forced while For You is on screen; otherwise the next visit builds it.
    pub fn set_recommendation_ids(&mut self, ids: Vec<TrackId>) {
        if self.recommendation_ids == ids {
            return;
        }
        self.recommendation_ids = ids;
        if self.quick_filter == QuickFilter::ForYou {
            self.displayed_key = None;
        }
    }

    /// Forget the feed after the worker resets or the feature is disabled.
    pub fn clear_recommendation_ids(&mut self) {
        self.set_recommendation_ids(Vec::new());
    }

    /// Materialise the pending recommendation ids in ranking order.
    ///
    /// Only the dozen or so recommended tracks are cloned, not the library.
    /// A single pass collects the wanted ids; building a 100,000-entry map
    /// just to look up twelve tracks would allocate and hash the whole catalog
    /// on every playback edge.
    pub(super) fn recommended_tracks(&self) -> Vec<Track> {
        if self.recommendation_ids.is_empty() {
            return Vec::new();
        }
        let wanted: HashSet<&TrackId> = self.recommendation_ids.iter().collect();
        let mut found: HashMap<TrackId, Track> = HashMap::with_capacity(wanted.len());
        for track in &self.all_tracks {
            if wanted.contains(&track.id) {
                found.insert(track.id, track.clone());
            }
        }
        self.recommendation_ids
            .iter()
            .filter_map(|id| found.remove(id))
            .collect()
    }
}

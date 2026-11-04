//! Smart-playlist evaluation, caching, and refresh policy.

use super::*;

impl LibraryState {
    /// Note that the library changed.
    ///
    /// Under the library-change policy this is what makes the smart
    /// playlists stale; under the manual policy it changes nothing, because
    /// the user asked for the result to be frozen.
    pub fn note_library_change(&mut self) {
        self.library_changed = true;
    }

    /// Ask for the smart playlists to be evaluated again.
    ///
    /// This is the manual refresh: the user pressed the button, so the
    /// policy does not get a say. Every cached result is dropped, so the
    /// next visit to any smart playlist runs its query again.
    pub fn request_smart_refresh(&mut self) {
        self.refresh_requested = true;
        // The visible list is rebuilt, or the view would keep showing the
        // result the refresh was supposed to replace.
        self.displayed_key = None;
    }

    /// Whether a timer-driven refresh is due.
    ///
    /// Under the timer policy the playlists are re-evaluated every few
    /// minutes; under the other two policies the timer says nothing.
    pub fn smart_refresh_due(&self) -> bool {
        if self.smart_refresh.policy != SmartRefreshPolicy::Interval {
            return false;
        }
        let now = now_secs();
        let interval = self.smart_refresh.interval_secs();
        self.smart_playlist_cache
            .values()
            .any(|snapshot| now - snapshot.evaluated_at >= interval)
    }

    /// The tracks a smart playlist matches, evaluated according to the policy.
    ///
    /// The query runs when the policy says it should — the library changed,
    /// the timer is due, or the user asked — and the previous result is
    /// served in between.
    pub fn smart_playlist_tracks(
        &mut self,
        repo: &SmartPlaylistRepo,
        sp: &SmartPlaylist,
        now: i64,
    ) -> Vec<Track> {
        if self.refresh_is_due(sp, now) {
            // Every cached result is dropped, not just this one: the library
            // changed, so every smart playlist's answer changed too.
            self.smart_playlist_cache.clear();
            self.library_changed = false;
            self.refresh_requested = false;
        }

        if let Some(snapshot) = self.smart_playlist_cache.get(&sp.id) {
            return snapshot.tracks.clone();
        }

        let tracks = match repo.evaluate(sp) {
            Ok(tracks) => tracks,
            Err(e) => {
                log::warn!("Could not run the smart playlist: {e}");
                Vec::new()
            }
        };
        self.smart_playlist_cache.insert(
            sp.id,
            SmartPlaylistSnapshot {
                evaluated_at: now,
                tracks: tracks.clone(),
            },
        );
        tracks
    }

    /// Whether this playlist's query is due to run again.
    fn refresh_is_due(&self, sp: &SmartPlaylist, now: i64) -> bool {
        if self.refresh_requested {
            return true;
        }
        match self.smart_refresh.policy {
            // The library changed: the answer may have changed with it.
            SmartRefreshPolicy::LibraryChange => self.library_changed,
            // The timer is the point of this policy; a library change also
            // invalidates the answer.
            SmartRefreshPolicy::Interval => {
                self.library_changed
                    || match self.smart_playlist_cache.get(&sp.id) {
                        Some(snapshot) => {
                            now - snapshot.evaluated_at >= self.smart_refresh.interval_secs()
                        }
                        None => true,
                    }
            }
            // The result is frozen until the user asks. The only exception is
            // a playlist that was never run: it has to run once to show
            // anything at all.
            SmartRefreshPolicy::Manual => !self.smart_playlist_cache.contains_key(&sp.id),
        }
    }

    /// Drop every cached smart-playlist result.
    ///
    /// Used when the policy itself changes: the next evaluation must follow
    /// the new rules, not the old ones.
    pub fn clear_smart_cache(&mut self) {
        self.smart_playlist_cache.clear();
        self.library_changed = true;
        self.refresh_requested = false;
    }

    /// The time of the last evaluation of a smart playlist, if any.
    pub fn smart_playlist_evaluated_at(&self, id: &PlaylistId) -> Option<i64> {
        self.smart_playlist_cache.get(id).map(|s| s.evaluated_at)
    }

    /// Materialise the tracks matched by a smart playlist (editor preview).
    pub fn evaluate_smart(&mut self, repo: &SmartPlaylistRepo, sp: &SmartPlaylist) {
        self.smart_playlist_tracks = repo.evaluate(sp).unwrap_or_default();
    }
}

//! Rebuilding the materialised library and the displayed track list.

use super::sort::apply_quick_filter;
use super::*;

impl LibraryState {
    /// Pre-aggregate album metadata and pick one track per album to stand in for cover art.
    fn rebuild_album_covers(&mut self) {
        self.album_analyzer = library::AlbumAnalyzer::analyze(&self.all_tracks);
        self.album_cover_tracks = self.album_analyzer.cover_tracks();
        if self.albums.is_empty() && !self.all_tracks.is_empty() {
            self.albums = self.album_analyzer.to_albums();
        }

        let mut artist_covers: HashMap<String, (TrackId, PathBuf)> =
            HashMap::with_capacity(self.artists.len());
        for track in &self.all_tracks {
            if track.artist.is_empty() {
                continue;
            }
            artist_covers
                .entry(track.artist.clone())
                .or_insert((track.id, track.path.clone()));
        }
        self.artist_cover_tracks = artist_covers;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reload(
        &mut self,
        track_repo: &TrackRepo,
        favorites_repo: &FavoritesRepo,
        playlist_repo: &PlaylistRepo,
        smart_repo: &SmartPlaylistRepo,
        album_repo: &AlbumRepo,
        artist_repo: &ArtistRepo,
        genre_repo: &GenreRepo,
        scan_roots_repo: &ScanRootsRepo,
        history_repo: &HistoryRepo,
        search_engine: &SearchEngine,
        active_nav: NavItem,
        search_query: &str,
        current_sort: SortField,
        sort_order: SortOrder,
    ) {
        if let Ok(tracks) = track_repo.list(
            domain::models::SortField::TrackNumber,
            domain::models::SortOrder::Ascending,
        ) {
            self.all_tracks = tracks;
        }
        // Selected ids may reference tracks that were just removed.
        self.prune_selection();

        if let Ok(favs) = favorites_repo.list() {
            self.favorite_ids = favs.into_iter().map(|t| t.id).collect();
        }

        if let Ok(pls) = playlist_repo.list() {
            self.playlists = pls;
        }

        if let Ok(sps) = smart_repo.list() {
            self.smart_playlists = sps;
        }

        if let Ok(albs) = album_repo.list() {
            self.albums = albs;
        }

        if let Ok(arts) = artist_repo.list() {
            self.artists = arts;
        }

        // The cover maps are built from the track list, but their capacity
        // hints want the album and artist lists, so build them after those.
        self.rebuild_album_covers();

        if let Ok(gnrs) = genre_repo.list() {
            self.genres = gnrs;
        }

        if let Ok(roots) = scan_roots_repo.list() {
            self.scan_roots = roots;
        }

        // Force a rebuild: underlying data changed.
        self.displayed_key = None;

        self.update_displayed_tracks(
            search_engine,
            history_repo,
            playlist_repo,
            smart_repo,
            album_repo,
            artist_repo,
            genre_repo,
            track_repo,
            active_nav,
            search_query,
            current_sort,
            sort_order,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_displayed_tracks(
        &mut self,
        search_engine: &SearchEngine,
        history_repo: &HistoryRepo,
        playlist_repo: &PlaylistRepo,
        smart_repo: &SmartPlaylistRepo,
        album_repo: &AlbumRepo,
        artist_repo: &ArtistRepo,
        genre_repo: &GenreRepo,
        track_repo: &TrackRepo,
        active_nav: NavItem,
        search_query: &str,
        current_sort: SortField,
        sort_order: SortOrder,
    ) {
        // Skip the rebuild (and the clone of up to 10k tracks) if nothing that
        // affects the visible list has changed.
        let key = (
            active_nav,
            search_query.to_string(),
            current_sort,
            sort_order,
            self.quick_filter,
        );
        if self.displayed_key.as_ref() == Some(&key) {
            return;
        }

        // Only the sort changed: the list in front of the user is the same
        // set of tracks in a different order, so it is sorted where it
        // lies.
        //
        // This is the difference between a sort that costs a copy of the
        // library and one that costs a sort. At 100,000 tracks the copy is
        // five hundred thousand string allocations — a visible hitch every
        // time the user clicks a column header.
        if let Some(previous) = self.displayed_key.as_ref() {
            let same_list = previous.0 == active_nav
                && previous.1 == search_query
                && previous.4 == self.quick_filter;
            let only_the_sort_changed =
                same_list && (previous.2 != current_sort || previous.3 != sort_order);
            if only_the_sort_changed {
                // Recently Added owns its running order: it is defined by
                // recency. For You is ranked by the engine, so a column header
                // must not reorder it either.
                if active_nav != NavItem::RecentlyAdded && self.quick_filter != QuickFilter::ForYou
                {
                    sort_tracks(&mut self.displayed_tracks, current_sort, sort_order);
                }
                self.displayed_key = Some(key);
                return;
            }
        }
        self.displayed_key = Some(key);
        self.rebuilds += 1;

        let mut tracks =
            if self.quick_filter == QuickFilter::ForYou && search_query.trim().is_empty() {
                // The feed is the whole view, not a narrowing of the current tab.
                self.search_result = search::SearchResult::default();
                self.recommended_tracks()
            } else if !search_query.trim().is_empty() {
                let result = search_engine.search(search_query, 100);
                self.search_result = result.clone();
                result.tracks
            } else {
                self.search_result = search::SearchResult::default();
                match active_nav {
                    NavItem::Favorites => {
                        let mut favs =
                            Vec::with_capacity(self.favorite_ids.len().min(self.all_tracks.len()));
                        for t in &self.all_tracks {
                            if self.favorite_ids.contains(&t.id) {
                                favs.push(t.clone());
                            }
                        }
                        favs
                    }
                    NavItem::RecentlyPlayed => match history_repo.list_recently_played(100) {
                        Ok(tracks) => tracks,
                        Err(e) => {
                            log::warn!("Could not load the recently played list: {e}");
                            Vec::new()
                        }
                    },
                    NavItem::MostPlayed => match history_repo.list_most_played(100) {
                        Ok(tracks) => tracks,
                        Err(e) => {
                            log::warn!("Could not load the most played list: {e}");
                            Vec::new()
                        }
                    },
                    // Newest imports first. Index sort avoids deep-cloning all 100k
                    // tracks just to keep 200, saving ~500k allocations on low-end CPUs.
                    NavItem::RecentlyAdded => {
                        let k = 200.min(self.all_tracks.len());
                        if k == 0 {
                            Vec::new()
                        } else {
                            let mut indices: Vec<usize> = (0..self.all_tracks.len()).collect();
                            if indices.len() > k {
                                indices.select_nth_unstable_by_key(k - 1, |&i| {
                                    std::cmp::Reverse(self.all_tracks[i].added_at)
                                });
                                indices.truncate(k);
                            }
                            indices.sort_unstable_by_key(|&i| {
                                std::cmp::Reverse(self.all_tracks[i].added_at)
                            });
                            indices
                                .into_iter()
                                .map(|i| self.all_tracks[i].clone())
                                .collect()
                        }
                    }
                    NavItem::Playlist(pl_id) => match playlist_repo.get_tracks_resolved(&pl_id) {
                        Ok(tracks) => tracks,
                        Err(e) => {
                            log::warn!("Could not load the playlist: {e}");
                            Vec::new()
                        }
                    },
                    NavItem::SmartPlaylist(sp_id) => {
                        let playlist = self.smart_playlists.iter().find(|sp| sp.id == sp_id);
                        match playlist {
                            // The policy decides whether the query runs now or
                            // the previous result is served.
                            Some(sp) => {
                                let sp = sp.clone();
                                self.smart_playlist_tracks(smart_repo, &sp, now_secs())
                            }
                            None => Vec::new(),
                        }
                    }
                    _ => self.all_tracks.clone(),
                }
            };

        // Quality/format chips narrow whatever the view produced. Safe on top
        // of search results too.
        apply_quick_filter(
            &mut tracks,
            &self.favorite_ids,
            &self.recommendation_ids,
            self.quick_filter,
        );

        // "Recently Added" defines itself by recency, so the user's global sort
        // would fight the view. For You keeps the engine's ranking.
        if active_nav != NavItem::RecentlyAdded && self.quick_filter != QuickFilter::ForYou {
            sort_tracks(&mut tracks, current_sort, sort_order);
        }

        self.displayed_tracks = tracks;

        // Update drilldown views
        if let Some((ref title, ref artist)) = self.selected_album {
            self.album_tracks = album_repo
                .get_tracks_by_album(title, artist)
                .unwrap_or_default();
        }
        if let Some(ref artist) = self.selected_artist {
            self.artist_tracks = artist_repo.get_tracks_by_artist(artist).unwrap_or_default();
        }
        if let Some(ref genre) = self.selected_genre {
            self.genre_tracks = genre_repo.get_tracks_by_genre(genre).unwrap_or_default();
        }
        if let Some(ref folder) = self.selected_folder {
            self.folder_tracks = track_repo.get_tracks_by_folder(folder).unwrap_or_default();
        }
    }
}

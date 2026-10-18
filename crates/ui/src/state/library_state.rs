use std::collections::HashSet;
use std::path::PathBuf;

use database::{
    AlbumRepo, ArtistRepo, FavoritesRepo, GenreRepo, HistoryRepo, PlaylistRepo, ScanRootsRepo,
    SmartPlaylistRepo, TrackRepo,
};
use domain::ids::TrackId;
use domain::models::{Album, Artist, Genre, Playlist, ScanRoot, Track};
use domain::smart::SmartPlaylist;
use search::SearchEngine;

use crate::components::sidebar::NavItem;
use crate::components::songs_table::SortField;

/// State store managing music tracks, collections, playlists, and drilldown views
pub struct LibraryState {
    pub all_tracks: Vec<Track>,
    pub displayed_tracks: Vec<Track>,
    pub favorite_ids: HashSet<TrackId>,

    pub playlists: Vec<Playlist>,
    pub smart_playlists: Vec<SmartPlaylist>,
    /// Tracks matched by the smart playlist currently open in the editor.
    pub smart_playlist_tracks: Vec<Track>,
    /// Multi-selection for the songs table (survives tab switches).
    pub selected_track_ids: HashSet<TrackId>,
    /// Anchor row for shift-range selection.
    pub last_clicked_index: Option<usize>,
    pub albums: Vec<Album>,
    pub selected_album: Option<(String, String)>,
    pub album_tracks: Vec<Track>,

    pub artists: Vec<Artist>,
    pub selected_artist: Option<String>,
    pub artist_tracks: Vec<Track>,

    pub genres: Vec<Genre>,
    pub selected_genre: Option<String>,
    pub genre_tracks: Vec<Track>,

    pub scan_roots: Vec<ScanRoot>,
    pub selected_folder: Option<PathBuf>,
    pub folder_tracks: Vec<Track>,
    pub new_folder_input: String,

    /// Identity of the currently materialised `displayed_tracks`. Used to skip
    /// needless rebuilds (and the associated 10k-track clones) when the user
    /// bounces between tabs.
    displayed_key: Option<(NavItem, String, SortField)>,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            all_tracks: Vec::new(),
            displayed_tracks: Vec::new(),
            favorite_ids: HashSet::new(),

            playlists: Vec::new(),
            smart_playlists: Vec::new(),
            smart_playlist_tracks: Vec::new(),
            selected_track_ids: HashSet::new(),
            last_clicked_index: None,
            albums: Vec::new(),
            selected_album: None,
            album_tracks: Vec::new(),

            artists: Vec::new(),
            selected_artist: None,
            artist_tracks: Vec::new(),

            genres: Vec::new(),
            selected_genre: None,
            genre_tracks: Vec::new(),

            scan_roots: Vec::new(),
            selected_folder: None,
            folder_tracks: Vec::new(),
            new_folder_input: String::new(),

            displayed_key: None,
        }
    }
}

impl LibraryState {
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
        );
    }

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
    ) {
        // Skip the rebuild (and the clone of up to 10k tracks) if nothing that
        // affects the visible list has changed.
        let key = (active_nav, search_query.to_string(), current_sort);
        if self.displayed_key.as_ref() == Some(&key) {
            return;
        }
        self.displayed_key = Some(key);

        let mut tracks = if !search_query.trim().is_empty() {
            search_engine.search(search_query, 100).tracks
        } else {
            match active_nav {
                NavItem::Favorites => self
                    .all_tracks
                    .iter()
                    .filter(|t| self.favorite_ids.contains(&t.id))
                    .cloned()
                    .collect(),
                NavItem::RecentlyPlayed => {
                    history_repo.list_recently_played(100).unwrap_or_default()
                }
                NavItem::MostPlayed => history_repo.list_most_played(100).unwrap_or_default(),
                NavItem::HiRes => self
                    .all_tracks
                    .iter()
                    .filter(|t| t.sample_rate > 48000 || t.bitrate_kbps.unwrap_or(0) > 1000)
                    .cloned()
                    .collect(),
                NavItem::Lossless => self
                    .all_tracks
                    .iter()
                    .filter(|t| t.is_lossless)
                    .cloned()
                    .collect(),
                NavItem::Playlist(pl_id) => {
                    playlist_repo.get_tracks(&pl_id).unwrap_or_default()
                }
                NavItem::SmartPlaylist(sp_id) => self
                    .smart_playlists
                    .iter()
                    .find(|sp| sp.id == sp_id)
                    .and_then(|sp| smart_repo.evaluate(sp).ok())
                    .unwrap_or_default(),
                _ => self.all_tracks.clone(),
            }
        };

        match current_sort {
            SortField::Default => tracks.sort_by_key(|t| t.track_number.unwrap_or(0)),
            SortField::Title => {
                tracks.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            }
            SortField::Artist => {
                tracks.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
            }
            SortField::Album => tracks.sort_by(|a, b| a.album.cmp(&b.album)),
            SortField::Duration => tracks.sort_by(|a, b| a.duration_secs.total_cmp(&b.duration_secs)),
            SortField::DateAdded => tracks.sort_by_key(|t| t.added_at),
        }

        self.displayed_tracks = tracks;

        // Update drilldown collections
        if let Some((ref title, ref artist)) = self.selected_album {
            self.album_tracks = album_repo.get_tracks_by_album(title, artist).unwrap_or_default();
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

    /// Materialise the tracks matched by a smart playlist (editor preview).
    pub fn evaluate_smart(&mut self, repo: &SmartPlaylistRepo, sp: &SmartPlaylist) {
        self.smart_playlist_tracks = repo.evaluate(sp).unwrap_or_default();
    }

    /// Mark a track as favourite (idempotent; used by bulk actions).
    pub fn add_favorite(&mut self, favorites_repo: &FavoritesRepo, track_id: TrackId) {
        if !self.favorite_ids.contains(&track_id) {
            let _ = favorites_repo.add(&track_id);
            self.favorite_ids.insert(track_id);
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

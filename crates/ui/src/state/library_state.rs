use std::collections::{HashMap, HashSet};
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
use crate::components::songs_table::QuickFilter;
use domain::models::{SortField, SortOrder};

/// State store managing music tracks, collections, playlists, and drilldown views
#[derive(Default)]
pub struct LibraryState {
    pub all_tracks: Vec<Track>,
    pub displayed_tracks: Vec<Track>,
    /// Non-track matches from the latest query, used for the grouped search
    /// strip above the table.
    pub search_result: search::SearchResult,
    pub favorite_ids: HashSet<TrackId>,

    pub playlists: Vec<Playlist>,
    pub smart_playlists: Vec<SmartPlaylist>,
    /// Tracks matched by the smart playlist currently open in the editor.
    pub smart_playlist_tracks: Vec<Track>,
    /// Multi-selection for the songs table (survives tab switches).
    pub selected_track_ids: HashSet<TrackId>,
    /// Anchor row for shift-range selection.
    pub last_clicked_index: Option<usize>,
    /// Quality/format chips narrowing the current view.
    pub quick_filter: QuickFilter,
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
    displayed_key: Option<(NavItem, String, SortField, SortOrder, QuickFilter)>,

    /// A representative track for each album, used to show the cover art in
    /// the album grid. The grid used to paint a permanent placeholder disc,
    /// because artwork is cached per *track* and the grid only knows the
    /// album.
    pub album_cover_tracks: HashMap<(String, String), (TrackId, PathBuf)>,
    /// A representative track for each artist, used to show a cover in the
    /// artist grid.
    pub artist_cover_tracks: HashMap<String, (TrackId, PathBuf)>,
}

impl LibraryState {
    /// Pick one track per album to stand in for its cover art.
    ///
    /// Artwork is cached per track, so the album grid needs to know which
    /// track stands in for each album. The first track of an album wins, which
    /// is the one the album's own detail view uses too.
    fn rebuild_album_covers(&mut self) {
        let mut covers: HashMap<(String, String), (TrackId, PathBuf)> =
            HashMap::with_capacity(self.albums.len());
        for track in &self.all_tracks {
            if track.album.is_empty() {
                continue;
            }
            let key = (track.album.clone(), track.artist.clone());
            covers.entry(key).or_insert((track.id, track.path.clone()));
        }
        self.album_cover_tracks = covers;

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
        self.rebuild_album_covers();

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
        self.displayed_key = Some(key);

        let mut tracks = if !search_query.trim().is_empty() {
            let result = search_engine.search(search_query, 100);
            self.search_result = result.clone();
            result.tracks
        } else {
            self.search_result = search::SearchResult::default();
            match active_nav {
                NavItem::Favorites => self
                    .all_tracks
                    .iter()
                    .filter(|t| self.favorite_ids.contains(&t.id))
                    .cloned()
                    .collect(),
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
                // Newest imports first. `all_tracks` is already in memory, so
                // this is a sort rather than another query.
                NavItem::RecentlyAdded => {
                    let mut recent = self.all_tracks.clone();
                    recent.sort_by_key(|t| std::cmp::Reverse(t.added_at));
                    recent.truncate(200);
                    recent
                }
                NavItem::Playlist(pl_id) => match playlist_repo.get_tracks(&pl_id) {
                    Ok(tracks) => tracks,
                    Err(e) => {
                        log::warn!("Could not load the playlist: {e}");
                        Vec::new()
                    }
                },
                NavItem::SmartPlaylist(sp_id) => {
                    let playlist = self.smart_playlists.iter().find(|sp| sp.id == sp_id);
                    match playlist {
                        Some(sp) => match smart_repo.evaluate(sp) {
                            Ok(tracks) => tracks,
                            Err(e) => {
                                log::warn!("Could not run the smart playlist: {e}");
                                Vec::new()
                            }
                        },
                        None => Vec::new(),
                    }
                }
                _ => self.all_tracks.clone(),
            }
        };

        // Quality/format chips narrow whatever the view produced. Safe on top
        // of search results too.
        apply_quick_filter(&mut tracks, &self.favorite_ids, self.quick_filter);

        // "Recently Added" defines itself by recency, so the user's global sort
        // would fight the view. Everything else honours the chosen sort.
        if active_nav != NavItem::RecentlyAdded {
            sort_tracks(&mut tracks, current_sort, sort_order);
        }

        self.displayed_tracks = tracks;

        // Update drilldown collections
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

    /// Mark a track as favorite (idempotent; used by bulk actions).
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

/// Sort a materialised list by `field` and `order`.
///
/// Keys are case-folded once per track instead of twice per comparison, which
/// used to allocate two `String`s on every sort step. `Album` used to compare
/// case-*sensitively* while `Title` and `Artist` did not, so the same list could
/// order differently depending only on which column was chosen.
/// Apply the active quality/format chip to a materialised track list.
fn apply_quick_filter(
    tracks: &mut Vec<Track>,
    favorite_ids: &HashSet<TrackId>,
    filter: QuickFilter,
) {
    match filter {
        QuickFilter::All => {}
        QuickFilter::Lossless => tracks.retain(|t| t.is_lossless),
        QuickFilter::HiRes => tracks.retain(|t| t.is_hires()),
        QuickFilter::Favorites => tracks.retain(|t| favorite_ids.contains(&t.id)),
        QuickFilter::Played => tracks.retain(|t| t.play_count > 0),
        QuickFilter::Rating => tracks.retain(|t| t.rating > 0),
    }
}

pub fn sort_tracks(tracks: &mut [Track], field: SortField, order: SortOrder) {
    match field {
        SortField::Title => tracks.sort_by_cached_key(|t| t.title.to_lowercase()),
        SortField::Artist => tracks.sort_by_cached_key(|t| {
            (
                t.artist.to_lowercase(),
                t.album.to_lowercase(),
                t.track_number.unwrap_or(0),
            )
        }),
        SortField::Album => tracks.sort_by_cached_key(|t| {
            (
                t.album.to_lowercase(),
                t.disc_number.unwrap_or(1),
                t.track_number.unwrap_or(0),
            )
        }),
        SortField::Duration => tracks.sort_by(|a, b| a.duration_secs.total_cmp(&b.duration_secs)),
        SortField::TrackNumber => tracks.sort_by_key(|t| {
            (
                t.disc_number.unwrap_or(1),
                t.track_number.unwrap_or(0),
                t.title.to_lowercase(),
            )
        }),
        SortField::DateAdded => tracks.sort_by_key(|t| t.added_at),
        // Favorites first regardless of direction: "sort by favorites" with
        // the least-favorite track at the top would be a strange reading.
        SortField::Favorite => {
            tracks.sort_by_key(|t| std::cmp::Reverse(t.is_favorite));
            return;
        }
        SortField::Rating => tracks.sort_by_key(|t| t.rating),
    }
    if order == SortOrder::Descending {
        tracks.reverse();
    }
}

#[cfg(test)]
mod sort_tests {
    use super::*;
    use domain::ids::TrackId;
    use std::path::PathBuf;

    fn track(title: &str, artist: &str, album: &str, favorite: bool, duration: f32) -> Track {
        Track {
            id: TrackId::new(),
            path: PathBuf::from("/music/x.flac"),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: None,
            year: None,
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: duration,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            bit_depth: None,
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 0,
            mtime_unix_secs: 0,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: favorite,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 0,
        }
    }

    fn titles(tracks: &[Track]) -> Vec<&str> {
        tracks.iter().map(|t| t.title.as_str()).collect()
    }

    #[test]
    fn sorting_is_case_insensitive_and_direction_aware() {
        let mut tracks = vec![
            track("banana", "Zed", "Beta", false, 3.0),
            track("Apple", "alpha", "alpha", false, 1.0),
            track("cherry", "Beta", "Gamma", false, 2.0),
        ];

        sort_tracks(&mut tracks, SortField::Title, SortOrder::Ascending);
        assert_eq!(titles(&tracks), ["Apple", "banana", "cherry"]);

        sort_tracks(&mut tracks, SortField::Title, SortOrder::Descending);
        assert_eq!(titles(&tracks), ["cherry", "banana", "Apple"]);

        sort_tracks(&mut tracks, SortField::Artist, SortOrder::Ascending);
        assert_eq!(titles(&tracks), ["Apple", "cherry", "banana"]);

        // Album used to compare case-sensitively, which put `Beta` before
        // `alpha` even though `alpha` comes first alphabetically.
        sort_tracks(&mut tracks, SortField::Album, SortOrder::Ascending);
        assert_eq!(titles(&tracks), ["Apple", "banana", "cherry"]);

        sort_tracks(&mut tracks, SortField::Duration, SortOrder::Ascending);
        assert_eq!(titles(&tracks), ["Apple", "cherry", "banana"]);
    }

    #[test]
    fn favorites_lead_in_both_directions() {
        let mut tracks = vec![
            track("one", "A", "A", false, 1.0),
            track("two", "A", "A", true, 1.0),
            track("three", "A", "A", false, 1.0),
        ];
        sort_tracks(&mut tracks, SortField::Favorite, SortOrder::Ascending);
        assert_eq!(tracks[0].title, "two");
        sort_tracks(&mut tracks, SortField::Favorite, SortOrder::Descending);
        assert_eq!(tracks[0].title, "two", "a favorite is never last");
    }

    #[test]
    fn rating_sort_puts_five_stars_first_when_descending() {
        let mut tracks = vec![
            track("one", "A", "A", false, 1.0),
            track("two", "A", "A", false, 1.0),
            track("three", "A", "A", false, 1.0),
        ];
        tracks[0].rating = 2;
        tracks[1].rating = 5;
        // tracks[2] is deliberately unrated.
        sort_tracks(&mut tracks, SortField::Rating, SortOrder::Descending);
        assert_eq!(titles(&tracks), ["two", "one", "three"]);
        sort_tracks(&mut tracks, SortField::Rating, SortOrder::Ascending);
        assert_eq!(titles(&tracks), ["three", "one", "two"]);
    }

    #[test]
    fn the_rating_chip_hides_unrated_tracks() {
        let mut tracks = vec![
            track("one", "A", "A", false, 1.0),
            track("two", "A", "A", false, 1.0),
            track("three", "A", "A", false, 1.0),
        ];
        tracks[1].rating = 3;
        let favorites = HashSet::new();
        apply_quick_filter(&mut tracks, &favorites, QuickFilter::Rating);
        assert_eq!(titles(&tracks), ["two"]);
        assert!(tracks[0].rating > 0);
    }
}

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use database::{
    AlbumRepo, ArtistRepo, FavoritesRepo, GenreRepo, HistoryRepo, PlaylistRepo, ScanRootsRepo,
    SmartPlaylistRepo, TrackRepo,
};
use domain::ids::TrackId;
use domain::models::{Album, Artist, Genre, Playlist, ScanRoot, Track};
use domain::smart::{SmartPlaylist, SmartRefreshPolicy, SmartRefreshSettings};
use search::SearchEngine;

use crate::components::sidebar::NavItem;
use crate::components::songs_table::QuickFilter;
use domain::ids::PlaylistId;
use domain::models::{SortField, SortOrder};

/// The current time, in seconds since the epoch.
///
/// The refresh policy measures intervals in seconds, and the clock is only
/// needed when a smart playlist is actually consulted.
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// One smart playlist's last evaluation.
#[derive(Debug, Clone)]
pub struct SmartPlaylistSnapshot {
    /// When the query ran, in seconds since the epoch.
    pub evaluated_at: i64,
    /// The tracks it matched.
    pub tracks: Vec<Track>,
}

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
    /// When the smart playlists are re-evaluated.
    ///
    /// The policy is a preference, not a fact about the library, so it lives
    /// in the state the app pushes the saved value into.
    pub smart_refresh: SmartRefreshSettings,
    /// The last evaluation of each smart playlist.
    ///
    /// A smart playlist is a query over the library, and a query over a large
    /// library is not free. Under the timer and manual policies the result is
    /// kept, and the query runs again only when the policy says so.
    smart_playlist_cache: HashMap<PlaylistId, SmartPlaylistSnapshot>,
    /// The library changed since the cache was filled.
    library_changed: bool,
    /// The user asked for a refresh; it happens on the next evaluation.
    refresh_requested: bool,
    /// Multi-selection for the songs table (survives tab switches).
    pub selected_track_ids: HashSet<TrackId>,
    /// Anchor row for shift-range selection.
    pub last_clicked_index: Option<usize>,
    /// Quality/format chips narrowing the current view.
    pub quick_filter: QuickFilter,
    pub albums: Vec<Album>,
    pub selected_album: Option<(String, String)>,
    pub album_tracks: Vec<Track>,
    /// Markdown liner notes for the album open in the detail view.
    pub album_notes: crate::state::AlbumNotesState,

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

    /// The track the current local radio station was seeded from.
    pub radio_seed: Option<TrackId>,
    /// A human-readable station name, e.g. "Boards of Canada Radio".
    pub radio_label: String,
    /// The station's current running order. Kept separately from
    /// `displayed_tracks` so leaving and returning to Radio does not re-query
    /// or reshuffle the station.
    pub radio_tracks: Vec<Track>,

    /// The recommendation engine's pending list, in ranking order.
    ///
    /// The worker owns the rolling session; this is only the id projection the
    /// table needs. For You renders it in this exact order rather than applying
    /// the user's global sort, the same way Radio owns its shuffle.
    pub recommendation_ids: Vec<TrackId>,

    /// Identity of the currently materialised `displayed_tracks`. Used to skip
    /// needless rebuilds (and the associated 10k-track clones) when the user
    /// bounces between tabs.
    displayed_key: Option<(NavItem, String, SortField, SortOrder, QuickFilter)>,

    /// How many times the visible list was built from scratch.
    ///
    /// A *sort* does not count: changing the sort re-orders the list that is
    /// already there. This counter is what the library budget test watches —
    /// a rebuild that came back would show up here.
    pub rebuilds: u64,

    /// A representative track for each album, used to show the cover art in
    /// the album grid.
    pub album_cover_tracks: HashMap<(String, String), (TrackId, PathBuf)>,
    /// Pre-aggregated in-memory album index for $O(1)$ album metadata reads.
    pub album_analyzer: library::AlbumAnalyzer,
    /// A representative track for each artist, used to show a cover in the
    /// artist grid.
    pub artist_cover_tracks: HashMap<String, (TrackId, PathBuf)>,
}

mod radio;
mod reload;
mod selection;
mod smart;
mod sort;

pub use sort::sort_tracks;

#[cfg(test)]
mod smart_refresh_tests;
#[cfg(test)]
mod sort_tests;

use database::{AlbumRepo, ArtistRepo, Database, PlaylistRepo, TrackRepo};
use domain::models::{Playlist, Track};

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub artists: Vec<String>,
    pub albums: Vec<String>,
}

impl SearchResult {
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
            && self.playlists.is_empty()
            && self.artists.is_empty()
            && self.albums.is_empty()
    }
}

pub struct SearchEngine {
    track_repo: TrackRepo,
    playlist_repo: PlaylistRepo,
    artist_repo: ArtistRepo,
    album_repo: AlbumRepo,
}

impl SearchEngine {
    pub fn new(db: Database) -> Self {
        Self {
            track_repo: TrackRepo::new(db.clone()),
            playlist_repo: PlaylistRepo::new(db.clone()),
            artist_repo: ArtistRepo::new(db.clone()),
            album_repo: AlbumRepo::new(db),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> SearchResult {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return SearchResult::default();
        }

        // A failed query and an empty result must not look the same.
        let tracks = match self.track_repo.search_fts(trimmed, limit) {
            Ok(tracks) => tracks,
            Err(e) => {
                log::warn!("Search failed for {trimmed:?}: {e}");
                Vec::new()
            }
        };

        // Artists, albums and playlists are queried in their own right. Deriving
        // them from `tracks` meant an artist whose songs ranked past `limit`
        // never showed up in the search at all.
        let artists = match self.artist_repo.search(trimmed, limit) {
            Ok(artists) => artists,
            Err(e) => {
                log::warn!("Artist search failed for {trimmed:?}: {e}");
                Vec::new()
            }
        };
        let albums = match self.album_repo.search(trimmed, limit) {
            Ok(albums) => albums,
            Err(e) => {
                log::warn!("Album search failed for {trimmed:?}: {e}");
                Vec::new()
            }
        };
        let playlists = match self.playlist_repo.search(trimmed, limit) {
            Ok(playlists) => playlists,
            Err(e) => {
                log::warn!("Playlist search failed for {trimmed:?}: {e}");
                Vec::new()
            }
        };

        SearchResult {
            tracks,
            playlists,
            artists,
            albums,
        }
    }
}

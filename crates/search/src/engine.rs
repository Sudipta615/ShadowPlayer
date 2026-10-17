use database::{Database, PlaylistRepo, TrackRepo};
use domain::models::{Playlist, Track};
use std::collections::HashSet;

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
}

impl SearchEngine {
    pub fn new(db: Database) -> Self {
        Self {
            track_repo: TrackRepo::new(db.clone()),
            playlist_repo: PlaylistRepo::new(db),
        }
    }

    pub fn search(&self, query: &str, limit: usize) -> SearchResult {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return SearchResult::default();
        }

        let tracks = self.track_repo.search_fts(trimmed, limit).unwrap_or_default();

        let mut artists_set = HashSet::new();
        let mut albums_set = HashSet::new();
        let query_lower = trimmed.to_lowercase();

        for t in &tracks {
            if t.artist.to_lowercase().contains(&query_lower) {
                artists_set.insert(t.artist.clone());
            }
            if t.album.to_lowercase().contains(&query_lower) {
                albums_set.insert(t.album.clone());
            }
        }

        let all_playlists = self.playlist_repo.list().unwrap_or_default();
        let playlists: Vec<Playlist> = all_playlists
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect();

        SearchResult {
            tracks,
            playlists,
            artists: artists_set.into_iter().collect(),
            albums: albums_set.into_iter().collect(),
        }
    }
}

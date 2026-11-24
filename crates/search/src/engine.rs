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

/// In-memory search over the library. No database access (AGENTS.md §1).
#[derive(Debug, Clone, Default)]
pub struct SearchEngine;

impl SearchEngine {
    pub fn new() -> Self {
        Self
    }

    pub fn search(
        &self,
        query: &str,
        tracks: &[Track],
        playlists: &[Playlist],
        limit: usize,
    ) -> SearchResult {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return SearchResult::default();
        }

        // Tokenise like the old FTS path: whitespace-separated terms, each
        // term's alphanumeric runs lowercased and ANDed. A term with no
        // alphanumeric run is dropped; if nothing survives, return nothing.
        let terms: Vec<Vec<String>> = trimmed
            .split_whitespace()
            .map(|term| {
                term.split(|c: char| !c.is_alphanumeric())
                    .filter(|t| !t.is_empty())
                    .map(|t| t.to_lowercase())
                    .collect()
            })
            .filter(|t: &Vec<String>| !t.is_empty())
            .collect();
        if terms.is_empty() {
            return SearchResult::default();
        }

        let matches_terms = |text: &str| -> bool {
            let haystack = text.to_lowercase();
            terms.iter().all(|term_tokens| {
                // Each whitespace term must have all its alphanumeric runs
                // present, in order, as substrings.
                let mut pos = 0;
                term_tokens.iter().all(|tok| {
                    if let Some(idx) = haystack[pos..].find(tok) {
                        pos += idx + tok.len();
                        true
                    } else {
                        false
                    }
                })
            })
        };

        // Tracks: match across the text fields, ranked by simple title-first
        // then input order, truncated to `limit`.
        let matched_tracks: Vec<Track> = tracks
            .iter()
            .filter(|t| {
                matches_terms(&t.title)
                    || matches_terms(&t.artist)
                    || matches_terms(&t.album)
                    || t.genre.as_deref().map(matches_terms).unwrap_or(false)
            })
            .cloned()
            .take(limit)
            .collect();

        // Artists: distinct, containing the query.
        let mut artists: Vec<String> = tracks
            .iter()
            .map(|t| t.artist.clone())
            .filter(|a| !a.is_empty() && matches_terms(a))
            .collect();
        artists.sort();
        artists.dedup();

        // Albums: distinct.
        let mut albums: Vec<String> = tracks
            .iter()
            .map(|t| t.album.clone())
            .filter(|a| !a.is_empty() && matches_terms(a))
            .collect();
        albums.sort();
        albums.dedup();

        // Playlists by name.
        let matched_playlists: Vec<Playlist> = playlists
            .iter()
            .filter(|p| matches_terms(&p.name))
            .cloned()
            .collect();

        SearchResult {
            tracks: matched_tracks,
            playlists: matched_playlists,
            artists,
            albums,
        }
    }
}
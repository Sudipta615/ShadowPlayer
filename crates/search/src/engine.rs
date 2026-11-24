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

        // Tokenise like the FTS query this replaces: whitespace-separated
        // terms, each term's alphanumeric runs lowercased and ANDed. A term
        // with no alphanumeric run is dropped; if nothing survives, return
        // nothing.
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

        // A term may match any of the columns the ``tracks_fts`` row indexed,
        // and different terms may match different columns — a search for
        // "miles kind" still finds Miles Davis' *Kind of Blue*. The path is
        // included because folder and file names were indexed too.
        let text_matches = |fields: &[&str]| -> bool {
            terms
                .iter()
                .all(|tokens| fields.iter().any(|field| term_in_field(field, tokens)))
        };

        // Tracks: a title hit ranks above a hit in another field, then input
        // order, and only the first `limit` are cloned.
        let mut matched: Vec<(bool, &Track)> = tracks
            .iter()
            .filter_map(|track| {
                let genre = track.genre.as_deref().unwrap_or("");
                let path = track.path.to_string_lossy();
                let fields = [
                    track.title.as_str(),
                    track.artist.as_str(),
                    track.album.as_str(),
                    genre,
                    path.as_ref(),
                ];
                if !text_matches(&fields) {
                    return None;
                }
                Some((text_matches(&[track.title.as_str()]), track))
            })
            .collect();
        matched.sort_by_key(|(title_match, _)| !*title_match);
        let matched_tracks: Vec<Track> = matched
            .into_iter()
            .take(limit)
            .map(|(_, track)| track.clone())
            .collect();

        // Artists: distinct, containing every term. Capped like the old
        // per-repository queries, so a one-letter query cannot produce a
        // result list the size of the library.
        let mut artists: Vec<String> = tracks
            .iter()
            .map(|t| t.artist.clone())
            .filter(|a| !a.is_empty() && text_matches(&[a]))
            .collect();
        artists.sort();
        artists.dedup();
        artists.truncate(limit);

        // Albums: distinct, same cap.
        let mut albums: Vec<String> = tracks
            .iter()
            .map(|t| t.album.clone())
            .filter(|a| !a.is_empty() && text_matches(&[a]))
            .collect();
        albums.sort();
        albums.dedup();
        albums.truncate(limit);

        // Playlists by name.
        let matched_playlists: Vec<Playlist> = playlists
            .iter()
            .filter(|p| text_matches(&[&p.name]))
            .take(limit)
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

/// Whether every token of a whitespace term appears in `field`, in order.
fn term_in_field(field: &str, tokens: &[String]) -> bool {
    let haystack = field.to_lowercase();
    let mut pos = 0;
    tokens.iter().all(|token| {
        if let Some(index) = haystack[pos..].find(token) {
            pos += index + token.len();
            true
        } else {
            false
        }
    })
}

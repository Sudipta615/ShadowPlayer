use std::collections::BTreeSet;

use domain::models::{Playlist, Track};

/// Bitmask of ASCII letters present in a lowercased string: bit (c - 'a') is
/// set when the string contains that letter. Used to make single-letter
/// queries O(1) per track.
fn ascii_mask(s: &str) -> u32 {
    let mut mask = 0u32;
    for b in s.bytes() {
        if b.is_ascii_lowercase() {
            mask |= 1u32 << (b - b'a');
        }
    }
    mask
}

fn contains_token(field: &str, mask: u32, token: &str) -> bool {
    // Fast reject: every ASCII letter in the token must be present in the
    // field. This avoids an expensive substring search for most misses.
    let mut required = 0u32;
    for b in token.bytes() {
        if b.is_ascii_lowercase() {
            required |= 1u32 << (b - b'a');
        }
    }
    if required != 0 && (mask & required) != required {
        return false;
    }
    if token.len() == 1 {
        let b = token.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return (mask >> (b - b'a')) & 1 == 1;
        }
    }
    field.contains(token)
}

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
        SearchIndex::build(tracks).search(query, playlists, limit)
    }
}

/// A library pre-lowercased once, so a per-keystroke search never allocates
/// or re-folds a tag. `LibraryState` keeps one of these and rebuilds it only
/// when the library changes.
#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    entries: Vec<IndexedTrack>,
    distinct_artists: Vec<String>,
    distinct_albums: Vec<String>,
}

#[derive(Debug, Clone)]
struct IndexedTrack {
    track: Track,
    title: String,
    title_mask: u32,
    haystack: String,
    haystack_mask: u32,
}

impl SearchIndex {
    pub fn build(tracks: &[Track]) -> Self {
        let entries = tracks
            .iter()
            .map(|track| {
                let title = track.title.to_lowercase();
                let artist = track.artist.to_lowercase();
                let album = track.album.to_lowercase();
                let genre = track.genre.as_deref().unwrap_or("").to_lowercase();
                let path = track.path.to_string_lossy().to_lowercase();
                // Combined haystack used for the cheap "does this track match
                // at all" test; one contains per term instead of five.
                let haystack = format!("{title} {artist} {album} {genre} {path}");
                IndexedTrack {
                    title_mask: ascii_mask(&title),
                    title,
                    haystack_mask: ascii_mask(&haystack),
                    haystack,
                    track: track.clone(),
                }
            })
            .collect();
        // Unique artist/album names, so side lists are matched over the
        // distinct set instead of once per track.
        let mut artists: BTreeSet<String> = BTreeSet::new();
        let mut albums: BTreeSet<String> = BTreeSet::new();
        for track in tracks {
            if !track.artist.is_empty() {
                artists.insert(track.artist.clone());
            }
            if !track.album.is_empty() {
                albums.insert(track.album.clone());
            }
        }
        Self {
            entries,
            distinct_artists: artists.into_iter().collect(),
            distinct_albums: albums.into_iter().collect(),
        }
    }

    pub fn search(&self, query: &str, playlists: &[Playlist], limit: usize) -> SearchResult {
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
        } // term_in checks that every token of a term appears (in order, as
          // substrings) in the field. The haystacks are already lowercased.
          // Single ASCII letters are tested against a precomputed bitmask,
          // which is much cheaper than a substring search.
        let term_in = |field: &str, mask: u32, tokens: &[String]| -> bool {
            tokens
                .iter()
                .all(|token| contains_token(field, mask, token))
        };
        let all_terms_in = |field: &str, mask: u32| -> bool {
            terms.iter().all(|tokens| term_in(field, mask, tokens))
        };

        let mut title_matches: Vec<&Track> = Vec::new();
        let mut other_matches: Vec<&Track> = Vec::new();

        for entry in &self.entries {
            // Cheap combined test first: one contains per term.
            let combined_hit = terms
                .iter()
                .all(|tokens| term_in(&entry.haystack, entry.haystack_mask, tokens));
            if !combined_hit {
                continue;
            }

            let title_hit = all_terms_in(&entry.title, entry.title_mask);

            if title_hit {
                if title_matches.len() < limit {
                    title_matches.push(&entry.track);
                }
            } else if other_matches.len() < limit {
                other_matches.push(&entry.track);
            }
        }

        // Side lists matched over distinct names only.
        let matched_artists: Vec<String> = self
            .distinct_artists
            .iter()
            .filter(|name| {
                let lower = name.to_lowercase();
                all_terms_in(&lower, ascii_mask(&lower))
            })
            .take(limit)
            .cloned()
            .collect();
        let matched_albums: Vec<String> = self
            .distinct_albums
            .iter()
            .filter(|name| {
                let lower = name.to_lowercase();
                all_terms_in(&lower, ascii_mask(&lower))
            })
            .take(limit)
            .cloned()
            .collect();

        let matched_tracks: Vec<Track> = title_matches
            .into_iter()
            .chain(other_matches)
            .take(limit)
            .cloned()
            .collect();
        let matched_playlists: Vec<Playlist> = playlists
            .iter()
            .filter(|p| {
                let name = p.name.to_lowercase();
                terms
                    .iter()
                    .all(|tokens| tokens.iter().all(|token| name.contains(token.as_str())))
            })
            .take(limit)
            .cloned()
            .collect();

        SearchResult {
            tracks: matched_tracks,
            playlists: matched_playlists,
            artists: matched_artists,
            albums: matched_albums,
        }
    }
}

//! In-memory pre-aggregated album index (`AlbumAnalyzer`).
//!
//! Replaces repeated per-frame SQLite queries and $O(N)$ track scans with
//! an in-memory aggregation index over all tracks in `LibraryState`.
//!
//! Inspired by the JRiver Media Center `AlbumAnalyzer` pattern:
//! Pre-aggregates album metadata in-memory into `(AlbumArtist, Album)` records
//! during library load and updates so grid and detail views require $O(1)$ reads.

use std::collections::HashMap;
use std::path::PathBuf;

use domain::ids::{AlbumId, TrackId};
use domain::models::{Album, Track};

/// Pre-aggregated metadata summary for a single album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumSummary {
    pub artist: String,
    pub album: String,
    pub track_count: usize,
    pub total_duration_secs: u64,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub max_sample_rate: u32,
    pub max_bit_depth: u8,
    pub is_hires: bool,
    pub has_lossless: bool,
    pub cover_track_id: Option<TrackId>,
    pub cover_track_path: Option<PathBuf>,
}

impl AlbumSummary {
    /// The stable album identity.
    pub fn id(&self) -> AlbumId {
        AlbumId::from_identity(&self.album, &self.artist)
    }

    /// Convert into a `domain::models::Album` suitable for UI views.
    pub fn to_album(&self) -> Album {
        Album {
            id: self.id(),
            title: self.album.clone(),
            artist: self.artist.clone(),
            year: self.year_min.and_then(|y| u32::try_from(y).ok()),
            track_count: self.track_count as u32,
            artwork_path: self.cover_track_path.clone(),
        }
    }

    /// Format the release year or year range (e.g. `"1998"` or `"1998–2002"`).
    pub fn year_display(&self) -> Option<String> {
        match (self.year_min, self.year_max) {
            (Some(min), Some(max)) if min == max => Some(format!("{min}")),
            (Some(min), Some(max)) => Some(format!("{min}–{max}")),
            (Some(min), None) => Some(format!("{min}")),
            (None, Some(max)) => Some(format!("{max}")),
            (None, None) => None,
        }
    }

    /// Quality badge for the album tier.
    pub fn quality_badge(&self) -> Option<&'static str> {
        if self.is_hires {
            Some("HI-RES")
        } else if self.has_lossless {
            Some("LOSSLESS")
        } else {
            None
        }
    }
}

/// In-memory pre-aggregated album index.
#[derive(Debug, Clone, Default)]
pub struct AlbumAnalyzer {
    summaries: Vec<AlbumSummary>,
    index_by_key: HashMap<(String, String), usize>,
}

impl AlbumAnalyzer {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Analyze a slice of tracks, aggregating album metadata into pre-computed summaries.
    pub fn analyze(tracks: &[Track]) -> Self {
        let mut summaries: Vec<AlbumSummary> = Vec::new();
        let mut key_to_idx: HashMap<(&str, &str), usize> = HashMap::new();

        for track in tracks {
            if track.album.trim().is_empty() {
                continue;
            }
            let key = (track.artist.as_str(), track.album.as_str());
            if let Some(&idx) = key_to_idx.get(&key) {
                let s = &mut summaries[idx];
                s.track_count += 1;
                s.total_duration_secs += track.duration_secs.max(0.0) as u64;
                if let Some(y) = track.year {
                    let yi = y as i32;
                    s.year_min = Some(s.year_min.map_or(yi, |curr| curr.min(yi)));
                    s.year_max = Some(s.year_max.map_or(yi, |curr| curr.max(yi)));
                }
                s.max_sample_rate = s.max_sample_rate.max(track.sample_rate);
                if let Some(bd) = track.bit_depth {
                    s.max_bit_depth = s.max_bit_depth.max(bd as u8);
                }
                s.is_hires = s.is_hires || track.is_hires();
                s.has_lossless = s.has_lossless || track.is_lossless;
                if s.cover_track_id.is_none() {
                    s.cover_track_id = Some(track.id);
                    s.cover_track_path = Some(track.path.clone());
                } else if track.track_number == Some(1) && track.disc_number.unwrap_or(1) == 1 {
                    // Track 1 of disc 1 is the preferred cover stand-in
                    s.cover_track_id = Some(track.id);
                    s.cover_track_path = Some(track.path.clone());
                }
            } else {
                let yi = track.year.map(|y| y as i32);
                let summary = AlbumSummary {
                    artist: track.artist.clone(),
                    album: track.album.clone(),
                    track_count: 1,
                    total_duration_secs: track.duration_secs.max(0.0) as u64,
                    year_min: yi,
                    year_max: yi,
                    max_sample_rate: track.sample_rate,
                    max_bit_depth: track.bit_depth.map(|bd| bd as u8).unwrap_or(0),
                    is_hires: track.is_hires(),
                    has_lossless: track.is_lossless,
                    cover_track_id: Some(track.id),
                    cover_track_path: Some(track.path.clone()),
                };
                let idx = summaries.len();
                summaries.push(summary);
                key_to_idx.insert(key, idx);
            }
        }

        // Sort summaries by album title (case-insensitive), then artist (case-insensitive)
        summaries.sort_by(|a, b| {
            a.album
                .to_lowercase()
                .cmp(&b.album.to_lowercase())
                .then_with(|| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
        });

        // Fast lookup index mapping (album, artist) -> index in summaries
        let mut index_by_key = HashMap::with_capacity(summaries.len());
        for (idx, s) in summaries.iter().enumerate() {
            index_by_key.insert((s.album.clone(), s.artist.clone()), idx);
        }

        Self {
            summaries,
            index_by_key,
        }
    }

    /// Access all pre-aggregated album summaries.
    pub fn summaries(&self) -> &[AlbumSummary] {
        &self.summaries
    }

    /// Number of distinct albums in the index.
    pub fn len(&self) -> usize {
        self.summaries.len()
    }

    /// True if there are zero albums in the index.
    pub fn is_empty(&self) -> bool {
        self.summaries.is_empty()
    }

    /// Lookup an album summary by title and artist.
    pub fn get(&self, album: &str, artist: &str) -> Option<&AlbumSummary> {
        self.index_by_key
            .get(&(album.to_string(), artist.to_string()))
            .and_then(|&idx| self.summaries.get(idx))
    }

    /// Lookup an album summary by `(album, artist)` key reference without allocations.
    pub fn get_by_key(&self, key: &(String, String)) -> Option<&AlbumSummary> {
        self.index_by_key
            .get(key)
            .and_then(|&idx| self.summaries.get(idx))
    }

    /// Convert all album summaries into UI `Album` records.
    pub fn to_albums(&self) -> Vec<Album> {
        self.summaries.iter().map(|s| s.to_album()).collect()
    }

    /// Map `(album, artist)` to the representative track for album cover art.
    pub fn cover_tracks(&self) -> HashMap<(String, String), (TrackId, PathBuf)> {
        let mut map = HashMap::with_capacity(self.summaries.len());
        for s in &self.summaries {
            if let (Some(id), Some(path)) = (s.cover_track_id, &s.cover_track_path) {
                map.insert((s.album.clone(), s.artist.clone()), (id, path.clone()));
            }
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn make_test_track(
        title: &str,
        artist: &str,
        album: &str,
        track_no: u32,
        disc_no: u32,
        year: Option<u32>,
        sample_rate: u32,
        bit_depth: Option<u32>,
        lossless: bool,
    ) -> Track {
        Track {
            id: TrackId::new(),
            path: PathBuf::from(format!("/music/{artist}/{album}/{track_no}.flac")),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: Some("Ambient".to_string()),
            year,
            track_number: Some(track_no),
            disc_number: Some(disc_no),
            duration_secs: 180.0,
            bpm: None,
            sample_rate,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: Some(1000),
            bit_depth,
            is_lossless: lossless,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 30_000_000,
            mtime_unix_secs: 1_700_000_000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
            sort_artist_folded: None,
            sort_title_folded: None,
            sort_album_folded: None,
        }
    }

    #[test]
    fn pre_aggregates_album_metadata_accurately() {
        let tracks = vec![
            make_test_track(
                "Track 1",
                "Aphex Twin",
                "Selected Ambient Works",
                1,
                1,
                Some(1992),
                44100,
                Some(16),
                true,
            ),
            make_test_track(
                "Track 2",
                "Aphex Twin",
                "Selected Ambient Works",
                2,
                1,
                Some(1992),
                44100,
                Some(16),
                true,
            ),
            make_test_track(
                "Track 3",
                "Aphex Twin",
                "Selected Ambient Works",
                3,
                1,
                Some(1993),
                96000,
                Some(24),
                true,
            ),
            make_test_track(
                "Kid A",
                "Radiohead",
                "Kid A",
                1,
                1,
                Some(2000),
                44100,
                Some(16),
                true,
            ),
        ];

        let analyzer = AlbumAnalyzer::analyze(&tracks);
        assert_eq!(analyzer.len(), 2);

        let saw = analyzer
            .get("Selected Ambient Works", "Aphex Twin")
            .expect("saw found");
        assert_eq!(saw.track_count, 3);
        assert_eq!(saw.total_duration_secs, 540);
        assert_eq!(saw.year_min, Some(1992));
        assert_eq!(saw.year_max, Some(1993));
        assert_eq!(saw.year_display(), Some("1992–1993".to_string()));
        assert_eq!(saw.max_sample_rate, 96000);
        assert_eq!(saw.max_bit_depth, 24);
        assert!(saw.is_hires);
        assert!(saw.has_lossless);
        assert_eq!(saw.quality_badge(), Some("HI-RES"));

        let kid_a = analyzer.get("Kid A", "Radiohead").expect("kid a found");
        assert_eq!(kid_a.track_count, 1);
        assert_eq!(kid_a.year_display(), Some("2000".to_string()));
        assert_eq!(kid_a.quality_badge(), Some("LOSSLESS"));

        let albums = analyzer.to_albums();
        assert_eq!(albums.len(), 2);
        // Alphabetical sort by album title
        assert_eq!(albums[0].title, "Kid A");
        assert_eq!(albums[1].title, "Selected Ambient Works");

        let covers = analyzer.cover_tracks();
        assert_eq!(covers.len(), 2);
    }
}

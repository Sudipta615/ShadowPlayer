//! Search-level tests.
//!
//! The engine used to derive its artist and album results from the already
//! truncated track page, so a match that ranked past `limit` did not exist as
//! far as the search box was concerned. These tests pin the direct queries.

#[cfg(test)]
mod unit {
    use crate::SearchEngine;
    use database::{Database, PlaylistRepo, TrackRepo};
    use domain::ids::TrackId;
    use domain::models::Track;
    use std::path::PathBuf;

    fn track(path: &str, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id: TrackId::new(),
            path: PathBuf::from(path),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: None,
            year: Some(2024),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 180.0,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: Some(900),
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 1_000,
            mtime_unix_secs: 1_700_000_000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
        }
    }

    /// An artist whose tracks all rank past the limit still gets an artist hit.
    #[test]
    fn an_artist_past_the_track_limit_still_appears() {
        let db = Database::in_memory().unwrap();
        let tracks = TrackRepo::new(db.clone());
        for i in 0..5 {
            tracks
                .upsert(&track(
                    &format!("/music/deep/{i}.flac"),
                    &format!("Filler {i}"),
                    "Obscure Collective",
                    "Deep Cuts",
                ))
                .unwrap();
        }

        let engine = SearchEngine::new(db);
        let result = engine.search("Obscure", 1);

        assert_eq!(result.tracks.len(), 1, "the track list is truncated");
        assert_eq!(
            result.artists,
            vec!["Obscure Collective".to_string()],
            "but the artist is found by its own query"
        );
    }

    /// Albums are found by their own title, deduplicated like before.
    #[test]
    fn albums_are_found_and_deduplicated() {
        let db = Database::in_memory().unwrap();
        let tracks = TrackRepo::new(db.clone());
        // Two different artists with an album of the same name, plus one more
        // track on each copy so the ordering is by track count.
        for (i, artist) in ["First Artist", "Second Artist"].iter().enumerate() {
            for j in 0..(i + 1) {
                tracks
                    .upsert(&track(
                        &format!("/music/same/{artist}/{j}.flac"),
                        &format!("Song {j}"),
                        artist,
                        "Shared Name",
                    ))
                    .unwrap();
            }
        }

        let engine = SearchEngine::new(db);
        let result = engine.search("Shared", 10);
        assert_eq!(result.albums, vec!["Shared Name".to_string()]);
    }

    /// Playlists are queried by name, not scanned from a full table read.
    #[test]
    fn playlists_are_found_by_name() {
        let db = Database::in_memory().unwrap();
        let playlists = PlaylistRepo::new(db.clone());
        playlists.create("Road Trip", None).unwrap();
        playlists.create("Focus", None).unwrap();

        let engine = SearchEngine::new(db);
        let result = engine.search("road", 10);
        assert_eq!(result.playlists.len(), 1);
        assert_eq!(result.playlists[0].name, "Road Trip");
    }

    /// A query the tokenizer cannot turn into tokens returns nothing, and says
    /// nothing: no panic, no FTS syntax error, no accidental "match all".
    #[test]
    fn a_query_without_tokens_returns_nothing() {
        let db = Database::in_memory().unwrap();
        TrackRepo::new(db.clone())
            .upsert(&track("/music/a.flac", "Anything", "Someone", "Somewhere"))
            .unwrap();

        let engine = SearchEngine::new(db);
        let result = engine.search("!!!", 10);
        assert!(result.is_empty());
    }

    /// `%` and `_` in a query are literals in the artist/album lookups.
    #[test]
    fn wildcards_in_a_query_are_literal() {
        let db = Database::in_memory().unwrap();
        let tracks = TrackRepo::new(db.clone());
        tracks
            .upsert(&track("/music/a.flac", "A", "100% Proof", "Covered"))
            .unwrap();
        tracks
            .upsert(&track("/music/b.flac", "B", "Plain Artist", "Plain Album"))
            .unwrap();

        let engine = SearchEngine::new(db);
        let result = engine.search("100%", 10);
        assert_eq!(result.artists, vec!["100% Proof".to_string()]);
        assert!(
            result.albums.is_empty(),
            "'%' must not match every album: {:?}",
            result.albums
        );
    }
}

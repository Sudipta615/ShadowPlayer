#[cfg(test)]
mod unit {
    use crate::SearchEngine;
    use domain::ids::TrackId;
    use domain::models::{Playlist, Track};
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
            bpm: None,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: Some(900),
            bit_depth: Some(16),
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
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
        }
    }

    fn playlist(name: &str) -> Playlist {
        Playlist {
            id: domain::ids::PlaylistId::new(),
            name: name.to_string(),
            description: None,
            track_count: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn an_artist_past_the_track_limit_still_appears() {
        let tracks: Vec<Track> = (0..5)
            .map(|i| {
                track(
                    &format!("/music/deep/{i}.flac"),
                    &format!("Filler {i}"),
                    "Obscure Collective",
                    "Deep Cuts",
                )
            })
            .collect();

        let engine = SearchEngine::new();
        let result = engine.search("Obscure", &tracks, &[], 1);

        assert_eq!(result.tracks.len(), 1, "the track list is truncated");
        assert_eq!(
            result.artists,
            vec!["Obscure Collective".to_string()],
            "but the artist is found by its own query"
        );
    }

    #[test]
    fn albums_are_found_and_deduplicated() {
        let mut tracks = Vec::new();
        for (i, artist) in ["First Artist", "Second Artist"].iter().enumerate() {
            for j in 0..(i + 1) {
                tracks.push(track(
                    &format!("/music/same/{artist}/{j}.flac"),
                    &format!("Song {j}"),
                    artist,
                    "Shared Name",
                ));
            }
        }

        let engine = SearchEngine::new();
        let result = engine.search("Shared", &tracks, &[], 10);
        assert_eq!(result.albums, vec!["Shared Name".to_string()]);
    }

    #[test]
    fn playlists_are_found_by_name() {
        let playlists = vec![playlist("Road Trip"), playlist("Focus")];

        let engine = SearchEngine::new();
        let result = engine.search("road", &[], &playlists, 10);
        assert_eq!(result.playlists.len(), 1);
        assert_eq!(result.playlists[0].name, "Road Trip");
    }

    #[test]
    fn a_query_without_tokens_returns_nothing() {
        let tracks = vec![track("/music/a.flac", "Anything", "Someone", "Somewhere")];

        let engine = SearchEngine::new();
        let result = engine.search("!!!", &tracks, &[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn wildcards_in_a_query_are_literal() {
        let tracks = vec![
            track("/music/a.flac", "A", "100% Proof", "Covered"),
            track("/music/b.flac", "B", "Plain Artist", "Plain Album"),
        ];

        let engine = SearchEngine::new();
        let result = engine.search("100%", &tracks, &[], 10);
        assert_eq!(result.artists, vec!["100% Proof".to_string()]);
        assert!(
            result.albums.is_empty(),
            "'%' must not match every album: {:?}",
            result.albums
        );
    }
}
PY
#[cfg(test)]
mod unit {
    use crate::db::Database;
    use crate::repos::*;
    use domain::ids::TrackId;
    use domain::models::{AppSettings, SortField, SortOrder, Track, TrackDspProfile};

    use std::path::PathBuf;

    fn sample_track(id: TrackId, path_str: &str, title: &str, artist: &str, album: &str) -> Track {
        Track {
            id,
            path: PathBuf::from(path_str),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            genre: Some("Synthwave".to_string()),
            year: Some(2024),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 238.0,
            bpm: None,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: Some(950),
            bit_depth: Some(16),
            is_lossless: true,
            replaygain_track_db: Some(-6.5),
            replaygain_track_peak: Some(0.98),
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: Some(-16.0),
            file_size_bytes: 25_000_000,
            mtime_unix_secs: 1700000000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 1700000000,
        }
    }

    /// Two files whose names differ only in bytes that are not valid UTF-8 must
    /// be two rows, each reachable by its own path. `to_string_lossy()` used to
    /// turn both into the same U+FFFD text, so the second upsert overwrote the
    /// first and the file simply never appeared.
    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_are_distinct_rows() {
        use std::os::unix::ffi::OsStrExt;

        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let raw_path = |bytes: &[u8]| PathBuf::from(std::ffi::OsStr::from_bytes(bytes));

        let a_path = raw_path(b"/music/song-\x8a.flac");
        let b_path = raw_path(b"/music/song-\x8b.flac");
        let a = Track {
            path: a_path.clone(),
            ..sample_track(TrackId::new(), "/unused", "A", "Artist", "Album")
        };
        let b = Track {
            path: b_path.clone(),
            ..sample_track(TrackId::new(), "/unused", "B", "Artist", "Album")
        };

        repo.upsert(&a).expect("first upsert");
        repo.upsert(&b).expect("second upsert");
        assert_eq!(repo.count().unwrap(), 2, "both rows survive");

        // The exact bytes come back out of the database.
        assert_eq!(repo.get_by_path(&a_path).unwrap().unwrap().path, a_path);
        assert_eq!(repo.get_by_path(&b_path).unwrap().unwrap().path, b_path);

        // The lossy text they share is not a key for either of them.
        assert!(repo
            .get_by_path(&PathBuf::from("/music/song-\u{fffd}.flac"))
            .unwrap()
            .is_none());

        // Renaming is also lossless.
        let renamed = raw_path(b"/music/renamed-\x8a.flac");
        assert!(repo.relink_path(&a.id, &renamed, None).unwrap());
        assert_eq!(repo.get_by_path(&renamed).unwrap().unwrap().path, renamed);
    }

    /// Paths that are ordinary text keep their key: a row indexed by an older
    /// build is still found by `get_by_path` after the upgrade.
    #[test]
    fn ordinary_paths_keep_their_key() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        for path in ["/music/100% real.flac", "/music/song%20name.flac"] {
            let track = sample_track(TrackId::new(), path, "T", "A", "B");
            repo.upsert(&track).unwrap();
            assert_eq!(
                repo.get_by_path(std::path::Path::new(path))
                    .unwrap()
                    .unwrap()
                    .id,
                track.id
            );
        }
    }

    #[test]
    fn test_settings_persistence() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let settings_repo = SettingsRepo::new(db.clone());

        let s = AppSettings {
            volume: 0.92,
            sort_field: SortField::Duration,
            sort_order: SortOrder::Descending,
            ..AppSettings::default()
        };

        settings_repo.save(&s).unwrap();
        let loaded = settings_repo.load().unwrap();

        assert!((loaded.volume - 0.92).abs() < 1e-4);
        assert_eq!(loaded.sort_field, SortField::Duration);
        assert_eq!(loaded.sort_order, SortOrder::Descending);

        // Test arbitrary key-value persistence for EQ and Studio Settings
        settings_repo
            .set_value(
                "eq_panel_v1",
                r#"{"layout":"TenBand","graphic_gains":[1.5,-2.0]}"#,
            )
            .unwrap();
        let eq_json = settings_repo.get_value("eq_panel_v1").unwrap();
        assert_eq!(
            eq_json,
            Some(r#"{"layout":"TenBand","graphic_gains":[1.5,-2.0]}"#.to_string())
        );

        let non_existent = settings_repo.get_value("non_existent_key").unwrap();
        assert_eq!(non_existent, None);
    }

    #[test]
    fn test_album_artist_genre_and_folder_repos() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let track_repo = TrackRepo::new(db.clone());
        let album_repo = AlbumRepo::new(db.clone());
        let artist_repo = ArtistRepo::new(db.clone());
        let genre_repo = GenreRepo::new(db.clone());

        let t1 = sample_track(
            TrackId::new(),
            "/music/rock/1.flac",
            "Song 1",
            "Artist A",
            "Album X",
        );
        let t2 = sample_track(
            TrackId::new(),
            "/music/rock/2.flac",
            "Song 2",
            "Artist A",
            "Album X",
        );
        let mut t3 = sample_track(
            TrackId::new(),
            "/music/jazz/3.flac",
            "Song 3",
            "Artist B",
            "Album Y",
        );
        t3.genre = Some("Jazz".to_string());

        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();
        track_repo.upsert(&t3).unwrap();

        // Album repo list and get tracks
        let albums = album_repo.list().unwrap();
        assert_eq!(albums.len(), 2);
        let x_tracks = album_repo
            .get_tracks_by_album("Album X", "Artist A")
            .unwrap();
        assert_eq!(x_tracks.len(), 2);

        // Artist repo list and get tracks
        let artists = artist_repo.list().unwrap();
        assert_eq!(artists.len(), 2);
        let a_tracks = artist_repo.get_tracks_by_artist("Artist A").unwrap();
        assert_eq!(a_tracks.len(), 2);

        // Genre repo list and get tracks
        let genres = genre_repo.list().unwrap();
        assert_eq!(genres.len(), 2);
        let jazz_tracks = genre_repo.get_tracks_by_genre("Jazz").unwrap();
        assert_eq!(jazz_tracks.len(), 1);
        assert_eq!(jazz_tracks[0].title, "Song 3");

        // Folder querying
        let rock_tracks = track_repo
            .get_tracks_by_folder(&PathBuf::from("/music/rock"))
            .unwrap();
        assert_eq!(rock_tracks.len(), 2);
        let jazz_folder_tracks = track_repo
            .get_tracks_by_folder(&PathBuf::from("/music/jazz"))
            .unwrap();
        assert_eq!(jazz_folder_tracks.len(), 1);
    }

    #[test]
    fn test_loudness_scan_bookkeeping() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let mut t1 = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        t1.replaygain_track_db = None;
        t1.replaygain_track_peak = None;
        t1.r128_loudness_lufs = None;
        let mut t2 = sample_track(TrackId::new(), "/m/b.flac", "B", "Artist", "Album");
        t2.replaygain_track_db = Some(-3.0);
        repo.upsert(&t1).unwrap();
        repo.upsert(&t2).unwrap();

        // Only the track without loudness data is queued for scanning.
        let missing = repo.list_missing_replaygain().unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].1, PathBuf::from("/m/a.flac"));

        repo.set_loudness(&missing[0].0, Some(-7.5), Some(0.91), Some(-14.2))
            .unwrap();
        let got = repo.get_by_id(&missing[0].0).unwrap().unwrap();
        assert_eq!(got.replaygain_track_db, Some(-7.5));
        assert_eq!(got.replaygain_track_peak, Some(0.91));
        assert_eq!(got.r128_loudness_lufs, Some(-14.2));

        assert_eq!(repo.list_missing_replaygain().unwrap().len(), 0);
    }

    #[test]
    fn test_album_gain_is_stored_for_every_track_of_the_album() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let a = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        let b = sample_track(TrackId::new(), "/m/b.flac", "B", "Artist", "Album");
        let other = sample_track(TrackId::new(), "/m/c.flac", "C", "Artist", "Other");
        repo.upsert(&a).unwrap();
        repo.upsert(&b).unwrap();
        repo.upsert(&other).unwrap();

        let updated = repo
            .set_album_loudness("Album", "Artist", Some(-6.25), Some(0.51))
            .unwrap();
        assert_eq!(updated, 2, "both tracks of the album are updated");

        for id in [a.id, b.id] {
            let got = repo.get_by_id(&id).unwrap().unwrap();
            assert_eq!(got.replaygain_album_db, Some(-6.25));
            assert_eq!(got.replaygain_album_peak, Some(0.51));
        }
        // A different album is untouched.
        let got = repo.get_by_id(&other.id).unwrap().unwrap();
        assert_eq!(got.replaygain_album_db, None);
    }

    #[test]
    fn test_skip_count_and_dsp_profile_round_trip() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let dsp = TrackDspRepo::new(db.clone());

        let track = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        repo.upsert(&track).unwrap();

        // A skip is counted.
        repo.increment_skip_count(&track.id).unwrap();
        repo.increment_skip_count(&track.id).unwrap();
        let got = repo.get_by_id(&track.id).unwrap().unwrap();
        assert_eq!(got.skip_count, 2);
        assert_eq!(got.play_count, 0, "a skip is not a play");

        // The most-skipped list finds it.
        let skipped = repo.list_most_skipped(10).unwrap();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].id, track.id);

        // A per-track DSP profile round-trips.
        let profile = TrackDspProfile {
            graphic_gains: vec![1.5, -2.0],
            graphic_enabled: true,
            graphic_preamp_db: -3.0,
            overrides: None,
            replaygain_override_db: Some(-2.5),
        };
        dsp.set(&track.id, &profile).unwrap();
        let read = dsp
            .get(&track.id)
            .unwrap()
            .expect("the profile must be there");
        assert_eq!(read, profile);
        assert!(!read.is_neutral());

        // And it can be removed.
        dsp.clear(&track.id).unwrap();
        assert!(dsp.get(&track.id).unwrap().is_none());
    }
}

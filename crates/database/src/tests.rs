#[cfg(test)]
mod tests {
    use crate::db::Database;
    use crate::repos::*;
    use domain::ids::TrackId;
    use domain::models::{AppSettings, SortField, SortOrder, Track};
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
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: Some(950),
            is_lossless: true,
            replaygain_track_db: Some(-6.5),
            replaygain_track_peak: Some(0.98),
            r128_loudness_lufs: Some(-16.0),
            file_size_bytes: 25_000_000,
            mtime_unix_secs: 1700000000,
            content_hash: None,
            is_favorite: false,
            play_count: 0,
            last_played_at: None,
            added_at: 1700000000,
        }
    }

    #[test]
    fn test_track_crud_and_favorites() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let fav_repo = FavoritesRepo::new(db.clone());

        let t1_id = TrackId::new();
        let t1 = sample_track(t1_id, "/music/midnight_dreams.flac", "Midnight Dreams", "Horizon Lines", "Lost in Reverie");
        repo.upsert(&t1).expect("Failed to insert track");

        assert_eq!(repo.count().unwrap(), 1);

        let retrieved = repo.get_by_id(&t1_id).unwrap().expect("Track not found");
        assert_eq!(retrieved.title, "Midnight Dreams");
        assert_eq!(retrieved.is_favorite, false);

        // Toggle favorite
        repo.set_favorite(&t1_id, true).unwrap();
        let favs = fav_repo.list().unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].id, t1_id);

        repo.set_favorite(&t1_id, false).unwrap();
        let favs_after = fav_repo.list().unwrap();
        assert_eq!(favs_after.len(), 0);

        // Increment play count
        repo.increment_play_count(&t1_id).unwrap();
        let played = repo.get_by_id(&t1_id).unwrap().unwrap();
        assert_eq!(played.play_count, 1);
    }

    #[test]
    fn test_fts5_search() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let t1 = sample_track(TrackId::new(), "/music/1.flac", "Midnight Dreams", "Horizon Lines", "Lost in Reverie");
        let t2 = sample_track(TrackId::new(), "/music/2.flac", "Echoes", "Aurora Skies", "Waves of Time");
        let t3 = sample_track(TrackId::new(), "/music/3.flac", "Starlight", "Nova Bloom", "Starlight EP");

        repo.upsert(&t1).unwrap();
        repo.upsert(&t2).unwrap();
        repo.upsert(&t3).unwrap();

        // Search by title prefix
        let res1 = repo.search_fts("Mid", 10).unwrap();
        assert_eq!(res1.len(), 1);
        assert_eq!(res1[0].title, "Midnight Dreams");

        // Search by artist
        let res2 = repo.search_fts("Aurora", 10).unwrap();
        assert_eq!(res2.len(), 1);
        assert_eq!(res2[0].title, "Echoes");

        // Search by album
        let res3 = repo.search_fts("Starlight", 10).unwrap();
        assert_eq!(res3.len(), 1);
        assert_eq!(res3[0].title, "Starlight");
    }

    #[test]
    fn test_playlists_and_queue() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let track_repo = TrackRepo::new(db.clone());
        let pl_repo = PlaylistRepo::new(db.clone());
        let q_repo = QueueRepo::new(db.clone());

        let t1 = sample_track(TrackId::new(), "/music/1.flac", "Midnight Dreams", "Horizon Lines", "Lost in Reverie");
        let t2 = sample_track(TrackId::new(), "/music/2.flac", "Echoes", "Aurora Skies", "Waves of Time");
        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();

        // Playlist creation & tracks
        let pl = pl_repo.create("Chill Vibes", Some("Evening tunes")).unwrap();
        pl_repo.add_track(&pl.id, &t1.id).unwrap();
        pl_repo.add_track(&pl.id, &t2.id).unwrap();

        let pl_tracks = pl_repo.get_tracks(&pl.id).unwrap();
        assert_eq!(pl_tracks.len(), 2);
        assert_eq!(pl_tracks[0].id, t1.id);
        assert_eq!(pl_tracks[1].id, t2.id);

        // Saved Queue persistence
        let queue_entries = vec![(t1.id, t1.path.clone()), (t2.id, t2.path.clone())];
        q_repo.save_queue(&queue_entries).unwrap();

        let loaded = q_repo.load_queue().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, t1.id);

        q_repo.clear().unwrap();
        assert_eq!(q_repo.load_queue().unwrap().len(), 0);
    }

    #[test]
    fn test_settings_persistence() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let settings_repo = SettingsRepo::new(db.clone());

        let mut s = AppSettings::default();
        s.volume = 0.92;
        s.sort_field = SortField::Duration;
        s.sort_order = SortOrder::Descending;

        settings_repo.save(&s).unwrap();
        let loaded = settings_repo.load().unwrap();

        assert!((loaded.volume - 0.92).abs() < 1e-4);
        assert_eq!(loaded.sort_field, SortField::Duration);
        assert_eq!(loaded.sort_order, SortOrder::Descending);

        // Test arbitrary key-value persistence for EQ and Studio Settings
        settings_repo.set_value("eq_panel_v1", r#"{"layout":"TenBand","graphic_gains":[1.5,-2.0]}"#).unwrap();
        let eq_json = settings_repo.get_value("eq_panel_v1").unwrap();
        assert_eq!(eq_json, Some(r#"{"layout":"TenBand","graphic_gains":[1.5,-2.0]}"#.to_string()));

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

        let t1 = sample_track(TrackId::new(), "/music/rock/1.flac", "Song 1", "Artist A", "Album X");
        let t2 = sample_track(TrackId::new(), "/music/rock/2.flac", "Song 2", "Artist A", "Album X");
        let mut t3 = sample_track(TrackId::new(), "/music/jazz/3.flac", "Song 3", "Artist B", "Album Y");
        t3.genre = Some("Jazz".to_string());

        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();
        track_repo.upsert(&t3).unwrap();

        // Album repo list and get tracks
        let albums = album_repo.list().unwrap();
        assert_eq!(albums.len(), 2);
        let x_tracks = album_repo.get_tracks_by_album("Album X", "Artist A").unwrap();
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
        let rock_tracks = track_repo.get_tracks_by_folder(&PathBuf::from("/music/rock")).unwrap();
        assert_eq!(rock_tracks.len(), 2);
        let jazz_folder_tracks = track_repo.get_tracks_by_folder(&PathBuf::from("/music/jazz")).unwrap();
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
    fn test_moved_file_keeps_identity() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let favorites = FavoritesRepo::new(db.clone());
        let playlists = PlaylistRepo::new(db.clone());

        let mut t1 = sample_track(TrackId::new(), "/music/old/album/01.flac", "Song", "Band", "Album");
        t1.content_hash = Some("12345-abcdef".to_string());
        repo.upsert(&t1).unwrap();

        // The user favourites it and adds it to a playlist.
        favorites.add(&t1.id).unwrap();
        let pl = playlists.create("Mixtape", None).unwrap();
        playlists.add_track(&pl.id, &t1.id).unwrap();

        // The file is moved to a new folder; the new row has a fresh
        // path-derived id but the same content fingerprint.
        let moved = sample_track(TrackId::new(), "/music/new/album/01.flac", "Song", "Band", "Album");
        assert_ne!(moved.id, t1.id, "a new path yields a new id");

        let found = repo.find_by_content_hash("12345-abcdef").unwrap();
        let (old_id, old_path) = found.expect("the original row is found by content hash");
        assert_eq!(old_id, t1.id);
        assert_eq!(old_path, t1.path);

        assert!(repo
            .relink_path(&old_id, &moved.path, Some("12345-abcdef"))
            .unwrap());

        // Identity, favourites and playlist membership all survived.
        let after = repo.get_by_id(&t1.id).unwrap().unwrap();
        assert_eq!(after.path, moved.path);
        assert_eq!(repo.get_by_path(&moved.path).unwrap().unwrap().id, t1.id);
        assert!(favorites.list().unwrap().iter().any(|t| t.id == t1.id));
        assert!(playlists
            .get_tracks(&pl.id)
            .unwrap()
            .iter()
            .any(|t| t.id == t1.id));

        // Re-upserting the moved file at its new path updates metadata in place
        // rather than creating a second row.
        let mut refreshed = moved.clone();
        refreshed.id = t1.id;
        refreshed.title = "Song (Remaster)".to_string();
        repo.upsert(&refreshed).unwrap();
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(repo.get_by_id(&t1.id).unwrap().unwrap().title, "Song (Remaster)");

        // An ambiguous fingerprint (two rows sharing content) is never used to
        // relink, so we can't silently steal the wrong row.
        let mut dup_a = sample_track(TrackId::new(), "/m/dup-a.flac", "DupA", "X", "Y");
        dup_a.content_hash = Some("same-hash".to_string());
        let mut dup_b = sample_track(TrackId::new(), "/m/dup-b.flac", "DupB", "X", "Y");
        dup_b.content_hash = Some("same-hash".to_string());
        repo.upsert(&dup_a).unwrap();
        repo.upsert(&dup_b).unwrap();
        assert!(repo.find_by_content_hash("same-hash").unwrap().is_none());
    }

    #[test]
    fn test_smart_playlists() {
        use domain::ids::PlaylistId;
        use domain::smart::{SmartField, SmartOp, SmartPlaylist, SmartRule};

        let db = Database::in_memory().expect("In-memory DB creation failed");
        let track_repo = TrackRepo::new(db.clone());
        let sp_repo = SmartPlaylistRepo::new(db.clone());

        let t1 = sample_track(TrackId::new(), "/m/1.flac", "Alpha", "Artist A", "Album X");
        let mut t2 = sample_track(TrackId::new(), "/m/2.mp3", "Beta", "Artist B", "Album Y");
        t2.format = "MP3".to_string();
        t2.is_lossless = false;
        t2.bitrate_kbps = Some(320);
        let mut t3 = sample_track(TrackId::new(), "/m/3.flac", "Gamma", "Artist A", "Album Z");
        t3.play_count = 9;

        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();
        track_repo.upsert(&t3).unwrap();

        // Rule: lossless = yes  -> the two FLACs.
        let mut sp = SmartPlaylist::draft(PlaylistId::new());
        sp.name = "Lossless".to_string();
        sp.rules = vec![SmartRule {
            field: SmartField::Lossless,
            op: SmartOp::IsTrue,
            value: String::new(),
        }];
        let saved = sp_repo.save(&sp).unwrap();
        assert_eq!(saved.name, "Lossless");
        assert_eq!(sp_repo.evaluate(&saved).unwrap().len(), 2);

        // AND: Artist = Artist A AND play_count > 5 -> only Gamma.
        let mut sp2 = SmartPlaylist::draft(PlaylistId::new());
        sp2.name = "A heavy".to_string();
        sp2.match_all = true;
        sp2.rules = vec![
            SmartRule {
                field: SmartField::Artist,
                op: SmartOp::Equals,
                value: "Artist A".to_string(),
            },
            SmartRule {
                field: SmartField::PlayCount,
                op: SmartOp::GreaterThan,
                value: "5".to_string(),
            },
        ];
        let sp2 = sp_repo.save(&sp2).unwrap();
        let m2 = sp_repo.evaluate(&sp2).unwrap();
        assert_eq!(m2.len(), 1);
        assert_eq!(m2[0].title, "Gamma");

        // OR: format contains "MP" OR title contains "Alpha" -> t1 + t2.
        let mut sp3 = SmartPlaylist::draft(PlaylistId::new());
        sp3.name = "Any".to_string();
        sp3.match_all = false;
        sp3.rules = vec![
            SmartRule {
                field: SmartField::Format,
                op: SmartOp::Contains,
                value: "MP".to_string(),
            },
            SmartRule {
                field: SmartField::Title,
                op: SmartOp::Contains,
                value: "Alpha".to_string(),
            },
        ];
        let sp3 = sp_repo.save(&sp3).unwrap();
        assert_eq!(sp_repo.evaluate(&sp3).unwrap().len(), 2);

        // Incomplete rules (empty value) are ignored, not fatal.
        let mut sp4 = SmartPlaylist::draft(PlaylistId::new());
        sp4.name = "Incomplete".to_string();
        sp4.rules = vec![SmartRule {
            field: SmartField::Title,
            op: SmartOp::Contains,
            value: "   ".to_string(),
        }];
        let sp4 = sp_repo.save(&sp4).unwrap();
        assert_eq!(sp_repo.evaluate(&sp4).unwrap().len(), 3);

        // Limit + sort descending by title -> only "Gamma".
        let mut sp5 = SmartPlaylist::draft(PlaylistId::new());
        sp5.name = "Top one".to_string();
        sp5.rules = vec![];
        sp5.limit_count = Some(1);
        sp5.sort_by = Some(SmartField::Title);
        sp5.sort_desc = true;
        let sp5 = sp_repo.save(&sp5).unwrap();
        let m5 = sp_repo.evaluate(&sp5).unwrap();
        assert_eq!(m5.len(), 1);
        assert_eq!(m5[0].title, "Gamma");

        // A minute-based "in the last N days" rule excludes never-played tracks.
        let mut sp6 = SmartPlaylist::draft(PlaylistId::new());
        sp6.name = "Fresh".to_string();
        sp6.rules = vec![SmartRule {
            field: SmartField::LastPlayed,
            op: SmartOp::InLastDays,
            value: "7".to_string(),
        }];
        let sp6 = sp_repo.save(&sp6).unwrap();
        assert_eq!(sp_repo.evaluate(&sp6).unwrap().len(), 0);

        // Persistence round-trip.
        let all = sp_repo.list().unwrap();
        assert_eq!(all.len(), 6);
        let got = sp_repo.get(&saved.id).unwrap().unwrap();
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].field, SmartField::Lossless);

        sp_repo.delete(&sp2.id).unwrap();
        assert_eq!(sp_repo.list().unwrap().len(), 5);
    }
}


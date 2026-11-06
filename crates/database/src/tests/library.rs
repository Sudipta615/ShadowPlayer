#[cfg(test)]
mod unit {
    use crate::db::Database;
    use crate::repos::*;
    use domain::ids::{AlbumId, PlaylistId, TrackId};
    use domain::models::{AppSettings, SortField, SortOrder, Track};

    use domain::smart::{SmartField, SmartOp, SmartPlaylist, SmartRule};
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

    #[test]
    fn test_track_crud_and_favorites() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let fav_repo = FavoritesRepo::new(db.clone());

        let t1_id = TrackId::new();
        let t1 = sample_track(
            t1_id,
            "/music/midnight_dreams.flac",
            "Midnight Dreams",
            "Horizon Lines",
            "Lost in Reverie",
        );
        repo.upsert(&t1).expect("Failed to insert track");

        assert_eq!(repo.count().unwrap(), 1);

        let retrieved = repo.get_by_id(&t1_id).unwrap().expect("Track not found");
        assert_eq!(retrieved.title, "Midnight Dreams");
        assert!(!retrieved.is_favorite);

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

        let t1 = sample_track(
            TrackId::new(),
            "/music/1.flac",
            "Midnight Dreams",
            "Horizon Lines",
            "Lost in Reverie",
        );
        let t2 = sample_track(
            TrackId::new(),
            "/music/2.flac",
            "Echoes",
            "Aurora Skies",
            "Waves of Time",
        );
        let t3 = sample_track(
            TrackId::new(),
            "/music/3.flac",
            "Starlight",
            "Nova Bloom",
            "Starlight EP",
        );

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

        let t1 = sample_track(
            TrackId::new(),
            "/music/1.flac",
            "Midnight Dreams",
            "Horizon Lines",
            "Lost in Reverie",
        );
        let t2 = sample_track(
            TrackId::new(),
            "/music/2.flac",
            "Echoes",
            "Aurora Skies",
            "Waves of Time",
        );
        track_repo.upsert(&t1).unwrap();
        track_repo.upsert(&t2).unwrap();

        // Playlist creation & tracks
        let pl = pl_repo
            .create("Chill Vibes", Some("Evening tunes"))
            .unwrap();
        pl_repo.add_track(&pl.id, &t1.id).unwrap();
        pl_repo.add_track(&pl.id, &t2.id).unwrap();

        let pl_tracks = pl_repo.get_tracks(&pl.id).unwrap();
        assert_eq!(pl_tracks.len(), 2);
        assert_eq!(pl_tracks[0].id, t1.id);
        assert_eq!(pl_tracks[1].id, t2.id);

        // Saved Queue persistence
        let queue_entries = vec![t1.id, t2.id];
        q_repo.save_queue(&queue_entries).unwrap();

        let loaded = q_repo.load_queue().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, t1.id);

        q_repo.clear().unwrap();
        assert_eq!(q_repo.load_queue().unwrap().len(), 0);
    }

    #[test]
    fn playlist_links_resolve_and_reject_cycles() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let tracks = TrackRepo::new(db.clone());
        let playlists = PlaylistRepo::new(db.clone());

        let a = sample_track(TrackId::new(), "/music/link-a.flac", "A", "X", "Album");
        let b = sample_track(TrackId::new(), "/music/link-b.flac", "B", "X", "Album");
        tracks.upsert(&a).unwrap();
        tracks.upsert(&b).unwrap();

        let outer = playlists.create("Outer", None).unwrap();
        let inner = playlists.create("Inner", None).unwrap();
        playlists.add_track(&outer.id, &a.id).unwrap();
        playlists.add_track(&inner.id, &b.id).unwrap();
        playlists.add_link(&outer.id, &inner.id).unwrap();

        let resolved: Vec<_> = playlists
            .get_tracks_resolved(&outer.id)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(
            resolved,
            ["A", "B"],
            "the linked playlist resolves at play time"
        );

        assert!(
            playlists.add_link(&inner.id, &outer.id).is_err(),
            "a link that closes the loop must be rejected"
        );
        assert!(playlists.add_link(&outer.id, &outer.id).is_err());
    }

    #[test]
    fn reordering_playlist_tracks_renumbers_positions() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let tracks = TrackRepo::new(db.clone());
        let playlists = PlaylistRepo::new(db.clone());

        let a = sample_track(TrackId::new(), "/music/a.flac", "A", "X", "Album");
        let b = sample_track(TrackId::new(), "/music/b.flac", "B", "X", "Album");
        let c = sample_track(TrackId::new(), "/music/c.flac", "C", "X", "Album");
        for track in [&a, &b, &c] {
            tracks.upsert(track).unwrap();
        }

        let playlist = playlists.create("Ordered", None).unwrap();
        for track in [&a, &b, &c] {
            playlists.add_track(&playlist.id, &track.id).unwrap();
        }

        playlists.move_track(&playlist.id, 2, 0).unwrap();
        let order: Vec<_> = playlists
            .get_tracks(&playlist.id)
            .unwrap()
            .into_iter()
            .map(|track| track.title)
            .collect();
        assert_eq!(order, ["C", "A", "B"]);

        playlists.move_track(&playlist.id, 0, 2).unwrap();
        let order: Vec<_> = playlists
            .get_tracks(&playlist.id)
            .unwrap()
            .into_iter()
            .map(|track| track.title)
            .collect();
        assert_eq!(order, ["A", "B", "C"]);

        // Out-of-range moves are a no-op rather than dropping a row.
        playlists.move_track(&playlist.id, 9, 0).unwrap();
        assert_eq!(playlists.get_tracks(&playlist.id).unwrap().len(), 3);
    }

    /// A queued track that leaves the library must leave the queue: the row is
    /// a child of the track, and the queue used to keep a phantom entry that
    /// `load_queue` silently dropped.
    #[test]
    fn deleting_a_track_removes_it_from_the_saved_queue() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let tracks = TrackRepo::new(db.clone());
        let queue = QueueRepo::new(db.clone());

        let keep = sample_track(TrackId::new(), "/music/keep.flac", "Keep", "X", "A");
        let drop = sample_track(TrackId::new(), "/music/drop.flac", "Drop", "X", "A");
        tracks.upsert(&keep).unwrap();
        tracks.upsert(&drop).unwrap();
        queue.save_queue(&[keep.id, drop.id]).unwrap();

        tracks.delete(&drop.id).unwrap();

        let loaded = queue.load_queue().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, keep.id);
    }

    /// The queue purge follows the track's own path, which is the only copy.
    #[test]
    fn queue_purge_uses_the_tracks_own_path() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let tracks = TrackRepo::new(db.clone());
        let queue = QueueRepo::new(db.clone());

        let inside = sample_track(TrackId::new(), "/music/demo/a.flac", "A", "X", "A");
        let outside = sample_track(TrackId::new(), "/music/real/b.flac", "B", "X", "B");
        tracks.upsert(&inside).unwrap();
        tracks.upsert(&outside).unwrap();
        queue.save_queue(&[inside.id, outside.id]).unwrap();

        assert_eq!(
            queue
                .purge_path_prefix(PathBuf::from("/music/demo").as_path())
                .unwrap(),
            1
        );
        let loaded = queue.load_queue().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, outside.id);
    }

    #[test]
    fn test_the_scan_queue_carries_the_album() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let mut t = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        t.replaygain_track_db = None;
        repo.upsert(&t).unwrap();

        let items = repo.list_missing_replaygain_with_album().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].2, "Album");
        assert_eq!(items[0].3, "Artist");
    }

    #[test]
    fn test_scrobbles_are_recorded_separately_from_plays() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let history = HistoryRepo::new(db.clone());

        let track = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        repo.upsert(&track).unwrap();

        // A play that was abandoned is a play, but not a scrobble.
        history.record_play(&track.id, 5.0, false, false).unwrap();
        // One that was heard is both.
        history.record_play(&track.id, 120.0, false, true).unwrap();

        assert_eq!(history.count_scrobbles().unwrap(), 1);
        let scrobbles = history.list_scrobbles(10).unwrap();
        assert_eq!(scrobbles.len(), 1);
        assert_eq!(scrobbles[0].0.id, track.id);
    }

    #[test]
    fn test_scrobbles_keep_the_text_after_the_track_is_deleted() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let history = ScrobbleRepo::new(db.clone());

        let track = sample_track(TrackId::new(), "/m/a.flac", "A", "Artist", "Album");
        repo.upsert(&track).unwrap();
        history.record_at(&track, 1_700_000_000, 200.0).unwrap();
        assert_eq!(history.count().unwrap(), 1);
        assert_eq!(history.first_played_at().unwrap(), Some(1_700_000_000));

        repo.delete(&track.id).unwrap();

        let all = history.list_all().unwrap();
        assert_eq!(all.len(), 1, "a scrobble is not a child of the track row");
        assert_eq!(all[0].track_id, None, "the link is dropped, the text stays");
        assert_eq!(all[0].artist, "Artist");
        assert_eq!(all[0].title, "A");
        assert_eq!(all[0].album, "Album");
    }

    #[test]
    fn test_listening_history_aggregates_by_artist_album_track_and_day() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let history = ScrobbleRepo::new(db.clone());

        let a = sample_track(TrackId::new(), "/m/a.flac", "A", "One", "Album X");
        let b = sample_track(TrackId::new(), "/m/b.flac", "B", "One", "Album X");
        let c = sample_track(TrackId::new(), "/m/c.flac", "C", "Two", "Album Y");
        for t in [&a, &b, &c] {
            repo.upsert(t).unwrap();
        }

        // Two days, three tracks, two artists and two albums.
        history.record_at(&a, 1_700_000_000, 100.0).unwrap();
        history.record_at(&a, 1_700_000_100, 100.0).unwrap();
        history.record_at(&b, 1_700_000_200, 100.0).unwrap();
        history.record_at(&c, 1_700_100_000, 50.0).unwrap();

        assert_eq!(history.count().unwrap(), 4);
        assert_eq!(history.count_since(Some(1_700_050_000)).unwrap(), 1);
        assert_eq!(
            history.total_listened_secs_since(None).unwrap(),
            350.0,
            "listening time is summed, not estimated from track lengths"
        );

        let artists = history.top_artists(None, 10).unwrap();
        assert_eq!(artists[0].name, "One");
        assert_eq!(artists[0].count, 3);
        assert_eq!(artists[1].name, "Two");

        let albums = history.top_albums(None, 10).unwrap();
        assert_eq!(albums[0].name, "Album X");
        assert_eq!(albums[0].detail, "One");
        assert_eq!(albums[0].count, 3);

        let tracks = history.top_tracks(None, 10).unwrap();
        assert_eq!(tracks[0].name, "A");
        assert_eq!(tracks[0].count, 2);

        let days = history.per_day(None).unwrap();
        assert_eq!(days.len(), 2, "two distinct local days");
        assert_eq!(days.iter().map(|(_, n)| n).sum::<usize>(), 4);
        // The days are consecutive-ish and ascending, which is what the
        // streak arithmetic relies on.
        assert!(days[0].0 < days[1].0);

        let hours = history.per_hour(None).unwrap();
        assert_eq!(hours.iter().sum::<usize>(), 4);
        assert_eq!(history.distinct_days().unwrap(), vec![days[0].0, days[1].0]);
    }

    #[test]
    fn test_moved_file_keeps_identity() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());
        let favorites = FavoritesRepo::new(db.clone());
        let playlists = PlaylistRepo::new(db.clone());

        let mut t1 = sample_track(
            TrackId::new(),
            "/music/old/album/01.flac",
            "Song",
            "Band",
            "Album",
        );
        t1.content_hash = Some("12345-abcdef".to_string());
        repo.upsert(&t1).unwrap();

        // The user favourites it and adds it to a playlist.
        favorites.add(&t1.id).unwrap();
        let pl = playlists.create("Mixtape", None).unwrap();
        playlists.add_track(&pl.id, &t1.id).unwrap();

        // The file is moved to a new folder; the new row has a fresh
        // path-derived id but the same content fingerprint.
        let moved = sample_track(
            TrackId::new(),
            "/music/new/album/01.flac",
            "Song",
            "Band",
            "Album",
        );
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
        assert_eq!(
            repo.get_by_id(&t1.id).unwrap().unwrap().title,
            "Song (Remaster)"
        );

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

        // Skip count: the tracks that keep getting jumped past.
        track_repo.increment_skip_count(&t2.id).unwrap();
        track_repo.increment_skip_count(&t2.id).unwrap();
        let mut sp_skip = SmartPlaylist::draft(PlaylistId::new());
        sp_skip.name = "Skipped".to_string();
        sp_skip.rules = vec![SmartRule {
            field: SmartField::SkipCount,
            op: SmartOp::GreaterThan,
            value: "1".to_string(),
        }];
        let sp_skip = sp_repo.save(&sp_skip).unwrap();
        let m_skip = sp_repo.evaluate(&sp_skip).unwrap();
        assert_eq!(m_skip.len(), 1);
        assert_eq!(m_skip[0].title, "Beta");

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

        // Incomplete rules are invalid and match nothing. Dropping the rule
        // would widen an AND playlist to "everything", which is worse than an
        // empty result the editor explains.
        let mut sp4 = SmartPlaylist::draft(PlaylistId::new());
        sp4.name = "Incomplete".to_string();
        sp4.rules = vec![SmartRule {
            field: SmartField::Title,
            op: SmartOp::Contains,
            value: "   ".to_string(),
        }];
        let sp4 = sp_repo.save(&sp4).unwrap();
        assert_eq!(sp_repo.evaluate(&sp4).unwrap().len(), 0);

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
        assert_eq!(all.len(), 7);
        let got = sp_repo.get(&saved.id).unwrap().unwrap();
        assert_eq!(got.rules.len(), 1);
        assert_eq!(got.rules[0].field, SmartField::Lossless);

        sp_repo.delete(&sp2.id).unwrap();
        assert_eq!(sp_repo.list().unwrap().len(), 6);
    }

    #[test]
    fn test_find_duplicates_groups_by_content_hash() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        // Two copies of the same audio (identical fingerprint, different tags).
        let a = TrackId::new();
        let b = TrackId::new();
        let mut ta = sample_track(a, "/music/a/01.flac", "Song", "Artist", "Album");
        ta.content_hash = Some("deadbeef".to_string());
        let mut tb = sample_track(b, "/music/backup/01 copy.flac", "Song (copy)", "Artist", "");
        tb.content_hash = Some("deadbeef".to_string());

        // A unique track, and one with no fingerprint at all.
        let c = TrackId::new();
        let mut tc = sample_track(c, "/music/unique.flac", "Unique", "Artist", "Album");
        tc.content_hash = Some("cafef00d".to_string());
        let d = TrackId::new();
        let td = sample_track(d, "/music/unhashed.flac", "Unhashed", "Artist", "Album");

        for t in [&ta, &tb, &tc, &td] {
            repo.upsert(t).unwrap();
        }

        let groups = repo.find_duplicates().unwrap();
        assert_eq!(groups.len(), 1, "only the two identical copies group");
        assert_eq!(groups[0].len(), 2);
        let ids: Vec<TrackId> = groups[0].iter().map(|t| t.id).collect();
        assert!(ids.contains(&a) && ids.contains(&b));
    }

    /// Groups are ordered largest first, whatever order the query returned.
    #[test]
    fn duplicate_groups_come_back_largest_first() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let mut tracks = Vec::new();
        for (i, hash) in ["two", "two", "three", "three", "three", "solo"]
            .iter()
            .enumerate()
        {
            let mut t = sample_track(
                TrackId::new(),
                &format!("/music/dup/{i}.flac"),
                &format!("Track {i}"),
                "X",
                "A",
            );
            t.content_hash = Some((*hash).to_string());
            repo.upsert(&t).unwrap();
            tracks.push(t);
        }

        let groups = repo.find_duplicates().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 3, "the largest group leads");
        assert_eq!(groups[1].len(), 2);
    }

    /// Deleting a folder must not touch a sibling whose name merely starts with
    /// the same text.
    #[test]
    fn path_prefix_operations_stop_at_a_directory_boundary() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let rock = sample_track(TrackId::new(), "/music/rock/a.flac", "A", "X", "Rock");
        let rockabilly = sample_track(
            TrackId::new(),
            "/music/rockabilly/b.flac",
            "B",
            "X",
            "Billy",
        );
        repo.upsert(&rock).unwrap();
        repo.upsert(&rockabilly).unwrap();

        // The folder listing is scoped to the folder itself.
        let listed = repo
            .get_tracks_by_folder(&PathBuf::from("/music/rock"))
            .unwrap();
        assert_eq!(listed.len(), 1, "only the file inside /music/rock");
        assert_eq!(listed[0].title, "A");

        let removed = repo
            .delete_by_path_prefix(&PathBuf::from("/music/rock"))
            .unwrap();
        assert_eq!(removed, 1, "the sibling directory must survive");
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(
            repo.get_by_path(&PathBuf::from("/music/rockabilly/b.flac"))
                .unwrap()
                .unwrap()
                .title,
            "B"
        );
    }

    /// `%` and `_` in a folder name are literals, not wildcards.
    #[test]
    fn path_prefix_operations_escape_like_wildcards() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let literal = sample_track(TrackId::new(), "/music/100% hits/a.flac", "A", "X", "Hits");
        let other = sample_track(TrackId::new(), "/music/100X hits/b.flac", "B", "X", "Hits");
        repo.upsert(&literal).unwrap();
        repo.upsert(&other).unwrap();

        let removed = repo
            .delete_by_path_prefix(&PathBuf::from("/music/100% hits"))
            .unwrap();
        assert_eq!(removed, 1, "the % must not match another character");
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(
            repo.list_paths_under_prefix(&PathBuf::from("/music/100% hits"))
                .unwrap()
                .len(),
            0
        );
    }

    /// Re-registering a scan root must return the stored row, not a phantom id.
    #[test]
    fn scan_root_add_is_idempotent() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = ScanRootsRepo::new(db.clone());

        let first = repo.add(&PathBuf::from("/music")).unwrap();
        let second = repo.add(&PathBuf::from("/music")).unwrap();
        assert_eq!(first.id, second.id, "the same path is one root");
        assert_eq!(repo.list().unwrap().len(), 1);

        // The returned id is real, so it can be updated.
        repo.update_last_scanned(&second.id, 1_700_000_000).unwrap();
        let roots = repo.list().unwrap();
        assert_eq!(roots[0].last_scanned_at, Some(1_700_000_000));

        repo.remove(&second.id).unwrap();
        assert!(repo.list().unwrap().is_empty());
    }

    /// A corrupt id must surface as an error, not as a freshly minted UUID that
    /// can never be matched back to its row.
    #[test]
    fn a_malformed_id_is_reported_rather_than_reinvented() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let good = sample_track(TrackId::new(), "/music/good.flac", "Good", "X", "A");
        repo.upsert(&good).unwrap();

        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, album, genre, year, track_number,
                    disc_number, duration_secs, sample_rate, channels, format, bitrate_kbps,
                    is_lossless, replaygain_track_db, replaygain_track_peak, r128_loudness_lufs,
                    file_size_bytes, mtime_unix_secs, content_hash, is_favorite, play_count,
                    last_played_at, added_at)
                 VALUES ('not-a-uuid', '/music/bad.flac', 'Bad', 'X', 'A', NULL, NULL, NULL,
                    NULL, 1.0, 44100, 2, 'FLAC', NULL, 1, NULL, NULL, NULL, 0, 0, NULL, 0, 0,
                    NULL, 0);",
                [],
            )?;
            Ok(())
        })
        .expect("fixture insert");

        assert!(
            repo.list(SortField::Title, SortOrder::Ascending).is_err(),
            "a corrupt id must not be silently reinvented"
        );
    }

    /// A settings blob written by a build with a different field set still
    /// loads, so an upgrade never wipes the user's configuration.
    #[test]
    fn settings_from_an_older_build_load_instead_of_resetting() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = SettingsRepo::new(db.clone());

        repo.set_value("app_settings", r#"{"theme_dark":false,"volume":0.25}"#)
            .unwrap();

        let loaded = repo.load().unwrap();
        assert!(!loaded.theme_dark, "present fields are honoured");
        assert!((loaded.volume - 0.25).abs() < f32::EPSILON);
        assert_eq!(
            loaded.sort_field,
            AppSettings::default().sort_field,
            "absent fields fall back to their defaults"
        );
    }

    /// Liner notes are keyed by the same stable identity the album list uses,
    /// and they do not disappear when the album's tracks are deleted: notes
    /// are user writing, not a cache of the library.
    #[test]
    fn album_notes_round_trip_and_outlive_their_tracks() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let tracks = TrackRepo::new(db.clone());
        let notes = AlbumNotesRepo::new(db.clone());
        let track = sample_track(
            TrackId::new(),
            "/music/notes/01.flac",
            "One",
            "The Artist",
            "The Album",
        );
        tracks.upsert(&track).unwrap();

        let id = AlbumId::from_identity("The Album", "The Artist");
        assert!(notes.get(&id).unwrap().is_none());
        notes
            .save(
                &id,
                "The Album",
                "The Artist",
                "# Liner notes\n\nA **quiet** record.",
            )
            .unwrap();

        let loaded = notes.get(&id).unwrap().expect("saved note");
        assert_eq!(loaded.album_id, id);
        assert_eq!(loaded.title, "The Album");
        assert_eq!(loaded.artist, "The Artist");
        assert_eq!(loaded.markdown, "# Liner notes\n\nA **quiet** record.");
        assert!(loaded.updated_at > 0);
        assert_eq!(notes.list().unwrap().len(), 1);
        assert_eq!(notes.ids_with_notes().unwrap(), vec![id]);

        // The same title by another artist is another album, not a collision.
        let other = AlbumId::from_identity("The Album", "Another Artist");
        assert!(notes.get(&other).unwrap().is_none());

        tracks.delete(&track.id).unwrap();
        assert!(
            notes.get(&id).unwrap().is_some(),
            "removing a file must not erase the user's writing"
        );

        assert!(notes.delete(&id).unwrap());
        assert!(!notes.delete(&id).unwrap(), "deleting twice is harmless");
        assert!(notes.get(&id).unwrap().is_none());
    }

    #[test]
    fn blank_notes_delete_instead_of_storing_an_empty_document() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let notes = AlbumNotesRepo::new(db.clone());
        let id = AlbumId::from_identity("Album", "Artist");
        notes.save(&id, "Album", "Artist", "Something").unwrap();
        notes.save(&id, "Album", "Artist", "  \n\t ").unwrap();
        assert!(notes.get(&id).unwrap().is_none());
        assert!(notes.list().unwrap().is_empty());
    }

    #[test]
    fn absurdly_large_liner_notes_are_refused() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let notes = AlbumNotesRepo::new(db.clone());
        let id = AlbumId::from_identity("Album", "Artist");
        let huge = "x".repeat(256 * 1024 + 1);
        assert!(notes.save(&id, "Album", "Artist", &huge).is_err());
        assert!(notes.save(&id, "Album", "Artist", "a\0b").is_err());
        assert!(notes.get(&id).unwrap().is_none());
    }

    /// An album's year is a property of the album, not of whichever row SQLite
    /// happened to pick for the group.
    #[test]
    fn album_year_is_aggregated_deterministically() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let track_repo = TrackRepo::new(db.clone());
        let album_repo = AlbumRepo::new(db.clone());

        let mut older = sample_track(TrackId::new(), "/music/repress/a.flac", "A", "X", "Reissue");
        older.year = Some(1994);
        let mut newer = sample_track(TrackId::new(), "/music/repress/b.flac", "B", "X", "Reissue");
        newer.year = Some(2011);
        track_repo.upsert(&older).unwrap();
        track_repo.upsert(&newer).unwrap();

        let albums = album_repo.list().unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(
            albums[0].year,
            Some(1994),
            "the earliest year is the album's"
        );
        assert_eq!(albums[0].track_count, 2);
    }

    /// Punctuation must not put music out of reach: the tokenizer splits
    /// `AC/DC` into `ac` and `dc`, so the query has to ask for those.
    #[test]
    fn search_reaches_artist_names_with_punctuation() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let mut acdc = sample_track(
            TrackId::new(),
            "/music/rock/01.flac",
            "Thunderstruck",
            "AC/DC",
            "The Razors Edge",
        );
        acdc.artist = "AC/DC".to_string();
        repo.upsert(&acdc).unwrap();
        let mut dont = sample_track(
            TrackId::new(),
            "/music/pop/02.flac",
            "Don't Stop",
            "Fleetwood Mac",
            "Rumours",
        );
        dont.title = "Don't Stop".to_string();
        repo.upsert(&dont).unwrap();

        let hits = |q: &str| repo.search_fts(q, 20).unwrap().len();
        assert_eq!(hits("AC/DC"), 1, "punctuation must not hide the artist");
        assert_eq!(hits("acdc"), 0, "no such token was indexed");
        assert_eq!(hits("Thunder"), 1, "prefix search still works");
        assert_eq!(hits("Don't Stop"), 1, "apostrophes are token separators");
        assert_eq!(hits("rumours"), 1, "case-insensitive");
        assert_eq!(
            hits("!!!"),
            0,
            "no tokens means no results, not a syntax error"
        );
    }

    #[test]
    fn ratings_are_clamped_bulk_rated_sorted_and_cleared() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = TrackRepo::new(db.clone());

        let a = sample_track(TrackId::new(), "/music/a.flac", "A", "X", "A");
        let b = sample_track(TrackId::new(), "/music/b.flac", "B", "X", "A");
        let c = sample_track(TrackId::new(), "/music/c.flac", "C", "X", "A");
        for track in [&a, &b, &c] {
            repo.upsert(track).unwrap();
        }

        repo.set_rating(&a.id, 9).unwrap();
        assert_eq!(
            repo.get_by_id(&a.id).unwrap().unwrap().rating,
            5,
            "a rating above five is clamped, not stored as an impossible value"
        );

        repo.rate_many(&[b.id, c.id], 3).unwrap();
        assert_eq!(repo.get_by_id(&b.id).unwrap().unwrap().rating, 3);
        assert_eq!(repo.get_by_id(&c.id).unwrap().unwrap().rating, 3);

        repo.clear_rating(&c.id).unwrap();
        let ascending = repo.list(SortField::Rating, SortOrder::Ascending).unwrap();
        assert_eq!(
            ascending.iter().map(|t| t.rating).collect::<Vec<_>>(),
            vec![0, 3, 5]
        );
        let descending = repo.list(SortField::Rating, SortOrder::Descending).unwrap();
        assert_eq!(
            descending.iter().map(|t| t.rating).collect::<Vec<_>>(),
            vec![5, 3, 0]
        );
    }

    #[test]
    fn a_rating_survives_closing_and_reopening_the_database() {
        let path = std::env::temp_dir().join(format!(
            "sp_rating_persist_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let track = sample_track(TrackId::new(), "/music/persist.flac", "Star", "X", "A");
        let id = track.id;

        {
            let db = Database::open(&path).expect("open database");
            let repo = TrackRepo::new(db);
            repo.upsert(&track).unwrap();
            repo.set_rating(&id, 4).unwrap();
        }

        {
            let db = Database::open(&path).expect("reopen database");
            let repo = TrackRepo::new(db);
            let loaded = repo.get_by_id(&id).unwrap().expect("track survives");
            assert_eq!(loaded.rating, 4, "the rating belongs to the library row");
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn new_smart_fields_filter_and_invalid_rules_match_nothing() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = SmartPlaylistRepo::new(db.clone());
        let track_repo = TrackRepo::new(db.clone());

        let mut a = sample_track(TrackId::new(), "/music/a.flac", "A", "X", "A");
        a.rating = 5;
        a.has_lyrics = true;
        a.bit_depth = Some(24);
        let mut b = sample_track(TrackId::new(), "/music/b.flac", "B", "X", "A");
        b.rating = 2;
        b.has_lyrics = false;
        b.bit_depth = Some(16);
        b.channels = 6;
        b.bitrate_kbps = Some(320);
        b.play_count = 5;
        let mut c = sample_track(TrackId::new(), "/music/c.flac", "C", "X", "A");
        c.has_lyrics = true;
        c.bit_depth = Some(24);
        for track in [&a, &b, &c] {
            track_repo.upsert(track).unwrap();
        }

        let evaluate = |field: SmartField, op: SmartOp, value: &str| {
            let mut playlist = SmartPlaylist::draft(PlaylistId::new());
            playlist.name = format!("{} {}", field.label(), op.label());
            playlist.rules = vec![SmartRule {
                field,
                op,
                value: value.to_string(),
            }];
            repo.save(&playlist).unwrap();
            repo.evaluate(&playlist).unwrap()
        };

        let rated = evaluate(SmartField::Rating, SmartOp::GreaterThan, "3");
        assert_eq!(rated.iter().map(|t| t.id).collect::<Vec<_>>(), vec![a.id]);

        let lyrics = evaluate(SmartField::HasLyrics, SmartOp::IsTrue, "");
        assert_eq!(lyrics.len(), 2, "two tracks have lyrics");

        let never_played = evaluate(SmartField::NeverPlayed, SmartOp::IsTrue, "");
        assert_eq!(
            never_played.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![a.id, c.id],
            "play_count is the source of truth, not a stored flag"
        );

        let hi_depth = evaluate(SmartField::BitDepth, SmartOp::GreaterThan, "16");
        assert_eq!(
            hi_depth.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![a.id, c.id]
        );

        let multi_chan = evaluate(SmartField::Channels, SmartOp::GreaterThan, "2");
        assert_eq!(
            multi_chan.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![b.id],
            "channels field evaluates correctly"
        );

        let hi_bitrate = evaluate(SmartField::Bitrate, SmartOp::GreaterThan, "500");
        assert_eq!(
            hi_bitrate.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![a.id, c.id],
            "bitrate field evaluates correctly"
        );

        // An impossible value is rejected visibly before it reaches SQL, and
        // evaluation matches nothing rather than silently dropping the rule.
        let mut invalid = SmartPlaylist::draft(PlaylistId::new());
        invalid.name = "Impossible".to_string();
        invalid.rules = vec![SmartRule {
            field: SmartField::Rating,
            op: SmartOp::GreaterThan,
            value: "9".to_string(),
        }];
        assert!(invalid.validation_error().is_some());
        repo.save(&invalid).unwrap();
        assert!(repo.evaluate(&invalid).unwrap().is_empty());

        let mut incomplete = SmartPlaylist::draft(PlaylistId::new());
        incomplete.name = "Incomplete".to_string();
        incomplete.rules = vec![SmartRule {
            field: SmartField::Title,
            op: SmartOp::Contains,
            value: "   ".to_string(),
        }];
        assert!(incomplete.validation_error().is_some());
        assert!(repo.evaluate(&incomplete).unwrap().is_empty());
    }

    /// Smart-playlist rules that the field cannot express are repaired on load,
    /// instead of silently matching nothing.
    #[test]
    fn smart_playlist_rules_are_normalised_on_load() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = SmartPlaylistRepo::new(db.clone());
        let track_repo = TrackRepo::new(db.clone());

        let mut a = sample_track(TrackId::new(), "/music/a.flac", "Alpha", "X", "A");
        a.added_at = 1_700_000_000;
        track_repo.upsert(&a).unwrap();
        let mut b = sample_track(TrackId::new(), "/music/b.flac", "Beta", "X", "B");
        b.is_favorite = true;
        track_repo.upsert(&b).unwrap();
        TrackRepo::new(db.clone())
            .set_favorite(&b.id, true)
            .unwrap();

        // `Date Added is 2024` can never be true as stored: `added_at` is a
        // unix timestamp. It is read as "in the last 2024 days" instead.
        let mut playlist = SmartPlaylist::draft(PlaylistId::new());
        playlist.name = "Recent-ish".to_string();
        playlist.rules = vec![SmartRule {
            field: SmartField::Added,
            op: SmartOp::Equals,
            value: "2024".to_string(),
        }];
        repo.save(&playlist).unwrap();

        let loaded = repo.get(&playlist.id).unwrap().expect("playlist");
        assert_eq!(loaded.rules[0].op, SmartOp::InLastDays);
        let matched = repo.evaluate(&loaded).unwrap();
        assert_eq!(matched.len(), 2, "the repaired rule is satisfiable");
    }

    /// Removing the middle of a playlist renumbers the rest in order.
    #[test]
    fn removing_a_playlist_track_keeps_the_remaining_order() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let playlists = PlaylistRepo::new(db.clone());
        let tracks = TrackRepo::new(db.clone());

        let mut ids = Vec::new();
        for i in 0..4 {
            let t = sample_track(
                TrackId::new(),
                &format!("/music/{i}.flac"),
                &format!("Track {i}"),
                "X",
                "A",
            );
            tracks.upsert(&t).unwrap();
            ids.push(t.id);
        }

        let playlist = playlists.create("Order", None).unwrap();
        for id in &ids {
            playlists.add_track(&playlist.id, id).unwrap();
        }
        playlists.remove_track(&playlist.id, &ids[1]).unwrap();

        let remaining: Vec<TrackId> = playlists
            .get_tracks(&playlist.id)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(remaining, vec![ids[0], ids[2], ids[3]], "order survives");
    }

    /// A rule JSON blob that will not parse must not turn an AND playlist into
    /// "match everything".
    #[test]
    fn corrupt_smart_playlist_rules_do_not_widen_the_match() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = SmartPlaylistRepo::new(db.clone());
        let track_repo = TrackRepo::new(db.clone());
        track_repo
            .upsert(&sample_track(
                TrackId::new(),
                "/music/a.flac",
                "A",
                "X",
                "A",
            ))
            .unwrap();

        let mut playlist = SmartPlaylist::draft(PlaylistId::new());
        playlist.name = "Broken".to_string();
        repo.save(&playlist).unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE smart_playlists SET rules_json = 'not json' WHERE id = ?1;",
                rusqlite::params![playlist.id.to_string()],
            )?;
            Ok(())
        })
        .unwrap();

        let loaded = repo.get(&playlist.id).unwrap().expect("playlist");
        assert!(loaded.rules.is_empty());
        // An empty AND playlist matches nothing, which is the safe reading.
        assert!(repo.evaluate(&loaded).unwrap().is_empty());
    }
}

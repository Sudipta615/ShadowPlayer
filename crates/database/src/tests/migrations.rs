#[cfg(test)]
mod unit {
    use crate::db::Database;
    use crate::repos::*;
    use domain::ids::TrackId;
    use domain::models::Track;

    use std::path::PathBuf;

    #[allow(dead_code)]
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

    /// V5 adds the sub-second mtime column, the indexes the hot queries need,
    /// and replaces the blanket FTS trigger with one scoped to the indexed
    /// columns. It must also survive a lost migration row.
    #[test]
    fn migration_v5_adds_mtimes_indexes_and_scopes_the_fts_trigger() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (
                 id TEXT PRIMARY KEY NOT NULL,
                 title TEXT NOT NULL,
                 artist TEXT NOT NULL,
                 album TEXT NOT NULL,
                 genre TEXT,
                 path TEXT NOT NULL,
                 play_count INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE tracks_fts (
                 track_id TEXT, title TEXT, artist TEXT, album TEXT, genre TEXT, path TEXT
             );
             CREATE TABLE play_history (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, track_id TEXT NOT NULL
             );
             CREATE TABLE playlist_tracks (
                 playlist_id TEXT NOT NULL, track_id TEXT NOT NULL, position INTEGER NOT NULL
             );
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES (4, 0);
             -- The old trigger fired for *any* UPDATE, including play counts.
             CREATE TRIGGER tracks_au AFTER UPDATE ON tracks BEGIN
                 DELETE FROM tracks_fts WHERE track_id = old.id;
                 INSERT INTO tracks_fts(track_id, title, artist, album, genre, path)
                 VALUES (new.id, new.title, new.artist, new.album, coalesce(new.genre, ''), new.path);
             END;
             INSERT INTO tracks (id, title, artist, album, path) VALUES ('t1', 'Real Title', 'A', 'B', '/music/a.flac');
             -- A sentinel standing in for the indexed row: only the trigger can
             -- change it, so watching it shows whether the trigger fired.
             INSERT INTO tracks_fts VALUES ('t1', 'sentinel', 'A', 'B', '', '/music/a.flac');",
        )
        .unwrap();

        crate::schema::apply_v5_for_test(&mut conn).expect("V5 must apply");

        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(tracks);").unwrap();
            stmt.query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert!(columns.iter().any(|c| c == "mtime_nanos"), "column added");

        let index_names: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name;")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        for wanted in [
            "idx_history_track",
            "idx_playlist_tracks_track",
            "idx_tracks_genre",
            "idx_tracks_album_artist",
        ] {
            assert!(
                index_names.iter().any(|name| name == wanted),
                "{wanted} must exist; found {index_names:?}"
            );
        }

        let trigger: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'tracks_au';",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            trigger.contains("UPDATE OF title, artist, album, genre, path"),
            "the trigger must be scoped to the indexed columns: {trigger}"
        );

        // Play counts update on the hot path; the search index must not care.
        conn.execute(
            "UPDATE tracks SET play_count = play_count + 1 WHERE id = 't1';",
            [],
        )
        .unwrap();
        let title: String = conn
            .query_row(
                "SELECT title FROM tracks_fts WHERE track_id = 't1';",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            title, "sentinel",
            "a play-count update must not rebuild the index"
        );

        // A real editorial change still does.
        conn.execute("UPDATE tracks SET title = 'New Title' WHERE id = 't1';", [])
            .unwrap();
        let title: String = conn
            .query_row(
                "SELECT title FROM tracks_fts WHERE track_id = 't1';",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "New Title");

        // A lost migration row must not make the ALTER run twice.
        conn.execute("DELETE FROM schema_migrations WHERE version = 5;", [])
            .unwrap();
        crate::schema::apply_v5_for_test(&mut conn)
            .expect("re-applying V5 with the column present must be safe");
    }

    #[test]
    fn migration_v10_records_the_file_identity_and_merges_the_rows_that_share_one() {
        // Two rows for one file: the case-insensitive-filesystem bug, where
        // `Track.FLAC` and `track.flac` are the same file under two spellings.
        let root = std::env::temp_dir().join(format!(
            "sp_v10_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // The file is now spelled `track.flac`; `Track.FLAC` is the spelling the
        // stale row still carries. On a case-insensitive filesystem both
        // resolve to this one file; on a case-sensitive one only the real
        // spelling does — which is exactly why the stale row's identity was
        // recorded when the file still had that name.
        let file = root.join("track.flac");
        std::fs::write(&file, b"x").unwrap();
        let identity = domain::file_identity::file_identity(&file)
            .expect("an existing file has an identity")
            .to_key();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (
                 id TEXT PRIMARY KEY NOT NULL,
                 title TEXT NOT NULL,
                 artist TEXT NOT NULL,
                 album TEXT NOT NULL,
                 genre TEXT,
                 path TEXT NOT NULL,
                 play_count INTEGER NOT NULL DEFAULT 0,
                 is_favorite INTEGER NOT NULL DEFAULT 0,
                 skip_count INTEGER NOT NULL DEFAULT 0,
                 last_played_at INTEGER,
                 added_at INTEGER NOT NULL DEFAULT 0,
                 content_hash TEXT,
                 file_id TEXT
             );
             CREATE TABLE favorites (track_id TEXT PRIMARY KEY NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE playlist_tracks (
                 id TEXT PRIMARY KEY NOT NULL, playlist_id TEXT NOT NULL, track_id TEXT NOT NULL,
                 position INTEGER NOT NULL, added_at INTEGER NOT NULL
             );
             CREATE TABLE play_history (
                 id TEXT PRIMARY KEY NOT NULL, track_id TEXT NOT NULL, played_at INTEGER NOT NULL,
                 duration_listened_secs REAL NOT NULL, completed INTEGER NOT NULL
             );
             CREATE TABLE scrobbles (
                 id TEXT PRIMARY KEY NOT NULL, track_id TEXT, artist TEXT NOT NULL, title TEXT NOT NULL,
                 album TEXT NOT NULL, played_at INTEGER NOT NULL,
                 duration_secs REAL NOT NULL DEFAULT 0, listened_secs REAL NOT NULL DEFAULT 0
             );
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES (9, 0);",
        )
        .unwrap();

        let stale = domain::paths::encode_path_key(&root.join("Track.FLAC"));
        let real = domain::paths::encode_path_key(&file);
        // The stale row was indexed while the file still had that name, so it
        // carries the identity the filesystem gave it then.
        conn.execute(
            "INSERT INTO tracks (id, title, artist, album, path, play_count, is_favorite, added_at, file_id)
             VALUES ('stale', 'T', 'A', 'B', ?1, 2, 1, 100, ?3);",
            rusqlite::params![stale, real, identity],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, title, artist, album, path, play_count, added_at, file_id)
             VALUES ('current', 'T', 'A', 'B', ?1, 9, 200, ?2);",
            rusqlite::params![real, identity],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO favorites (track_id, created_at) VALUES ('stale', 5);",
            [],
        )
        .unwrap();

        crate::schema::apply_v10_for_test(&mut conn).expect("V10 must apply");

        // One file, one row.
        let rows: Vec<(String, i64, i64)> = {
            let mut stmt = conn
                .prepare("SELECT id, play_count, is_favorite FROM tracks ORDER BY id;")
                .unwrap();
            let mapped = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .unwrap();
            let mut out = Vec::new();
            for row in mapped {
                out.push(row.unwrap());
            }
            out
        };

        assert_eq!(rows.len(), 1, "one file, one row: {rows:?}");
        let (id, play_count, is_favorite) = &rows[0];
        // The survivor is the row with the most plays.
        assert_eq!(id, "current", "the most-played row survives");
        assert_eq!(*play_count, 11, "the play counts are added together");
        assert_eq!(*is_favorite, 1, "the favourite flag is carried over");

        // The favourite moved to the survivor.
        let favourite_owner: String = conn
            .query_row("SELECT track_id FROM favorites;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(favourite_owner, "current");

        // And the identity was recorded.
        let file_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT file_id FROM tracks;").unwrap();
            let mapped = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            let mut out = Vec::new();
            for row in mapped {
                out.push(row.unwrap());
            }
            out
        };
        assert_eq!(file_ids, vec![identity]);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Two rows for one file that is present under *both* names — a hard link —
    /// are left alone. The library is holding one file on purpose, and merging
    /// the rows would silently drop one of them.
    #[test]
    fn migration_v10_leaves_a_hard_link_alone() {
        let root = std::env::temp_dir().join(format!(
            "sp_v10_link_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.flac");
        let link = root.join("link.flac");
        std::fs::write(&original, b"x").unwrap();
        std::fs::hard_link(&original, &link).unwrap();
        let identity = domain::file_identity::file_identity(&original)
            .expect("an existing file has an identity")
            .to_key();

        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (
                 id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, artist TEXT NOT NULL,
                 album TEXT NOT NULL, genre TEXT, path TEXT NOT NULL,
                 play_count INTEGER NOT NULL DEFAULT 0, is_favorite INTEGER NOT NULL DEFAULT 0,
                 skip_count INTEGER NOT NULL DEFAULT 0, last_played_at INTEGER,
                 added_at INTEGER NOT NULL DEFAULT 0, content_hash TEXT, file_id TEXT
             );
             CREATE TABLE favorites (track_id TEXT PRIMARY KEY NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE playlist_tracks (
                 id TEXT PRIMARY KEY NOT NULL, playlist_id TEXT NOT NULL, track_id TEXT NOT NULL,
                 position INTEGER NOT NULL, added_at INTEGER NOT NULL
             );
             CREATE TABLE play_history (
                 id TEXT PRIMARY KEY NOT NULL, track_id TEXT NOT NULL, played_at INTEGER NOT NULL,
                 duration_listened_secs REAL NOT NULL, completed INTEGER NOT NULL
             );
             CREATE TABLE scrobbles (
                 id TEXT PRIMARY KEY NOT NULL, track_id TEXT, artist TEXT NOT NULL, title TEXT NOT NULL,
                 album TEXT NOT NULL, played_at INTEGER NOT NULL,
                 duration_secs REAL NOT NULL DEFAULT 0, listened_secs REAL NOT NULL DEFAULT 0
             );
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES (9, 0);",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO tracks (id, title, artist, album, path, play_count, added_at, file_id)
             VALUES ('a', 'T', 'A', 'B', ?1, 1, 100, ?3);",
            rusqlite::params![
                domain::paths::encode_path_key(&original),
                domain::paths::encode_path_key(&link),
                identity
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, title, artist, album, path, play_count, added_at, file_id)
             VALUES ('b', 'T', 'A', 'B', ?1, 1, 100, ?2);",
            rusqlite::params![domain::paths::encode_path_key(&link), identity],
        )
        .unwrap();

        crate::schema::apply_v10_for_test(&mut conn).expect("V10 must apply");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "a hard link is two entries, and stays two");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn migration_v11_defaults_old_tracks_to_unrated_and_adds_the_indexes() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (
                 id TEXT PRIMARY KEY NOT NULL,
                 title TEXT NOT NULL,
                 artist TEXT NOT NULL,
                 album TEXT NOT NULL,
                 genre TEXT,
                 path TEXT NOT NULL
             );
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES (10, 0);
             INSERT INTO tracks (id, title, artist, album, path)
             VALUES ('old', 'Old', 'Artist', 'Album', '/music/old.flac');",
        )
        .unwrap();

        crate::schema::apply_v11_for_test(&mut conn).expect("V11 must apply");

        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(tracks);").unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        for name in ["rating", "bit_depth", "has_lyrics"] {
            assert!(columns.iter().any(|column| column == name), "{columns:?}");
        }

        let defaults: (i64, Option<i64>, i64) = conn
            .query_row(
                "SELECT rating, bit_depth, has_lyrics FROM tracks WHERE id = 'old';",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            defaults,
            (0, None, 0),
            "an old row is unrated, has unknown depth, and has no known lyrics"
        );

        let indexes: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = 'index';")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        for name in [
            "idx_tracks_rating",
            "idx_tracks_bit_depth",
            "idx_tracks_has_lyrics",
        ] {
            assert!(indexes.iter().any(|index| index == name), "{indexes:?}");
        }

        // A lost migration row must not make the ALTER run twice.
        conn.execute("DELETE FROM schema_migrations WHERE version = 11;", [])
            .unwrap();
        crate::schema::apply_v11_for_test(&mut conn)
            .expect("re-applying V11 with the columns present must be safe");
    }

    /// V4 rebuilds the queue: real entries are carried over (in order), an
    /// orphan is dropped, and the two unused tables are gone.
    #[test]
    fn migration_v4_rebuilds_the_queue_and_drops_the_dead_tables() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (id TEXT PRIMARY KEY NOT NULL);
             CREATE TABLE saved_queue (
                 position INTEGER PRIMARY KEY NOT NULL,
                 track_id TEXT NOT NULL,
                 path TEXT NOT NULL
             );
             CREATE TABLE lyrics (track_id TEXT PRIMARY KEY NOT NULL, content_json TEXT NOT NULL);
             CREATE TABLE artwork (id TEXT PRIMARY KEY NOT NULL, cache_path TEXT NOT NULL);
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at INTEGER NOT NULL);
             INSERT INTO schema_migrations VALUES (3, 0);
             INSERT INTO tracks (id) VALUES ('real-track'), ('second-track');
             INSERT INTO saved_queue VALUES (0, 'real-track', '/music/real.flac');
             INSERT INTO saved_queue VALUES (1, 'ghost', '/music/gone.flac');
             INSERT INTO saved_queue VALUES (2, 'second-track', '/music/second.flac');",
        )
        .unwrap();

        crate::schema::apply_v4_for_test(&mut conn).expect("V4 must apply");

        let columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(saved_queue);").unwrap();
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows
        };
        assert_eq!(
            columns,
            vec!["position".to_string(), "track_id".to_string()]
        );
        assert!(
            columns.iter().all(|c| c != "path"),
            "the write-only path column must be gone"
        );

        let queue: Vec<(i64, String)> = {
            let mut stmt = conn
                .prepare("SELECT position, track_id FROM saved_queue ORDER BY position;")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows
        };
        assert_eq!(
            queue,
            vec![
                (0i64, "real-track".to_string()),
                (1, "second-track".to_string())
            ],
            "real entries in order, orphan dropped"
        );

        for table in ["lyrics", "artwork"] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1;",
                    rusqlite::params![table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(exists, 0, "{table} must be dropped");
        }

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations;", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, 4);
    }

    /// Once rebuilt, the queue refuses a row for a track that does not exist.
    #[test]
    fn the_rebuilt_queue_enforces_its_foreign_key() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let result = db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO saved_queue (position, track_id) VALUES (0, 'no-such-track');",
                [],
            )?;
            Ok(())
        });
        assert!(result.is_err(), "an orphan queue row must be rejected");
    }

    #[test]
    fn zone_repo_round_trip_and_migration_v16() {
        let db = Database::in_memory().expect("In-memory DB creation failed");
        let repo = ZoneRepo::new(db.clone());

        // Migration V16 should have seeded the default Main Zone
        let zones = repo.list_zones().unwrap();
        assert!(!zones.is_empty());
        let default_zone = zones.iter().find(|z| z.is_default).expect("default zone");
        assert_eq!(default_zone.name, "Main Zone");

        // Create a custom secondary zone
        let mut dac_zone =
            domain::zone::ZoneConfig::new_custom("Living Room DAC", Some("USB Audio DAC".into()));
        dac_zone.set_timing_offset(15.5);
        dac_zone.set_clock_drift(0.025);
        dac_zone.linked_master_id = Some(default_zone.id);
        repo.save_zone(&dac_zone).unwrap();

        // Verify retrieval
        let loaded_dac = repo
            .get_zone(dac_zone.id)
            .unwrap()
            .expect("loaded dac zone");
        assert_eq!(loaded_dac.name, "Living Room DAC");
        assert_eq!(loaded_dac.output_device.as_deref(), Some("USB Audio DAC"));
        assert_eq!(loaded_dac.timing_offset_ms, 15.5);
        assert_eq!(loaded_dac.clock_drift_pct, 0.025);
        assert_eq!(loaded_dac.linked_master_id, Some(default_zone.id));

        // Create a ZoneSwitch rule
        let rule = domain::zone::ZoneSwitchRule::new(
            "Auto Route DSD",
            dac_zone.id,
            vec![domain::zone::ZoneSwitchCondition::new(
                domain::zone::ZoneMatchField::Format,
                domain::smart::SmartOp::Equals,
                "DSD",
            )],
            100,
        );
        repo.save_rule(&rule).unwrap();

        let rules = repo.list_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "Auto Route DSD");
        assert_eq!(rules[0].target_zone_id, dac_zone.id);

        // Cannot delete default zone
        assert!(!repo.delete_zone(default_zone.id).unwrap());

        // Can delete custom zone
        assert!(repo.delete_zone(dac_zone.id).unwrap());
        assert!(repo.get_zone(dac_zone.id).unwrap().is_none());

        // Cascade deleted rule
        let rules_after = repo.list_rules().unwrap();
        assert!(rules_after.is_empty());
    }

    #[test]
    fn migration_v17_creates_the_recommendation_tables_and_reapplies_cleanly() {
        let db = Database::in_memory().expect("In-memory DB creation failed");

        // Re-run from a database that claims to be at V16: V17 is idempotent
        // and must create its tables even though the earlier migrations ran.
        db.with_conn(|conn| {
            conn.execute("DELETE FROM schema_migrations WHERE version = 17;", [])?;
            crate::schema::run_migrations(conn)
        })
        .unwrap();

        let version: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT MAX(version) FROM schema_migrations;", [], |row| {
                    row.get(0)
                })
            })
            .unwrap();
        assert_eq!(version, 17);

        let tables: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                      WHERE type = 'table'
                        AND name IN ('track_features', 'recommendation_feedback');",
                    [],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(tables, 2);
    }
}

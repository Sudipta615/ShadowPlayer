use log::{info, warn};
use rusqlite::{Connection, Result};

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;

    // Confirm the pragmas rather than assuming them. `journal_mode` and
    // `foreign_keys` can silently refuse to apply (network filesystems, a DB
    // moved between machines), and with `foreign_keys` off every
    // `ON DELETE CASCADE` quietly stops pruning favorites, playlist entries
    // and play history.
    match conn.query_row("PRAGMA journal_mode = WAL;", [], |r| r.get::<_, String>(0)) {
        Ok(mode) if mode.eq_ignore_ascii_case("wal") => {}
        Ok(mode) => warn!("SQLite journal_mode is {mode}, not WAL; concurrency will suffer"),
        Err(e) => warn!("Could not enable WAL journal mode: {e}"),
    }
    if let Err(e) = conn.execute_batch("PRAGMA synchronous = NORMAL;") {
        warn!("Could not set PRAGMA synchronous: {e}");
    }
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys;", [], |r| r.get(0))?;
    if foreign_keys != 1 {
        warn!(
            "SQLite foreign keys are OFF; deletes will not cascade and orphan \
             rows will accumulate"
        );
    }

    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            applied_at INTEGER NOT NULL
        );",
        [],
    )?;

    let current_version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current_version < 1 {
        apply_v1(conn)?;
    }

    if current_version < 2 {
        apply_v2(conn)?;
    }

    if current_version < 3 {
        apply_v3(conn)?;
    }

    if current_version < 4 {
        apply_v4(conn)?;
    }

    if current_version < 5 {
        apply_v5(conn)?;
    }

    Ok(())
}

fn apply_v1(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V1");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS tracks (
            id TEXT PRIMARY KEY NOT NULL,
            path TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            album TEXT NOT NULL,
            genre TEXT,
            year INTEGER,
            track_number INTEGER,
            disc_number INTEGER,
            duration_secs REAL NOT NULL,
            sample_rate INTEGER NOT NULL,
            channels INTEGER NOT NULL,
            format TEXT NOT NULL,
            bitrate_kbps INTEGER,
            is_lossless INTEGER NOT NULL,
            replaygain_track_db REAL,
            replaygain_track_peak REAL,
            r128_loudness_lufs REAL,
            file_size_bytes INTEGER NOT NULL,
            mtime_unix_secs INTEGER NOT NULL,
            is_favorite INTEGER NOT NULL DEFAULT 0,
            play_count INTEGER NOT NULL DEFAULT 0,
            last_played_at INTEGER,
            added_at INTEGER NOT NULL
        );",
        "CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);",
        "CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);",
        "CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);",
        "CREATE INDEX IF NOT EXISTS idx_tracks_favorite ON tracks(is_favorite);",
        "CREATE INDEX IF NOT EXISTS idx_tracks_play_count ON tracks(play_count);",
        "CREATE INDEX IF NOT EXISTS idx_tracks_last_played ON tracks(last_played_at);",
        "CREATE TABLE IF NOT EXISTS artists (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL UNIQUE,
            track_count INTEGER NOT NULL DEFAULT 0,
            album_count INTEGER NOT NULL DEFAULT 0
        );",
        "CREATE TABLE IF NOT EXISTS albums (
            id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            year INTEGER,
            track_count INTEGER NOT NULL DEFAULT 0,
            artwork_path TEXT,
            UNIQUE(title, artist)
        );",
        "CREATE TABLE IF NOT EXISTS genres (
            name TEXT PRIMARY KEY NOT NULL,
            track_count INTEGER NOT NULL DEFAULT 0
        );",
        "CREATE TABLE IF NOT EXISTS folders (
            id TEXT PRIMARY KEY NOT NULL,
            path TEXT NOT NULL UNIQUE,
            parent_path TEXT
        );",
        "CREATE TABLE IF NOT EXISTS playlists (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL UNIQUE,
            description TEXT,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS playlist_tracks (
            id TEXT PRIMARY KEY NOT NULL,
            playlist_id TEXT NOT NULL,
            track_id TEXT NOT NULL,
            position INTEGER NOT NULL,
            added_at INTEGER NOT NULL,
            FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );",
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_pl ON playlist_tracks(playlist_id, position);",
        "CREATE TABLE IF NOT EXISTS favorites (
            track_id TEXT PRIMARY KEY NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );",
        "CREATE TABLE IF NOT EXISTS play_history (
            id TEXT PRIMARY KEY NOT NULL,
            track_id TEXT NOT NULL,
            played_at INTEGER NOT NULL,
            duration_listened_secs REAL NOT NULL,
            completed INTEGER NOT NULL,
            FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );",
        "CREATE INDEX IF NOT EXISTS idx_history_time ON play_history(played_at DESC);",
        "CREATE TABLE IF NOT EXISTS scan_roots (
            id TEXT PRIMARY KEY NOT NULL,
            path TEXT NOT NULL UNIQUE,
            watch_enabled INTEGER NOT NULL DEFAULT 1,
            last_scanned_at INTEGER
        );",
        "CREATE TABLE IF NOT EXISTS scan_state (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS artwork (
            id TEXT PRIMARY KEY NOT NULL,
            track_id TEXT,
            album_id TEXT,
            cache_path TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            width INTEGER NOT NULL,
            height INTEGER NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS lyrics (
            track_id TEXT PRIMARY KEY NOT NULL,
            is_synced INTEGER NOT NULL,
            content_json TEXT NOT NULL,
            plain_text TEXT,
            FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );",
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS saved_queue (
            position INTEGER PRIMARY KEY NOT NULL,
            track_id TEXT NOT NULL,
            path TEXT NOT NULL
        );",
        "CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
            track_id UNINDEXED,
            title,
            artist,
            album,
            genre,
            path,
            tokenize = 'porter unicode61'
        );",
        "CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
            INSERT INTO tracks_fts(track_id, title, artist, album, genre, path)
            VALUES (new.id, new.title, new.artist, new.album, coalesce(new.genre, ''), new.path);
        END;",
        "CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
            DELETE FROM tracks_fts WHERE track_id = old.id;
        END;",
        "CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
            DELETE FROM tracks_fts WHERE track_id = old.id;
            INSERT INTO tracks_fts(track_id, title, artist, album, genre, path)
            VALUES (new.id, new.title, new.artist, new.album, coalesce(new.genre, ''), new.path);
        END;",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (1, strftime('%s', 'now'));",
    ];

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

fn apply_v2(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V2 (smart playlists)");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS smart_playlists (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL UNIQUE,
            rules_json TEXT NOT NULL,
            match_all INTEGER NOT NULL DEFAULT 1,
            limit_count INTEGER,
            sort_by TEXT,
            sort_desc INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (2, strftime('%s', 'now'));",
    ];

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V3: content fingerprints, so a moved/renamed file keeps its identity
/// (play counts, favourites, playlist membership) instead of becoming a
/// brand-new track.
fn apply_v3(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V3 (track content fingerprints)");
    let tx = conn.transaction()?;

    // `ALTER TABLE ADD COLUMN` has no `IF NOT EXISTS`, so guard it explicitly:
    // if the migration row is ever lost while the column survives, the
    // unconditional ALTER would make the whole database refuse to open.
    let has_content_hash: bool = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for name in names {
            if name? == "content_hash" {
                found = true;
            }
        }
        found
    };

    let statements: Vec<&str> = if has_content_hash {
        vec![
            "CREATE INDEX IF NOT EXISTS idx_tracks_content_hash ON tracks(content_hash);",
            "INSERT INTO schema_migrations (version, applied_at) VALUES (3, strftime('%s', 'now'));",
        ]
    } else {
        vec![
            "ALTER TABLE tracks ADD COLUMN content_hash TEXT;",
            "CREATE INDEX IF NOT EXISTS idx_tracks_content_hash ON tracks(content_hash);",
            "INSERT INTO schema_migrations (version, applied_at) VALUES (3, strftime('%s', 'now'));",
        ]
    };

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V4: make the saved queue a real child of the library, and drop two tables
/// nothing ever wrote to.
///
/// `saved_queue.track_id` had no foreign key, so a queue row outlived the track
/// it named: the row survived every deletion, `load_queue` silently skipped it
/// (its `JOIN tracks` found nothing) and the queue quietly shrank. The `path`
/// column was written on every save and read by nobody except a purge that can
/// now follow the track's own path.
///
/// The `lyrics` and `artwork` tables were created in V1 and never used — no
/// repository, no inserts, no reads: lyrics are read from disk on demand and
/// artwork dimensions are re-measured when needed. Carrying an empty table that
/// promises a cache is worse than not having one.
fn apply_v4(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V4 (queue foreign key, drop unused tables)");
    let tx = conn.transaction()?;

    // SQLite cannot add a foreign key to an existing table, so the queue is
    // rebuilt. Orphan rows (whose track is gone) are dropped by the join rather
    // than copied into the new table.
    let statements = [
        "CREATE TABLE saved_queue_v4 (
            position INTEGER PRIMARY KEY NOT NULL,
            track_id TEXT NOT NULL,
            FOREIGN KEY(track_id) REFERENCES tracks(id) ON DELETE CASCADE
        );",
        // `ROW_NUMBER` (rather than copying `position`) closes the gap the
        // dropped orphans leave behind: `save_queue` writes a dense 0..n
        // ordering and the rest of the queue code expects one.
        "INSERT INTO saved_queue_v4 (position, track_id)
            SELECT ROW_NUMBER() OVER (ORDER BY sq.position ASC) - 1, sq.track_id
            FROM saved_queue sq
            JOIN tracks t ON t.id = sq.track_id;",
        "DROP TABLE saved_queue;",
        "ALTER TABLE saved_queue_v4 RENAME TO saved_queue;",
        "DROP TABLE IF EXISTS lyrics;",
        "DROP TABLE IF EXISTS artwork;",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (4, strftime('%s', 'now'));",
    ];

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V4 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// Test hook: apply V4 alone to a hand-built legacy schema.
///
/// `run_migrations` takes a database from zero to current, so the upgrade path
/// — which has to carry real data across a table rebuild — would otherwise only
/// be exercised on a user's machine.
#[cfg(test)]
pub(crate) fn apply_v4_for_test(conn: &mut Connection) -> Result<()> {
    apply_v4(conn)
}

/// Test-only entry point for V5, for the same reason as [`apply_v4_for_test`].
#[cfg(test)]
pub(crate) fn apply_v5_for_test(conn: &mut Connection) -> Result<()> {
    apply_v5(conn)
}

/// V5: sub-second modification times, the indexes the hot queries need, and an
/// FTS trigger that only fires when an indexed column changes.
fn apply_v5(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V5 (mtimes, indexes, scoped FTS trigger)");
    let tx = conn.transaction()?;

    // Same guard as V3: `ALTER TABLE ADD COLUMN` has no `IF NOT EXISTS`, and a
    // lost migration row must not make the database refuse to open.
    let has_mtime_nanos: bool = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for name in names {
            if name? == "mtime_nanos" {
                found = true;
            }
        }
        found
    };

    let mut statements: Vec<&str> = Vec::new();

    if !has_mtime_nanos {
        statements.push("ALTER TABLE tracks ADD COLUMN mtime_nanos INTEGER NOT NULL DEFAULT 0;");
    }

    statements.extend([
        // `list_most_played` groups by track and the foreign key cascade has to
        // find the children; neither had an index.
        "CREATE INDEX IF NOT EXISTS idx_history_track ON play_history(track_id);",
        // Same for playlist membership: deleting a track cascades through
        // `playlist_tracks`, which was a full scan per deletion.
        "CREATE INDEX IF NOT EXISTS idx_playlist_tracks_track ON playlist_tracks(track_id);",
        // The genre chips and the Genres view filter on this column.
        "CREATE INDEX IF NOT EXISTS idx_tracks_genre ON tracks(genre);",
        // `idx_tracks_album` covers `album` alone; the album view asks for
        // `WHERE album = ? AND artist = ?` and could not use it.
        "CREATE INDEX IF NOT EXISTS idx_tracks_album_artist ON tracks(album, artist);",
        // Favourites and play stats update on the playback hot path. The old
        // trigger deleted and reinserted an FTS row for *any* column, so every
        // play-count increment rewrote the search index. `UPDATE OF` limits it
        // to the columns the index actually contains.
        "DROP TRIGGER IF EXISTS tracks_au;",
        "CREATE TRIGGER tracks_au AFTER UPDATE OF title, artist, album, genre, path ON tracks BEGIN
            DELETE FROM tracks_fts WHERE track_id = old.id;
            INSERT INTO tracks_fts(track_id, title, artist, album, genre, path)
            VALUES (new.id, new.title, new.artist, new.album, coalesce(new.genre, ''), new.path);
        END;",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (5, strftime('%s', 'now'));",
    ]);

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V5 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

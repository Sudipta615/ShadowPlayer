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
    // Optimizations for low-spec hardware and spinning HDDs:
    // 1. -8000 KiB (8 MiB) cache keeps index pages in RAM, preventing mechanical seek latency.
    // 2. MEMORY temp_store ensures query sorts/temp tables don't thrash spinning disks.
    // 3. Bounded 32 MiB mmap reduces read() syscall overhead while staying safe on low-RAM machines.
    if let Err(e) = conn.execute_batch(
        "PRAGMA cache_size = -8000;
         PRAGMA temp_store = MEMORY;
         PRAGMA mmap_size = 33554432;",
    ) {
        warn!("Could not set performance PRAGMAs: {e}");
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

    if current_version < 6 {
        apply_v6(conn)?;
    }

    if current_version < 7 {
        apply_v7(conn)?;
    }

    if current_version < 8 {
        apply_v8(conn)?;
    }

    if current_version < 9 {
        apply_v9(conn)?;
    }

    if current_version < 10 {
        apply_v10(conn)?;
    }

    if current_version < 11 {
        apply_v11(conn)?;
    }

    if current_version < 12 {
        apply_v12(conn)?;
    }

    if current_version < 13 {
        apply_v13(conn)?;
    }

    if current_version < 14 {
        apply_v14(conn)?;
    }

    if current_version < 15 {
        apply_v15(conn)?;
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

/// V9: the permanent listening history.
///
/// `play_history` is a seven-day rolling window: it exists so "Recently
/// Played" and "Most Played" can be answered without a growing scan. A
/// listening history is the opposite kind of table — it is the record of
/// everything ever heard, and it is never pruned.
///
/// The artist, title and album are copied in at write time. This is what makes
/// the table a *history* rather than a join over the library: deleting a track
/// must not erase the times it was played. `track_id` is nullable for the same
/// reason (`ON DELETE SET NULL`), and only kept so a scrobble can still be
/// matched to the track when it is in the library.
fn apply_v9(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V9 (permanent listening history)");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS scrobbles (
            id TEXT PRIMARY KEY NOT NULL,
            track_id TEXT REFERENCES tracks(id) ON DELETE SET NULL,
            artist TEXT NOT NULL,
            title TEXT NOT NULL,
            album TEXT NOT NULL,
            played_at INTEGER NOT NULL,
            duration_secs REAL NOT NULL DEFAULT 0,
            listened_secs REAL NOT NULL DEFAULT 0
        );",
        "CREATE INDEX IF NOT EXISTS idx_scrobbles_played_at ON scrobbles(played_at DESC);",
        "CREATE INDEX IF NOT EXISTS idx_scrobbles_artist ON scrobbles(artist COLLATE NOCASE);",
        "CREATE INDEX IF NOT EXISTS idx_scrobbles_title ON scrobbles(title COLLATE NOCASE);",
        "CREATE INDEX IF NOT EXISTS idx_scrobbles_track ON scrobbles(track_id);",
        // Carry the week of scrobbles an existing user may still have in
        // `play_history` across, so upgrading does not start the history from
        // an empty table.
        "INSERT INTO scrobbles
            (id, track_id, artist, title, album, played_at, duration_secs, listened_secs)
         SELECT lower(hex(randomblob(16))), h.track_id, t.artist, t.title, t.album,
                h.played_at, t.duration_secs, h.duration_listened_secs
           FROM play_history h
           JOIN tracks t ON t.id = h.track_id
          WHERE h.is_scrobble = 1;",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (9, strftime('%s', 'now'));",
    ];

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V9 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V8: skip counts and scrobbles.
///
/// A skip count is how many times a track was skipped before it was heard,
/// which is what a "skip this track" smart playlist needs. The scrobble flag
/// marks the plays that count as scrobbles: half the track, or thirty
/// seconds, whichever comes first.
fn apply_v8(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V8 (skip counts and scrobbles)");
    let tx = conn.transaction()?;

    let existing: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };
    let history_columns: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(play_history);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };

    let mut statements: Vec<&str> = Vec::new();
    if !existing.iter().any(|n| n == "skip_count") {
        statements.push("ALTER TABLE tracks ADD COLUMN skip_count INTEGER NOT NULL DEFAULT 0;");
    }
    if !history_columns.iter().any(|n| n == "is_scrobble") {
        statements
            .push("ALTER TABLE play_history ADD COLUMN is_scrobble INTEGER NOT NULL DEFAULT 0;");
    }
    statements.push(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (8, strftime('%s', 'now'));",
    );

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V8 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V7: per-track DSP profiles.
///
/// A track can carry its own graphic EQ and preamp, applied when that track
/// plays. The global chain stays as it is; this is the per-track override.
fn apply_v7(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V7 (per-track DSP profiles)");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS track_dsp_profiles (
            track_id TEXT PRIMARY KEY NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            profile TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (7, strftime('%s', 'now'));",
    ];

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V7 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V6: ReplayGain album gain.
///
/// The engine could already read an album's gain from the file's own
/// `REPLAYGAIN_ALBUM` tag and apply it, but the library never indexed it, so
/// the app could neither show it nor fall back to a computed value for an
/// album that has no tag.
fn apply_v6(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V6 (ReplayGain album gain)");
    let tx = conn.transaction()?;

    // `ALTER TABLE ADD COLUMN` has no `IF NOT EXISTS`, so guard it explicitly:
    // a lost migration row must not make the database refuse to open.
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };

    let mut statements: Vec<&str> = Vec::new();
    if !existing.iter().any(|n| n == "replaygain_album_db") {
        statements.push("ALTER TABLE tracks ADD COLUMN replaygain_album_db REAL;");
    }
    if !existing.iter().any(|n| n == "replaygain_album_peak") {
        statements.push("ALTER TABLE tracks ADD COLUMN replaygain_album_peak REAL;");
    }
    statements.push(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (6, strftime('%s', 'now'));",
    );

    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V6 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

/// V10: the filesystem's own identity for a file.
///
/// The library used to key a track on its *path*, and the path is a name. On a
/// case-insensitive filesystem — the default on Windows, and on macOS — the
/// same file has two spellings, and a rename that only changes the case is a
/// rename the library cannot see: the old row's path still resolves, so it is
/// never pruned, and the new spelling becomes a second track with its own
/// play count and its own place in every playlist.
///
/// The fix is not to fold the case in the key — that would merge two genuinely
/// different files on a case-sensitive filesystem, which is worse — but to
/// record what the filesystem says about the file: the device it lives on and
/// its index within that device. Two names for one file then resolve to one
/// row, and a file that was renamed keeps its identity whatever the rename
/// was.
///
/// The column is filled in here for the rows that are already indexed, and
/// rows that turn out to be the same file are merged: the one with the most
/// plays survives, and the others' favourites, playlists and history move to
/// it.
fn apply_v10(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V10 (filesystem file identity)");
    let tx = conn.transaction()?;

    // Same guard as V3 and V5: `ALTER TABLE ADD COLUMN` has no
    // `IF NOT EXISTS`, and a lost migration row must not make the database
    // refuse to open.
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };

    if !existing.iter().any(|n| n == "file_id") {
        tx.execute("ALTER TABLE tracks ADD COLUMN file_id TEXT;", [])?;
    }
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_file_id ON tracks(file_id);",
        [],
    )?;

    // Backfill: read the identity of every row whose file is still on disk.
    // A row whose file has gone keeps a NULL identity — it is waiting to be
    // pruned by the scanner's reconciliation, not to be guessed at.
    let rows: Vec<(String, String)> = {
        let mut stmt = tx.prepare("SELECT id, path FROM tracks WHERE file_id IS NULL;")?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get::<_, String>("id")?, row.get::<_, String>("path")?))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        out
    };

    let mut backfilled = 0usize;
    for (id, path_key) in &rows {
        let path = domain::paths::decode_path_key(path_key);
        let Some(identity) = domain::file_identity::file_identity(&path) else {
            continue;
        };
        tx.execute(
            "UPDATE tracks SET file_id = ?1 WHERE id = ?2;",
            rusqlite::params![identity.to_key(), id],
        )?;
        backfilled += 1;
    }
    if backfilled > 0 {
        info!("V10: recorded the filesystem identity of {backfilled} track(s)");
    }

    // Merge the rows that turn out to be the same file.
    let merged = merge_duplicate_file_identities(&tx)?;
    if merged > 0 {
        warn!("V10: merged {merged} duplicate track row(s) that pointed at the same file");
    }

    tx.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (10, strftime('%s', 'now'));",
        [],
    )?;

    tx.commit()?;
    Ok(())
}

/// The stored path of a track row.
fn loser_path(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<String> {
    tx.query_row(
        "SELECT path FROM tracks WHERE id = ?1;",
        rusqlite::params![id],
        |r| r.get::<_, String>(0),
    )
}

/// Merge rows that the filesystem says are the same file.
///
/// The survivor is the row with the most plays — the one the user has the
/// strongest claim on — and the losers' favourites, playlist entries, play
/// history and scrobbles move to it before the losers are deleted. Every
/// merge is counted and reported; nothing is dropped silently.
fn merge_duplicate_file_identities(tx: &rusqlite::Transaction<'_>) -> Result<usize> {
    // The groups: every file identity that more than one row claims.
    let groups: Vec<(String, Vec<String>)> = {
        let mut stmt = tx.prepare(
            "SELECT file_id, GROUP_CONCAT(id) FROM tracks
              WHERE file_id IS NOT NULL
              GROUP BY file_id
             HAVING COUNT(*) > 1;",
        )?;
        let mapped = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            let (file_id, ids) = row?;
            let members: Vec<String> = ids.split(',').map(|s| s.to_string()).collect();
            out.push((file_id, members));
        }
        out
    };

    let mut merged = 0usize;

    for (_file_id, members) in groups {
        // The survivor: the most played, then the earliest added, then the
        // lowest id — a total order, so the same database always merges the
        // same way.
        let mut ranked: Vec<(u32, i64, String)> = Vec::new();
        for id in &members {
            let row = tx.query_row(
                "SELECT play_count, added_at FROM tracks WHERE id = ?1;",
                rusqlite::params![id],
                |r| Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)?)),
            )?;
            ranked.push((row.0, row.1, id.clone()));
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let survivor = ranked[0].2.clone();
        let losers: Vec<String> = ranked[1..].iter().map(|(_, _, id)| id.clone()).collect();

        for loser in &losers {
            // A row whose directory entry is *still there* is a hard link, not a
            // rename: the library is holding one file under two names on
            // purpose, and merging them would silently drop one of them.
            // Only a row whose entry has been replaced — the case-only rename
            // this migration exists for — is merged.
            if !domain::file_identity::entry_has_gone(&domain::paths::decode_path_key(&loser_path(
                tx, loser,
            )?)) {
                continue;
            }

            // Favourites: the survivor's own row wins; the loser's is kept
            // only if the survivor is not already a favourite.
            tx.execute(
                "INSERT OR IGNORE INTO favorites (track_id, created_at)
                 SELECT ?1, created_at FROM favorites WHERE track_id = ?2;",
                rusqlite::params![survivor, loser],
            )?;
            tx.execute(
                "DELETE FROM favorites WHERE track_id = ?1;",
                rusqlite::params![loser],
            )?;

            // Playlist entries: move them across, dropping any that would
            // duplicate an entry the survivor already has in that playlist.
            tx.execute(
                "UPDATE OR IGNORE playlist_tracks SET track_id = ?1 WHERE track_id = ?2;",
                rusqlite::params![survivor, loser],
            )?;
            tx.execute(
                "DELETE FROM playlist_tracks WHERE track_id = ?1;",
                rusqlite::params![loser],
            )?;

            // History and scrobbles: the events happened, so they move to the
            // surviving track rather than vanishing with the row.
            tx.execute(
                "UPDATE play_history SET track_id = ?1 WHERE track_id = ?2;",
                rusqlite::params![survivor, loser],
            )?;
            tx.execute(
                "UPDATE scrobbles SET track_id = ?1 WHERE track_id = ?2;",
                rusqlite::params![survivor, loser],
            )?;

            // The loser's own numbers move onto the survivor.
            tx.execute(
                "UPDATE tracks SET
                     is_favorite   = MAX(is_favorite, (SELECT is_favorite FROM tracks WHERE id = ?2)),
                     play_count    = play_count + (SELECT play_count FROM tracks WHERE id = ?2),
                     skip_count    = skip_count + (SELECT skip_count FROM tracks WHERE id = ?2),
                     last_played_at = MAX(COALESCE(last_played_at, 0),
                                          COALESCE((SELECT last_played_at FROM tracks WHERE id = ?2), 0))
                 WHERE id = ?1;",
                rusqlite::params![survivor, loser],
            )?;

            tx.execute(
                "DELETE FROM tracks WHERE id = ?1;",
                rusqlite::params![loser],
            )?;
            merged += 1;
        }
    }

    Ok(merged)
}

/// Exposed for the migration's own test.
#[cfg(test)]
pub(crate) fn apply_v10_for_test(conn: &mut Connection) -> Result<()> {
    apply_v10(conn)
}

/// Exposed for the migration's own test.
#[cfg(test)]
pub(crate) fn apply_v11_for_test(conn: &mut Connection) -> Result<()> {
    apply_v11(conn)
}

/// V11: star ratings, indexed stream precision, and the lyrics flag.
///
/// A rating belongs to the listener, not to the file: foobar2000 and AIMP both
/// store it in their library database, and so does ShadowPlayer. Re-tagging
/// every file just to record a personal score would rewrite the user's music
/// and make the rating travel with the file instead of the person.
///
/// `bit_depth` and `has_lyrics` are indexed alongside it so the smart-playlist
/// fields "Bit Depth >= N" and "Has Lyrics" can be answered by SQL rather than
/// by opening every file whenever a playlist is viewed.
fn apply_v11(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V11 (ratings, bit depth, lyrics flag)");
    let tx = conn.transaction()?;

    // `ALTER TABLE ADD COLUMN` has no `IF NOT EXISTS`, so every column is
    // guarded exactly as V3/V5/V6/V8/V10 do.
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };

    if !existing.iter().any(|n| n == "rating") {
        tx.execute(
            "ALTER TABLE tracks ADD COLUMN rating INTEGER NOT NULL DEFAULT 0;",
            [],
        )?;
    }
    if !existing.iter().any(|n| n == "bit_depth") {
        tx.execute("ALTER TABLE tracks ADD COLUMN bit_depth INTEGER;", [])?;
    }
    if !existing.iter().any(|n| n == "has_lyrics") {
        tx.execute(
            "ALTER TABLE tracks ADD COLUMN has_lyrics INTEGER NOT NULL DEFAULT 0;",
            [],
        )?;
    }

    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_rating ON tracks(rating);",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_bit_depth ON tracks(bit_depth);",
        [],
    )?;
    tx.execute(
        "CREATE INDEX IF NOT EXISTS idx_tracks_has_lyrics ON tracks(has_lyrics);",
        [],
    )?;
    tx.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (11, strftime('%s', 'now'));",
        [],
    )?;

    tx.commit()?;
    Ok(())
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

/// V12: playlist cross-references.
///
/// A playlist can include another playlist as an entry. The link is stored
/// separately from `playlist_tracks` so the existing track foreign key and
/// per-track reordering keep working; resolution happens when the playlist is
/// played, with cycle detection in [`crate::repos::PlaylistRepo`].
fn apply_v12(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V12 (playlist cross-references)");
    let tx = conn.transaction()?;
    let statements = [
        "CREATE TABLE IF NOT EXISTS playlist_links (
            playlist_id TEXT NOT NULL,
            linked_playlist_id TEXT NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            added_at INTEGER NOT NULL,
            PRIMARY KEY (playlist_id, linked_playlist_id),
            FOREIGN KEY(playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY(linked_playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
        );",
        "CREATE INDEX IF NOT EXISTS idx_playlist_links_linked
            ON playlist_links(linked_playlist_id);",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (12, strftime('%s', 'now'));",
    ];
    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V12 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }
    tx.commit()?;
    Ok(())
}

/// V13: Tier 2 — waveforms, biographies, podcasts, and BPM.
///
/// Waveforms are cached peaks, not audio: one byte per peak keeps a
/// ten-minute track under a kilobyte, and the seekbar can paint from the
/// cache before the file is decoded again.
///
/// Podcasts live in their own tables. An episode is not a library track —
/// it has its own resume position, its own download state, and removing a
/// feed must not leave holes in playlists or play counts.
fn apply_v13(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V13 (waveforms, biographies, podcasts, BPM)");
    let tx = conn.transaction()?;

    // `ALTER TABLE ADD COLUMN` has no `IF NOT EXISTS`; guard it like V3/V6/V11.
    let existing: Vec<String> = {
        let mut stmt = tx.prepare("PRAGMA table_info(tracks);")?;
        let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = Vec::new();
        for name in names {
            found.push(name?);
        }
        found
    };
    if !existing.iter().any(|n| n == "bpm") {
        tx.execute("ALTER TABLE tracks ADD COLUMN bpm INTEGER;", [])?;
    }

    let statements = [
        "CREATE INDEX IF NOT EXISTS idx_tracks_bpm ON tracks(bpm);",
        "CREATE TABLE IF NOT EXISTS track_waveforms (
            track_id TEXT PRIMARY KEY NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            peaks BLOB NOT NULL,
            peak_count INTEGER NOT NULL,
            duration_secs REAL NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS biographies (
            entity_kind TEXT NOT NULL,
            entity_name TEXT NOT NULL,
            body TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'user',
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (entity_kind, entity_name)
        );",
        "CREATE TABLE IF NOT EXISTS podcast_feeds (
            id TEXT PRIMARY KEY NOT NULL,
            url TEXT NOT NULL UNIQUE,
            title TEXT NOT NULL,
            description TEXT,
            artwork_url TEXT,
            last_fetched_at INTEGER,
            added_at INTEGER NOT NULL
        );",
        "CREATE TABLE IF NOT EXISTS podcast_episodes (
            id TEXT PRIMARY KEY NOT NULL,
            feed_id TEXT NOT NULL REFERENCES podcast_feeds(id) ON DELETE CASCADE,
            guid TEXT NOT NULL,
            title TEXT NOT NULL,
            description TEXT,
            enclosure_url TEXT NOT NULL,
            published_at INTEGER,
            duration_secs REAL,
            local_path TEXT,
            downloaded_at INTEGER,
            position_secs REAL NOT NULL DEFAULT 0,
            completed INTEGER NOT NULL DEFAULT 0,
            added_at INTEGER NOT NULL,
            UNIQUE(feed_id, guid)
        );",
        "CREATE INDEX IF NOT EXISTS idx_podcast_episodes_feed
            ON podcast_episodes(feed_id, published_at DESC);",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (13, strftime('%s', 'now'));",
    ];
    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V13 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }
    tx.commit()?;
    Ok(())
}

/// V14: Tier 3 — album liner notes.
///
/// Notes are keyed by the stable [`domain::ids::AlbumId`] rather than by a row
/// in the historical `albums` table, which the scanner does not populate. The
/// title and artist are copied alongside so a note remains identifiable if the
/// album's tracks disappear from the library, and so notes can be listed
/// without joining a table that may have no row for an album whose tags
/// changed.
fn apply_v14(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V14 (album liner notes)");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS album_notes (
            album_id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            notes_markdown TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "CREATE INDEX IF NOT EXISTS idx_album_notes_updated
            ON album_notes(updated_at DESC);",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (14, strftime('%s', 'now'));",
    ];
    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V14 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }
    tx.commit()?;
    Ok(())
}

/// V15: a first-class EQ preset library and its bindings.
///
/// Before this, a saved curve lived inside the `eq_settings` JSON blob and a
/// per-track profile stored a full copy of the chain. That made a preset
/// impossible to tag, bind to an output device, or update in place. The rows
/// here are the durable model:
///
/// * `eq_presets` owns the curve as an opaque JSON payload. The curve shape
///   (graphic or parametric) belongs to the UI/config crate, so the database
///   never needs a migration when a band field is added.
/// * `source` follows the established taxonomy: 0 user, 10 built-in, 20
///   AutoEQ. Provenance is what lets the library distinguish "my room curve"
///   from "Sennheiser HD 600".
/// * `output_pattern` binds a preset to an output device by case-insensitive
///   substring; the longest pattern wins, so "USB DAC" beats "DAC".
/// * `eq_preset_tracks` is a *pointer*, not a copy: editing the preset
///   updates every bound track, which a duplicated chain diff cannot do.
fn apply_v15(conn: &mut Connection) -> Result<()> {
    info!("Applying SQLite migration V15 (EQ preset library)");
    let tx = conn.transaction()?;

    let statements = [
        "CREATE TABLE IF NOT EXISTS eq_presets (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            source INTEGER NOT NULL DEFAULT 0,
            curve_json TEXT NOT NULL,
            output_pattern TEXT,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );",
        "CREATE INDEX IF NOT EXISTS idx_eq_presets_name
            ON eq_presets(name COLLATE NOCASE, id);",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_eq_presets_one_default
            ON eq_presets(is_default) WHERE is_default = 1;",
        "CREATE TABLE IF NOT EXISTS eq_preset_tracks (
            track_id TEXT PRIMARY KEY NOT NULL,
            preset_id TEXT NOT NULL REFERENCES eq_presets(id) ON DELETE CASCADE
        );",
        "CREATE INDEX IF NOT EXISTS idx_eq_preset_tracks_preset
            ON eq_preset_tracks(preset_id);",
        "INSERT INTO schema_migrations (version, applied_at) VALUES (15, strftime('%s', 'now'));",
    ];
    for stmt in statements {
        if let Err(e) = tx.execute(stmt, []) {
            log::error!("Migration V15 statement failed: {e}\nStmt: {stmt}");
            return Err(e);
        }
    }
    tx.commit()?;
    Ok(())
}

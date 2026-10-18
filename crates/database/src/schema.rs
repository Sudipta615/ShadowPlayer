use log::info;
use rusqlite::{Connection, Result};

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let _ = conn.query_row("PRAGMA journal_mode = WAL;", [], |_| Ok(()));
    let _ = conn.query_row("PRAGMA synchronous = NORMAL;", [], |_| Ok(()));
    let _ = conn.query_row("PRAGMA foreign_keys = ON;", [], |_| Ok(()));

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
            eprintln!("Error executing statement: {}\nStmt: {}", e, stmt);
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
            eprintln!("Error executing statement: {}\nStmt: {}", e, stmt);
            return Err(e);
        }
    }

    tx.commit()?;
    Ok(())
}

use log::info;
use rusqlite::{Connection, Result};

pub(crate) fn apply_v9(conn: &mut Connection) -> Result<()> {
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
pub(crate) fn apply_v8(conn: &mut Connection) -> Result<()> {
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
pub(crate) fn apply_v7(conn: &mut Connection) -> Result<()> {
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

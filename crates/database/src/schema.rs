use log::warn;
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
    fn sqlite_failure(message: impl Into<String>) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(19), Some(message.into()))
    }

    if foreign_keys != 1 {
        return Err(sqlite_failure(format!(
            "expected SQLite PRAGMA foreign_keys to be 1, got {foreign_keys}; \
                  ON DELETE CASCADE and orphan cleanup would be disabled"
        )));
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

    if current_version < 16 {
        apply_v16(conn)?;
    }

    if current_version < 17 {
        apply_v17(conn)?;
    }

    Ok(())
}

mod first;
mod second;
mod third;

pub(crate) use first::*;
pub(crate) use second::*;
pub(crate) use third::*;

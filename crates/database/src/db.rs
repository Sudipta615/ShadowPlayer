use crate::schema::run_migrations;
use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, Result};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    writer: Arc<Mutex<Connection>>,
    reader: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create SQLite database at file path with separate writer and reader
    /// connections for non-blocking WAL reads on low-end dual-core hardware.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_ref = path.as_ref();
        let mut writer = Connection::open(path_ref)?;
        run_migrations(&mut writer)?;

        // Open a dedicated read connection with read-only flags.
        // In WAL mode, this connection can query without locking or being locked
        // by background writer transactions (e.g. scanner or loudness passes).
        let reader = Connection::open_with_flags(
            path_ref,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reader.busy_timeout(std::time::Duration::from_secs(5))?;
        let _ = reader.execute_batch(
            "PRAGMA cache_size = -8000;
             PRAGMA temp_store = MEMORY;
             PRAGMA mmap_size = 33554432;
             PRAGMA foreign_keys = ON;",
        );

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            reader: Arc::new(Mutex::new(reader)),
        })
    }

    /// Open an in-memory SQLite database (ideal for tests).
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations(&mut conn)?;
        let shared = Arc::new(Mutex::new(conn));
        Ok(Self {
            writer: shared.clone(),
            reader: shared,
        })
    }

    /// Execute a closure against the primary/writer connection.
    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        let mut conn = self.writer.lock();
        f(&mut conn)
    }

    /// Explicit alias for `with_conn` for writing queries.
    pub fn with_write_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        self.with_conn(f)
    }

    /// Execute a read-only closure against the dedicated reader connection.
    /// In WAL mode, this executes concurrently with active background writes.
    pub fn with_read_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        let mut conn = self.reader.lock();
        f(&mut conn)
    }
}

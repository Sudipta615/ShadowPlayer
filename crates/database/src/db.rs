use crate::schema::run_migrations;
use parking_lot::Mutex;
use rusqlite::{Connection, Result};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create SQLite database at file path.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        run_migrations(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory SQLite database (ideal for tests).
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        run_migrations(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn with_conn<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        let mut conn = self.conn.lock();
        f(&mut conn)
    }
}

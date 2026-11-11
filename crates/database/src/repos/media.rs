//! Media-derived repositories: waveforms and biographies.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use rusqlite::{params, OptionalExtension, Result};

use super::common::now_secs;

// ── Tier 2 caches ───────────────────────────────────────────────────────────

/// One cached waveform: one byte per peak, plus the duration the peaks were
/// extracted from.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformPeaks {
    pub peaks: Vec<u8>,
    pub duration_secs: f32,
}

impl WaveformPeaks {
    /// Normalised 0.0–1.0 peaks, ready to paint.
    pub fn amplitudes(&self) -> Vec<f32> {
        self.peaks.iter().map(|p| *p as f32 / 255.0).collect()
    }
}

/// Cached waveform peaks, keyed by track.
pub struct WaveformRepo {
    db: Database,
}

impl WaveformRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn get(&self, id: &TrackId) -> Result<Option<WaveformPeaks>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT peaks, duration_secs FROM track_waveforms WHERE track_id = ?1;",
                params![id.to_string()],
                |row| {
                    Ok(WaveformPeaks {
                        peaks: row.get(0)?,
                        duration_secs: row.get(1)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn put(&self, id: &TrackId, peaks: &[u8], duration_secs: f32) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO track_waveforms
                    (track_id, peaks, peak_count, duration_secs, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(track_id) DO UPDATE SET
                    peaks = excluded.peaks,
                    peak_count = excluded.peak_count,
                    duration_secs = excluded.duration_secs,
                    updated_at = excluded.updated_at;",
                params![
                    id.to_string(),
                    peaks,
                    peaks.len() as i64,
                    duration_secs,
                    now_secs(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn count(&self) -> Result<u32> {
        self.db.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM track_waveforms;", [], |r| {
                r.get::<_, i64>(0).map(|n| n as u32)
            })
        })
    }
}

/// Cached artist and album biographies.
pub struct BiographyRepo {
    db: Database,
}

impl BiographyRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn get(&self, kind: &str, name: &str) -> Result<Option<Biography>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT entity_kind, entity_name, body, source, updated_at
                   FROM biographies WHERE entity_kind = ?1 AND entity_name = ?2;",
                params![kind, name],
                |row| {
                    Ok(Biography {
                        entity_kind: row.get(0)?,
                        entity_name: row.get(1)?,
                        text: row.get(2)?,
                        source: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn put(&self, kind: &str, name: &str, text: &str, source: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO biographies
                    (entity_kind, entity_name, body, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(entity_kind, entity_name) DO UPDATE SET
                    body = excluded.body,
                    source = excluded.source,
                    updated_at = excluded.updated_at;",
                params![kind, name, text, source, now_secs()],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, kind: &str, name: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM biographies WHERE entity_kind = ?1 AND entity_name = ?2;",
                params![kind, name],
            )?;
            Ok(())
        })
    }
}

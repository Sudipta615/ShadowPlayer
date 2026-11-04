use crate::db::Database;
use crate::repos::common::parse_id;
use domain::ids::TrackId;
use domain::models::TrackDspProfile;
use rusqlite::{params, Result};

/// Per-track DSP profiles.
///
/// A profile is a track's own graphic EQ and preamp, applied when that track
/// plays. The global chain is untouched.
#[allow(dead_code)]
pub struct TrackDspRepo {
    db: Database,
}

#[allow(dead_code)]
impl TrackDspRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// The profile for a track, if it has one.
    pub fn get(&self, track_id: &TrackId) -> Result<Option<TrackDspProfile>> {
        self.db.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT profile FROM track_dsp_profiles WHERE track_id = ?1;")?;
            let row = stmt.query_row(params![track_id.to_string()], |row| row.get::<_, String>(0));
            match row {
                Ok(json) => Ok(serde_json::from_str(&json).ok()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
    }

    /// Every track that has a profile.
    pub fn list(&self) -> Result<Vec<(TrackId, TrackDspProfile)>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT track_id, profile FROM track_dsp_profiles ORDER BY updated_at DESC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let profile: String = row.get(1)?;
                Ok((id, profile))
            })?;
            let mut out = Vec::new();
            for row in iter {
                let (id, json) = row?;
                let id = parse_id(&id, "track_dsp_profiles.track_id")?;
                if let Ok(profile) = serde_json::from_str::<TrackDspProfile>(&json) {
                    out.push((id, profile));
                }
            }
            Ok(out)
        })
    }

    /// Store a track's profile, replacing any previous one.
    pub fn set(&self, track_id: &TrackId, profile: &TrackDspProfile) -> Result<()> {
        let json = serde_json::to_string(profile)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO track_dsp_profiles (track_id, profile, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(track_id) DO UPDATE SET
                    profile = excluded.profile,
                    updated_at = excluded.updated_at;",
                params![track_id.to_string(), json, now],
            )?;
            Ok(())
        })
    }

    /// Remove a track's profile, so it plays with the global settings again.
    pub fn clear(&self, track_id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM track_dsp_profiles WHERE track_id = ?1;",
                params![track_id.to_string()],
            )?;
            Ok(())
        })
    }
}

//! The EQ preset library and its bindings.
//!
//! A preset is a *curve plus provenance*, not a copy of the whole chain. The
//! curve is stored as an opaque JSON payload owned by the UI: the database
//! only guarantees that a preset has a stable id, a name, a source tag and,
//! optionally, an output-device pattern and a per-track binding.
//!
//! Binding rules enforced here:
//!
//! * At most one preset is the global default. `set_default` clears the old
//!   row in the same transaction that sets the new one, and a partial unique
//!   index (V15) refuses a second default even if a caller bypasses this API.
//! * An output pattern matches a device name case-insensitively as a
//!   substring. When several patterns match, the longest one wins, so a
//!   "USB DAC" preset is not shadowed by a generic "DAC" preset.
//! * A track binding is a pointer with `ON DELETE CASCADE`: deleting the
//!   preset unplugs the track instead of leaving it pointing at nothing.

use rusqlite::{params, OptionalExtension, Result, Row};
use uuid::Uuid;

use crate::db::Database;

/// A stored preset as the library knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct EqPresetRecord {
    pub id: String,
    pub name: String,
    /// `0` user, `10` built-in, `20` AutoEQ. See [`SOURCE_USER`] and friends.
    pub source: i64,
    /// Opaque curve payload. The UI serializes and validates it.
    pub curve_json: String,
    /// Case-insensitive substring of the output device name.
    pub output_pattern: Option<String>,
    /// Whether this is the global fallback when nothing else matches.
    pub is_default: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A preset the user created or imported.
pub const SOURCE_USER: i64 = 0;
/// A preset that ships with the player.
pub const SOURCE_BUILTIN: i64 = 10;
/// A preset generated from an AutoEQ measurement.
pub const SOURCE_AUTOEQ: i64 = 20;

pub struct EqPresetsRepo {
    db: Database,
}

impl EqPresetsRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Every preset, name-sorted for a stable list.
    pub fn list(&self) -> Result<Vec<EqPresetRecord>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, source, curve_json, output_pattern, is_default,
                        created_at, updated_at
                 FROM eq_presets
                 ORDER BY name COLLATE NOCASE, id;",
            )?;
            let rows = stmt.query_map([], row_to_record)?;
            rows.collect()
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<EqPresetRecord>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, source, curve_json, output_pattern, is_default,
                        created_at, updated_at
                 FROM eq_presets WHERE id = ?1;",
                params![id],
                row_to_record,
            )
            .optional()
        })
    }

    /// Insert a new preset and return the stored row (with its generated id).
    ///
    /// `is_default` is exclusive: an existing default is cleared in the same
    /// transaction, so the partial unique index can never fire in normal use.
    pub fn insert(
        &self,
        name: &str,
        source: i64,
        curve_json: &str,
        output_pattern: Option<&str>,
        is_default: bool,
    ) -> Result<EqPresetRecord> {
        let name = name.trim();
        if name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "an EQ preset needs a name".to_string(),
            ));
        }
        let id = Uuid::new_v4().to_string();
        let now = now_secs();
        let pattern = output_pattern
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty());

        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            if is_default {
                tx.execute(
                    "UPDATE eq_presets SET is_default = 0 WHERE is_default = 1;",
                    [],
                )?;
            }
            tx.execute(
                "INSERT INTO eq_presets
                    (id, name, source, curve_json, output_pattern, is_default,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7);",
                params![
                    id,
                    name,
                    source,
                    curve_json,
                    pattern,
                    is_default as i64,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(EqPresetRecord {
                id,
                name: name.to_string(),
                source,
                curve_json: curve_json.to_string(),
                output_pattern: pattern.map(str::to_string),
                is_default,
                created_at: now,
                updated_at: now,
            })
        })
    }

    /// Replace an existing preset's mutable fields.
    pub fn update(&self, record: &EqPresetRecord) -> Result<()> {
        let name = record.name.trim();
        if name.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "an EQ preset needs a name".to_string(),
            ));
        }
        let pattern = record
            .output_pattern
            .as_deref()
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty());
        let now = now_secs();

        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            if record.is_default {
                tx.execute(
                    "UPDATE eq_presets SET is_default = 0 WHERE is_default = 1;",
                    [],
                )?;
            }
            let changed = tx.execute(
                "UPDATE eq_presets
                 SET name = ?2, source = ?3, curve_json = ?4, output_pattern = ?5,
                     is_default = ?6, updated_at = ?7
                 WHERE id = ?1;",
                params![
                    record.id,
                    name,
                    record.source,
                    record.curve_json,
                    pattern,
                    record.is_default as i64,
                    now
                ],
            )?;
            if changed == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Delete a preset. Returns false when the id was already gone.
    pub fn delete(&self, id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let removed = conn.execute("DELETE FROM eq_presets WHERE id = ?1;", params![id])?;
            Ok(removed > 0)
        })
    }

    /// Make `id` the single global default.
    pub fn set_default(&self, id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM eq_presets WHERE id = ?1);",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap_or(false);
            if !exists {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            tx.execute(
                "UPDATE eq_presets SET is_default = 0 WHERE is_default = 1 AND id <> ?1;",
                params![id],
            )?;
            tx.execute(
                "UPDATE eq_presets SET is_default = 1, updated_at = ?2 WHERE id = ?1;",
                params![id, now_secs()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn default_preset(&self) -> Result<Option<EqPresetRecord>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, source, curve_json, output_pattern, is_default,
                        created_at, updated_at
                 FROM eq_presets WHERE is_default = 1 LIMIT 1;",
                [],
                row_to_record,
            )
            .optional()
        })
    }

    /// The most specific preset bound to `device_name`.
    ///
    /// Matching is a case-insensitive substring in both directions of the
    /// naming problem: the user types "HD 600", the engine reports
    /// "Built-in Audio Analog Stereo / Headphones". The pattern is what the
    /// user controls, so the device name is the haystack.
    pub fn find_for_output(&self, device_name: &str) -> Result<Option<EqPresetRecord>> {
        if device_name.trim().is_empty() {
            return Ok(None);
        }
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, source, curve_json, output_pattern, is_default,
                        created_at, updated_at
                 FROM eq_presets
                 WHERE output_pattern IS NOT NULL
                   AND output_pattern <> ''
                   AND instr(lower(?1), lower(output_pattern)) > 0
                 ORDER BY length(output_pattern) DESC, name COLLATE NOCASE
                 LIMIT 1;",
                params![device_name],
                row_to_record,
            )
            .optional()
        })
    }

    /// Point a track at a preset. Replaces any previous binding.
    pub fn bind_track(&self, track_id: &str, preset_id: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO eq_preset_tracks (track_id, preset_id) VALUES (?1, ?2)
                 ON CONFLICT(track_id) DO UPDATE SET preset_id = excluded.preset_id;",
                params![track_id, preset_id],
            )?;
            Ok(())
        })
    }

    pub fn unbind_track(&self, track_id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let removed = conn.execute(
                "DELETE FROM eq_preset_tracks WHERE track_id = ?1;",
                params![track_id],
            )?;
            Ok(removed > 0)
        })
    }

    /// The preset bound to a track, if the binding still resolves.
    pub fn preset_for_track(&self, track_id: &str) -> Result<Option<EqPresetRecord>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT p.id, p.name, p.source, p.curve_json, p.output_pattern,
                        p.is_default, p.created_at, p.updated_at
                 FROM eq_preset_tracks t
                 JOIN eq_presets p ON p.id = t.preset_id
                 WHERE t.track_id = ?1;",
                params![track_id],
                row_to_record,
            )
            .optional()
        })
    }
}

fn row_to_record(row: &Row<'_>) -> Result<EqPresetRecord> {
    Ok(EqPresetRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        source: row.get(2)?,
        curve_json: row.get(3)?,
        output_pattern: row.get(4)?,
        is_default: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> EqPresetsRepo {
        EqPresetsRepo::new(Database::in_memory().expect("in-memory database"))
    }

    #[test]
    fn insert_list_and_get_round_trip() {
        let repo = repo();
        let record = repo
            .insert(
                "Room",
                SOURCE_USER,
                "{\"mode\":\"parametric\"}",
                None,
                false,
            )
            .unwrap();
        assert_eq!(record.name, "Room");
        assert!(!record.id.is_empty());

        let all = repo.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(repo.get(&record.id).unwrap().unwrap(), record);
        assert!(repo.get("missing").unwrap().is_none());
    }

    #[test]
    fn only_one_preset_is_the_default() {
        let repo = repo();
        let first = repo.insert("First", SOURCE_USER, "{}", None, true).unwrap();
        let second = repo
            .insert("Second", SOURCE_USER, "{}", None, true)
            .unwrap();

        assert!(!repo.get(&first.id).unwrap().unwrap().is_default);
        assert!(repo.get(&second.id).unwrap().unwrap().is_default);
        assert_eq!(repo.default_preset().unwrap().unwrap().id, second.id);
    }

    #[test]
    fn output_matching_is_case_insensitive_and_prefers_the_longest_pattern() {
        let repo = repo();
        repo.insert("Generic", SOURCE_USER, "{}", Some("dac"), false)
            .unwrap();
        let specific = repo
            .insert("My DAC", SOURCE_USER, "{}", Some("USB DAC"), false)
            .unwrap();

        let matched = repo
            .find_for_output("usb dac (schiit modi)")
            .unwrap()
            .expect("the specific pattern should win");
        assert_eq!(matched.id, specific.id);
        assert!(repo.find_for_output("Speakers").unwrap().is_none());
    }

    #[test]
    fn update_replaces_the_curve_and_the_pattern() {
        let repo = repo();
        let mut record = repo
            .insert("Curve", SOURCE_USER, "{\"v\":1}", None, false)
            .unwrap();
        record.curve_json = "{\"v\":2}".to_string();
        record.output_pattern = Some("  Headphones  ".to_string());
        record.name = "  Curve v2  ".to_string();
        repo.update(&record).unwrap();

        let stored = repo.get(&record.id).unwrap().unwrap();
        assert_eq!(stored.name, "Curve v2");
        assert_eq!(stored.curve_json, "{\"v\":2}");
        assert_eq!(stored.output_pattern.as_deref(), Some("Headphones"));
    }

    #[test]
    fn deleting_a_preset_unbinds_its_tracks() {
        let repo = repo();
        let preset = repo
            .insert("Headphones", SOURCE_USER, "{}", None, false)
            .unwrap();
        let track_id = "11111111-1111-4111-8111-111111111111";
        repo.bind_track(track_id, &preset.id).unwrap();

        assert_eq!(
            repo.preset_for_track(track_id).unwrap().unwrap().id,
            preset.id
        );
        assert!(repo.delete(&preset.id).unwrap());
        assert!(repo.preset_for_track(track_id).unwrap().is_none());
    }

    #[test]
    fn binding_a_missing_preset_is_refused_by_the_foreign_key() {
        let repo = repo();
        let result = repo.bind_track("track", "not-a-preset");
        assert!(result.is_err(), "the binding must not dangle");
    }

    #[test]
    fn deleting_is_idempotent() {
        let repo = repo();
        let preset = repo.insert("P", SOURCE_USER, "{}", None, false).unwrap();
        assert!(repo.delete(&preset.id).unwrap());
        assert!(!repo.delete(&preset.id).unwrap());
    }

    #[test]
    fn v15_presets_and_bindings_survive_a_reopen() {
        let path =
            std::env::temp_dir().join(format!("shadowplayer-eq-v15-{}.db", uuid::Uuid::new_v4()));
        let preset_id = {
            let repo = EqPresetsRepo::new(Database::open(&path).unwrap());
            let preset = repo
                .insert(
                    "Persisted",
                    SOURCE_USER,
                    "{\"mode\":\"graphic\"}",
                    Some("DAC"),
                    false,
                )
                .unwrap();
            repo.bind_track("track-1", &preset.id).unwrap();
            preset.id
        };

        // Reopening must not re-apply V15 (which would duplicate the version
        // row) and must keep the preset and its binding.
        let repo = EqPresetsRepo::new(Database::open(&path).unwrap());
        assert_eq!(repo.list().unwrap().len(), 1);
        assert_eq!(repo.default_preset().unwrap(), None);
        assert_eq!(
            repo.preset_for_track("track-1").unwrap().unwrap().id,
            preset_id
        );
        assert_eq!(
            repo.find_for_output("USB DAC").unwrap().unwrap().id,
            preset_id
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }
}

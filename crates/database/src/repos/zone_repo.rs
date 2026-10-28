//! Persistence repository for audio zones and ZoneSwitch routing rules.

use domain::ids::ZoneId;
use domain::zone::{ZoneConfig, ZoneSwitchCondition, ZoneSwitchRule};
use rusqlite::{params, OptionalExtension, Result, Row};

use crate::db::Database;
use crate::repos::common::parse_id;

pub struct ZoneRepo {
    db: Database,
}

impl ZoneRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// List all configured audio zones.
    pub fn list_zones(&self) -> Result<Vec<ZoneConfig>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, output_device, output_backend, volume, muted, dsp_preset,
                        linked_master_id, timing_offset_ms, clock_drift_pct, is_default, created_at
                 FROM audio_zones
                 ORDER BY is_default DESC, name COLLATE NOCASE ASC",
            )?;
            let rows = stmt.query_map([], map_zone_row)?;
            let mut zones = Vec::new();
            for r in rows {
                zones.push(r?);
            }
            Ok(zones)
        })
    }

    /// Find an audio zone by its unique id.
    pub fn get_zone(&self, id: ZoneId) -> Result<Option<ZoneConfig>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, output_device, output_backend, volume, muted, dsp_preset,
                        linked_master_id, timing_offset_ms, clock_drift_pct, is_default, created_at
                 FROM audio_zones WHERE id = ?1",
                params![id.to_string()],
                map_zone_row,
            )
            .optional()
        })
    }

    /// Fetch the designated default primary audio zone.
    pub fn get_default_zone(&self) -> Result<Option<ZoneConfig>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, output_device, output_backend, volume, muted, dsp_preset,
                        linked_master_id, timing_offset_ms, clock_drift_pct, is_default, created_at
                 FROM audio_zones WHERE is_default = 1",
                [],
                map_zone_row,
            )
            .optional()
        })
    }

    /// Upsert an audio zone configuration.
    pub fn save_zone(&self, zone: &ZoneConfig) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            if zone.is_default {
                tx.execute(
                    "UPDATE audio_zones SET is_default = 0 WHERE is_default = 1",
                    [],
                )?;
            }
            tx.execute(
                "INSERT INTO audio_zones (
                    id, name, output_device, output_backend, volume, muted, dsp_preset,
                    linked_master_id, timing_offset_ms, clock_drift_pct, is_default, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    output_device = excluded.output_device,
                    output_backend = excluded.output_backend,
                    volume = excluded.volume,
                    muted = excluded.muted,
                    dsp_preset = excluded.dsp_preset,
                    linked_master_id = excluded.linked_master_id,
                    timing_offset_ms = excluded.timing_offset_ms,
                    clock_drift_pct = excluded.clock_drift_pct,
                    is_default = excluded.is_default;",
                params![
                    zone.id.to_string(),
                    zone.name,
                    zone.output_device,
                    zone.output_backend,
                    zone.volume as f64,
                    if zone.muted { 1 } else { 0 },
                    zone.dsp_preset,
                    zone.linked_master_id.map(|id| id.to_string()),
                    zone.timing_offset_ms as f64,
                    zone.clock_drift_pct as f64,
                    if zone.is_default { 1 } else { 0 },
                    zone.created_at_unix as i64,
                ],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Delete a non-default audio zone. Returns true if removed, false if not found or cannot delete default.
    pub fn delete_zone(&self, id: ZoneId) -> Result<bool> {
        self.db.with_conn(|conn| {
            let is_default: bool = conn
                .query_row(
                    "SELECT is_default FROM audio_zones WHERE id = ?1",
                    params![id.to_string()],
                    |r| r.get::<_, i64>(0),
                )
                .map(|v| v != 0)
                .unwrap_or(false);

            if is_default {
                return Ok(false);
            }

            let affected = conn.execute(
                "DELETE FROM audio_zones WHERE id = ?1",
                params![id.to_string()],
            )?;
            Ok(affected > 0)
        })
    }

    /// Set an audio zone as the default primary zone.
    pub fn set_default_zone(&self, id: ZoneId) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute("UPDATE audio_zones SET is_default = 0", [])?;
            tx.execute(
                "UPDATE audio_zones SET is_default = 1 WHERE id = ?1",
                params![id.to_string()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// List all ZoneSwitch rules ordered by priority descending.
    pub fn list_rules(&self) -> Result<Vec<ZoneSwitchRule>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, target_zone_id, conditions_json, priority, enabled, created_at
                 FROM zone_switch_rules
                 ORDER BY priority DESC, name COLLATE NOCASE ASC",
            )?;
            let rows = stmt.query_map([], map_rule_row)?;
            let mut rules = Vec::new();
            for r in rows {
                rules.push(r?);
            }
            Ok(rules)
        })
    }

    /// Upsert a ZoneSwitch rule.
    pub fn save_rule(&self, rule: &ZoneSwitchRule) -> Result<()> {
        let conditions_json =
            serde_json::to_string(&rule.conditions).unwrap_or_else(|_| "[]".into());
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO zone_switch_rules (
                    id, name, target_zone_id, conditions_json, priority, enabled, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    target_zone_id = excluded.target_zone_id,
                    conditions_json = excluded.conditions_json,
                    priority = excluded.priority,
                    enabled = excluded.enabled;",
                params![
                    rule.id,
                    rule.name,
                    rule.target_zone_id.to_string(),
                    conditions_json,
                    rule.priority,
                    if rule.enabled { 1 } else { 0 },
                    rule.created_at_unix as i64,
                ],
            )?;
            Ok(())
        })
    }

    /// Delete a ZoneSwitch rule by id.
    pub fn delete_rule(&self, id: &str) -> Result<bool> {
        self.db.with_conn(|conn| {
            let affected =
                conn.execute("DELETE FROM zone_switch_rules WHERE id = ?1", params![id])?;
            Ok(affected > 0)
        })
    }
}

fn map_zone_row(row: &Row<'_>) -> Result<ZoneConfig> {
    let id_str: String = row.get(0)?;
    let id = parse_id(&id_str, "id")?;
    let name: String = row.get(1)?;
    let output_device: Option<String> = row.get(2)?;
    let output_backend: Option<String> = row.get(3)?;
    let volume: f64 = row.get(4)?;
    let muted: i64 = row.get(5)?;
    let dsp_preset: Option<String> = row.get(6)?;
    let master_id_str: Option<String> = row.get(7)?;
    let linked_master_id = match master_id_str {
        Some(ref s) => Some(parse_id(s, "linked_master_id")?),
        None => None,
    };
    let timing_offset_ms: f64 = row.get(8)?;
    let clock_drift_pct: f64 = row.get(9)?;
    let is_default: i64 = row.get(10)?;
    let created_at: i64 = row.get(11)?;

    Ok(ZoneConfig {
        id,
        name,
        output_device,
        output_backend,
        volume: volume as f32,
        muted: muted != 0,
        dsp_preset,
        linked_master_id,
        timing_offset_ms: timing_offset_ms as f32,
        clock_drift_pct: clock_drift_pct as f32,
        is_default: is_default != 0,
        created_at_unix: created_at.max(0) as u64,
    })
}

fn map_rule_row(row: &Row<'_>) -> Result<ZoneSwitchRule> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let target_zone_str: String = row.get(2)?;
    let target_zone_id = parse_id(&target_zone_str, "target_zone_id")?;
    let conditions_json: String = row.get(3)?;
    let conditions: Vec<ZoneSwitchCondition> =
        serde_json::from_str(&conditions_json).unwrap_or_default();
    let priority: i32 = row.get(4)?;
    let enabled: i64 = row.get(5)?;
    let created_at: i64 = row.get(6)?;

    Ok(ZoneSwitchRule {
        id,
        name,
        target_zone_id,
        conditions,
        priority,
        enabled: enabled != 0,
        created_at_unix: created_at.max(0) as u64,
    })
}

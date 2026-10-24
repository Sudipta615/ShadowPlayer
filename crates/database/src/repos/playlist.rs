//! Playlist and smart-playlist repositories.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::smart::*;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Result, Row};

use super::common::{like_substring, map_track, now_secs, parse_id};

/// The ceiling a smart playlist's own limit is clamped to.
///
/// A rule list is a saved query that runs on every view; a limit of "however
/// many rows there are" is how a UI ends up rendering a hundred thousand rows.
const MAX_SMART_PLAYLIST_LIMIT: u32 = 100_000;

pub struct PlaylistRepo {
    db: Database,
}

impl PlaylistRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn create(&self, name: &str, description: Option<&str>) -> Result<Playlist> {
        let id = PlaylistId::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO playlists (id, name, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5);",
                params![id.to_string(), name, description, now, now],
            )?;
            Ok(Playlist {
                id,
                name: name.to_string(),
                description: description.map(|s| s.to_string()),
                track_count: 0,
                created_at: now,
                updated_at: now,
            })
        })
    }

    /// Playlists whose name matches `query`.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Playlist>> {
        let like = like_substring(query);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                        COUNT(pt.track_id) as track_count
                 FROM playlists p
                 LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
                 WHERE p.name LIKE ?1 ESCAPE '\\'
                 GROUP BY p.id
                 ORDER BY p.name COLLATE NOCASE
                 LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![like, limit as i64], |row| {
                let id_str: String = row.get(0)?;
                Ok(Playlist {
                    id: parse_id(&id_str, "playlists.id")?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    track_count: row.get(5)?,
                })
            })?;
            let mut playlists = Vec::new();
            for p in iter {
                playlists.push(p?);
            }
            Ok(playlists)
        })
    }

    pub fn list(&self) -> Result<Vec<Playlist>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.name, p.description, p.created_at, p.updated_at,
                        COUNT(pt.track_id) as track_count
                 FROM playlists p
                 LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id
                 GROUP BY p.id
                 ORDER BY p.name COLLATE NOCASE;",
            )?;
            let iter = stmt.query_map([], |row| {
                let id_str: String = row.get(0)?;
                Ok(Playlist {
                    id: parse_id(&id_str, "playlists.id")?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    track_count: row.get(5)?,
                })
            })?;
            let mut playlists = Vec::new();
            for p in iter {
                playlists.push(p?);
            }
            Ok(playlists)
        })
    }

    pub fn add_track(&self, playlist_id: &PlaylistId, track_id: &TrackId) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.db.with_conn(|conn| {
            let next_pos: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM playlist_tracks WHERE playlist_id = ?1;",
                params![playlist_id.to_string()],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT INTO playlist_tracks (id, playlist_id, track_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5);",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    playlist_id.to_string(),
                    track_id.to_string(),
                    next_pos,
                    now
                ],
            )?;
            conn.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2;",
                params![now, playlist_id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Move a playlist track from one position to another, then renumber the
    /// remaining rows so the stored order stays contiguous.
    pub fn move_track(&self, playlist_id: &PlaylistId, from: usize, to: usize) -> Result<()> {
        if from == to {
            return Ok(());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            let mut row_ids: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT id FROM playlist_tracks
                     WHERE playlist_id = ?1
                     ORDER BY position ASC, id ASC;",
                )?;
                let rows = stmt.query_map(params![playlist_id.to_string()], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<Result<Vec<_>>>()?
            };
            if from >= row_ids.len() || to >= row_ids.len() {
                return Ok(());
            }
            let moved = row_ids.remove(from);
            row_ids.insert(to, moved);
            {
                let mut update =
                    tx.prepare("UPDATE playlist_tracks SET position = ?1 WHERE id = ?2;")?;
                for (position, row_id) in row_ids.iter().enumerate() {
                    update.execute(params![position as i64, row_id])?;
                }
            }
            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2;",
                params![now, playlist_id.to_string()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn get_tracks(&self, playlist_id: &PlaylistId) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 JOIN playlist_tracks pt ON pt.track_id = t.id
                 WHERE pt.playlist_id = ?1
                 ORDER BY pt.position ASC;",
            )?;
            let iter = stmt.query_map(params![playlist_id.to_string()], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    /// Tracks as they actually play: the playlist's own tracks followed by the
    /// contents of every linked playlist, recursively. A link that would loop
    /// is ignored if it was written before the guard existed (the guard is in
    /// [`Self::add_link`]); a deleted linked playlist simply contributes
    /// nothing.
    pub fn get_tracks_resolved(&self, playlist_id: &PlaylistId) -> Result<Vec<Track>> {
        let mut out = Vec::new();
        let mut visited = Vec::new();
        self.resolve_into(playlist_id, &mut visited, &mut out)?;
        Ok(out)
    }

    fn resolve_into(
        &self,
        playlist_id: &PlaylistId,
        visited: &mut Vec<PlaylistId>,
        out: &mut Vec<Track>,
    ) -> Result<()> {
        if visited.contains(playlist_id) {
            return Ok(());
        }
        visited.push(*playlist_id);
        out.extend(self.get_tracks(playlist_id)?);
        for linked in self.links(playlist_id)? {
            self.resolve_into(&linked, visited, out)?;
        }
        visited.pop();
        Ok(())
    }

    /// Playlists directly referenced by `playlist_id`, in link order.
    pub fn links(&self, playlist_id: &PlaylistId) -> Result<Vec<PlaylistId>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT linked_playlist_id FROM playlist_links
                 WHERE playlist_id = ?1
                 ORDER BY position ASC, added_at ASC;",
            )?;
            let rows = stmt.query_map(params![playlist_id.to_string()], |row| {
                let raw: String = row.get(0)?;
                parse_id(&raw, "playlist_links.linked_playlist_id")
            })?;
            rows.collect::<Result<Vec<_>>>()
        })
    }

    /// Link `linked` into `playlist_id`. Returns an error if that would create
    /// a cycle. Linking the same playlist twice is a no-op.
    pub fn add_link(&self, playlist_id: &PlaylistId, linked: &PlaylistId) -> Result<()> {
        if playlist_id == linked {
            return Err(rusqlite::Error::InvalidParameterName(
                "a playlist cannot link to itself".into(),
            ));
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.db.with_conn(|conn| {
            let cycles: bool = conn.query_row(
                "WITH RECURSIVE reachable(id) AS (
                    SELECT linked_playlist_id FROM playlist_links WHERE playlist_id = ?1
                    UNION
                    SELECT pl.linked_playlist_id
                    FROM playlist_links pl JOIN reachable r ON pl.playlist_id = r.id
                 )
                 SELECT EXISTS(SELECT 1 FROM reachable WHERE id = ?2);",
                params![linked.to_string(), playlist_id.to_string()],
                |r| r.get(0),
            )?;
            if cycles {
                return Err(rusqlite::Error::InvalidParameterName(
                    "playlist link would create a cycle".into(),
                ));
            }
            let next: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1
                 FROM playlist_links WHERE playlist_id = ?1;",
                params![playlist_id.to_string()],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO playlist_links
                    (playlist_id, linked_playlist_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4);",
                params![playlist_id.to_string(), linked.to_string(), next, now],
            )?;
            conn.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2;",
                params![now, playlist_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn remove_link(&self, playlist_id: &PlaylistId, linked: &PlaylistId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM playlist_links
                 WHERE playlist_id = ?1 AND linked_playlist_id = ?2;",
                params![playlist_id.to_string(), linked.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &PlaylistId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM playlists WHERE id = ?1;",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Rename a playlist (and bump its `updated_at`). Returns an error if the
    /// new name collides with an existing playlist.
    pub fn rename(&self, id: &PlaylistId, new_name: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE playlists SET name = ?1, updated_at = ?2 WHERE id = ?3;",
                params![new_name, now, id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Remove a single track from a playlist and compact the remaining
    /// positions so ordering stays contiguous.
    pub fn remove_track(&self, playlist_id: &PlaylistId, track_id: &TrackId) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND track_id = ?2;",
                params![playlist_id.to_string(), track_id.to_string()],
            )?;
            // Re-number positions to keep them contiguous. Done in Rust from an
            // explicitly ordered read: the previous correlated
            // `UPDATE ... (SELECT COUNT(*) FROM playlist_tracks ...)` counted the
            // rows of the table it was updating, so which number each row ended
            // up with depended on SQLite's unspecified row order within the
            // statement — the running order could survive a delete of the track
            // currently playing, or not, run to run.
            let ordered_ids: Vec<String> = {
                let mut stmt = tx.prepare(
                    "SELECT id FROM playlist_tracks
                     WHERE playlist_id = ?1
                     ORDER BY position ASC, id ASC;",
                )?;
                let rows = stmt.query_map(params![playlist_id.to_string()], |row| {
                    row.get::<_, String>(0)
                })?;
                rows.collect::<Result<Vec<_>>>()?
            };
            {
                let mut update =
                    tx.prepare("UPDATE playlist_tracks SET position = ?1 WHERE id = ?2;")?;
                for (position, row_id) in ordered_ids.iter().enumerate() {
                    update.execute(params![position as i64, row_id])?;
                }
            }
            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2;",
                params![now, playlist_id.to_string()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}

fn map_smart_playlist(row: &Row<'_>) -> Result<SmartPlaylist> {
    let id_str: String = row.get("id")?;
    let rules_json: String = row.get("rules_json")?;
    let sort_by: Option<String> = row.get("sort_by")?;
    let limit: Option<i64> = row.get("limit_count")?;

    // A rule JSON blob that will not parse used to become "no rules", which for
    // an AND playlist means "match everything" — the opposite of what the user
    // saved, and with no trace of the corruption. Report it, and keep the
    // playlist matching nothing until it is fixed, rather than everything.
    let (rules, rules_unreadable): (Vec<SmartRule>, bool) =
        match serde_json::from_str::<Vec<SmartRule>>(&rules_json) {
            Ok(rules) => (rules, false),
            Err(e) => {
                log::warn!(
                    "Smart playlist {id_str} has unreadable rules ({e}); it will match nothing. \
                     The stored definition is left untouched — re-save the playlist to replace it."
                );
                (Vec::new(), true)
            }
        };

    // Repair field/operator pairs the field cannot use, so a saved playlist
    // does not silently match nothing.
    let rules: Vec<SmartRule> = rules
        .into_iter()
        .map(|rule| {
            let (rule, note) = rule.normalized();
            if let Some(note) = note {
                log::warn!("Smart playlist {id_str}: {note}");
            }
            rule
        })
        .collect();

    Ok(SmartPlaylist {
        id: parse_id(&id_str, "smart_playlists.id")?,
        name: row.get("name")?,
        match_all: row.get::<_, i32>("match_all")? != 0,
        rules,
        limit_count: limit.map(|n| n.max(0) as u32),
        sort_by: sort_by.and_then(|s| serde_json::from_str(&s).ok()),
        sort_desc: row.get::<_, i32>("sort_desc")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        rules_unreadable,
    })
}

/// Translate one rule into a SQL fragment plus its bound parameters. Returns
/// `None` when the rule is incomplete (e.g. an empty value), so half-typed
/// rules are simply ignored rather than matching nothing.
fn rule_to_clause(rule: &SmartRule) -> Option<(String, Vec<SqlValue>)> {
    // A pair the field cannot use (see `SmartRule::normalized`) is ignored
    // rather than translated into a comparison that can never be true — the old
    // behaviour for `Date Added is 2024` was `added_at = '2024' COLLATE NOCASE`,
    // which matches nothing and says nothing.
    if !rule.field.operators().contains(&rule.op) {
        return None;
    }

    let col = rule.field.column();
    let v = rule.value.trim();

    // "Never Played" is a property of the play count, not a stored flag.
    if rule.field == SmartField::NeverPlayed {
        return match rule.op {
            SmartOp::IsTrue => Some(("play_count = 0".to_string(), vec![])),
            SmartOp::IsFalse => Some(("play_count > 0".to_string(), vec![])),
            _ => None,
        };
    }

    match rule.op {
        SmartOp::IsTrue => Some((format!("{col} = 1"), vec![])),
        SmartOp::IsFalse => Some((format!("{col} = 0"), vec![])),
        SmartOp::IsEmpty => Some((format!("({col} IS NULL OR {col} = '')"), vec![])),
        SmartOp::IsNotEmpty => Some((format!("({col} IS NOT NULL AND {col} != '')"), vec![])),
        SmartOp::Contains => {
            if v.is_empty() {
                None
            } else {
                Some((
                    format!("{col} LIKE ?"),
                    vec![SqlValue::Text(format!("%{v}%"))],
                ))
            }
        }
        SmartOp::NotContains => {
            if v.is_empty() {
                None
            } else {
                Some((
                    format!("({col} IS NULL OR {col} NOT LIKE ?)"),
                    vec![SqlValue::Text(format!("%{v}%"))],
                ))
            }
        }
        SmartOp::Equals | SmartOp::NotEquals => {
            let neq = rule.op == SmartOp::NotEquals;
            if rule.field.is_numeric() {
                let n = parse_num(v)?;
                Some((format!("{col} {} ?", if neq { "!=" } else { "=" }), vec![n]))
            } else if v.is_empty() {
                None
            } else {
                Some((
                    format!("{col} {} ? COLLATE NOCASE", if neq { "!=" } else { "=" }),
                    vec![SqlValue::Text(v.to_string())],
                ))
            }
        }
        SmartOp::GreaterThan | SmartOp::LessThan => {
            let n = parse_num(v)?;
            let op = if rule.op == SmartOp::GreaterThan {
                ">"
            } else {
                "<"
            };
            Some((format!("{col} {op} ?"), vec![n]))
        }
        SmartOp::InLastDays | SmartOp::NotInLastDays => {
            let days: i64 = v.parse().ok()?;
            let cutoff = days.max(0) * 86_400;
            if rule.op == SmartOp::InLastDays {
                Some((
                    format!("{col} >= strftime('%s','now') - ?"),
                    vec![SqlValue::Integer(cutoff)],
                ))
            } else {
                Some((
                    format!("({col} IS NULL OR {col} < strftime('%s','now') - ?)"),
                    vec![SqlValue::Integer(cutoff)],
                ))
            }
        }
    }
}

fn parse_num(v: &str) -> Option<SqlValue> {
    if let Ok(i) = v.trim().parse::<i64>() {
        Some(SqlValue::Integer(i))
    } else {
        v.trim().parse::<f64>().ok().map(SqlValue::Real)
    }
}

/// Build the `SELECT` tail (WHERE/ORDER BY/LIMIT) for a smart playlist.
fn build_smart_query(sp: &SmartPlaylist) -> (String, Vec<SqlValue>) {
    // A definition whose rules could not be read is not a definition with no
    // conditions: matching the whole library would silently relabel every track
    // as a member of a playlist that is, in fact, broken.
    if sp.rules_unreadable {
        return (" WHERE 0".to_string(), Vec::new());
    }

    // A rule with an impossible pair or an unparseable value must not be
    // silently dropped: dropping it from an AND playlist widens the result,
    // and a playlist that matches everything is the opposite of what the user
    // saved. Match nothing and let the editor show the validation message.
    if sp.rules.iter().any(|rule| rule.validate().is_err()) {
        return (" WHERE 0".to_string(), Vec::new());
    }

    let mut clauses = Vec::new();
    let mut vals: Vec<SqlValue> = Vec::new();

    for rule in &sp.rules {
        if let Some((clause, mut v)) = rule_to_clause(rule) {
            clauses.push(clause);
            vals.append(&mut v);
        }
    }

    let mut sql = String::new();
    if !clauses.is_empty() {
        let joiner = if sp.match_all { " AND " } else { " OR " };
        sql.push_str(" WHERE (");
        sql.push_str(&clauses.join(joiner));
        sql.push(')');
    }

    let sort = sp.sort_by.unwrap_or(SmartField::Title);
    let dir = if sp.sort_desc { "DESC" } else { "ASC" };
    // Text columns sort case-insensitively, mirroring the rest of the library.
    let collate = if sort.is_numeric() || sort.is_date() {
        ""
    } else {
        " COLLATE NOCASE"
    };
    sql.push_str(&format!(" ORDER BY {}{collate} {dir}", sort.column()));

    if let Some(n) = sp.limit_count {
        if n > 0 {
            sql.push_str(&format!(" LIMIT {}", n.min(MAX_SMART_PLAYLIST_LIMIT)));
        }
    }

    (sql, vals)
}

/// Saved, self-updating ("smart") playlists and their evaluation.
pub struct SmartPlaylistRepo {
    db: Database,
}

impl SmartPlaylistRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<SmartPlaylist>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, rules_json, match_all, limit_count, sort_by, sort_desc,
                        created_at, updated_at
                 FROM smart_playlists
                 ORDER BY name COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], map_smart_playlist)?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        })
    }

    pub fn get(&self, id: &PlaylistId) -> Result<Option<SmartPlaylist>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, rules_json, match_all, limit_count, sort_by, sort_desc,
                        created_at, updated_at
                 FROM smart_playlists WHERE id = ?1;",
                params![id.to_string()],
                map_smart_playlist,
            )
            .optional()
        })
    }

    /// Insert (or replace) a smart playlist. Returns the stored definition.
    pub fn save(&self, sp: &SmartPlaylist) -> Result<SmartPlaylist> {
        let now = now_secs();
        let created = if sp.created_at == 0 {
            now
        } else {
            sp.created_at
        };
        let rules_json = serde_json::to_string(&sp.rules).unwrap_or_else(|_| "[]".to_string());
        let sort_by = sp.sort_by.and_then(|f| serde_json::to_string(&f).ok());
        let limit = sp.limit_count.map(|n| n as i64);

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO smart_playlists
                    (id, name, rules_json, match_all, limit_count, sort_by, sort_desc,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    rules_json = excluded.rules_json,
                    match_all = excluded.match_all,
                    limit_count = excluded.limit_count,
                    sort_by = excluded.sort_by,
                    sort_desc = excluded.sort_desc,
                    updated_at = excluded.updated_at;",
                params![
                    sp.id.to_string(),
                    sp.name,
                    rules_json,
                    sp.match_all as i32,
                    limit,
                    sort_by,
                    sp.sort_desc as i32,
                    created,
                    now,
                ],
            )?;
            Ok(())
        })?;

        Ok(SmartPlaylist {
            created_at: created,
            updated_at: now,
            ..sp.clone()
        })
    }

    pub fn delete(&self, id: &PlaylistId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM smart_playlists WHERE id = ?1;",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Run a smart playlist's rules and return the matching tracks.
    pub fn evaluate(&self, sp: &SmartPlaylist) -> Result<Vec<Track>> {
        let (tail, vals) = build_smart_query(sp);
        let sql = format!("SELECT * FROM tracks{tail};");
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(&sql)?;
            let iter = stmt.query_map(params_from_iter(vals.iter()), map_track)?;
            let mut out = Vec::new();
            for t in iter {
                out.push(t?);
            }
            Ok(out)
        })
    }
}

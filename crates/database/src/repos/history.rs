//! Favourites, play history, scrobbles, and the play queue.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use rusqlite::{params, Result, Row};
use std::path::Path;

use super::common::{like_prefix, map_track, parse_id};

pub struct FavoritesRepo {
    db: Database,
}

impl FavoritesRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 JOIN favorites f ON f.track_id = t.id
                 ORDER BY f.created_at DESC;",
            )?;
            let iter = stmt.query_map([], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    pub fn add(&self, track_id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO favorites (track_id, created_at) VALUES (?1, strftime('%s', 'now'));",
                params![track_id.to_string()],
            )?;
            conn.execute(
                "UPDATE tracks SET is_favorite = 1 WHERE id = ?1;",
                params![track_id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn remove(&self, track_id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM favorites WHERE track_id = ?1;",
                params![track_id.to_string()],
            )?;
            conn.execute(
                "UPDATE tracks SET is_favorite = 0 WHERE id = ?1;",
                params![track_id.to_string()],
            )?;
            Ok(())
        })
    }
}

pub struct HistoryRepo {
    db: Database,
}

impl HistoryRepo {
    /// Rolling window for "Recently Played" and "Most Played" (7 days).
    pub const WINDOW_SECS: i64 = 7 * 24 * 60 * 60;
    /// Minimum plays inside the window for a track to appear in "Most Played".
    pub const MOST_PLAYED_MIN: i64 = 5;

    pub fn new(db: Database) -> Self {
        Self { db }
    }

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    pub fn record_play(
        &self,
        track_id: &TrackId,
        duration_listened_secs: f32,
        completed: bool,
        is_scrobble: bool,
    ) -> Result<()> {
        let now = Self::now_secs();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO play_history
                    (id, track_id, played_at, duration_listened_secs, completed, is_scrobble)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    track_id.to_string(),
                    now,
                    duration_listened_secs,
                    if completed { 1 } else { 0 },
                    if is_scrobble { 1 } else { 0 },
                ],
            )?;
            // Keep play_history bounded: drop anything older than the window.
            conn.execute(
                "DELETE FROM play_history WHERE played_at < ?1;",
                params![now - Self::WINDOW_SECS],
            )?;
            Ok(())
        })
    }

    /// Delete play-history rows older than the rolling window.
    pub fn prune_older_than_window(&self) -> Result<usize> {
        let cutoff = Self::now_secs() - Self::WINDOW_SECS;
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM play_history WHERE played_at < ?1;",
                params![cutoff],
            )?;
            Ok(n)
        })
    }

    /// Tracks played within the last 7 days, most recent first.
    pub fn list_recently_played(&self, limit: usize) -> Result<Vec<Track>> {
        let cutoff = Self::now_secs() - Self::WINDOW_SECS;
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 WHERE t.last_played_at IS NOT NULL AND t.last_played_at >= ?1
                 ORDER BY t.last_played_at DESC
                 LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![cutoff, limit as i64], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    /// The scrobble log: the plays that count as scrobbles, newest first.
    ///
    /// A scrobble is a play of at least thirty seconds, or half the track,
    /// whichever comes first — the rule the scrobbling services use.
    pub fn list_scrobbles(&self, limit: usize) -> Result<Vec<(Track, i64, f32)>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.*, h.played_at, h.duration_listened_secs
                   FROM play_history h
                   JOIN tracks t ON t.id = h.track_id
                  WHERE h.is_scrobble = 1
                  ORDER BY h.played_at DESC
                  LIMIT ?1;",
            )?;
            let iter = stmt.query_map(params![limit as i64], |row| {
                let track = map_track(row)?;
                let played_at: i64 = row.get("played_at")?;
                let listened: f64 = row.get("duration_listened_secs")?;
                Ok((track, played_at, listened as f32))
            })?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// How many scrobbles there are.
    pub fn count_scrobbles(&self) -> Result<usize> {
        self.db.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM play_history WHERE is_scrobble = 1;",
                [],
                |row| row.get(0),
            )?;
            Ok(n as usize)
        })
    }

    /// Tracks with at least [`Self::MOST_PLAYED_MIN`] plays inside the last 7
    /// days, ordered by play count.
    pub fn list_most_played(&self, limit: usize) -> Result<Vec<Track>> {
        let cutoff = Self::now_secs() - Self::WINDOW_SECS;
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 JOIN (
                     SELECT track_id, COUNT(*) AS plays
                     FROM play_history
                     WHERE played_at >= ?1
                     GROUP BY track_id
                     HAVING plays >= ?2
                 ) h ON h.track_id = t.id
                 ORDER BY h.plays DESC, t.last_played_at DESC
                 LIMIT ?3;",
            )?;
            let iter = stmt.query_map(
                params![cutoff, Self::MOST_PLAYED_MIN, limit as i64],
                map_track,
            )?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }
}

/// The permanent listening history.
///
/// This is the local equivalent of a scrobbling service's database: every play
/// that counted, kept forever, with the artist, title and album denormalised
/// at write time so the record survives the file being deleted.
pub struct ScrobbleRepo {
    db: Database,
}

impl ScrobbleRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    /// Record a scrobble for `track`, now.
    pub fn record(&self, track: &Track, listened_secs: f32) -> Result<()> {
        self.record_at(track, Self::now_secs(), listened_secs)
    }

    /// Record a scrobble at an explicit time.
    ///
    /// The explicit form exists for tests and for importing a history from
    /// another player; normal playback uses [`Self::record`].
    pub fn record_at(&self, track: &Track, played_at: i64, listened_secs: f32) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO scrobbles
                    (id, track_id, artist, title, album, played_at, duration_secs, listened_secs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    track.id.to_string(),
                    &track.artist,
                    &track.title,
                    &track.album,
                    played_at,
                    track.duration_secs,
                    listened_secs,
                ],
            )?;
            Ok(())
        })
    }

    /// The most recent scrobbles, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<Scrobble>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT track_id, artist, title, album, played_at, duration_secs, listened_secs
                   FROM scrobbles
                  ORDER BY played_at DESC
                  LIMIT ?1;",
            )?;
            let iter = stmt.query_map(params![limit as i64], map_scrobble)?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Every scrobble, oldest first — what an export walks.
    pub fn list_all(&self) -> Result<Vec<Scrobble>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT track_id, artist, title, album, played_at, duration_secs, listened_secs
                   FROM scrobbles
                  ORDER BY played_at ASC;",
            )?;
            let iter = stmt.query_map([], map_scrobble)?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// How many scrobbles have ever been recorded.
    pub fn count(&self) -> Result<usize> {
        self.count_since(None)
    }

    /// Drop scrobbles whose linked track lives inside `prefix`.
    ///
    /// Scrobbles normally outlive their track on purpose: the artist, title
    /// and album are copied in at write time, and deleting a file must not
    /// erase the fact that it was played. Development fixture rows are the
    /// exception — they describe files that never existed, so their history
    /// has to go with them. The link is matched before the track row is
    /// deleted; afterwards the `ON DELETE SET NULL` rule has already erased
    /// the only connection back to the path.
    pub fn purge_track_path_prefix(&self, prefix: &Path) -> Result<usize> {
        let like = like_prefix(prefix);
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM scrobbles
                 WHERE track_id IN (SELECT id FROM tracks WHERE path LIKE ?1 ESCAPE '\\');",
                params![like],
            )?;
            Ok(n)
        })
    }

    /// How many scrobbles fall inside a period (`None` is all time).
    pub fn count_since(&self, since: Option<i64>) -> Result<usize> {
        self.db.with_conn(|conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(*) FROM scrobbles WHERE (?1 IS NULL OR played_at >= ?1);",
                params![since],
                |row| row.get(0),
            )?;
            Ok(n as usize)
        })
    }

    /// Total listening time inside a period, in seconds.
    pub fn total_listened_secs_since(&self, since: Option<i64>) -> Result<f64> {
        self.db.with_conn(|conn| {
            let secs: f64 = conn.query_row(
                "SELECT COALESCE(SUM(listened_secs), 0) FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1);",
                params![since],
                |row| row.get(0),
            )?;
            Ok(secs)
        })
    }

    /// The timestamp of the very first scrobble.
    pub fn first_played_at(&self) -> Result<Option<i64>> {
        self.db.with_conn(|conn| {
            conn.query_row("SELECT MIN(played_at) FROM scrobbles;", [], |row| {
                row.get(0)
            })
        })
    }

    /// Artists by scrobble count, most played first.
    pub fn top_artists(&self, since: Option<i64>, limit: usize) -> Result<Vec<ScrobbleCount>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT artist, COUNT(*) AS plays
                   FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1)
                  GROUP BY artist COLLATE NOCASE
                  ORDER BY plays DESC, artist COLLATE NOCASE ASC
                  LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![since, limit as i64], |row| {
                Ok(ScrobbleCount {
                    name: row.get(0)?,
                    detail: String::new(),
                    count: row.get::<_, i64>(1)? as usize,
                })
            })?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Albums by scrobble count, most played first.
    pub fn top_albums(&self, since: Option<i64>, limit: usize) -> Result<Vec<ScrobbleCount>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT album, artist, COUNT(*) AS plays
                   FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1)
                  GROUP BY album COLLATE NOCASE, artist COLLATE NOCASE
                  ORDER BY plays DESC, album COLLATE NOCASE ASC
                  LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![since, limit as i64], |row| {
                Ok(ScrobbleCount {
                    name: row.get(0)?,
                    detail: row.get(1)?,
                    count: row.get::<_, i64>(2)? as usize,
                })
            })?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Tracks by scrobble count, most played first.
    pub fn top_tracks(&self, since: Option<i64>, limit: usize) -> Result<Vec<ScrobbleCount>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT title, artist, COUNT(*) AS plays
                   FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1)
                  GROUP BY title COLLATE NOCASE, artist COLLATE NOCASE
                  ORDER BY plays DESC, title COLLATE NOCASE ASC
                  LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![since, limit as i64], |row| {
                Ok(ScrobbleCount {
                    name: row.get(0)?,
                    detail: row.get(1)?,
                    count: row.get::<_, i64>(2)? as usize,
                })
            })?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Scrobbles per calendar day in the user's local timezone.
    ///
    /// The key is a *day number* (days since the Unix epoch) rather than a
    /// formatted date: consecutive-day arithmetic is then plain integer
    /// arithmetic, which is what the streak calculation needs. Pair it with
    /// [`Self::today_day_number`] to place "today".
    pub fn per_day(&self, since: Option<i64>) -> Result<Vec<(i64, usize)>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CAST(strftime('%s', date(played_at, 'unixepoch', 'localtime')) AS INTEGER) / 86400 AS day,
                        COUNT(*) AS plays
                   FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1)
                  GROUP BY day
                  ORDER BY day ASC;",
            )?;
            let iter = stmt.query_map(params![since], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? as usize))
            })?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Scrobbles per hour of the local day, 0..24.
    pub fn per_hour(&self, since: Option<i64>) -> Result<[usize; 24]> {
        let mut hours = [0usize; 24];
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT CAST(strftime('%H', played_at, 'unixepoch', 'localtime') AS INTEGER) AS hour,
                        COUNT(*) AS plays
                   FROM scrobbles
                  WHERE (?1 IS NULL OR played_at >= ?1)
                  GROUP BY hour;",
            )?;
            let iter = stmt.query_map(params![since], |row| {
                Ok((row.get::<_, i64>(0)? as usize, row.get::<_, i64>(1)? as usize))
            })?;
            for row in iter {
                let (hour, count) = row?;
                if hour < hours.len() {
                    hours[hour] = count;
                }
            }
            Ok(hours)
        })
    }

    /// The distinct local days that have a scrobble, ascending.
    pub fn distinct_days(&self) -> Result<Vec<i64>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT
                        CAST(strftime('%s', date(played_at, 'unixepoch', 'localtime')) AS INTEGER) / 86400 AS day
                   FROM scrobbles
                  ORDER BY day ASC;",
            )?;
            let iter = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut out = Vec::new();
            for row in iter {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Today, as the same day number [`Self::per_day`] uses.
    pub fn today_day_number(&self) -> Result<i64> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT CAST(strftime('%s', 'now', 'localtime') AS INTEGER) / 86400;",
                [],
                |row| row.get(0),
            )
        })
    }
}

/// Map a `scrobbles` row.
fn map_scrobble(row: &Row) -> Result<Scrobble> {
    let track_id: Option<String> = row.get("track_id")?;
    let track_id = match track_id {
        Some(id) => Some(parse_id::<TrackId>(&id, "scrobbles.track_id")?),
        None => None,
    };
    let listened: f64 = row.get("listened_secs")?;
    let duration: f64 = row.get("duration_secs")?;
    Ok(Scrobble {
        track_id,
        artist: row.get("artist")?,
        title: row.get("title")?,
        album: row.get("album")?,
        played_at: row.get("played_at")?,
        duration_secs: duration as f32,
        listened_secs: listened as f32,
    })
}

pub struct QueueRepo {
    db: Database,
}
impl QueueRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Replace the persisted queue.
    ///
    /// Only ids are stored: the queue is an ordered list of tracks, and every
    /// other fact about a track (its path above all) belongs to the track row
    /// that owns it. The column that duplicated the path was never read back and
    /// went stale the moment a file was organized.
    pub fn save_queue(&self, track_ids: &[TrackId]) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM saved_queue;", [])?;
            for (pos, id) in track_ids.iter().enumerate() {
                tx.execute(
                    "INSERT INTO saved_queue (position, track_id) VALUES (?1, ?2);",
                    params![pos as i64, id.to_string()],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn load_queue(&self) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 JOIN saved_queue sq ON sq.track_id = t.id
                 ORDER BY sq.position ASC;",
            )?;
            let iter = stmt.query_map([], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    pub fn clear(&self) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM saved_queue;", [])?;
            Ok(())
        })
    }

    /// Drops persisted queue entries whose track lives inside `prefix`.
    ///
    /// The prefix is matched against the track's own path, which is the only
    /// copy of it: the queue table stores ids, and a queued track that is
    /// deleted takes its queue row with it through the foreign key.
    pub fn purge_path_prefix(&self, prefix: &Path) -> Result<usize> {
        let like = like_prefix(prefix);
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM saved_queue
                 WHERE track_id IN (SELECT id FROM tracks WHERE path LIKE ?1 ESCAPE '\\');",
                params![like],
            )?;
            Ok(n)
        })
    }
}

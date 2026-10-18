use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::smart::*;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Result, Row};
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn map_track(row: &Row<'_>) -> Result<Track> {
    let id_str: String = row.get("id")?;
    let path_str: String = row.get("path")?;
    let is_fav_int: i32 = row.get("is_favorite")?;
    let is_lossless_int: i32 = row.get("is_lossless")?;

    Ok(Track {
        id: TrackId::from_str(&id_str).unwrap_or_default(),
        path: PathBuf::from(path_str),
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        genre: row.get("genre")?,
        year: row.get("year")?,
        track_number: row.get("track_number")?,
        disc_number: row.get("disc_number")?,
        duration_secs: row.get("duration_secs")?,
        sample_rate: row.get("sample_rate")?,
        channels: row.get("channels")?,
        format: row.get("format")?,
        bitrate_kbps: row.get("bitrate_kbps")?,
        is_lossless: is_lossless_int != 0,
        replaygain_track_db: row.get("replaygain_track_db")?,
        replaygain_track_peak: row.get("replaygain_track_peak")?,
        r128_loudness_lufs: row.get("r128_loudness_lufs")?,
        file_size_bytes: row.get("file_size_bytes")?,
        mtime_unix_secs: row.get("mtime_unix_secs")?,
        is_favorite: is_fav_int != 0,
        play_count: row.get("play_count")?,
        last_played_at: row.get("last_played_at")?,
        added_at: row.get("added_at")?,
    })
}

pub struct TrackRepo {
    db: Database,
}

impl TrackRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn upsert(&self, track: &Track) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tracks (
                    id, path, title, artist, album, genre, year, track_number, disc_number,
                    duration_secs, sample_rate, channels, format, bitrate_kbps, is_lossless,
                    replaygain_track_db, replaygain_track_peak, r128_loudness_lufs,
                    file_size_bytes, mtime_unix_secs, is_favorite, play_count, last_played_at, added_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
                ON CONFLICT(path) DO UPDATE SET
                    title = excluded.title,
                    artist = excluded.artist,
                    album = excluded.album,
                    genre = excluded.genre,
                    year = excluded.year,
                    track_number = excluded.track_number,
                    disc_number = excluded.disc_number,
                    duration_secs = excluded.duration_secs,
                    sample_rate = excluded.sample_rate,
                    channels = excluded.channels,
                    format = excluded.format,
                    bitrate_kbps = excluded.bitrate_kbps,
                    is_lossless = excluded.is_lossless,
                    replaygain_track_db = excluded.replaygain_track_db,
                    replaygain_track_peak = excluded.replaygain_track_peak,
                    r128_loudness_lufs = excluded.r128_loudness_lufs,
                    file_size_bytes = excluded.file_size_bytes,
                    mtime_unix_secs = excluded.mtime_unix_secs",
                params![
                    track.id.to_string(),
                    track.path.to_string_lossy(),
                    track.title,
                    track.artist,
                    track.album,
                    track.genre,
                    track.year,
                    track.track_number,
                    track.disc_number,
                    track.duration_secs,
                    track.sample_rate,
                    track.channels,
                    track.format,
                    track.bitrate_kbps,
                    if track.is_lossless { 1 } else { 0 },
                    track.replaygain_track_db,
                    track.replaygain_track_peak,
                    track.r128_loudness_lufs,
                    track.file_size_bytes,
                    track.mtime_unix_secs,
                    if track.is_favorite { 1 } else { 0 },
                    track.play_count,
                    track.last_played_at,
                    track.added_at,
                ],
            )?;

            // NOTE: the `artists` / `albums` tables are vestigial — albums and
            // artists are derived from `tracks` on demand (see AlbumRepo /
            // ArtistRepo). Maintaining counters here doubled the write cost of
            // every import and inflated on every re-upsert, so it is removed.

            Ok(())
        })
    }

    pub fn get_by_id(&self, id: &TrackId) -> Result<Option<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE id = ?1;")?;
            stmt.query_row(params![id.to_string()], map_track).optional()
        })
    }

    pub fn get_by_path(&self, path: &Path) -> Result<Option<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE path = ?1;")?;
            stmt.query_row(params![path.to_string_lossy()], map_track).optional()
        })
    }

    pub fn count(&self) -> Result<usize> {
        self.db.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM tracks;", [], |r| r.get(0))
        })
    }

    pub fn list(&self, sort: SortField, order: SortOrder) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let sort_col = match sort {
                SortField::Title => "title COLLATE NOCASE",
                SortField::Artist => "artist COLLATE NOCASE, album COLLATE NOCASE, track_number",
                SortField::Album => "album COLLATE NOCASE, track_number",
                SortField::Duration => "duration_secs",
                SortField::TrackNumber => "track_number",
                SortField::DateAdded => "added_at",
            };
            let dir = match order {
                SortOrder::Ascending => "ASC",
                SortOrder::Descending => "DESC",
            };
            let query = format!("SELECT * FROM tracks ORDER BY {} {};", sort_col, dir);
            let mut stmt = conn.prepare(&query)?;
            let iter = stmt.query_map([], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    pub fn set_favorite(&self, id: &TrackId, is_favorite: bool) -> Result<()> {
        self.db.with_conn(|conn| {
            let fav_int = if is_favorite { 1 } else { 0 };
            conn.execute(
                "UPDATE tracks SET is_favorite = ?1 WHERE id = ?2;",
                params![fav_int, id.to_string()],
            )?;
            if is_favorite {
                conn.execute(
                    "INSERT OR IGNORE INTO favorites (track_id, created_at) VALUES (?1, strftime('%s', 'now'));",
                    params![id.to_string()],
                )?;
            } else {
                conn.execute(
                    "DELETE FROM favorites WHERE track_id = ?1;",
                    params![id.to_string()],
                )?;
            }
            Ok(())
        })
    }

    pub fn increment_play_count(&self, id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET play_count = play_count + 1, last_played_at = strftime('%s', 'now') WHERE id = ?1;",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM tracks WHERE id = ?1;", params![id.to_string()])?;
            Ok(())
        })
    }

    /// Update the user-editable metadata tags of a track. Content that is
    /// derived from the file itself (path, duration, format, ...) is untouched.
    pub fn update_metadata(&self, track: &Track) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET
                    title = ?1, artist = ?2, album = ?3, genre = ?4, year = ?5,
                    track_number = ?6, disc_number = ?7
                 WHERE id = ?8;",
                params![
                    track.title,
                    track.artist,
                    track.album,
                    track.genre,
                    track.year,
                    track.track_number,
                    track.disc_number,
                    track.id.to_string(),
                ],
            )?;
            Ok(())
        })
    }

    /// Deletes every track whose path starts with `prefix`. Returns the number
    /// of rows removed. Foreign-key cascades clean up favorites, playlist
    /// entries, play history and lyrics; the FTS delete trigger fires too.
    pub fn delete_by_path_prefix(&self, prefix: &Path) -> Result<usize> {
        let like = format!("{}%", prefix.to_string_lossy());
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM tracks WHERE path LIKE ?1;", params![like])?;
            Ok(n)
        })
    }

    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        self.db.with_conn(|conn| {
            // Escape special characters and add prefix match token
            let sanitized: String = trimmed
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect();
            if sanitized.is_empty() {
                return Ok(Vec::new());
            }
            let fts_query = format!("{}*", sanitized);
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 JOIN tracks_fts fts ON fts.track_id = t.id
                 WHERE tracks_fts MATCH ?1
                 ORDER BY bm25(tracks_fts)
                 LIMIT ?2;",
            )?;
            let iter = stmt.query_map(params![fts_query, limit as i64], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    pub fn get_tracks_by_folder(&self, folder_path: &Path) -> Result<Vec<Track>> {
        let prefix = folder_path.to_string_lossy().to_string();
        let search = format!("{}%", prefix);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE path LIKE ?1 ORDER BY path ASC;")?;
            let iter = stmt.query_map(params![search], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }
}

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
                    id: PlaylistId::from_str(&id_str).unwrap_or_default(),
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
            let next_pos: usize = conn.query_row(
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

    pub fn delete(&self, id: &PlaylistId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM playlists WHERE id = ?1;", params![id.to_string()])?;
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
            // Re-number positions to keep them contiguous.
            tx.execute(
                "UPDATE playlist_tracks
                 SET position = (
                     SELECT COUNT(*) FROM playlist_tracks AS p2
                     WHERE p2.playlist_id = playlist_tracks.playlist_id
                       AND (p2.position < playlist_tracks.position
                            OR (p2.position = playlist_tracks.position AND p2.id <= playlist_tracks.id))
                 ) - 1
                 WHERE playlist_id = ?1;",
                params![playlist_id.to_string()],
            )?;
            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2;",
                params![now, playlist_id.to_string()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
}

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
    ) -> Result<()> {
        let now = Self::now_secs();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO play_history (id, track_id, played_at, duration_listened_secs, completed)
                 VALUES (?1, ?2, ?3, ?4, ?5);",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    track_id.to_string(),
                    now,
                    duration_listened_secs,
                    if completed { 1 } else { 0 },
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
            let n = conn.execute("DELETE FROM play_history WHERE played_at < ?1;", params![cutoff])?;
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

pub struct QueueRepo {
    db: Database,
}

impl QueueRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn save_queue(&self, tracks: &[(TrackId, PathBuf)]) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            tx.execute("DELETE FROM saved_queue;", [])?;
            for (pos, (id, path)) in tracks.iter().enumerate() {
                tx.execute(
                    "INSERT INTO saved_queue (position, track_id, path) VALUES (?1, ?2, ?3);",
                    params![pos as i64, id.to_string(), path.to_string_lossy()],
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

    /// Drops persisted queue entries whose path starts with `prefix`.
    pub fn purge_path_prefix(&self, prefix: &Path) -> Result<usize> {
        let like = format!("{}%", prefix.to_string_lossy());
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM saved_queue WHERE path LIKE ?1;", params![like])?;
            Ok(n)
        })
    }
}

pub struct SettingsRepo {
    db: Database,
}

impl SettingsRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn load(&self) -> Result<AppSettings> {
        self.db.with_conn(|conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'app_settings';",
                    [],
                    |r| r.get(0),
                )
                .optional()?;

            if let Some(s) = json {
                if let Ok(settings) = serde_json::from_str(&s) {
                    return Ok(settings);
                }
            }
            Ok(AppSettings::default())
        })
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        let json = serde_json::to_string(settings).unwrap_or_default();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('app_settings', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                params![json],
            )?;
            Ok(())
        })
    }

    pub fn get_value(&self, key: &str) -> Result<Option<String>> {
        self.db.with_conn(|conn| {
            let val: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = ?1;",
                    params![key],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(val)
        })
    }

    pub fn set_value(&self, key: &str, value: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
                params![key, value],
            )?;
            Ok(())
        })
    }
}

pub struct ScanRootsRepo {
    db: Database,
}

impl ScanRootsRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<ScanRoot>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT id, path, watch_enabled, last_scanned_at FROM scan_roots;")?;
            let iter = stmt.query_map([], |row| {
                let id_str: String = row.get(0)?;
                let path_str: String = row.get(1)?;
                let watch_int: i32 = row.get(2)?;
                Ok(ScanRoot {
                    id: ScanRootId::from_str(&id_str).unwrap_or_default(),
                    path: PathBuf::from(path_str),
                    watch_enabled: watch_int != 0,
                    last_scanned_at: row.get(3)?,
                })
            })?;
            let mut roots = Vec::new();
            for r in iter {
                roots.push(r?);
            }
            Ok(roots)
        })
    }

    pub fn add(&self, path: &Path) -> Result<ScanRoot> {
        let id = ScanRootId::new();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO scan_roots (id, path, watch_enabled, last_scanned_at)
                 VALUES (?1, ?2, 1, NULL)
                 ON CONFLICT(path) DO NOTHING;",
                params![id.to_string(), path.to_string_lossy()],
            )?;
            Ok(ScanRoot {
                id,
                path: path.to_path_buf(),
                watch_enabled: true,
                last_scanned_at: None,
            })
        })
    }

    pub fn update_last_scanned(&self, id: &ScanRootId, timestamp: i64) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE scan_roots SET last_scanned_at = ?1 WHERE id = ?2;",
                params![timestamp, id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn remove(&self, id: &ScanRootId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM scan_roots WHERE id = ?1;", params![id.to_string()])?;
            Ok(())
        })
    }
}

pub struct AlbumRepo {
    db: Database,
}

impl AlbumRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<Album>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT album, artist, year, COUNT(*) as cnt
                 FROM tracks
                 WHERE album IS NOT NULL AND album != ''
                 GROUP BY album, artist
                 ORDER BY album COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let title: String = row.get(0)?;
                let artist: String = row.get(1)?;
                let year: Option<u32> = row.get(2)?;
                let track_count: u32 = row.get(3)?;
                Ok(Album {
                    id: AlbumId::from_identity(&title, &artist),
                    title,
                    artist,
                    year,
                    track_count,
                    artwork_path: None,
                })
            })?;
            let mut albums = Vec::new();
            for a in iter {
                albums.push(a?);
            }
            Ok(albums)
        })
    }

    pub fn get_tracks_by_album(&self, title: &str, artist: &str) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                 WHERE album = ?1 AND artist = ?2
                 ORDER BY disc_number ASC, track_number ASC, title COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map(params![title, artist], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }
}

pub struct ArtistRepo {
    db: Database,
}

impl ArtistRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<Artist>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT artist, COUNT(DISTINCT album) as alb_cnt, COUNT(*) as trk_cnt
                 FROM tracks
                 WHERE artist IS NOT NULL AND artist != ''
                 GROUP BY artist
                 ORDER BY artist COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let album_count: u32 = row.get(1)?;
                let track_count: u32 = row.get(2)?;
                Ok(Artist {
                    id: ArtistId::from_name(&name),
                    name,
                    track_count,
                    album_count,
                })
            })?;
            let mut artists = Vec::new();
            for a in iter {
                artists.push(a?);
            }
            Ok(artists)
        })
    }

    pub fn get_tracks_by_artist(&self, artist: &str) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                 WHERE artist = ?1
                 ORDER BY year ASC, album COLLATE NOCASE ASC, track_number ASC;",
            )?;
            let iter = stmt.query_map(params![artist], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }
}

pub struct GenreRepo {
    db: Database,
}

impl GenreRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<Genre>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT genre, COUNT(*) as trk_cnt
                 FROM tracks
                 WHERE genre IS NOT NULL AND genre != ''
                 GROUP BY genre
                 ORDER BY genre COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let name: String = row.get(0)?;
                let track_count: u32 = row.get(1)?;
                Ok(Genre { name, track_count })
            })?;
            let mut genres = Vec::new();
            for g in iter {
                genres.push(g?);
            }
            Ok(genres)
        })
    }

    pub fn get_tracks_by_genre(&self, genre: &str) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                 WHERE genre = ?1
                 ORDER BY artist COLLATE NOCASE ASC, album COLLATE NOCASE ASC, track_number ASC;",
            )?;
            let iter = stmt.query_map(params![genre], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn map_smart_playlist(row: &Row<'_>) -> Result<SmartPlaylist> {
    let id_str: String = row.get("id")?;
    let rules_json: String = row.get("rules_json")?;
    let sort_by: Option<String> = row.get("sort_by")?;
    let limit: Option<i64> = row.get("limit_count")?;

    Ok(SmartPlaylist {
        id: PlaylistId::from_str(&id_str).unwrap_or_default(),
        name: row.get("name")?,
        match_all: row.get::<_, i32>("match_all")? != 0,
        rules: serde_json::from_str(&rules_json).unwrap_or_default(),
        limit_count: limit.map(|n| n.max(0) as u32),
        sort_by: sort_by.and_then(|s| serde_json::from_str(&s).ok()),
        sort_desc: row.get::<_, i32>("sort_desc")? != 0,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// Translate one rule into a SQL fragment plus its bound parameters. Returns
/// `None` when the rule is incomplete (e.g. an empty value), so half-typed
/// rules are simply ignored rather than matching nothing.
fn rule_to_clause(rule: &SmartRule) -> Option<(String, Vec<SqlValue>)> {
    let col = rule.field.column();
    let v = rule.value.trim();

    match rule.op {
        SmartOp::IsTrue => Some((format!("{col} = 1"), vec![])),
        SmartOp::IsFalse => Some((format!("{col} = 0"), vec![])),
        SmartOp::IsEmpty => Some((format!("({col} IS NULL OR {col} = '')"), vec![])),
        SmartOp::IsNotEmpty => Some((format!("({col} IS NOT NULL AND {col} != '')"), vec![])),
        SmartOp::Contains => {
            if v.is_empty() {
                None
            } else {
                Some((format!("{col} LIKE ?"), vec![SqlValue::Text(format!("%{v}%"))]))
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
            let op = if rule.op == SmartOp::GreaterThan { ">" } else { "<" };
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
            sql.push_str(&format!(" LIMIT {}", n.min(100_000)));
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
        let created = if sp.created_at == 0 { now } else { sp.created_at };
        let rules_json = serde_json::to_string(&sp.rules).unwrap_or_else(|_| "[]".to_string());
        let sort_by = sp
            .sort_by
            .and_then(|f| serde_json::to_string(&f).ok());
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


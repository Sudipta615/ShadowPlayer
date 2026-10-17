use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use rusqlite::{params, OptionalExtension, Result, Row};
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

            // Also maintain artists and albums counts
            conn.execute(
                "INSERT INTO artists (id, name, track_count)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(name) DO UPDATE SET track_count = track_count + 1",
                params![ArtistId::new().to_string(), track.artist],
            )?;

            conn.execute(
                "INSERT INTO albums (id, title, artist, year, track_count)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(title, artist) DO UPDATE SET track_count = track_count + 1",
                params![AlbumId::new().to_string(), track.album, track.artist, track.year],
            )?;

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
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn record_play(
        &self,
        track_id: &TrackId,
        duration_listened_secs: f32,
        completed: bool,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
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
            Ok(())
        })
    }

    pub fn list_recently_played(&self, limit: usize) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 WHERE t.last_played_at IS NOT NULL
                 ORDER BY t.last_played_at DESC
                 LIMIT ?1;",
            )?;
            let iter = stmt.query_map(params![limit as i64], map_track)?;
            let mut tracks = Vec::new();
            for t in iter {
                tracks.push(t?);
            }
            Ok(tracks)
        })
    }

    pub fn list_most_played(&self, limit: usize) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.* FROM tracks t
                 WHERE t.play_count > 0
                 ORDER BY t.play_count DESC, t.last_played_at DESC
                 LIMIT ?1;",
            )?;
            let iter = stmt.query_map(params![limit as i64], map_track)?;
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
                    id: AlbumId::new(),
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
                    id: ArtistId::new(),
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


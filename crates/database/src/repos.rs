use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::paths::{decode_path_key, encode_path_key};
use domain::smart::*;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, OptionalExtension, Result, Row};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Decode a UUID column.
///
/// A malformed value is a hard error rather than a fabricated id: the old
/// `unwrap_or_default()` produced a *fresh random* UUID for corrupt rows, so
/// the resulting track could never be matched back to the database and any
/// later update or delete silently did nothing.
fn parse_id<T: FromStr>(text: &str, column: &str) -> Result<T> {
    text.parse::<T>().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("column `{column}` holds a malformed id: {text:?}"),
            )),
        )
    })
}

/// Build an FTS5 query that can match what the table's `porter unicode61`
/// tokenizer actually indexed.
///
/// The old builder stripped every non-alphanumeric character and appended a
/// single `*`, so `AC/DC` became `ACDC*` — a token that is nowhere in the index,
/// because the tokenizer had stored `ac` and `dc` — and `don't` became `dont*`,
/// which never matched `don't Stop`. Nothing the index could contain was
/// reachable through punctuation.
///
/// Here the input is split exactly the way the tokenizer splits it (whitespace
/// gives terms, alphanumeric runs give tokens), and each term becomes either
/// `"token"*` or, for a term that spans several tokens, the phrase
/// `"first last"*` so `AC/DC` still finds `AC/DC` (and `AC DC`) while `ACDC`
/// stays a single unrelated token. Terms are `AND`ed. Everything is quoted, so
/// FTS5 operators a user happens to type (`AND`, `NOT`, `^`) are searched for
/// literally instead of being interpreted — and the result is always a valid
/// query, never a syntax error.
///
/// `None` means the input holds no searchable token at all.
fn fts_query(input: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();

    for term in input.split_whitespace() {
        // A term's own alphanumeric runs, in order — the tokenizer's output.
        let tokens: Vec<String> = term
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect();

        match tokens.len() {
            0 => continue,
            1 => terms.push(format!("\"{}\"*", tokens[0])),
            _ => terms.push(format!("\"{}\"*", tokens.join(" "))),
        }
    }

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

/// Escape a path for use as a `LIKE` prefix under `ESCAPE '\'`.
///
/// Two things the naive `format!("{}%", path)` got wrong: a prefix with no
/// trailing separator also matched sibling directories that merely start with
/// the same text (`/music/rock` matched `/music/rockabilly`), and any `%`, `_`
/// or `\` inside the path acted as a wildcard instead of a literal.
fn like_prefix(prefix: &Path) -> String {
    // `LIKE` runs against the stored path keys, so the prefix has to be
    // encoded the same way the paths were.
    let mut literal = encode_path_key(prefix);
    if literal.is_empty() {
        return "%".to_string();
    }
    if !literal.ends_with(std::path::MAIN_SEPARATOR) {
        literal.push(std::path::MAIN_SEPARATOR);
    }
    let escaped = literal
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

/// `LIKE` pattern that finds `needle` anywhere in a value.
///
/// The text comes straight from the search box, so its `%`, `_` and `\` have to
/// be escaped: typing `100%` must search for the percent sign, not for "one
/// followed by anything".
/// The ceiling a smart playlist's own limit is clamped to.
///
/// A rule list is a saved query that runs on every view; a limit of "however
/// many rows there are" is how a UI ends up rendering a hundred thousand rows.
const MAX_SMART_PLAYLIST_LIMIT: u32 = 100_000;

fn like_substring(needle: &str) -> String {
    let mut out = String::with_capacity(needle.len() + 2);
    out.push('%');
    for ch in needle.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

fn map_track(row: &Row<'_>) -> Result<Track> {
    let id_str: String = row.get("id")?;
    // The stored key is the lossless form; the real path is what the app
    // opens, plays and displays.
    let path_str: String = row.get("path")?;
    let is_fav_int: i32 = row.get("is_favorite")?;
    let is_lossless_int: i32 = row.get("is_lossless")?;

    Ok(Track {
        id: parse_id(&id_str, "tracks.id")?,
        path: decode_path_key(&path_str),
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
        mtime_nanos: row.get::<_, Option<i64>>("mtime_nanos")?.unwrap_or(0) as u32,
        content_hash: row.get("content_hash")?,
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
                    file_size_bytes, mtime_unix_secs, mtime_nanos, content_hash,
                    is_favorite, play_count, last_played_at, added_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)
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
                    mtime_unix_secs = excluded.mtime_unix_secs,
                    mtime_nanos = excluded.mtime_nanos,
                    content_hash = excluded.content_hash",
                params![
                    track.id.to_string(),
                    encode_path_key(&track.path),
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
                    track.mtime_nanos,
                    track.content_hash,
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

    /// Record a file's sub-second modification time.
    ///
    /// Used by the scanner when it accepts a second-granularity match from a row
    /// written before the column existed: the file is not re-decoded, but the
    /// next scan compares the finer value. Touches no indexed column, so it does
    /// not rewrite the search index.
    pub fn set_mtime_nanos(&self, id: &TrackId, nanos: u32) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET mtime_nanos = ?1 WHERE id = ?2;",
                params![nanos as i64, id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn get_by_id(&self, id: &TrackId) -> Result<Option<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE id = ?1;")?;
            stmt.query_row(params![id.to_string()], map_track)
                .optional()
        })
    }

    pub fn get_by_path(&self, path: &Path) -> Result<Option<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE path = ?1;")?;
            stmt.query_row(params![encode_path_key(path)], map_track)
                .optional()
        })
    }

    /// The identity of the track that already owns this content fingerprint.
    ///
    /// Returns `None` when the hash is unknown, or when it matches more than one
    /// row: an ambiguous match is ignored rather than risking the wrong relink.
    pub fn find_by_content_hash(&self, hash: &str) -> Result<Option<(TrackId, PathBuf)>> {
        self.db.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, path FROM tracks WHERE content_hash = ?1 LIMIT 2;")?;
            let mut rows = stmt.query(params![hash])?;
            let mut found: Vec<(TrackId, PathBuf)> = Vec::new();
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                found.push((parse_id(&id, "tracks.id")?, decode_path_key(&path)));
            }
            Ok(if found.len() == 1 { found.pop() } else { None })
        })
    }

    /// Point an existing track at a new location, preserving its id and hence
    /// its play counts, favourite flag, playlist membership, lyrics and
    /// artwork. Returns `false` when no such track exists.
    pub fn relink_path(
        &self,
        id: &TrackId,
        new_path: &Path,
        content_hash: Option<&str>,
    ) -> Result<bool> {
        self.db.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE tracks SET path = ?1, content_hash = ?2 WHERE id = ?3;",
                params![encode_path_key(new_path), content_hash, id.to_string()],
            )?;
            Ok(changed > 0)
        })
    }

    /// Store (or clear) a track's content fingerprint without touching anything
    /// else. Used to backfill rows indexed before fingerprints existed.
    pub fn set_content_hash(&self, id: &TrackId, content_hash: Option<&str>) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET content_hash = ?1 WHERE id = ?2;",
                params![content_hash, id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Groups of tracks whose contents are byte-identical (same fingerprint).
    ///
    /// Only fingerprinted tracks participate, so nothing is guessed from tags or
    /// file names. Groups are ordered by size (largest first).
    pub fn find_duplicates(&self) -> Result<Vec<Vec<Track>>> {
        self.db.with_conn(|conn| {
            // One query for the whole census. The old shape listed the hashes
            // first and then ran a full-track `SELECT` per hash — N+1 queries
            // over a table that can hold a whole library.
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                 WHERE content_hash IN (
                     SELECT content_hash FROM tracks
                     WHERE content_hash IS NOT NULL AND content_hash <> ''
                     GROUP BY content_hash HAVING COUNT(*) > 1
                 )
                 ORDER BY content_hash, id;",
            )?;
            let tracks: Vec<Track> = stmt.query_map([], map_track)?.collect::<Result<_, _>>()?;

            // The rows arrive grouped: start a new group whenever the hash
            // changes.
            let mut groups: Vec<Vec<Track>> = Vec::new();
            for track in tracks {
                match groups.last_mut() {
                    Some(group) if group[0].content_hash == track.content_hash => {
                        group.push(track);
                    }
                    _ => groups.push(vec![track]),
                }
            }

            // Largest groups first (as documented), then by hash so the order is
            // deterministic between runs.
            groups.sort_by(|a, b| {
                b.len()
                    .cmp(&a.len())
                    .then_with(|| a[0].content_hash.cmp(&b[0].content_hash))
            });
            Ok(groups)
        })
    }

    pub fn count(&self) -> Result<usize> {
        self.db
            .with_conn(|conn| conn.query_row("SELECT COUNT(*) FROM tracks;", [], |r| r.get(0)))
    }

    pub fn list(&self, sort: SortField, order: SortOrder) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let sort_col = match sort {
                SortField::Title => "title COLLATE NOCASE",
                SortField::Artist => "artist COLLATE NOCASE, album COLLATE NOCASE, track_number",
                SortField::Album => "album COLLATE NOCASE, track_number",
                SortField::Duration => "duration_secs",
                SortField::TrackNumber => "disc_number, track_number",
                SortField::DateAdded => "added_at",
                // One-sided by design: favourites first, then title, in both
                // directions the caller may ask for.
                SortField::Favorite => "is_favorite DESC, title COLLATE NOCASE",
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

    /// Store measured loudness values for a track (ReplayGain scan results).
    pub fn set_loudness(
        &self,
        id: &TrackId,
        replaygain_track_db: Option<f32>,
        replaygain_track_peak: Option<f32>,
        r128_loudness_lufs: Option<f32>,
    ) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks
                    SET replaygain_track_db = ?1,
                        replaygain_track_peak = ?2,
                        r128_loudness_lufs = ?3
                  WHERE id = ?4;",
                params![
                    replaygain_track_db,
                    replaygain_track_peak,
                    r128_loudness_lufs,
                    id.to_string()
                ],
            )?;
            Ok(())
        })
    }

    /// Tracks that have no measured loudness yet, for a library-wide scan.
    pub fn list_missing_replaygain(&self) -> Result<Vec<(TrackId, PathBuf)>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, path FROM tracks
                  WHERE replaygain_track_db IS NULL
                  ORDER BY artist COLLATE NOCASE ASC, album COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                Ok((parse_id(&id, "tracks.id")?, decode_path_key(&path)))
            })?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
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

    /// Deletes every track *inside* `prefix`. Returns the number of rows
    /// removed. Foreign-key cascades clean up favorites, playlist entries, play
    /// history and lyrics; the FTS delete trigger fires too.
    ///
    /// The prefix is matched up to and including its trailing separator, so
    /// deleting `/music/rock` leaves `/music/rockabilly` alone.
    pub fn delete_by_path_prefix(&self, prefix: &Path) -> Result<usize> {
        let like = like_prefix(prefix);
        self.db.with_conn(|conn| {
            let n = conn.execute(
                "DELETE FROM tracks WHERE path LIKE ?1 ESCAPE '\\';",
                params![like],
            )?;
            Ok(n)
        })
    }

    /// Every (id, path) pair inside `prefix`, for reconciling a scan root with
    /// what is actually on disk.
    pub fn list_paths_under_prefix(&self, prefix: &Path) -> Result<Vec<(TrackId, PathBuf)>> {
        let like = like_prefix(prefix);
        self.db.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, path FROM tracks WHERE path LIKE ?1 ESCAPE '\\';")?;
            let iter = stmt.query_map(params![like], |row| {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                Ok((parse_id(&id, "tracks.id")?, decode_path_key(&path)))
            })?;
            let mut out = Vec::new();
            for r in iter {
                out.push(r?);
            }
            Ok(out)
        })
    }

    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        self.db.with_conn(|conn| {
            let Some(fts_query) = fts_query(trimmed) else {
                return Ok(Vec::new());
            };
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
        let search = like_prefix(folder_path);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks WHERE path LIKE ?1 ESCAPE '\\' ORDER BY path ASC;",
            )?;
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

            match json {
                Some(s) => match serde_json::from_str(&s) {
                    Ok(settings) => Ok(settings),
                    Err(e) => {
                        // Do not fail the launch over unreadable settings, but
                        // make the loss visible: `AppSettings` carries
                        // `#[serde(default)]`, so a payload written by an older
                        // build with fewer fields still loads.
                        log::warn!("Ignoring unreadable app settings, using defaults: {e}");
                        Ok(AppSettings::default())
                    }
                },
                None => Ok(AppSettings::default()),
            }
        })
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        // `unwrap_or_default()` here used to write an *empty string* when
        // serialization failed, which the next load then treated as "no
        // settings" — silently wiping the user's configuration.
        let json = serde_json::to_string(settings).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("could not serialize app settings: {e}"),
            )))
        })?;
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
            let mut stmt =
                conn.prepare("SELECT id, path, watch_enabled, last_scanned_at FROM scan_roots;")?;
            let iter = stmt.query_map([], |row| {
                let id_str: String = row.get(0)?;
                let path_str: String = row.get(1)?;
                let watch_int: i32 = row.get(2)?;
                Ok(ScanRoot {
                    id: parse_id(&id_str, "scan_roots.id")?,
                    path: decode_path_key(&path_str),
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

    /// Register `path` as a scan root and return the stored row.
    ///
    /// Idempotent: re-adding an existing path returns the row that is already
    /// there (with its real id and last-scanned timestamp) rather than a newly
    /// generated id that no row carries — which used to make a later
    /// `update_last_scanned` or `remove` silently no-op.
    pub fn add(&self, path: &Path) -> Result<ScanRoot> {
        let id = ScanRootId::new();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO scan_roots (id, path, watch_enabled, last_scanned_at)
                 VALUES (?1, ?2, 1, NULL)
                 ON CONFLICT(path) DO NOTHING;",
                params![id.to_string(), encode_path_key(path)],
            )?;

            let stored = conn.query_row(
                "SELECT id, path, watch_enabled, last_scanned_at FROM scan_roots WHERE path = ?1;",
                params![encode_path_key(path)],
                |row| {
                    let id_str: String = row.get(0)?;
                    let path_str: String = row.get(1)?;
                    let watch_int: i32 = row.get(2)?;
                    Ok(ScanRoot {
                        id: parse_id(&id_str, "scan_roots.id")?,
                        path: decode_path_key(&path_str),
                        watch_enabled: watch_int != 0,
                        last_scanned_at: row.get(3)?,
                    })
                },
            )?;
            Ok(stored)
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
            conn.execute(
                "DELETE FROM scan_roots WHERE id = ?1;",
                params![id.to_string()],
            )?;
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
            // `MIN(year)`, not `year`: with a bare column in a GROUP BY, SQLite
            // picks the value from an arbitrary row of the group, so an album
            // whose tracks disagree about the year (reissues, compilations,
            // sloppy tags) could show a different year on each run.
            let mut stmt = conn.prepare(
                "SELECT album, artist, MIN(year), COUNT(*) as cnt
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

    /// Albums whose title matches `query`, most tracks first.
    ///
    /// Same reason as `ArtistRepo::search`: albums were derived from the
    /// truncated track page and quietly missed everything past the limit. The
    /// old shape also deduplicated titles (two artists can share an album
    /// name), so the grouping keeps that.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let like = like_substring(query);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT album, COUNT(*) as cnt
                 FROM tracks
                 WHERE album IS NOT NULL AND album != '' AND album LIKE ?1 ESCAPE '\\'
                 GROUP BY album
                 ORDER BY cnt DESC, album COLLATE NOCASE ASC
                 LIMIT ?2;",
            )?;
            let iter =
                stmt.query_map(params![like, limit as i64], |row| row.get::<_, String>(0))?;
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

    /// Artists whose name matches `query`, most-tracks first.
    ///
    /// The search box used to derive artists from the already-truncated track
    /// page, so an artist whose tracks ranked past the limit never appeared.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let like = like_substring(query);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT artist, COUNT(*) as trk_cnt
                 FROM tracks
                 WHERE artist IS NOT NULL AND artist != '' AND artist LIKE ?1 ESCAPE '\\'
                 GROUP BY artist
                 ORDER BY trk_cnt DESC, artist COLLATE NOCASE ASC
                 LIMIT ?2;",
            )?;
            let iter =
                stmt.query_map(params![like, limit as i64], |row| row.get::<_, String>(0))?;
            let mut artists = Vec::new();
            for a in iter {
                artists.push(a?);
            }
            Ok(artists)
        })
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

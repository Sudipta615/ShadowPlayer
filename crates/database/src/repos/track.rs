//! Track and track-DSP repositories.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::paths::{decode_path_key, encode_path_key};
use domain::smart::RadioQuery;
use rusqlite::{params, OptionalExtension, Result};
use std::path::{Path, PathBuf};

use super::common::{fts_query, like_prefix, map_track, parse_id};

/// Compact metadata record used to accelerate library scanning without loading
/// full Track structures or performing individual queries per track.
#[derive(Debug, Clone)]
pub struct TrackScanEntry {
    pub id: TrackId,
    pub path: PathBuf,
    pub mtime_unix_secs: i64,
    pub mtime_nanos: u32,
    pub file_size_bytes: u64,
    pub content_hash: Option<String>,
    pub file_id: Option<String>,
}

const UPSERT_TRACK_SQL: &str = "INSERT INTO tracks (
    id, path, title, artist, album, genre, year, track_number, disc_number,
    duration_secs, sample_rate, channels, format, bitrate_kbps, bit_depth, is_lossless,
    replaygain_track_db, replaygain_track_peak, r128_loudness_lufs,
    replaygain_album_db, replaygain_album_peak,
    file_size_bytes, mtime_unix_secs, mtime_nanos, content_hash, file_id,
    is_favorite, rating, has_lyrics, play_count, skip_count, last_played_at, added_at, bpm
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34)
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
    bit_depth = excluded.bit_depth,
    is_lossless = excluded.is_lossless,
    replaygain_track_db = excluded.replaygain_track_db,
    replaygain_track_peak = excluded.replaygain_track_peak,
    r128_loudness_lufs = excluded.r128_loudness_lufs,
    replaygain_album_db = excluded.replaygain_album_db,
    replaygain_album_peak = excluded.replaygain_album_peak,
    has_lyrics = excluded.has_lyrics,
    skip_count = excluded.skip_count,
    bpm = excluded.bpm,
    file_size_bytes = excluded.file_size_bytes,
    mtime_unix_secs = excluded.mtime_unix_secs,
    mtime_nanos = excluded.mtime_nanos,
    content_hash = excluded.content_hash,
    file_id = COALESCE(excluded.file_id, tracks.file_id)";

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
                UPSERT_TRACK_SQL,
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
                    track.channels as i64,
                    track.format,
                    track.bitrate_kbps,
                    track.bit_depth,
                    if track.is_lossless { 1 } else { 0 },
                    track.replaygain_track_db,
                    track.replaygain_track_peak,
                    track.r128_loudness_lufs,
                    track.replaygain_album_db,
                    track.replaygain_album_peak,
                    track.file_size_bytes as i64,
                    track.mtime_unix_secs,
                    track.mtime_nanos,
                    track.content_hash,
                    track.file_id,
                    if track.is_favorite { 1 } else { 0 },
                    track.rating.min(5) as i64,
                    if track.has_lyrics { 1 } else { 0 },
                    track.play_count,
                    track.skip_count as i64,
                    track.last_played_at,
                    track.added_at,
                    track.bpm,
                ],
            )?;

            Ok(())
        })
    }

    /// Upsert multiple tracks in a single transaction. Drastically reduces
    /// disk sync overhead on spinning HDDs and low-spec storage during scans.
    pub fn upsert_batch(&self, tracks: &[Track]) -> Result<()> {
        if tracks.is_empty() {
            return Ok(());
        }
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare_cached(UPSERT_TRACK_SQL)?;
                for track in tracks {
                    stmt.execute(params![
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
                        track.channels as i64,
                        track.format,
                        track.bitrate_kbps,
                        track.bit_depth,
                        if track.is_lossless { 1 } else { 0 },
                        track.replaygain_track_db,
                        track.replaygain_track_peak,
                        track.r128_loudness_lufs,
                        track.replaygain_album_db,
                        track.replaygain_album_peak,
                        track.file_size_bytes as i64,
                        track.mtime_unix_secs,
                        track.mtime_nanos,
                        track.content_hash,
                        track.file_id,
                        if track.is_favorite { 1 } else { 0 },
                        track.rating.min(5) as i64,
                        if track.has_lyrics { 1 } else { 0 },
                        track.play_count,
                        track.skip_count as i64,
                        track.last_played_at,
                        track.added_at,
                        track.bpm,
                    ])?;
                }
            }
            tx.commit()?;
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

    /// The rows that the filesystem says are this file.
    ///
    /// This is the lookup that survives a rename — including a rename that
    /// only changed the case, which a case-insensitive filesystem (Windows,
    /// macOS by default) treats as the same file under two spellings. The
    /// returned rows may have more than one entry only if the library was
    /// already holding duplicates before the file-identity migration merged
    /// them; the caller gets them all so it can decide.
    pub fn find_by_file_id(&self, file_id: &str) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT * FROM tracks WHERE file_id = ?1;")?;
            let mapped = stmt.query_map(params![file_id], map_track)?;
            let mut out = Vec::new();
            for row in mapped {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Record a track's filesystem identity, and/or point it at a new path.
    ///
    /// The two are updated together because they change together: a file that
    /// moved keeps its identity and gains a new path.
    pub fn set_file_id_and_path(
        &self,
        id: &TrackId,
        file_id: Option<&str>,
        new_path: Option<&Path>,
    ) -> Result<bool> {
        self.db.with_conn(|conn| {
            let changed = match (file_id, new_path) {
                (Some(file_id), Some(path)) => conn.execute(
                    "UPDATE tracks SET file_id = ?1, path = ?2 WHERE id = ?3;",
                    params![file_id, encode_path_key(path), id.to_string()],
                )?,
                (Some(file_id), None) => conn.execute(
                    "UPDATE tracks SET file_id = ?1 WHERE id = ?2;",
                    params![file_id, id.to_string()],
                )?,
                (None, Some(path)) => conn.execute(
                    "UPDATE tracks SET path = ?1 WHERE id = ?2;",
                    params![encode_path_key(path), id.to_string()],
                )?,
                (None, None) => 0,
            };
            Ok(changed > 0)
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
        self.db.with_conn(|conn| {
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM tracks;", [], |r| r.get(0))?;
            Ok(count.max(0) as usize)
        })
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
                SortField::Rating => "rating, title COLLATE NOCASE",
            };
            let dir = match order {
                SortOrder::Ascending => "ASC",
                SortOrder::Descending => "DESC",
            };
            // The direction applies to every key, not just the last one.
            // `ORDER BY rating, title DESC` sorts the rating ascending and
            // only breaks ties the other way, which made "sort by rating,
            // highest first" quietly do nothing.
            let order_expr = if sort == SortField::Favorite {
                sort_col.to_string()
            } else {
                sort_col
                    .split(", ")
                    .map(|column| format!("{column} {dir}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let query = format!("SELECT * FROM tracks ORDER BY {order_expr};");
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

    /// Store a listener's star rating. `0` means unrated; values above five
    /// are clamped rather than becoming an unreachable sixth star.
    pub fn set_rating(&self, id: &TrackId, rating: u8) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET rating = ?1 WHERE id = ?2;",
                params![rating.min(5) as i64, id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Rate several tracks in one transaction.
    ///
    /// Bulk rating is a common gesture (select an album, press Ctrl+4); doing
    /// one transaction per row would fsync the database once per track.
    pub fn rate_many(&self, ids: &[TrackId], rating: u8) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.db.with_conn(|conn| {
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare("UPDATE tracks SET rating = ?1 WHERE id = ?2;")?;
                for id in ids {
                    stmt.execute(params![rating.min(5) as i64, id.to_string()])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Clear a track's rating (set it back to unrated).
    pub fn clear_rating(&self, id: &TrackId) -> Result<()> {
        self.set_rating(id, 0)
    }

    /// Record whether a lyric was found for this track.
    pub fn set_has_lyrics(&self, id: &TrackId, has_lyrics: bool) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET has_lyrics = ?1 WHERE id = ?2;",
                params![if has_lyrics { 1 } else { 0 }, id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Note that a track was skipped before it was heard.
    pub fn increment_skip_count(&self, id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE tracks SET skip_count = skip_count + 1 WHERE id = ?1;",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }

    /// The tracks that are skipped most often.
    pub fn list_most_skipped(&self, limit: usize) -> Result<Vec<Track>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                  WHERE skip_count > 0
                  ORDER BY skip_count DESC, title COLLATE NOCASE ASC
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

    /// Store an album's ReplayGain gain for every track of that album.
    ///
    /// ReplayGain 2.0's album gain belongs to the album, not to a track, so
    /// every track of the album carries the same value. The album is
    /// identified by name and artist, which is how the library groups it.
    pub fn set_album_loudness(
        &self,
        album: &str,
        artist: &str,
        replaygain_album_db: Option<f32>,
        replaygain_album_peak: Option<f32>,
    ) -> Result<usize> {
        self.db.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE tracks
                    SET replaygain_album_db = ?1,
                        replaygain_album_peak = ?2
                  WHERE album = ?3 AND artist = ?4;",
                params![replaygain_album_db, replaygain_album_peak, album, artist],
            )?;
            Ok(updated)
        })
    }

    /// Tracks that have no measured loudness yet, for a library-wide scan.
    pub fn list_missing_replaygain(&self) -> Result<Vec<(TrackId, PathBuf)>> {
        Ok(self
            .list_missing_replaygain_with_album()?
            .into_iter()
            .map(|(id, path, _, _)| (id, path))
            .collect())
    }

    /// Tracks that have no measured loudness yet, with the album they belong
    /// to.
    ///
    /// The album and artist are needed because ReplayGain's album gain is a
    /// property of the album, so the scan has to group its results by album
    /// before it can compute one.
    pub fn list_missing_replaygain_with_album(
        &self,
    ) -> Result<Vec<(TrackId, PathBuf, String, String)>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, path, album, artist FROM tracks
                  WHERE replaygain_track_db IS NULL
                  ORDER BY artist COLLATE NOCASE ASC, album COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                let album: String = row.get(2)?;
                let artist: String = row.get(3)?;
                Ok((
                    parse_id(&id, "tracks.id")?,
                    decode_path_key(&path),
                    album,
                    artist,
                ))
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

    /// Fast census of all tracks in `prefix` with their file identity and timestamp info.
    /// Used by the library scanner to check for unchanged tracks in memory without issuing
    /// thousands of individual SELECT queries over spinning disk.
    pub fn list_scan_entries_under_prefix(
        &self,
        prefix: &Path,
    ) -> Result<std::collections::HashMap<PathBuf, TrackScanEntry>> {
        let like = like_prefix(prefix);
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, path, mtime_unix_secs, mtime_nanos, file_size_bytes, content_hash, file_id
                 FROM tracks WHERE path LIKE ?1 ESCAPE '\\';",
            )?;
            let iter = stmt.query_map(params![like], |row| {
                let id_str: String = row.get(0)?;
                let path_str: String = row.get(1)?;
                let mtime_unix_secs: i64 = row.get(2)?;
                let mtime_nanos: u32 = row.get(3)?;
                let file_size_bytes: i64 = row.get(4)?;
                let content_hash: Option<String> = row.get(5)?;
                let file_id: Option<String> = row.get(6)?;
                let id = parse_id(&id_str, "tracks.id")?;
                let path = decode_path_key(&path_str);
                Ok((
                    path.clone(),
                    TrackScanEntry {
                        id,
                        path,
                        mtime_unix_secs,
                        mtime_nanos,
                        file_size_bytes: file_size_bytes.max(0) as u64,
                        content_hash,
                        file_id,
                    },
                ))
            })?;
            let mut map = std::collections::HashMap::new();
            for r in iter {
                let (k, v) = r?;
                map.insert(k, v);
            }
            Ok(map)
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

/// Per-track DSP profiles.
///
/// A profile is a track's own graphic EQ and preamp, applied when that track
/// plays. The global chain is untouched.
pub struct TrackDspRepo {
    db: Database,
}

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

impl TrackRepo {
    /// Build a local radio station around `seed`.
    ///
    /// The SQL prefilter keeps this affordable on a large library: only tracks
    /// that share a genre, artist or a tempo within 20 BPM are scored. If that
    /// pool is too small (a seed with no genre and no BPM), the rest of the
    /// library is considered rather than returning an almost-empty station.
    pub fn radio_station(&self, seed_id: &TrackId, limit: usize, salt: u64) -> Result<Vec<Track>> {
        let seed = match self.get_by_id(seed_id)? {
            Some(track) => track,
            None => return Ok(Vec::new()),
        };
        let query = RadioQuery {
            genre: seed.genre.clone(),
            artist: seed.artist.clone(),
            bpm: seed.bpm,
            rating: seed.rating,
            salt,
        };
        let limit = limit.clamp(1, 500);

        let mut pool: Vec<Track> = self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT * FROM tracks
                  WHERE id != ?1
                    AND (genre = ?2 COLLATE NOCASE
                         OR artist = ?3 COLLATE NOCASE
                         OR (bpm IS NOT NULL AND ?4 IS NOT NULL
                             AND bpm BETWEEN ?4 - 20 AND ?4 + 20))
                  LIMIT 2000;",
            )?;
            let rows = stmt.query_map(
                params![seed.id.to_string(), seed.genre, seed.artist, seed.bpm],
                map_track,
            )?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })?;

        if pool.len() < limit {
            // Widen the net. This is still the user's own library and still
            // local; it just means the seed was too unusual to fill a station.
            let existing: std::collections::HashSet<TrackId> = pool.iter().map(|t| t.id).collect();
            let rest: Vec<Track> = self.db.with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT * FROM tracks WHERE id != ?1
                      ORDER BY play_count ASC, added_at DESC LIMIT 2000;",
                )?;
                let rows = stmt.query_map(params![seed.id.to_string()], map_track)?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })?;
            pool.extend(rest.into_iter().filter(|t| !existing.contains(&t.id)));
        }

        pool.sort_by(|a, b| {
            query
                .score(b)
                .cmp(&query.score(a))
                .then_with(|| query.shuffle_key(a).cmp(&query.shuffle_key(b)))
        });
        pool.truncate(limit);
        Ok(pool)
    }
}

//! Library hierarchy repositories: albums, notes, artists, genres, scan roots.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::paths::{decode_path_key, encode_path_key};
use rusqlite::{params, OptionalExtension, Result, Row};
use std::path::Path;

use super::common::{like_substring, map_track, now_secs, parse_id};

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

/// The largest liner-notes document the database accepts. The Markdown editor
/// uses the same ceiling; enforcing it here too means an older or alternative
/// client cannot fill the settings database with a pasted novel.
const MAX_ALBUM_NOTES_BYTES: usize = 256 * 1024;

/// Markdown liner notes, one document per album.
pub struct AlbumNotesRepo {
    db: Database,
}

impl AlbumNotesRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Load an album's notes, or `None` when it has not been written yet.
    pub fn get(&self, album_id: &AlbumId) -> Result<Option<AlbumNote>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT album_id, title, artist, notes_markdown, updated_at
                 FROM album_notes WHERE album_id = ?1;",
                params![album_id.to_string()],
                map_album_note,
            )
            .optional()
        })
    }

    /// Create or replace an album's notes.
    ///
    /// Empty notes delete the row rather than storing a blank document: the
    /// album view can then distinguish "never written" from "written and
    /// cleared", and an empty note does not shadow a future bundled note.
    pub fn save(
        &self,
        album_id: &AlbumId,
        title: &str,
        artist: &str,
        markdown: &str,
    ) -> Result<()> {
        if markdown.len() > MAX_ALBUM_NOTES_BYTES {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "liner notes are {} bytes; the limit is {MAX_ALBUM_NOTES_BYTES}",
                        markdown.len()
                    ),
                ),
            )));
        }
        if markdown.contains('\0') {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "liner notes contain a NUL byte".to_string(),
                ),
            )));
        }
        if markdown.trim().is_empty() {
            self.delete(album_id)?;
            return Ok(());
        }
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO album_notes
                    (album_id, title, artist, notes_markdown, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(album_id) DO UPDATE SET
                    title = excluded.title,
                    artist = excluded.artist,
                    notes_markdown = excluded.notes_markdown,
                    updated_at = excluded.updated_at;",
                params![album_id.to_string(), title, artist, markdown, now_secs()],
            )?;
            Ok(())
        })
    }

    /// Remove an album's notes. Returns whether a document existed.
    pub fn delete(&self, album_id: &AlbumId) -> Result<bool> {
        self.db.with_conn(|conn| {
            let removed = conn.execute(
                "DELETE FROM album_notes WHERE album_id = ?1;",
                params![album_id.to_string()],
            )?;
            Ok(removed > 0)
        })
    }

    /// Notes that have been written, newest edit first. This drives export and
    /// the "albums with notes" view without scanning the whole library.
    pub fn list(&self) -> Result<Vec<AlbumNote>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT album_id, title, artist, notes_markdown, updated_at
                 FROM album_notes
                 ORDER BY updated_at DESC, title COLLATE NOCASE ASC;",
            )?;
            let iter = stmt.query_map([], map_album_note)?;
            let mut notes = Vec::new();
            for note in iter {
                notes.push(note?);
            }
            Ok(notes)
        })
    }

    /// Every album that has notes. Used by the UI to mark which albums in the
    /// current page have notes without loading every note's text.
    pub fn ids_with_notes(&self) -> Result<Vec<AlbumId>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT album_id FROM album_notes;")?;
            let iter = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut ids = Vec::new();
            for id in iter {
                ids.push(parse_id::<AlbumId>(&id?, "album_id")?);
            }
            Ok(ids)
        })
    }
}

fn map_album_note(row: &Row<'_>) -> Result<AlbumNote> {
    let album_id: String = row.get(0)?;
    Ok(AlbumNote {
        album_id: parse_id(&album_id, "album_id")?,
        title: row.get(1)?,
        artist: row.get(2)?,
        markdown: row.get(3)?,
        updated_at: row.get(4)?,
    })
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

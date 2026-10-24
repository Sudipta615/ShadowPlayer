//! Media-derived repositories: waveforms, biographies, and podcasts.

use crate::db::Database;
use domain::ids::*;
use domain::models::*;
use domain::paths::{decode_path_key, encode_path_key};
use rusqlite::{params, OptionalExtension, Result, Row};

use std::path::Path;

use super::common::{now_secs, parse_id};

// ── Tier 2 caches and podcast library ───────────────────────────────────────

/// One cached waveform: one byte per peak, plus the duration the peaks were
/// extracted from.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformPeaks {
    pub peaks: Vec<u8>,
    pub duration_secs: f32,
}

impl WaveformPeaks {
    /// Normalised 0.0–1.0 peaks, ready to paint.
    pub fn amplitudes(&self) -> Vec<f32> {
        self.peaks.iter().map(|p| *p as f32 / 255.0).collect()
    }
}

/// Cached waveform peaks, keyed by track.
pub struct WaveformRepo {
    db: Database,
}

impl WaveformRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn get(&self, id: &TrackId) -> Result<Option<WaveformPeaks>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT peaks, duration_secs FROM track_waveforms WHERE track_id = ?1;",
                params![id.to_string()],
                |row| {
                    Ok(WaveformPeaks {
                        peaks: row.get(0)?,
                        duration_secs: row.get(1)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn put(&self, id: &TrackId, peaks: &[u8], duration_secs: f32) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO track_waveforms
                    (track_id, peaks, peak_count, duration_secs, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(track_id) DO UPDATE SET
                    peaks = excluded.peaks,
                    peak_count = excluded.peak_count,
                    duration_secs = excluded.duration_secs,
                    updated_at = excluded.updated_at;",
                params![
                    id.to_string(),
                    peaks,
                    peaks.len() as i64,
                    duration_secs,
                    now_secs(),
                ],
            )?;
            Ok(())
        })
    }

    pub fn count(&self) -> Result<u32> {
        self.db.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM track_waveforms;", [], |r| {
                r.get::<_, i64>(0).map(|n| n as u32)
            })
        })
    }
}

/// Cached artist and album biographies.
pub struct BiographyRepo {
    db: Database,
}

impl BiographyRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn get(&self, kind: &str, name: &str) -> Result<Option<Biography>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT entity_kind, entity_name, body, source, updated_at
                   FROM biographies WHERE entity_kind = ?1 AND entity_name = ?2;",
                params![kind, name],
                |row| {
                    Ok(Biography {
                        entity_kind: row.get(0)?,
                        entity_name: row.get(1)?,
                        text: row.get(2)?,
                        source: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()
        })
    }

    pub fn put(&self, kind: &str, name: &str, text: &str, source: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO biographies
                    (entity_kind, entity_name, body, source, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(entity_kind, entity_name) DO UPDATE SET
                    body = excluded.body,
                    source = excluded.source,
                    updated_at = excluded.updated_at;",
                params![kind, name, text, source, now_secs()],
            )?;
            Ok(())
        })
    }

    pub fn delete(&self, kind: &str, name: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM biographies WHERE entity_kind = ?1 AND entity_name = ?2;",
                params![kind, name],
            )?;
            Ok(())
        })
    }
}

/// Subscribed podcast feeds and their episodes.
pub struct PodcastRepo {
    db: Database,
}

impl PodcastRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn list_feeds(&self) -> Result<Vec<PodcastFeed>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, url, title, description, artwork_url, last_fetched_at, added_at
                   FROM podcast_feeds ORDER BY title COLLATE NOCASE ASC;",
            )?;
            let rows = stmt.query_map([], map_podcast_feed)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn get_feed(&self, id: &PodcastFeedId) -> Result<Option<PodcastFeed>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, url, title, description, artwork_url, last_fetched_at, added_at
                   FROM podcast_feeds WHERE id = ?1;",
                params![id.to_string()],
                map_podcast_feed,
            )
            .optional()
        })
    }

    pub fn get_feed_by_url(&self, url: &str) -> Result<Option<PodcastFeed>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, url, title, description, artwork_url, last_fetched_at, added_at
                   FROM podcast_feeds WHERE url = ?1;",
                params![url],
                map_podcast_feed,
            )
            .optional()
        })
    }

    /// Insert a new feed, or refresh the metadata of an existing URL.
    ///
    /// The id is kept for an existing URL so its episodes and their resume
    /// positions survive a refresh.
    pub fn upsert_feed(&self, feed: &PodcastFeed) -> Result<PodcastFeed> {
        let existing = self.get_feed_by_url(&feed.url)?;
        let id = existing.as_ref().map(|f| f.id).unwrap_or(feed.id);
        let added_at = existing
            .as_ref()
            .map(|f| f.added_at)
            .unwrap_or(feed.added_at);
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO podcast_feeds
                    (id, url, title, description, artwork_url, last_fetched_at, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(url) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    artwork_url = excluded.artwork_url,
                    last_fetched_at = excluded.last_fetched_at;",
                params![
                    id.to_string(),
                    feed.url,
                    feed.title,
                    feed.description,
                    feed.artwork_url,
                    feed.last_fetched_at,
                    added_at,
                ],
            )?;
            Ok(())
        })?;
        Ok(PodcastFeed {
            id,
            added_at,
            ..feed.clone()
        })
    }

    pub fn delete_feed(&self, id: &PodcastFeedId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM podcast_feeds WHERE id = ?1;",
                params![id.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn list_episodes(&self, feed_id: &PodcastFeedId) -> Result<Vec<PodcastEpisode>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, feed_id, guid, title, description, enclosure_url,
                        published_at, duration_secs, local_path, position_secs,
                        completed, added_at
                   FROM podcast_episodes WHERE feed_id = ?1
                  ORDER BY published_at DESC, added_at DESC;",
            )?;
            let rows = stmt.query_map(params![feed_id.to_string()], map_podcast_episode)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn get_episode(&self, id: &PodcastEpisodeId) -> Result<Option<PodcastEpisode>> {
        self.db.with_conn(|conn| {
            conn.query_row(
                "SELECT id, feed_id, guid, title, description, enclosure_url,
                        published_at, duration_secs, local_path, position_secs,
                        completed, added_at
                   FROM podcast_episodes WHERE id = ?1;",
                params![id.to_string()],
                map_podcast_episode,
            )
            .optional()
        })
    }

    /// Insert or update an episode by `(feed_id, guid)`, preserving the
    /// download and resume state that belongs to the listener.
    pub fn upsert_episode(&self, episode: &PodcastEpisode) -> Result<PodcastEpisode> {
        let id = self
            .db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT id FROM podcast_episodes WHERE feed_id = ?1 AND guid = ?2;",
                    params![episode.feed_id.to_string(), episode.guid],
                    |r| r.get::<_, String>(0),
                )
                .optional()
            })?
            .and_then(|s| s.parse::<PodcastEpisodeId>().ok())
            .unwrap_or(episode.id);

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO podcast_episodes
                    (id, feed_id, guid, title, description, enclosure_url, published_at,
                     duration_secs, local_path, position_secs, completed, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    description = excluded.description,
                    enclosure_url = excluded.enclosure_url,
                    published_at = excluded.published_at,
                    duration_secs = COALESCE(excluded.duration_secs, podcast_episodes.duration_secs);",
                params![
                    id.to_string(),
                    episode.feed_id.to_string(),
                    episode.guid,
                    episode.title,
                    episode.description,
                    episode.enclosure_url,
                    episode.published_at,
                    episode.duration_secs,
                    episode.local_path.as_deref().map(encode_path_key),
                    episode.position_secs,
                    if episode.completed { 1 } else { 0 },
                    episode.added_at,
                ],
            )?;
            Ok(())
        })?;

        Ok(PodcastEpisode {
            id,
            ..episode.clone()
        })
    }

    pub fn set_downloaded(&self, id: &PodcastEpisodeId, path: &Path) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE podcast_episodes SET local_path = ?1, downloaded_at = ?2 WHERE id = ?3;",
                params![encode_path_key(path), now_secs(), id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Persist resume state. Completed episodes store their duration as the
    /// position, so a replayed episode starts at the beginning next time.
    pub fn save_progress(
        &self,
        id: &PodcastEpisodeId,
        position_secs: f32,
        completed: bool,
    ) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE podcast_episodes SET position_secs = ?1, completed = ?2 WHERE id = ?3;",
                params![
                    position_secs.max(0.0),
                    if completed { 1 } else { 0 },
                    id.to_string(),
                ],
            )?;
            Ok(())
        })
    }
}

fn map_podcast_feed(row: &Row<'_>) -> Result<PodcastFeed> {
    Ok(PodcastFeed {
        id: parse_id(&row.get::<_, String>("id")?, "podcast_feeds.id")?,
        url: row.get("url")?,
        title: row.get("title")?,
        description: row.get("description")?,
        artwork_url: row.get("artwork_url")?,
        last_fetched_at: row.get("last_fetched_at")?,
        added_at: row.get("added_at")?,
    })
}

fn map_podcast_episode(row: &Row<'_>) -> Result<PodcastEpisode> {
    let local: Option<String> = row.get("local_path")?;
    Ok(PodcastEpisode {
        id: parse_id(&row.get::<_, String>("id")?, "podcast_episodes.id")?,
        feed_id: parse_id(
            &row.get::<_, String>("feed_id")?,
            "podcast_episodes.feed_id",
        )?,
        guid: row.get("guid")?,
        title: row.get("title")?,
        description: row.get("description")?,
        enclosure_url: row.get("enclosure_url")?,
        published_at: row.get("published_at")?,
        duration_secs: row.get("duration_secs")?,
        local_path: local.map(|p| decode_path_key(&p)),
        position_secs: row.get("position_secs")?,
        completed: row.get::<_, i64>("completed")? != 0,
        added_at: row.get("added_at")?,
    })
}

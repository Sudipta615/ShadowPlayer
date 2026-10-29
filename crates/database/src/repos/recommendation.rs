//! Persistence for the local recommendation engine: cached audio features and
//! the signed feedback events the taste profile learns from.
//!
//! The core recommender is dependency-free and does no I/O. This repository is
//! the host side of that boundary: it stores the 20-dimensional vectors the
//! analyser produces and the feedback the UI observes, and hands them back in
//! the core's own types.

use crate::db::Database;
use domain::ids::TrackId;
use domain::paths::decode_path_key;
use recommendation::{FeatureVector, FeedbackEvent, FeedbackKind};
use rusqlite::{params, Result};
use std::path::PathBuf;

use super::common::parse_id;

/// A lightweight library projection for ranking.
///
/// Ranking needs metadata and statistics, not the whole [`domain::models::Track`]
/// (paths, artwork flags, replay-gain fields). Reading only these columns keeps
/// the catalog build off the large-clone path.
#[derive(Debug, Clone)]
pub struct RecommendationTrackRow {
    pub track_id: TrackId,
    pub artist: String,
    pub album: String,
    pub genre: Option<String>,
    pub duration_secs: f32,
    pub added_at: i64,
    pub is_favorite: bool,
    pub rating: u8,
    pub play_count: u32,
    pub skip_count: u32,
    pub last_played_at: Option<i64>,
    pub features: Option<FeatureVector>,
}

/// One file the analysis worker still has to decode.
#[derive(Debug, Clone)]
pub struct RecommendationAnalysisTarget {
    pub track_id: TrackId,
    pub path: PathBuf,
    pub duration_secs: f32,
}

/// Cached feature vectors and recommendation feedback.
pub struct RecommendationRepo {
    db: Database,
}

impl RecommendationRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Store or replace one track's feature vector.
    pub fn put_feature(
        &self,
        track_id: &TrackId,
        features: &FeatureVector,
        schema_version: u32,
    ) -> Result<()> {
        let bytes = features.to_bytes();
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO track_features
                    (track_id, schema_version, features, analyzed_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(track_id) DO UPDATE SET
                    schema_version = excluded.schema_version,
                    features = excluded.features,
                    analyzed_at = excluded.analyzed_at;",
                params![
                    track_id.to_string(),
                    i64::from(schema_version),
                    bytes.as_slice(),
                    now_secs(),
                ],
            )?;
            Ok(())
        })
    }

    /// Load one track's feature vector when it was written by `schema_version`.
    pub fn get_feature(
        &self,
        track_id: &TrackId,
        schema_version: u32,
    ) -> Result<Option<FeatureVector>> {
        self.db.with_conn(|conn| {
            let row: Option<(Vec<u8>, i64)> = conn
                .query_row(
                    "SELECT features, schema_version FROM track_features WHERE track_id = ?1;",
                    params![track_id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            Ok(row.and_then(|(bytes, stored)| decode_feature(&bytes, stored, schema_version)))
        })
    }

    /// Remove one cached vector (used when a file changes).
    pub fn delete_feature(&self, track_id: &TrackId) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM track_features WHERE track_id = ?1;",
                params![track_id.to_string()],
            )?;
            Ok(())
        })
    }

    /// Drop every cached vector, forcing the next analysis run to re-read the
    /// files. Used by "Analyze all" when the schema changed.
    pub fn clear_features(&self) -> Result<()> {
        self.db
            .with_conn(|conn| conn.execute("DELETE FROM track_features;", []).map(|_| ()))
    }

    /// Number of tracks in the library.
    pub fn track_count(&self) -> Result<usize> {
        self.db.with_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM tracks;", [], |row| row.get(0))?;
            Ok(count.max(0) as usize)
        })
    }

    /// Number of vectors written by the current schema.
    pub fn feature_count(&self, schema_version: u32) -> Result<usize> {
        self.db.with_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM track_features WHERE schema_version = ?1;",
                params![i64::from(schema_version)],
                |row| row.get(0),
            )?;
            Ok(count.max(0) as usize)
        })
    }

    /// Every track with its cached vector, for building a ranking catalog.
    pub fn list_catalog_tracks(&self, schema_version: u32) -> Result<Vec<RecommendationTrackRow>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.artist, t.album, t.genre, t.duration_secs, t.added_at,
                        t.is_favorite, t.rating, t.play_count, t.skip_count, t.last_played_at,
                        f.features, f.schema_version
                   FROM tracks t
                   LEFT JOIN track_features f
                     ON f.track_id = t.id AND f.schema_version = ?1
                  ORDER BY t.artist COLLATE NOCASE, t.album COLLATE NOCASE,
                           t.disc_number, t.track_number, t.title COLLATE NOCASE;",
            )?;
            let rows = stmt.query_map(params![i64::from(schema_version)], |row| {
                let id: String = row.get(0)?;
                let features: Option<Vec<u8>> = row.get(11)?;
                let stored_schema: Option<i64> = row.get(12)?;
                Ok(RecommendationTrackRow {
                    track_id: parse_id(&id, "tracks.id")?,
                    artist: row.get(1)?,
                    album: row.get(2)?,
                    genre: row.get(3)?,
                    duration_secs: row.get(4)?,
                    added_at: row.get(5)?,
                    is_favorite: row.get::<_, i64>(6)? != 0,
                    rating: row.get::<_, i64>(7)?.clamp(0, 5) as u8,
                    play_count: row.get::<_, i64>(8)?.max(0) as u32,
                    skip_count: row.get::<_, i64>(9)?.max(0) as u32,
                    last_played_at: row.get(10)?,
                    features: features.and_then(|bytes| {
                        stored_schema
                            .and_then(|schema| decode_feature(&bytes, schema, schema_version))
                    }),
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Files that do not have a current-schema vector yet.
    pub fn list_tracks_missing_features(
        &self,
        schema_version: u32,
    ) -> Result<Vec<RecommendationAnalysisTarget>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.path, t.duration_secs
                   FROM tracks t
                   LEFT JOIN track_features f
                     ON f.track_id = t.id AND f.schema_version = ?1
                  WHERE f.track_id IS NULL
                  ORDER BY t.artist COLLATE NOCASE, t.album COLLATE NOCASE,
                           t.disc_number, t.track_number, t.title COLLATE NOCASE;",
            )?;
            let rows = stmt.query_map(params![i64::from(schema_version)], |row| {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                Ok(RecommendationAnalysisTarget {
                    track_id: parse_id(&id, "tracks.id")?,
                    path: decode_path_key(&path),
                    duration_secs: row.get(2)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// Append one feedback event.
    pub fn record_feedback(&self, event: &FeedbackEvent) -> Result<()> {
        let (kind, value) = encode_feedback_kind(event.kind);
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO recommendation_feedback
                    (track_id, kind, value, timestamp_ms, session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5);",
                params![
                    TrackId::from_u128(event.track_id).to_string(),
                    kind,
                    value,
                    event.timestamp_ms,
                    event.session_id.map(|id| id as i64),
                ],
            )?;
            Ok(())
        })
    }

    /// Feedback history, oldest first so the profile can replay it in order.
    pub fn list_feedback(&self, limit: usize) -> Result<Vec<FeedbackEvent>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT track_id, kind, value, timestamp_ms, session_id
                   FROM recommendation_feedback
                  ORDER BY timestamp_ms ASC, id ASC
                  LIMIT ?1;",
            )?;
            let rows = stmt.query_map(params![limit.max(1) as i64], |row| {
                let id: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let value: Option<f64> = row.get(2)?;
                let timestamp_ms: i64 = row.get(3)?;
                let session: Option<i64> = row.get(4)?;
                // The row stores the canonical UUID text; the core speaks
                // `u128`. Parse as a `TrackId` first so the text is validated
                // by the same code every other repository uses.
                let track_id: TrackId = parse_id(&id, "recommendation_feedback.track_id")?;
                Ok((track_id.as_u128(), kind, value, timestamp_ms, session))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (track_id, kind, value, timestamp_ms, session) = row?;
                // Unknown kinds from a newer build are ignored rather than
                // failing the whole history load.
                let Some(kind) = decode_feedback_kind(&kind, value.map(|v| v as f32)) else {
                    continue;
                };
                out.push(FeedbackEvent {
                    track_id,
                    timestamp_ms,
                    session_id: session.and_then(|id| u64::try_from(id).ok()),
                    kind,
                });
            }
            Ok(out)
        })
    }

    pub fn feedback_count(&self) -> Result<usize> {
        self.db.with_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM recommendation_feedback;", [], |row| {
                    row.get(0)
                })?;
            Ok(count.max(0) as usize)
        })
    }

    pub fn clear_feedback(&self) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute("DELETE FROM recommendation_feedback;", [])
                .map(|_| ())
        })
    }
}

fn decode_feature(bytes: &[u8], stored_schema: i64, wanted_schema: u32) -> Option<FeatureVector> {
    if stored_schema != i64::from(wanted_schema) {
        return None;
    }
    match FeatureVector::from_bytes(bytes) {
        Ok(features) => Some(features),
        Err(error) => {
            log::warn!("Ignoring corrupt recommendation feature vector: {error}");
            None
        }
    }
}

fn encode_feedback_kind(kind: FeedbackKind) -> (&'static str, Option<f32>) {
    match kind {
        FeedbackKind::PlayStarted => ("play_started", None),
        FeedbackKind::Progress { listened_fraction } => {
            ("progress", Some(listened_fraction.clamp(0.0, 1.0)))
        }
        FeedbackKind::Completed => ("completed", None),
        FeedbackKind::Skipped { listened_fraction } => {
            ("skipped", Some(listened_fraction.clamp(0.0, 1.0)))
        }
        FeedbackKind::Liked => ("liked", None),
        FeedbackKind::Disliked => ("disliked", None),
        FeedbackKind::Rated { rating } => ("rated", Some(rating.clamp(0.0, 5.0))),
        FeedbackKind::Replay => ("replay", None),
        FeedbackKind::AddedToPlaylist => ("added_to_playlist", None),
    }
}

/// The database returns the kind as a string. `None` means "written by a newer
/// build"; the caller skips it instead of inventing a signal.
fn decode_feedback_kind(kind: &str, value: Option<f32>) -> Option<FeedbackKind> {
    let value = value.unwrap_or(0.0);
    Some(match kind {
        "play_started" => FeedbackKind::PlayStarted,
        "progress" => FeedbackKind::Progress {
            listened_fraction: value,
        },
        "completed" => FeedbackKind::Completed,
        "skipped" => FeedbackKind::Skipped {
            listened_fraction: value,
        },
        "liked" => FeedbackKind::Liked,
        "disliked" => FeedbackKind::Disliked,
        "rated" => FeedbackKind::Rated { rating: value },
        "replay" => FeedbackKind::Replay,
        "added_to_playlist" => FeedbackKind::AddedToPlaylist,
        other => {
            log::debug!("Ignoring unknown recommendation feedback kind {other:?}");
            return None;
        }
    })
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use recommendation::FEATURE_SCHEMA_VERSION;

    fn db() -> Database {
        Database::in_memory().expect("in-memory database")
    }

    fn insert_track(db: &Database, id: TrackId) {
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tracks
                    (id, path, title, artist, album, duration_secs, sample_rate, channels,
                     format, is_lossless, file_size_bytes, mtime_unix_secs, added_at)
                 VALUES (?1, ?2, 'Title', 'Artist', 'Album', 180.0, 44100, 2,
                         'FLAC', 1, 0, 0, 0);",
                params![id.to_string(), format!("/music/{id}.flac")],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn features_round_trip_and_are_schema_scoped() {
        let db = db();
        let repo = RecommendationRepo::new(db.clone());
        let id = TrackId::new();
        insert_track(&db, id);
        let mut values = [0.0_f32; recommendation::FEATURE_COUNT];
        values[0] = 0.25;
        values[8] = 0.75;
        let features = FeatureVector::new(values, (1_u32 << recommendation::FEATURE_COUNT) - 1);
        repo.put_feature(&id, &features, FEATURE_SCHEMA_VERSION)
            .unwrap();
        assert_eq!(
            repo.get_feature(&id, FEATURE_SCHEMA_VERSION).unwrap(),
            Some(features)
        );
        assert_eq!(
            repo.get_feature(&id, FEATURE_SCHEMA_VERSION + 1).unwrap(),
            None
        );
        assert_eq!(repo.feature_count(FEATURE_SCHEMA_VERSION).unwrap(), 1);
    }

    #[test]
    fn feedback_round_trips_including_variant_payloads() {
        let db = db();
        let repo = RecommendationRepo::new(db.clone());
        let id = TrackId::new();
        insert_track(&db, id);
        for (index, kind) in [
            FeedbackKind::PlayStarted,
            FeedbackKind::Progress {
                listened_fraction: 0.4,
            },
            FeedbackKind::Completed,
            FeedbackKind::Skipped {
                listened_fraction: 0.2,
            },
            FeedbackKind::Liked,
            FeedbackKind::Rated { rating: 4.0 },
            FeedbackKind::AddedToPlaylist,
        ]
        .into_iter()
        .enumerate()
        {
            repo.record_feedback(&FeedbackEvent {
                track_id: id.as_u128(),
                timestamp_ms: 1_000 + index as i64,
                session_id: Some(7),
                kind,
            })
            .unwrap();
        }
        let events = repo.list_feedback(100).unwrap();
        assert_eq!(events.len(), 7);
        assert!(matches!(events[1].kind, FeedbackKind::Progress { .. }));
        assert!(matches!(events[5].kind, FeedbackKind::Rated { rating } if rating == 4.0));
        assert_eq!(repo.feedback_count().unwrap(), 7);
    }
}

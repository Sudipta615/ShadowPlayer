//! Background ranking for the local "For You" feed.
//!
//! The recommendation core is synchronous and does no I/O. This service is the
//! host boundary: a single worker owns the catalog projection, the rolling
//! [`RecommendationSession`], and the repository handle, so ranking a large
//! library never blocks the frame loop. Commands and results travel over
//! bounded channels; the UI only polls.

use crossbeam::channel::{bounded, Receiver, Sender};
use database::{Database, RecommendationRepo, RecommendationTrackRow};
use domain::ids::TrackId;
use recommendation::{
    FeedbackEvent, FeedbackKind, RecommendationSession, RecommendationSessionConfig, Track,
    TrackMetadata, TrackStats, FEATURE_SCHEMA_VERSION,
};

/// How many persisted feedback events are replayed into a new session.
///
/// The profile decays old evidence anyway; this cap keeps session creation
/// bounded even after years of listening.
const FEEDBACK_HISTORY_LIMIT: usize = 50_000;

/// The rolling list, as the UI needs it.
#[derive(Debug, Clone, Default)]
pub struct RecommendationSnapshot {
    /// Unplayed recommendations, in ranking order.
    pub ids: Vec<TrackId>,
    /// Number of unplayed recommendations.
    pub pending: usize,
    /// Tracks with a cached feature vector for the current schema.
    pub analyzed: usize,
    /// Tracks in the library.
    pub total: usize,
    /// Explicit feedback events stored in the database.
    pub feedback: usize,
}

/// Library-wide recommendation counters for the settings card.
#[derive(Debug, Clone, Default)]
pub struct RecommendationStats {
    pub analyzed: usize,
    pub total: usize,
    pub feedback: usize,
}

/// Events the worker sends back to the UI thread.
#[derive(Debug, Clone)]
pub enum RecommendationEvent {
    /// A new rolling list (or an empty one after a reset).
    Ready(RecommendationSnapshot),
    /// Counters changed without a new list.
    Stats(RecommendationStats),
    /// A ranking, persistence, or catalog error.
    Error(String),
}

enum Command {
    Start,
    Feedback(FeedbackEvent),
    SetBatchSize(usize),
    ReloadCatalog,
    RefreshStats,
    Reset,
    ClearHistory,
}

/// Owns the recommendation worker. Cheap to hold; the worker lives until the
/// service is dropped.
pub struct RecommendationService {
    tx: Sender<Command>,
    rx: Receiver<RecommendationEvent>,
    batch_size: usize,
}

impl RecommendationService {
    pub fn new(db: Database) -> Self {
        let (tx, command_rx) = bounded(64);
        let (event_tx, rx) = bounded(64);
        std::thread::Builder::new()
            .name("shadowplayer-recommend".into())
            .spawn(move || {
                let mut worker = Worker::new(db);
                for command in command_rx {
                    worker.handle(command, &event_tx);
                }
            })
            .expect("spawn recommendation worker");
        Self {
            tx,
            rx,
            batch_size: RecommendationSessionConfig::default().batch_size,
        }
    }

    /// Request the first batch. Repeated calls are harmless.
    pub fn start(&self) {
        self.send(Command::Start);
    }

    /// Persist one feedback event and let the worker update the rolling list.
    pub fn feedback(&self, event: FeedbackEvent) {
        self.send(Command::Feedback(event));
    }

    /// Change how many tracks each new batch adds.
    pub fn set_batch_size(&mut self, batch_size: usize) {
        let batch_size = batch_size.clamp(1, 100);
        if self.batch_size == batch_size {
            return;
        }
        self.batch_size = batch_size;
        self.send(Command::SetBatchSize(batch_size));
    }

    /// Re-read the catalog after analysis finished or the library changed.
    pub fn reload_catalog(&self) {
        self.send(Command::ReloadCatalog);
    }

    /// Ask for fresh analyzed/total/feedback counters.
    pub fn refresh_stats(&self) {
        self.send(Command::RefreshStats);
    }

    /// Drop the in-memory session but keep the learned feedback.
    pub fn reset(&self) {
        self.send(Command::Reset);
    }

    /// Delete all persisted feedback and drop the in-memory session.
    pub fn clear_history(&self) {
        self.send(Command::ClearHistory);
    }

    /// Non-blocking poll for worker events.
    pub fn poll(&self) -> Option<RecommendationEvent> {
        self.rx.try_recv().ok()
    }

    fn send(&self, command: Command) {
        if let Err(error) = self.tx.send(command) {
            log::warn!("Recommendation worker is not running: {error}");
        }
    }
}

struct Worker {
    repo: RecommendationRepo,
    catalog: Vec<Track>,
    session: Option<RecommendationSession>,
    batch_size: usize,
}

impl Worker {
    fn new(db: Database) -> Self {
        Self {
            repo: RecommendationRepo::new(db),
            catalog: Vec::new(),
            session: None,
            batch_size: RecommendationSessionConfig::default().batch_size,
        }
    }

    fn handle(&mut self, command: Command, tx: &Sender<RecommendationEvent>) {
        match command {
            Command::Start => self.start(tx),
            Command::Feedback(event) => self.feedback(event, tx),
            Command::SetBatchSize(batch_size) => {
                self.batch_size = batch_size.max(1);
                if let Some(session) = &mut self.session {
                    session.set_batch_size(self.batch_size);
                }
            }
            Command::ReloadCatalog => self.reload_catalog(tx),
            Command::RefreshStats => {
                let _ = tx.send(RecommendationEvent::Stats(self.stats()));
            }
            Command::Reset => {
                self.session = None;
                self.catalog.clear();
                let _ = tx.send(RecommendationEvent::Ready(self.stats().into()));
            }
            Command::ClearHistory => {
                if let Err(error) = self.repo.clear_feedback() {
                    log::warn!("Could not clear recommendation feedback: {error}");
                    let _ = tx.send(RecommendationEvent::Error(error.to_string()));
                }
                self.session = None;
                self.catalog.clear();
                let _ = tx.send(RecommendationEvent::Ready(self.stats().into()));
            }
        }
    }

    fn start(&mut self, tx: &Sender<RecommendationEvent>) {
        if self.session.is_some() {
            self.emit_ready(tx);
            return;
        }
        self.catalog = match self.load_catalog() {
            Ok(catalog) => catalog,
            Err(error) => {
                let _ = tx.send(RecommendationEvent::Error(error));
                return;
            }
        };
        let history = match self.repo.list_feedback(FEEDBACK_HISTORY_LIMIT) {
            Ok(history) => history,
            Err(error) => {
                log::warn!("Could not load recommendation feedback: {error}");
                Vec::new()
            }
        };
        let now = now_ms();
        let mut session = RecommendationSession::new(
            &self.catalog,
            history,
            now,
            RecommendationSessionConfig {
                batch_size: self.batch_size,
                ..RecommendationSessionConfig::default()
            },
        );
        session.start(&self.catalog, now);
        self.session = Some(session);
        self.emit_ready(tx);
    }

    fn feedback(&mut self, event: FeedbackEvent, tx: &Sender<RecommendationEvent>) {
        if let Err(error) = self.repo.record_feedback(&event) {
            log::warn!("Could not persist recommendation feedback: {error}");
            let _ = tx.send(RecommendationEvent::Error(error.to_string()));
        }
        let Some(session) = &mut self.session else {
            // Feedback is still persisted before the first feed starts, so the
            // settings counters should reflect it even with no session.
            let _ = tx.send(RecommendationEvent::Stats(self.stats()));
            return;
        };

        let explicit = matches!(
            event.kind,
            FeedbackKind::Liked
                | FeedbackKind::Disliked
                | FeedbackKind::Rated { .. }
                | FeedbackKind::AddedToPlaylist
        );
        session.record_feedback(event);

        let now = now_ms();
        // Explicit signals should reorder what is still on screen; play edges
        // only need a refill once the pending list gets short.
        if explicit {
            session.rerank_pending(&self.catalog, now);
        }
        if session.pending_count() < refill_threshold(self.batch_size) {
            session.refill(&self.catalog, now);
        }
        self.emit_ready(tx);
    }

    fn reload_catalog(&mut self, tx: &Sender<RecommendationEvent>) {
        // No session means no ranking to refresh; `Start` loads the catalog
        // itself. This keeps library reloads cheap until the user opens the
        // feed.
        if self.session.is_none() {
            let _ = tx.send(RecommendationEvent::Stats(self.stats()));
            return;
        }
        self.catalog = match self.load_catalog() {
            Ok(catalog) => catalog,
            Err(error) => {
                let _ = tx.send(RecommendationEvent::Error(error));
                return;
            }
        };
        let Some(session) = &mut self.session else {
            let _ = tx.send(RecommendationEvent::Stats(self.stats()));
            return;
        };
        let now = now_ms();
        session.rerank_pending(&self.catalog, now);
        if session.pending_count() < refill_threshold(self.batch_size) {
            session.refill(&self.catalog, now);
        }
        self.emit_ready(tx);
    }

    fn emit_ready(&self, tx: &Sender<RecommendationEvent>) {
        let ids: Vec<TrackId> = self
            .session
            .as_ref()
            .map(|session| {
                session
                    .pending_items()
                    .map(|item| TrackId::from_u128(item.track_id))
                    .collect()
            })
            .unwrap_or_default();
        let pending = ids.len();
        let stats = self.stats();
        let _ = tx.send(RecommendationEvent::Ready(RecommendationSnapshot {
            ids,
            pending,
            analyzed: stats.analyzed,
            total: stats.total,
            feedback: stats.feedback,
        }));
    }

    fn stats(&self) -> RecommendationStats {
        RecommendationStats {
            analyzed: self
                .repo
                .feature_count(FEATURE_SCHEMA_VERSION)
                .unwrap_or_default(),
            total: self.repo.track_count().unwrap_or_default(),
            feedback: self.repo.feedback_count().unwrap_or_default(),
        }
    }

    fn load_catalog(&self) -> Result<Vec<Track>, String> {
        self.repo
            .list_catalog_tracks(FEATURE_SCHEMA_VERSION)
            .map(|rows| rows.into_iter().map(to_core_track).collect())
            .map_err(|error| error.to_string())
    }
}

impl From<RecommendationStats> for RecommendationSnapshot {
    fn from(stats: RecommendationStats) -> Self {
        Self {
            ids: Vec::new(),
            pending: 0,
            analyzed: stats.analyzed,
            total: stats.total,
            feedback: stats.feedback,
        }
    }
}

/// Refill before the list is empty, so the user never sees a gap.
fn refill_threshold(batch_size: usize) -> usize {
    (batch_size / 3).max(2)
}

/// Project one database row into the dependency-free core's track shape.
fn to_core_track(row: RecommendationTrackRow) -> Track {
    let artist = row.artist.trim();
    let album = row.album.trim();
    let artist_id = (!artist.is_empty()).then(|| u128::from(stable_key(artist)));
    let album_id = (!artist.is_empty() && !album.is_empty())
        .then(|| stable_key(&format!("{artist}\u{1f}{album}")));
    Track {
        id: row.track_id.as_u128(),
        features: row.features,
        metadata: TrackMetadata {
            artist_id,
            album_id,
            genre_ids: row
                .genre
                .as_deref()
                .and_then(recommendation::genre_key)
                .into_iter()
                .collect(),
            duration_seconds: Some(row.duration_secs),
            added_at_ms: Some(row.added_at.saturating_mul(1_000)),
        },
        stats: TrackStats {
            play_count: row.play_count,
            skip_count: row.skip_count,
            completion_count: 0,
            like_count: u32::from(row.is_favorite),
            dislike_count: 0,
            rating: (row.rating > 0).then(|| f32::from(row.rating)),
            last_played_ms: row
                .last_played_at
                .map(|seconds| seconds.saturating_mul(1_000)),
            last_skipped_ms: None,
        },
    }
}

/// Stable FNV-1a key for a text identity the library does not expose as a UUID.
///
/// The catalog projection deliberately selects only ranking columns. Artist and
/// album affinity need a stable id, not a foreign key, so the label is hashed.
/// A hash collision merges two labels, which can only make the affinity signal
/// slightly broader; it never corrupts stored data.
fn stable_key(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use recommendation::{FeatureVector, FEATURE_COUNT};

    fn row() -> RecommendationTrackRow {
        RecommendationTrackRow {
            track_id: TrackId::new(),
            artist: " Boards of Canada ".to_string(),
            album: "Music Has the Right to Children".to_string(),
            genre: Some("Electronic".to_string()),
            duration_secs: 241.0,
            added_at: 1_700_000_000,
            is_favorite: true,
            rating: 4,
            play_count: 7,
            skip_count: 1,
            last_played_at: Some(1_700_000_100),
            features: Some(FeatureVector::new(
                [0.5; FEATURE_COUNT],
                (1_u32 << FEATURE_COUNT) - 1,
            )),
        }
    }

    #[test]
    fn the_projection_keeps_metadata_and_stats() {
        let row = row();
        let id = row.track_id;
        let track = to_core_track(row);
        assert_eq!(track.id, id.as_u128());
        assert!(track.features.is_some());
        assert_eq!(track.metadata.duration_seconds, Some(241.0));
        assert_eq!(track.metadata.genre_ids.len(), 1);
        assert_eq!(
            track.metadata.artist_id,
            Some(u128::from(stable_key("Boards of Canada")))
        );
        assert!(track.metadata.album_id.is_some());
        assert_eq!(track.stats.play_count, 7);
        assert_eq!(track.stats.skip_count, 1);
        assert_eq!(track.stats.like_count, 1);
        assert_eq!(track.stats.rating, Some(4.0));
        assert_eq!(track.stats.last_played_ms, Some(1_700_000_100_000));
    }

    #[test]
    fn blank_artists_do_not_create_affinity_ids() {
        let mut row = row();
        row.artist = "   ".to_string();
        row.album.clear();
        let track = to_core_track(row);
        assert_eq!(track.metadata.artist_id, None);
        assert_eq!(track.metadata.album_id, None);
    }

    #[test]
    fn the_refill_threshold_leaves_a_visible_buffer() {
        assert_eq!(refill_threshold(12), 4);
        assert_eq!(refill_threshold(3), 2);
        assert_eq!(refill_threshold(1), 2);
    }
}

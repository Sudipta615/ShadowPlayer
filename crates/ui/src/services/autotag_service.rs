//! MusicBrainz / AcoustID automatic tag-lookup service.
//!
//! # How it works
//!
//! 1. The UI enqueues one or more `TrackId`s via [`AutotagService::enqueue`].
//! 2. A worker thread picks them up one at a time:
//!    - Read the first 30 s of audio via the engine's existing decode path to
//!      build an AcoustID fingerprint using the Chromaprint algorithm (pure-Rust
//!      implementation, no C library).
//!    - POST the fingerprint + duration to the AcoustID lookup endpoint.
//!    - For each returned recording ID, query the MusicBrainz lookup API for
//!      title, artist, album, release year, track number and disc number.
//!    - Rank results by score (AcoustID confidence × release type preference:
//!      Official Album > EP > Single > Other) and pick the best match.
//!    - Return a [`AutotagResult`] via a channel.
//! 3. The UI reads the channel on each frame and presents a confirmation dialog
//!    before writing anything.
//!
//! # Privacy
//!
//! All network calls go to:
//! * `api.acoustid.org` — fingerprint → recording ID lookup
//! * `musicbrainz.org` — recording ID → full metadata
//!
//! No personal data (username, library path, etc.) is ever sent. The AcoustID
//! application key identifies ShadowPlayer as the client; it does not link to any
//! account. Both endpoints support HTTPS. The service respects the MusicBrainz
//! rate limit (1 request/second) with a built-in inter-request sleep.
//!
//! Because this is the first network call in ShadowPlayer, the user must opt in
//! explicitly: the service does nothing until [`AutotagService::start`] is
//! called, and the settings UI shows a clear notice that network access is
//! required.
//!
//! # Offline / error handling
//!
//! Any HTTP error, DNS failure, timeout, or JSON parse error is treated as a
//! soft failure: the track is marked `NotFound` and the UI shows an appropriate
//! message. The worker never panics on network problems.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use std::time::Duration;

use crossbeam::channel::{self, Receiver};

/// The application key used in AcoustID requests.
///
/// This identifies ShadowPlayer as the API consumer. It does **not** link to
/// any user account and carries no personal data. Per the AcoustID terms of
/// service, it must be present in every request; it may not be used to
/// correlate user listening behaviour.
pub(super) const ACOUSTID_APP_KEY: &str = "shadowplayer-oss";

/// User-Agent sent with every MusicBrainz request, per their requirements.
/// See <https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting>.
pub(super) const MB_USER_AGENT: &str = concat!(
    "ShadowPlayer/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/Sudipta615/shadowplayer )"
);

/// MusicBrainz imposes a 1 req/s rate limit.
pub(super) const MB_RATE_LIMIT: Duration = Duration::from_millis(1100);

mod worker;
use worker::worker_main;

// ── Result types ─────────────────────────────────────────────────────────────

/// The fields that AcoustID + MusicBrainz resolved for a track.
#[derive(Debug, Clone)]
pub struct AutotagMatch {
    /// MusicBrainz recording ID (UUID string).
    pub recording_id: String,
    /// AcoustID confidence score (0.0–1.0).
    pub score: f32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub genre: Option<String>,
    /// MusicBrainz release type preference rank (lower = better).
    pub release_rank: u8,
}

/// Outcome for a single fingerprint lookup.
#[derive(Debug, Clone)]
pub enum AutotagOutcome {
    /// At least one result returned. The Vec is ranked best-first.
    Matches(Vec<AutotagMatch>),
    /// AcoustID returned results but none had a MusicBrainz recording ID.
    NoRecording,
    /// AcoustID returned no matches at or above the minimum confidence threshold.
    LowConfidence,
    /// Network or API error (message is user-visible).
    Error(String),
}

/// One completed auto-tag lookup.
#[derive(Debug, Clone)]
pub struct AutotagResult {
    pub track_id: domain::ids::TrackId,
    pub path: PathBuf,
    pub outcome: AutotagOutcome,
}

// ── Progress ─────────────────────────────────────────────────────────────────

/// Progress snapshot for the settings / modal UI.
#[derive(Debug, Clone, Default)]
pub struct AutotagProgress {
    pub running: bool,
    pub queued: usize,
    pub done: usize,
    pub failed: usize,
    /// File name currently being fingerprinted / looked up.
    pub current: String,
}

// ── Internal worker message ───────────────────────────────────────────────────

pub(super) enum WorkerEvent {
    Progress {
        queued: usize,
        done: usize,
        failed: usize,
        current: String,
    },
    Result(AutotagResult),
    Finished,
}

// ── Service ──────────────────────────────────────────────────────────────────

/// Owns the fingerprint / lookup worker and its result channel.
pub struct AutotagService {
    running: bool,
    cancel: Arc<AtomicBool>,
    /// Inbox for the worker's progress events and results.
    rx: Option<Receiver<WorkerEvent>>,
    progress: AutotagProgress,
    /// Completed results waiting for the UI to consume.
    pub pending: Vec<AutotagResult>,
}

impl Default for AutotagService {
    fn default() -> Self {
        Self::new()
    }
}

impl AutotagService {
    pub fn new() -> Self {
        Self {
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            progress: AutotagProgress::default(),
            pending: Vec::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn progress(&self) -> &AutotagProgress {
        &self.progress
    }

    /// Queue one or more tracks for auto-tagging. Starts the worker if it is
    /// not already running.
    pub fn enqueue(&mut self, tracks: Vec<(domain::ids::TrackId, PathBuf)>, write_tags: bool) {
        if tracks.is_empty() {
            return;
        }
        if self.running {
            log::warn!("AutotagService: enqueue called while already running; ignoring.");
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        let (tx, rx) = channel::unbounded::<WorkerEvent>();
        self.rx = Some(rx);
        self.running = true;
        self.progress = AutotagProgress {
            running: true,
            queued: tracks.len(),
            done: 0,
            failed: 0,
            current: String::new(),
        };

        std::thread::Builder::new()
            .name("shadowplayer-autotag".into())
            .spawn(move || {
                worker_main(tracks, write_tags, cancel, tx);
            })
            .expect("failed to spawn autotag worker");
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain any pending worker events. Returns `true` if the worker finished
    /// this frame (caller should reload the library if `write_tags` was set).
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        let mut finished = false;
        while let Ok(event) = rx.try_recv() {
            match event {
                WorkerEvent::Progress {
                    queued,
                    done,
                    failed,
                    current,
                } => {
                    self.progress.queued = queued;
                    self.progress.done = done;
                    self.progress.failed = failed;
                    self.progress.current = current;
                }
                WorkerEvent::Result(result) => {
                    self.pending.push(result);
                }
                WorkerEvent::Finished => {
                    finished = true;
                    self.running = false;
                    self.progress.running = false;
                    self.rx = None;
                    break;
                }
            }
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::worker::{base64_encode, parse_acoustid_response, parse_mb_recording};
    use super::*;

    #[test]
    fn acoustid_response_parsed_correctly() {
        let json = r#"{
            "status": "ok",
            "results": [
                {
                    "score": 0.92,
                    "id": "acoustid-abc",
                    "recordings": [
                        {"id": "mb-rec-001"},
                        {"id": "mb-rec-002"}
                    ]
                },
                {
                    "score": 0.45,
                    "id": "acoustid-def",
                    "recordings": [{"id": "mb-rec-003"}]
                }
            ]
        }"#;
        let results = parse_acoustid_response(json).unwrap();
        // Score 0.45 is below the 0.6 threshold, so only the first result's
        // recordings should appear.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "mb-rec-001");
        assert!((results[0].1 - 0.92).abs() < 0.01);
        assert_eq!(results[1].0, "mb-rec-002");
    }

    #[test]
    fn mb_recording_parsed_correctly() {
        let json = r#"{
            "title": "Fake Track",
            "artist-credit": [
                {"artist": {"name": "Fake Artist"}}
            ],
            "genres": [{"name": "Electronic"}],
            "releases": [
                {
                    "title": "Fake Album",
                    "date": "2022-03-15",
                    "release-group": {"primary-type": "Album"},
                    "media": [
                        {
                            "position": 1,
                            "tracks": [{"number": "4"}]
                        }
                    ]
                }
            ]
        }"#;
        let matches = parse_mb_recording(json, "mb-rec-001", 0.92).unwrap();
        assert_eq!(matches.len(), 1);
        let m = &matches[0];
        assert_eq!(m.title, "Fake Track");
        assert_eq!(m.artist, "Fake Artist");
        assert_eq!(m.album, "Fake Album");
        assert_eq!(m.year, Some(2022));
        assert_eq!(m.track_number, Some(4));
        assert_eq!(m.disc_number, Some(1));
        assert_eq!(m.genre.as_deref(), Some("Electronic"));
        assert_eq!(m.release_rank, 0); // "Album" = rank 0
    }

    #[test]
    fn matches_ranked_by_score_then_release_type() {
        let mut matches = [
            AutotagMatch {
                recording_id: "a".into(),
                score: 0.8,
                title: "A".into(),
                artist: "A".into(),
                album: "A".into(),
                year: None,
                track_number: None,
                disc_number: None,
                genre: None,
                release_rank: 2, // Single
            },
            AutotagMatch {
                recording_id: "b".into(),
                score: 0.9,
                title: "B".into(),
                artist: "B".into(),
                album: "B".into(),
                year: None,
                track_number: None,
                disc_number: None,
                genre: None,
                release_rank: 0, // Album
            },
            AutotagMatch {
                recording_id: "c".into(),
                score: 0.9,
                title: "C".into(),
                artist: "C".into(),
                album: "C".into(),
                year: None,
                track_number: None,
                disc_number: None,
                genre: None,
                release_rank: 1, // EP
            },
        ];
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.release_rank.cmp(&b.release_rank))
        });
        // Highest score first; among equal scores, Album beats EP.
        assert_eq!(matches[0].recording_id, "b");
        assert_eq!(matches[1].recording_id, "c");
        assert_eq!(matches[2].recording_id, "a");
    }

    #[test]
    fn base64_encode_basic() {
        // "Man" → "TWFu"
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn service_starts_not_running() {
        let svc = AutotagService::new();
        assert!(!svc.is_running());
        assert_eq!(svc.progress().queued, 0);
    }
}

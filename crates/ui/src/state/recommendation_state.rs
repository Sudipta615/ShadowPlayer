//! UI state for the local, on-device "For You" feed.
//!
//! The state is deliberately small: the rolling list itself lives in
//! [`crate::state::LibraryState`] as an ordered id projection, while this
//! module owns the worker status, analysis progress, and the pure playback
//! tracker that turns transport observations into recommendation feedback.

use domain::ids::TrackId;
use recommendation::{FeedbackEvent, FeedbackKind};
use serde::{Deserialize, Serialize};

/// How much of each file the background analyser samples.
///
/// The recommendation core exposes these as [`recommendation::AnalysisProfile`];
/// this mirror exists because settings are persisted with serde and the core
/// crate is intentionally free of serialization dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AnalysisQuality {
    /// Cheapest: 8 kHz, short windows. Good for large libraries on modest CPUs.
    LowResource,
    /// The default balance between indexing time and feature stability.
    #[default]
    Balanced,
    /// More audio and a larger FFT. Slowest, but the most stable descriptors.
    HighQuality,
}

impl AnalysisQuality {
    pub const ALL: [Self; 3] = [Self::LowResource, Self::Balanced, Self::HighQuality];

    pub fn profile(self) -> recommendation::AnalysisProfile {
        match self {
            Self::LowResource => recommendation::AnalysisProfile::LowResource,
            Self::Balanced => recommendation::AnalysisProfile::Balanced,
            Self::HighQuality => recommendation::AnalysisProfile::HighQuality,
        }
    }
}

/// A user action that should be fed back to the recommendation engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecommendationFeedback {
    Liked(TrackId),
    Disliked(TrackId),
    Rated(TrackId, u8),
    AddedToPlaylist(TrackId),
}

/// Status and pure reducers for the recommendation feature.
#[derive(Debug, Clone, Default)]
pub struct RecommendationState {
    /// Whether the first `For You` batch has been requested.
    pub started: bool,
    /// A ranking/refill request is in flight.
    pub loading: bool,
    /// Last worker error, ready for a toast or the settings card.
    pub error: Option<String>,
    /// Tracks with a cached feature vector for the current schema.
    pub analyzed: usize,
    /// Tracks in the library.
    pub total: usize,
    /// Explicit feedback events stored in the database.
    pub feedback_count: usize,

    /// A library-wide analysis worker is running.
    pub analysis_running: bool,
    pub analysis_done: usize,
    pub analysis_total: usize,
    pub analysis_failed: usize,
    /// File currently being analysed, or the last status line.
    pub analysis_current: String,
    /// Whether an analysis run has completed at least once.
    pub analysis_has_run: bool,

    playback: PlaybackTracker,
}

impl RecommendationState {
    /// Clear the rolling feed without forgetting the learned history.
    pub fn reset_feed(&mut self) {
        self.started = false;
        self.loading = false;
        self.error = None;
        self.playback = PlaybackTracker::default();
    }

    /// Build a feedback event stamped with this session's id.
    pub fn feedback(&self, track_id: TrackId, kind: FeedbackKind, now_ms: i64) -> FeedbackEvent {
        FeedbackEvent {
            track_id: track_id.as_u128(),
            timestamp_ms: now_ms,
            // Before the first track is observed there is no play sequence to
            // join; `0` is not a real session id.
            session_id: (self.playback.session_id != 0).then_some(self.playback.session_id),
            kind,
        }
    }

    /// Observe the transport once per frame and return any feedback edges.
    ///
    /// A track change finalises the previous track: near the end it counts as a
    /// completion, otherwise a deliberate skip. The new track emits
    /// `PlayStarted` only when the transport is actually playing, so loading a
    /// paused track does not teach the engine anything.
    pub fn observe_playback(
        &mut self,
        track_id: Option<TrackId>,
        position_secs: f32,
        duration_secs: f32,
        is_playing: bool,
        now_ms: i64,
    ) -> Vec<FeedbackEvent> {
        let mut events = Vec::new();

        if track_id != self.playback.current {
            if let Some(event) = self.playback.finish(now_ms) {
                events.push(event);
            }
            self.playback.begin(track_id, position_secs, duration_secs);
            if let (Some(id), true) = (track_id, is_playing) {
                self.playback.play_started_reported = true;
                events.push(self.feedback(id, FeedbackKind::PlayStarted, now_ms));
            }
        } else if let Some(id) = track_id {
            if self
                .playback
                .observe(position_secs, duration_secs, is_playing)
            {
                // The track was loaded while paused and only now started.
                events.push(self.feedback(id, FeedbackKind::PlayStarted, now_ms));
            }
            if self.playback.should_report_completion() {
                events.push(self.feedback(id, FeedbackKind::Completed, now_ms));
                self.playback.completed_reported = true;
            }
        }

        events
    }
}

/// Tracks one listening edge at a time. It owns no clock: callers pass the
/// current position and timestamp, which keeps the reducer testable.
#[derive(Debug, Clone, Default)]
struct PlaybackTracker {
    current: Option<TrackId>,
    max_position_secs: f32,
    duration_secs: f32,
    completed_reported: bool,
    /// `PlayStarted` was emitted for this track. A track loaded while paused
    /// gets its event on the first frame where the transport is playing.
    play_started_reported: bool,
    /// Stable id for this app run. Feedback from one run forms one play
    /// sequence, which the profile uses for transition learning.
    session_id: u64,
}

impl PlaybackTracker {
    fn begin(&mut self, track_id: Option<TrackId>, position_secs: f32, duration_secs: f32) {
        self.current = track_id;
        self.max_position_secs = position_secs.max(0.0);
        self.duration_secs = duration_secs.max(0.0);
        self.completed_reported = false;
        self.play_started_reported = false;
        if self.session_id == 0 {
            self.session_id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1);
        }
    }

    /// Update the high-water mark and report whether playback has just begun
    /// for a track that was loaded while paused.
    fn observe(&mut self, position_secs: f32, duration_secs: f32, is_playing: bool) -> bool {
        if position_secs.is_finite() {
            self.max_position_secs = self.max_position_secs.max(position_secs.max(0.0));
        }
        if duration_secs.is_finite() && duration_secs > 0.0 {
            self.duration_secs = duration_secs;
        }
        if is_playing && !self.play_started_reported {
            self.play_started_reported = true;
            return true;
        }
        false
    }

    fn should_report_completion(&self) -> bool {
        !self.completed_reported
            && self.current.is_some()
            && self.duration_secs > 0.75
            && self.max_position_secs >= self.duration_secs - 0.75
    }

    fn finish(&self, now_ms: i64) -> Option<FeedbackEvent> {
        let track_id = self.current?;
        // The end-of-track frame already reported completion. Reporting it
        // again when the transport advances would double-count the play.
        if self.completed_reported {
            return None;
        }
        let duration = self.duration_secs;
        if !duration.is_finite() || duration <= 0.0 {
            return None;
        }
        let fraction = (self.max_position_secs / duration).clamp(0.0, 1.0);
        let kind = if fraction >= 0.90 {
            FeedbackKind::Completed
        } else if self.max_position_secs >= 1.0 {
            FeedbackKind::Skipped {
                listened_fraction: fraction,
            }
        } else {
            // A track that was loaded but never really started teaches
            // nothing; do not count it as a skip.
            return None;
        };
        Some(FeedbackEvent {
            track_id: track_id.as_u128(),
            timestamp_ms: now_ms,
            session_id: Some(self.session_id),
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> RecommendationState {
        RecommendationState::default()
    }

    #[test]
    fn a_track_change_reports_completion_or_skip() {
        let mut state = state();
        let first = TrackId::new();
        let second = TrackId::new();

        assert!(state
            .observe_playback(Some(first), 0.0, 100.0, true, 1_000)
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::PlayStarted)));

        // Most of the track was heard: completion, not a skip.
        state.observe_playback(Some(first), 95.0, 100.0, true, 2_000);
        let events = state.observe_playback(Some(second), 0.0, 200.0, true, 3_000);
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::Completed)));
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::PlayStarted)));
    }

    #[test]
    fn a_short_listen_is_a_skip_with_the_fraction() {
        let mut state = state();
        let first = TrackId::new();
        state.observe_playback(Some(first), 0.0, 100.0, true, 1_000);
        state.observe_playback(Some(first), 12.0, 100.0, true, 2_000);
        let events = state.observe_playback(None, 0.0, 0.0, false, 3_000);
        let skip = events
            .iter()
            .find_map(|event| match event.kind {
                FeedbackKind::Skipped { listened_fraction } => Some(listened_fraction),
                _ => None,
            })
            .expect("skip event");
        assert!((skip - 0.12).abs() < 1e-5);
    }

    #[test]
    fn stopping_at_the_end_reports_completion_once() {
        let mut state = state();
        let track = TrackId::new();
        state.observe_playback(Some(track), 0.0, 10.0, true, 1_000);
        let events = state.observe_playback(Some(track), 9.5, 10.0, false, 2_000);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, FeedbackKind::Completed))
                .count(),
            1
        );
        let again = state.observe_playback(Some(track), 9.6, 10.0, false, 3_000);
        assert!(!again
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::Completed)));
    }

    #[test]
    fn loading_a_paused_track_does_not_emit_play_started() {
        let mut state = state();
        let track = TrackId::new();
        let events = state.observe_playback(Some(track), 0.0, 100.0, false, 1_000);
        assert!(events.is_empty());
    }

    #[test]
    fn resuming_a_loaded_track_reports_play_started_once() {
        let mut state = state();
        let track = TrackId::new();
        // Loaded, still paused.
        state.observe_playback(Some(track), 0.0, 100.0, false, 1_000);
        // The user presses play: this is the real start.
        let events = state.observe_playback(Some(track), 0.0, 100.0, true, 2_000);
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, FeedbackKind::PlayStarted))
                .count(),
            1
        );
        // Pausing and resuming does not start a second play sequence.
        state.observe_playback(Some(track), 5.0, 100.0, false, 3_000);
        let events = state.observe_playback(Some(track), 5.0, 100.0, true, 4_000);
        assert!(!events
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::PlayStarted)));
    }

    #[test]
    fn a_completed_track_is_not_reported_again_on_the_transition() {
        let mut state = state();
        let first = TrackId::new();
        let second = TrackId::new();
        state.observe_playback(Some(first), 0.0, 10.0, true, 1_000);
        // Reaching the end emits the one completion.
        let at_end = state.observe_playback(Some(first), 9.5, 10.0, true, 2_000);
        assert_eq!(
            at_end
                .iter()
                .filter(|event| matches!(event.kind, FeedbackKind::Completed))
                .count(),
            1
        );
        // Advancing to the next track must not add a second one.
        let next = state.observe_playback(Some(second), 0.0, 20.0, true, 3_000);
        assert!(!next
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::Completed)));
        assert!(next
            .iter()
            .any(|event| matches!(event.kind, FeedbackKind::PlayStarted)));
    }

    #[test]
    fn feedback_before_any_playback_has_no_session_id() {
        let state = state();
        let event = state.feedback(TrackId::new(), FeedbackKind::Liked, 1_000);
        assert_eq!(event.session_id, None);
    }

    #[test]
    fn explicit_feedback_uses_the_running_session_id() {
        let mut state = state();
        let track = TrackId::new();
        state.observe_playback(Some(track), 0.0, 100.0, true, 1_000);
        let event = state.feedback(track, FeedbackKind::Liked, 2_000);
        assert_eq!(event.track_id, track.as_u128());
        assert_eq!(event.timestamp_ms, 2_000);
        assert!(event.session_id.is_some());
    }

    #[test]
    fn quality_presets_map_to_the_core_profiles() {
        assert_eq!(
            AnalysisQuality::LowResource.profile(),
            recommendation::AnalysisProfile::LowResource
        );
        assert_eq!(
            AnalysisQuality::Balanced.profile(),
            recommendation::AnalysisProfile::Balanced
        );
        assert_eq!(
            AnalysisQuality::HighQuality.profile(),
            recommendation::AnalysisProfile::HighQuality
        );
    }
}

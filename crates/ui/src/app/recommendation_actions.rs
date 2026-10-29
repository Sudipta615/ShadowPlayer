//! App-level glue for the local recommendation engine.
//!
//! The workers own ranking and analysis; the state owns the playback reducer.
//! This module only translates settings, table feedback, and transport
//! observations into worker commands, keeping the render code free of policy.

use super::*;
use crate::components::songs_table::QuickFilter;
use crate::services::{RecommendationAnalysisEvent, RecommendationEvent};
use crate::state::RecommendationFeedback;
use recommendation::FeedbackKind;

impl ShadowPlayerApp {
    pub(super) fn recommendations_enabled(&self) -> bool {
        self.dsp_state.settings_tab.recommendation_enabled
    }

    /// Ask the ranking worker for the first batch. Repeated calls are safe.
    pub(super) fn start_recommendation_feed(&mut self) {
        if !self.recommendations_enabled() {
            return;
        }
        if !self.recommendation_state.started {
            self.recommendation_state.started = true;
            self.recommendation_state.loading = true;
        }
        self.recommendation_service.start();
    }

    /// Analyse every track that has no feature vector for the current schema.
    pub(super) fn start_recommendation_analysis(&mut self) {
        if !self.recommendations_enabled() {
            return;
        }
        let quality = self.dsp_state.settings_tab.recommendation_analysis_quality;
        if self
            .recommendation_analysis_service
            .start(quality.profile())
        {
            self.recommendation_state.analysis_running = true;
            self.recommendation_state.analysis_done = 0;
            self.recommendation_state.analysis_total = 0;
            self.recommendation_state.analysis_failed = 0;
            self.recommendation_state.analysis_current.clear();
        }
    }

    /// Persist one explicit signal and let the ranking worker re-rank.
    pub(super) fn record_recommendation_feedback(&mut self, feedback: RecommendationFeedback) {
        if !self.recommendations_enabled() {
            return;
        }
        let now = now_ms();
        let event = match feedback {
            RecommendationFeedback::Liked(id) => {
                self.recommendation_state
                    .feedback(id, FeedbackKind::Liked, now)
            }
            RecommendationFeedback::Disliked(id) => {
                self.recommendation_state
                    .feedback(id, FeedbackKind::Disliked, now)
            }
            RecommendationFeedback::Rated(id, rating) => {
                let rating = f32::from(rating.min(5));
                self.recommendation_state
                    .feedback(id, FeedbackKind::Rated { rating }, now)
            }
            RecommendationFeedback::AddedToPlaylist(id) => {
                self.recommendation_state
                    .feedback(id, FeedbackKind::AddedToPlaylist, now)
            }
        };
        self.recommendation_service.feedback(event);
    }

    /// Observe the transport once per frame and forward any edge events.
    ///
    /// The tracker compares this frame's track and position with the previous
    /// one, so a skip is reported when the track actually changes, not when a
    /// button is pressed and the engine has not switched yet.
    pub(super) fn track_recommendation_playback(&mut self) {
        if !self.recommendations_enabled() {
            return;
        }
        let track_id = self.coordinator.current_track().map(|track| track.id);
        let events = self.recommendation_state.observe_playback(
            track_id,
            self.session.position_secs_compensated(),
            self.session.duration_secs(),
            self.session.is_playing(),
            now_ms(),
        );
        for event in events {
            self.recommendation_service.feedback(event);
        }
    }

    /// Apply the Recommendation Engine card's actions.
    pub(super) fn handle_recommendation_settings(
        &mut self,
        actions: &crate::views::SettingsViewActions,
    ) {
        if let Some(enabled) = actions.recommendation_enabled_changed {
            self.settings_dirty = true;
            if enabled {
                self.recommendation_service.refresh_stats();
            } else {
                // Opting out stops learning immediately and drops the feed;
                // the cached audio features stay, so re-enabling is cheap.
                self.recommendation_analysis_service.cancel();
                self.recommendation_service.reset();
                self.recommendation_state.reset_feed();
                self.library_state.clear_recommendation_ids();
                if self.library_state.quick_filter == QuickFilter::ForYou {
                    self.library_state.quick_filter = QuickFilter::All;
                    self.refresh_view();
                }
            }
        }
        if let Some(batch) = actions.recommendation_batch_size_changed {
            self.recommendation_service.set_batch_size(batch);
            self.settings_dirty = true;
        }
        if actions.recommendation_analysis_quality_changed.is_some() {
            self.settings_dirty = true;
        }
        if actions.start_recommendation_analysis {
            self.start_recommendation_analysis();
        }
        if actions.cancel_recommendation_analysis {
            self.recommendation_analysis_service.cancel();
        }
        if actions.clear_recommendation_history {
            self.recommendation_service.clear_history();
            self.recommendation_state.reset_feed();
            self.library_state.clear_recommendation_ids();
            // Clearing history while looking at the feed should replace it
            // with a fresh batch rather than leave an empty view.
            if self.recommendations_enabled()
                && self.library_state.quick_filter == QuickFilter::ForYou
            {
                self.start_recommendation_feed();
            }
        }
    }

    /// Drain worker events. Called once per frame from `poll_services`.
    pub(super) fn poll_recommendation_services(&mut self, ctx: &egui::Context) {
        while let Some(event) = self.recommendation_service.poll() {
            match event {
                RecommendationEvent::Ready(snapshot) => {
                    self.recommendation_state.loading = false;
                    self.recommendation_state.error = None;
                    self.recommendation_state.analyzed = snapshot.analyzed;
                    self.recommendation_state.total = snapshot.total;
                    self.recommendation_state.feedback_count = snapshot.feedback;
                    self.library_state.set_recommendation_ids(snapshot.ids);
                    // For You is an id projection; the table must rebuild to
                    // show the new order and drop played tracks.
                    self.refresh_view();
                    ctx.request_repaint();
                }
                RecommendationEvent::Stats(stats) => {
                    self.recommendation_state.analyzed = stats.analyzed;
                    self.recommendation_state.total = stats.total;
                    self.recommendation_state.feedback_count = stats.feedback;
                    ctx.request_repaint();
                }
                RecommendationEvent::Error(error) => {
                    self.recommendation_state.loading = false;
                    self.recommendation_state.error = Some(error.clone());
                    log::warn!("Recommendation engine: {error}");
                    self.toasts.warning(error);
                    ctx.request_repaint();
                }
            }
        }

        while let Some(event) = self.recommendation_analysis_service.poll() {
            match event {
                RecommendationAnalysisEvent::Finished {
                    done,
                    total,
                    failed,
                    cancelled,
                } => {
                    self.recommendation_state.analysis_running = false;
                    self.recommendation_state.analysis_done = done;
                    self.recommendation_state.analysis_total = total;
                    self.recommendation_state.analysis_failed = failed;
                    self.recommendation_state.analysis_has_run = true;
                    // New vectors are now available to the ranking worker.
                    self.recommendation_service.reload_catalog();
                    self.recommendation_service.refresh_stats();
                    let summary = if cancelled {
                        tr_args!(
                            "Recommendation analysis cancelled after {done} tracks",
                            "done" => done
                        )
                    } else {
                        tr_args!(
                            "Recommendation analysis finished: {done} tracks · {failed} failed",
                            "done" => done,
                            "failed" => failed
                        )
                    };
                    self.toasts.info(summary);
                    ctx.request_repaint();
                }
                RecommendationAnalysisEvent::Error(error) => {
                    self.recommendation_state.analysis_running = false;
                    self.recommendation_state.error = Some(error.clone());
                    log::warn!("Recommendation analysis: {error}");
                    self.toasts.warning(error);
                    ctx.request_repaint();
                }
                RecommendationAnalysisEvent::Progress { .. } => {}
            }
        }

        // The service updates its own progress struct while polling; mirror it
        // so the settings card renders live numbers without event bookkeeping.
        let progress = self.recommendation_analysis_service.progress();
        self.recommendation_state.analysis_running = progress.running;
        self.recommendation_state.analysis_done = progress.done;
        self.recommendation_state.analysis_total = progress.total;
        self.recommendation_state.analysis_failed = progress.failed;
        self.recommendation_state
            .analysis_current
            .clone_from(&progress.current);
        if self.recommendation_analysis_service.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

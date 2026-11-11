use super::super::*;
use super::PlaybackView;
use crate::components::now_playing::{NowPlayingCard, SleepTimerRequest};
use crate::state::sleep_timer::SleepTimer;
use domain::models::RepeatModeChoice;

impl ShadowPlayerApp {
    /// Render the unified now playing / player card across library views.
    pub(super) fn render_player_card(&mut self, ui: &mut egui::Ui, playback: &PlaybackView<'_>) {
        let current_track = playback.current_track;
        let title = current_track
            .map(|t| t.title.as_str())
            .unwrap_or("No Track Selected");
        let artist = current_track
            .map(|t| t.artist.as_str())
            .unwrap_or("ShadowPlayer");
        let album = current_track.map(|t| t.album.as_str()).unwrap_or("Ready");

        if let Some(t) = current_track {
            self.artwork_service.ensure_loaded(ui.ctx(), t);
        }
        let art_tex = current_track
            .and_then(|t| self.artwork_service.get_texture(&t.id))
            .cloned();

        let format_str = current_track.map(|t| {
            let sr_khz = t.sample_rate as f32 / 1000.0;
            if t.is_lossless {
                format!("{} {:.1}kHz", t.format, sr_khz)
            } else if let Some(br) = t.bitrate_kbps {
                format!("{} {}kbps", t.format, br)
            } else {
                t.format.clone()
            }
        });

        let is_cur_fav = current_track
            .map(|t| self.library_state.favorite_ids.contains(&t.id))
            .unwrap_or(false);

        let sleep_label = self.app_state.sleep_timer.as_ref().map(|t| t.label());
        let waveform = current_track.and_then(|t| self.waveform_service.peaks(&t.id));

        let np_events = NowPlayingCard {
            title,
            artist,
            album,
            is_playing: playback.telemetry.is_playing,
            is_favorite: is_cur_fav,
            rating: current_track.map(|t| t.rating).unwrap_or(0),
            is_shuffle: self.coordinator.is_shuffle(),
            is_repeat: self.coordinator.repeat_mode() != RepeatModeChoice::Off,
            position_secs: playback.telemetry.position_secs,
            duration_secs: playback.telemetry.duration_secs,
            spectrum_db: &playback.telemetry.spectrum.spectrum_db,
            waveform,
            artwork: art_tex.as_ref(),
            format_str: format_str.as_deref(),
            bit_perfect: playback.telemetry.bit_perfect,
            is_eq_open: self.dsp_state.eq_panel.is_open,
            sleep_timer_label: sleep_label.as_deref(),
            theme: &self.theme,
        }
        .show(ui);

        // Handle Now Playing card interactions
        if np_events.toggle_play {
            self.coordinator.toggle_play_pause();
        }
        if np_events.next_track {
            self.coordinator.next();
        }
        if np_events.prev_track {
            self.coordinator.previous();
        }
        if np_events.toggle_shuffle {
            let new_shuffle = !self.coordinator.is_shuffle();
            self.coordinator.set_shuffle(new_shuffle);
        }
        if np_events.toggle_repeat {
            let next_mode = match self.coordinator.repeat_mode() {
                RepeatModeChoice::Off => RepeatModeChoice::All,
                RepeatModeChoice::All => RepeatModeChoice::One,
                RepeatModeChoice::One => RepeatModeChoice::Off,
            };
            self.coordinator.set_repeat_mode(next_mode);
        }
        if let Some(seek) = np_events.seek_secs {
            self.session.seek(seek as f32);
        }
        if np_events.toggle_favorite {
            if let Some(t) = current_track {
                if !self.library_state.favorite_ids.contains(&t.id) {
                    self.record_recommendation_feedback(
                        crate::state::RecommendationFeedback::Liked(t.id),
                    );
                }
                self.library_state
                    .toggle_favorite(&self.favorites_repo, t.id);
                self.refresh_view();
            }
        }
        if let Some(req) = np_events.sleep_timer_request {
            if matches!(req, SleepTimerRequest::Off) {
                self.session.set_volume(if self.app_state.is_muted {
                    0.0
                } else {
                    self.app_state.volume
                });
            }
            self.app_state.sleep_timer = match req {
                SleepTimerRequest::Off => None,
                SleepTimerRequest::AfterSeconds(secs) => Some(SleepTimer::after_seconds(secs)),
                SleepTimerRequest::AfterSecondsFade(secs) => {
                    Some(SleepTimer::after_seconds_fade(secs, 10.0))
                }
                SleepTimerRequest::EndOfTrack => Some(SleepTimer::end_of_track()),
            };
        }
        if np_events.open_properties {
            self.app_state.track_properties_track = current_track.cloned();
            self.app_state.track_properties_open = true;
        }
        if np_events.open_bit_perfect {
            self.app_state.bit_perfect_modal_open = true;
        }
        if np_events.open_zones {
            self.app_state.zone_manager_modal.is_open = true;
        }
        if np_events.open_loudness {
            self.open_loudness_inspector();
        }
        if np_events.open_eq_panel {
            self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
            self.settings_dirty = true;
        }
    }
}

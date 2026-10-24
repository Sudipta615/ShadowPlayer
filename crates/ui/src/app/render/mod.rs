mod center;
mod drawer;
mod frame;
mod sidebar;

use super::*;
use crate::services::LyricsSource;
use domain::models::SyncedLyricLine;

impl ShadowPlayerApp {
    /// Top-bar queue/theme/mini actions. Returns nothing; each button updates
    /// the state it owns. Kept in one place so Settings and Help get the same
    /// controls as library views.
    pub(super) fn render_topbar_actions(&mut self, ui: &mut egui::Ui, window_w: f32) {
        ui.spacing_mut().item_spacing.x = 6.0;
        let wide = window_w >= 1100.0;
        let queue_active = if wide {
            self.app_state.right_panel_visible
        } else {
            self.app_state.right_overlay_open
        };
        if render_action_button(
            ui,
            TopbarAction::Queue,
            queue_active,
            &self.theme,
            "Queue & lyrics",
        ) {
            if wide {
                self.app_state.right_panel_visible = !self.app_state.right_panel_visible;
            } else {
                self.app_state.right_overlay_open = !self.app_state.right_overlay_open;
            }
        }
        // The right drawer already carries the sun/moon pill when it is a
        // column. Only duplicate it in the top bar for the narrow layout,
        // where the drawer is an overlay.
        if !wide
            && render_action_button(
                ui,
                TopbarAction::Theme,
                false,
                &self.theme,
                if self.theme.is_light {
                    "Switch to dark theme"
                } else {
                    "Switch to light theme"
                },
            )
        {
            self.theme.toggle();
            self.settings_dirty = true;
        }
        if render_action_button(
            ui,
            TopbarAction::MiniPlayer,
            self.app_state.mini_player,
            &self.theme,
            "Mini player",
        ) {
            self.set_mini_player(!self.app_state.mini_player);
        }
    }
}

/// Borrowed per-frame playback state shared by the render helpers.
pub(super) struct PlaybackView<'a> {
    pub current_track: Option<&'a Track>,
    pub telemetry: &'a TelemetryState,
    pub is_playing: bool,
    pub active_lyrics: &'a [SyncedLyricLine],
    pub plain_lyrics: Option<&'a str>,
    pub lyrics_source: Option<LyricsSource>,
    pub active_lyric_idx: Option<usize>,
}

impl ShadowPlayerApp {
    /// Draw the full-screen Now Playing shell. Returns `true` when it owns the
    /// frame and the normal library shell must not be drawn.
    pub(super) fn render_now_playing_full(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        playback: &PlaybackView<'_>,
    ) -> bool {
        // The full-screen Now Playing mode: the window gives itself over to
        // the artwork, the visualiser and the lyrics.
        if self.app_state.now_playing_full {
            let art = playback
                .current_track
                .and_then(|t| self.artwork_service.get_texture(&t.id))
                .cloned();
            let full_events = crate::components::now_playing_full::NowPlayingFull {
                title: playback
                    .current_track
                    .map(|t| t.title.as_str())
                    .unwrap_or("Nothing playing"),
                artist: playback
                    .current_track
                    .map(|t| t.artist.as_str())
                    .unwrap_or("ShadowPlayer"),
                album: playback
                    .current_track
                    .map(|t| t.album.as_str())
                    .unwrap_or(""),
                artwork: art.as_ref(),
                is_playing: playback.is_playing,
                position_secs: playback.telemetry.position_secs,
                duration_secs: playback.telemetry.duration_secs,
                spectrum_db: &playback.telemetry.spectrum.spectrum_db,
                waveform: playback
                    .current_track
                    .and_then(|t| self.waveform_service.peaks(&t.id)),
                lyrics_lines: playback.active_lyrics,
                active_lyric: playback.active_lyric_idx,
                theme: &self.theme,
            }
            .show(ui);

            if full_events.toggle_play {
                self.coordinator.toggle_play_pause();
            }
            if full_events.previous {
                self.coordinator.previous();
            }
            if full_events.next {
                self.coordinator.next();
            }
            if let Some(seek) = full_events.seek_secs {
                self.session.seek(seek as f32);
            }
            if full_events.close {
                self.set_now_playing_full(ctx, false);
            }
            return true;
        }
        false
    }

    /// Draw the compact mini player. Returns `true` when it owns the frame.
    pub(super) fn render_mini_player(
        &mut self,
        ui: &mut egui::Ui,
        playback: &PlaybackView<'_>,
    ) -> bool {
        // Mini player: a dedicated 320x110 shell instead of squeezing the
        // library chrome into a window that cannot hold it.
        if self.app_state.mini_player {
            let art = playback
                .current_track
                .and_then(|t| self.artwork_service.get_texture(&t.id))
                .cloned();
            let mini_events = MiniPlayer {
                title: playback
                    .current_track
                    .map(|t| t.title.as_str())
                    .unwrap_or("Nothing playing"),
                artist: playback
                    .current_track
                    .map(|t| t.artist.as_str())
                    .unwrap_or("ShadowPlayer"),
                artwork: art.as_ref(),
                is_playing: playback.is_playing,
                position_secs: playback.telemetry.position_secs,
                duration_secs: playback.telemetry.duration_secs,
                theme: &self.theme,
            }
            .show(ui);

            if mini_events.toggle_play {
                self.coordinator.toggle_play_pause();
            }
            if mini_events.previous {
                self.coordinator.previous();
            }
            if mini_events.next {
                self.coordinator.next();
            }
            if let Some(seek) = mini_events.seek_secs {
                self.session.seek(seek as f32);
            }
            if mini_events.restore {
                self.set_mini_player(false);
            }
            return true;
        }
        false
    }

    pub(super) fn render_main_layout(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        window_w: f32,
        playback: &PlaybackView<'_>,
    ) {
        self.render_sidebar_panel(ui);
        self.render_center_panel(ui, ctx, window_w, playback);
    }
}

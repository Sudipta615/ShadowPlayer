use eframe::egui::{self, Vec2};
use audio_session::AudioSession;
use domain::models::{SyncedLyricLine, Track};
use playback::PlaybackCoordinator;

use crate::components::right_panel::RightPanel;
use crate::services::ArtworkService;
use crate::state::{AppState, DspState};
use crate::theme::Theme;

/// Render right column drawer: window controls, Queue/Lyrics tabs, volume controls
pub fn render_right_drawer(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    right_w: f32,
    avail_h: f32,
    app_state: &mut AppState,
    dsp_state: &mut DspState,
    coordinator: &mut PlaybackCoordinator,
    session: &AudioSession,
    artwork_service: &ArtworkService,
    active_lyrics: &[SyncedLyricLine],
    plain_lyrics: Option<&str>,
    active_lyric_idx: Option<usize>,
    theme: &mut Theme,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(right_w, avail_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

            // 1. Top Window Controls + Theme Toggle
            let win_resp = crate::components::topbar::render_window_controls(ui, theme);
            if win_resp.close_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if win_resp.maximize_requested {
                let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
            }
            if win_resp.minimize_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
            if win_resp.theme_toggle_requested {
                theme.toggle();
            }

            // 2. Right Panel (Queue / Lyrics / Volume)
            let queue_tracks: Vec<Track> = coordinator.queue().to_vec();
            let cur_queue_idx = coordinator.current_index().unwrap_or(0);

            let right_action = RightPanel {
                active_tab: app_state.right_tab,
                queue_tracks: &queue_tracks,
                current_queue_idx: cur_queue_idx,
                lyrics_lines: active_lyrics,
                plain_lyrics,
                active_lyric_idx,
                volume: app_state.volume,
                is_muted: app_state.is_muted,
                artwork_map: artwork_service.texture_cache(),
                settings_state: &mut dsp_state.settings_tab,
                theme,
            }
            .show(ui);

            if let Some(new_tab) = right_action.tab_changed {
                app_state.right_tab = new_tab;
            }
            if let Some(q_idx) = right_action.play_queue_index {
                coordinator.play_index(q_idx);
            }
            if let Some(del_idx) = right_action.remove_queue_index {
                coordinator.remove_from_queue(del_idx);
            }
            if right_action.clear_queue {
                coordinator.clear_queue();
            }

            dsp_state.handle_settings_actions(session, &right_action.settings);

            if let Some(new_vol) = right_action.volume_changed {
                app_state.volume = new_vol;
                app_state.is_muted = false;
                session.set_volume(new_vol);
            }
            if right_action.toggle_mute {
                if app_state.is_muted {
                    app_state.is_muted = false;
                    app_state.volume = app_state.prev_volume;
                    session.set_volume(app_state.volume);
                } else {
                    app_state.prev_volume = app_state.volume;
                    app_state.is_muted = true;
                    session.set_volume(0.0);
                }
            }
        },
    );
}

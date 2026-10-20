use audio_session::AudioSession;
use domain::models::SyncedLyricLine;
use eframe::egui::{self, Vec2};
use playback::PlaybackCoordinator;

use crate::components::right_panel::RightPanel;
use crate::services::ArtworkService;
use crate::state::AppState;
use crate::theme::Theme;

/// Render right column drawer: window controls, Queue/Lyrics tabs, volume controls
#[allow(clippy::too_many_arguments)]
pub fn render_right_drawer(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    right_w: f32,
    avail_h: f32,
    app_state: &mut AppState,
    coordinator: &mut PlaybackCoordinator,
    session: &AudioSession,
    artwork_service: &mut ArtworkService,
    active_lyrics: &[SyncedLyricLine],
    plain_lyrics: Option<&str>,
    lyrics_source: Option<crate::services::LyricsSource>,
    active_lyric_idx: Option<usize>,
    theme: &mut Theme,
    show_theme_toggle: bool,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(right_w, avail_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

            // 1. Theme Toggle (wide drawer only). In narrow layouts the
            // top bar owns this control, so the overlay must not render a
            // second one behind it.
            if show_theme_toggle {
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
            }

            // 2. Right Panel (Queue / Lyrics / Volume)
            // Borrow the queue directly instead of cloning it every frame.
            let cur_queue_idx = coordinator.current_index().unwrap_or(0);

            let right_action = RightPanel {
                active_tab: app_state.right_tab,
                queue_tracks: coordinator.queue(),
                current_queue_idx: cur_queue_idx,
                lyrics_lines: active_lyrics,
                plain_lyrics,
                lyrics_source,
                active_lyric_idx,
                volume: app_state.volume,
                is_muted: app_state.is_muted,
                artwork: artwork_service,
                theme,
            }
            .show(ui);

            if let Some(new_tab) = right_action.tab_changed {
                app_state.right_tab = new_tab;
            }
            if let Some(q_idx) = right_action.play_queue_index {
                coordinator.play_index(q_idx);
            }
            if let Some((from, delta)) = right_action.move_queue {
                let to = (from as isize + delta).max(0) as usize;
                coordinator.move_in_queue(from, to);
            }
            if let Some(del_idx) = right_action.remove_queue_index {
                coordinator.remove_from_queue(del_idx);
            }
            if right_action.clear_queue {
                coordinator.clear_queue();
            }

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

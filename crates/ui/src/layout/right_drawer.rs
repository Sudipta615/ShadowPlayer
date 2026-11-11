use audio_session::AudioSession;
use domain::models::{SyncedLyricLine, Track};
use eframe::egui::{self, Vec2};
use playback::PlaybackCoordinator;

use crate::components::right_panel::RightPanel;
use crate::services::{ArtworkService, LyricsService};
use crate::state::AppState;
use crate::theme::Theme;

/// Render right column drawer: window controls, Queue/Lyrics tabs, volume controls
#[allow(clippy::too_many_arguments)]
pub fn render_right_drawer(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    right_w: f32,
    avail_h: f32,
    app_state: &mut AppState,
    coordinator: &mut PlaybackCoordinator,
    _session: &AudioSession,
    artwork_service: &mut ArtworkService,
    lyrics_service: &mut LyricsService,
    current_track: Option<&Track>,
    active_lyrics: &[SyncedLyricLine],
    plain_lyrics: Option<&str>,
    lyrics_source: Option<crate::services::LyricsSource>,
    active_lyric_idx: Option<usize>,
    theme: &mut Theme,
    _show_theme_toggle: bool,
) {
    ui.allocate_ui_with_layout(
        Vec2::new(right_w, avail_h),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            // 2. Right Panel (Queue / Lyrics / Volume)
            // Borrow the queue directly instead of cloning it every frame.
            let cur_queue_idx = coordinator.current_index().unwrap_or(0);

            let right_action = RightPanel {
                active_tab: app_state.right_tab,
                queue_tracks: coordinator.queue(),
                current_queue_idx: cur_queue_idx,
                current_track,
                lyrics_lines: active_lyrics,
                plain_lyrics,
                lyrics_source,
                active_lyric_idx,
                lyrics_editor: &mut app_state.lyrics_editor,
                artwork: artwork_service,
                theme,
            }
            .show(ui);

            if right_action.save_lyrics {
                if let Some(track) = current_track {
                    match lyrics_service.save_sidecar(track, &app_state.lyrics_editor.draft) {
                        Ok(_) => app_state.lyrics_editor.close(),
                        Err(e) => app_state.lyrics_editor.error = Some(e),
                    }
                }
            }

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
        },
    );
}

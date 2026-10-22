use audio_session::AudioSession;
use database::{ScanRootsRepo, TrackRepo};
use eframe::egui::{self, Vec2};
use std::path::PathBuf;

use crate::components::settings_tab::{show_settings_tab_extended, SettingsExtraContext};
use crate::services::{ImportKind, LoudnessProgress};
use crate::state::DspState;
use crate::theme::Theme;

/// Actions from the settings view that the app must handle (they need services
/// the view does not own, such as the loudness scan worker).
#[derive(Default)]
pub struct SettingsViewActions {
    pub start_loudness_scan: bool,
    pub cancel_loudness_scan: bool,
    /// Open the duplicate-track finder.
    pub find_duplicates: bool,
    /// The interface language changed. `None` means "follow the environment".
    pub locale_changed: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn render_settings_view(
    ui: &mut egui::Ui,
    dsp_state: &mut DspState,
    session: &AudioSession,
    theme: &mut Theme,
    scan_roots_repo: &ScanRootsRepo,
    track_repo: &TrackRepo,
    is_scanning: bool,
    import_active: bool,
    imported_dirs: &[PathBuf],
    loudness: &LoudnessProgress,
    // The saved language preference; empty means "follow the environment".
    locale: &str,
    mut on_start_scan: impl FnMut(PathBuf),
    mut on_import: impl FnMut(ImportKind),
) -> SettingsViewActions {
    let mut view_actions = SettingsViewActions::default();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_2xl);

        // ── 1. Lead line ───────────────────────────────────────────────────
        // The page's title is the topbar's, which is sticky; a second
        // 22px heading here was the same words twice.
        crate::components::page::page_lead(
            ui,
            theme,
            &format!(
                "Configure Audio Engine v{}, bit-perfect streaming, 3D acoustics, and library",
                engine::VERSION
            ),
        );

        // ── 2. Render Settings Content with Extra Context ──────────────────
        let scan_roots = scan_roots_repo.list().unwrap_or_default();
        let track_count = track_repo.count().unwrap_or_default();

        // The device list is cached on the DSP state and refreshed only when
        // the backend changes — enumerating devices is not a per-frame job.
        dsp_state.refresh_output_devices(session, false);
        let playback_info = session.playback_info();

        let extra = SettingsExtraContext {
            scan_roots: &scan_roots,
            track_count,
            is_scanning,
            is_dark_theme: !theme.is_light,
            import_active,
            imported_dirs,
            loudness_running: loudness.running,
            loudness_done: loudness.done,
            loudness_total: loudness.total,
            loudness_current: &loudness.current,
            loudness_has_run: loudness.has_run,
            latency_ms: session.latency_ms(),
            requested_backend: playback_info
                .output_info
                .as_ref()
                .and_then(|i| i.requested_backend),
            actual_backend: playback_info
                .output_info
                .as_ref()
                .and_then(|i| i.actual_backend),
            output_info: playback_info.output_info.as_ref(),
            output_devices: &dsp_state.output_devices,
            graph_build_ms: session.graph_build_stats().0,
            graph_builds: session.graph_build_stats().1,
            locale,
        };

        let s_actions = show_settings_tab_extended(ui, &mut dsp_state.settings_tab, theme, &extra);

        // ── 3. Handle Settings Actions ─────────────────────────────────────
        if s_actions.toggle_theme {
            theme.toggle();
        }
        if let Some(ref path) = s_actions.trigger_scan {
            let _ = scan_roots_repo.add(path);
            on_start_scan(path.clone());
        }
        if let Some(id) = s_actions.remove_scan_root {
            let _ = scan_roots_repo.remove(&id);
        }
        if s_actions.import_files {
            on_import(ImportKind::Files);
        }
        if s_actions.import_folder {
            on_import(ImportKind::Folder);
        }
        if s_actions.import_playlist {
            on_import(ImportKind::Playlist);
        }
        if s_actions.find_duplicates {
            view_actions.find_duplicates = true;
        }
        if s_actions.scan_loudness_library {
            view_actions.start_loudness_scan = true;
        }
        if s_actions.cancel_loudness_scan {
            view_actions.cancel_loudness_scan = true;
        }
        if let Some(locale) = s_actions.locale_changed.clone() {
            view_actions.locale_changed = Some(locale);
        }

        dsp_state.handle_settings_actions(session, &s_actions);
    });

    view_actions
}

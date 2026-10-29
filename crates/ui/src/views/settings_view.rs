use audio_session::AudioSession;
use database::{ScanRootsRepo, TrackRepo};
use eframe::egui::{self, Vec2};
use std::path::PathBuf;

use crate::components::settings_tab::{
    show_settings_tab_extended, CrossfadePreviewContext, RecommendationSettingsContext,
    SettingsExtraContext,
};
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
    pub preview_theme: Option<String>,
    pub apply_theme: Option<String>,
    pub cancel_theme_preview: bool,
    pub reload_themes: bool,
    pub import_theme: bool,
    pub frosted_glass_changed: Option<bool>,
    pub reduce_motion_changed: Option<bool>,
    /// User pressed "Fetch Missing Artwork".
    pub open_artwork_fetch: bool,
    /// Play or stop the crossfade audition.
    pub toggle_crossfade_preview: bool,
    /// The crossfade curve or duration changed while a preview might be
    /// playing.
    pub crossfade_config_changed: bool,
    /// The folder biographies are read from changed.
    pub biography_source_dir_changed: Option<String>,
    /// Open the room-measurement wizard.
    pub open_room_measurement: bool,
    /// Open the speaker setup & acoustic alignment modal.
    pub open_speaker_setup: bool,
    pub open_cd_ripper: bool,
    pub open_transcode_export: bool,
    pub open_update_check: bool,
    pub start_file_conversion: bool,
    pub cancel_file_conversion: bool,
    pub pick_converter_dir: bool,
    pub pick_converter_files: bool,
    pub clear_converter_queue: bool,
    /// The Recommendation Engine toggle changed.
    pub recommendation_enabled_changed: Option<bool>,
    /// How many tracks each refill appends.
    pub recommendation_batch_size_changed: Option<usize>,
    /// How much audio the analyser samples per track.
    pub recommendation_analysis_quality_changed: Option<crate::state::AnalysisQuality>,
    /// User pressed "Analyze all".
    pub start_recommendation_analysis: bool,
    /// User pressed "Cancel" during a library analysis.
    pub cancel_recommendation_analysis: bool,
    /// User pressed "Clear history".
    pub clear_recommendation_history: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn render_settings_view(
    ui: &mut egui::Ui,
    dsp_state: &mut DspState,
    session: &AudioSession,
    theme: &mut Theme,
    theme_manager: &crate::theme::ThemeManager,
    scan_roots_repo: &ScanRootsRepo,
    track_repo: &TrackRepo,
    is_scanning: bool,
    import_active: bool,
    imported_dirs: &[PathBuf],
    loudness: &LoudnessProgress,
    // The saved language preference; empty means "follow the environment".
    locale: &str,
    crossfade_preview: CrossfadePreviewContext<'_>,
    converter_busy: bool,
    converter_progress: Option<(usize, usize, String)>,
    converter_activity: &str,
    recommendation: RecommendationSettingsContext<'_>,
    mut on_start_scan: impl FnMut(PathBuf),
    mut on_import: impl FnMut(ImportKind),
) -> SettingsViewActions {
    let mut view_actions = SettingsViewActions::default();
    let biography_source_before = dsp_state.settings_tab.biography_source_dir.clone();
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
            crossfade_preview,
            scan_roots: &scan_roots,
            track_count,
            is_scanning,
            is_dark_theme: !theme.is_light,
            theme_manager: Some(theme_manager),
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
            apodizing: Some(&playback_info.apodizing),
            output_devices: &dsp_state.output_devices,
            graph_build_ms: session.graph_build_stats().0,
            graph_builds: session.graph_build_stats().1,
            locale,
            converter_busy,
            converter_progress,
            converter_activity,
            recommendation,
        };

        let s_actions = show_settings_tab_extended(ui, &mut dsp_state.settings_tab, theme, &extra);

        // ── 3. Handle Settings Actions ─────────────────────────────────────
        if s_actions.toggle_theme {
            theme.toggle();
        }
        if let Some(ref prev) = s_actions.preview_theme {
            view_actions.preview_theme = Some(prev.clone());
        }
        if let Some(ref app_id) = s_actions.apply_theme {
            view_actions.apply_theme = Some(app_id.clone());
        }
        if s_actions.cancel_theme_preview {
            view_actions.cancel_theme_preview = true;
        }
        if s_actions.reload_themes {
            view_actions.reload_themes = true;
        }
        if s_actions.import_theme {
            view_actions.import_theme = true;
        }
        if let Some(enabled) = s_actions.frosted_glass_changed {
            view_actions.frosted_glass_changed = Some(enabled);
        }
        if let Some(enabled) = s_actions.reduce_motion_changed {
            view_actions.reduce_motion_changed = Some(enabled);
        }
        if s_actions.open_artwork_fetch {
            view_actions.open_artwork_fetch = true;
        }
        if s_actions.open_room_measurement {
            view_actions.open_room_measurement = true;
        }
        if s_actions.open_speaker_setup {
            view_actions.open_speaker_setup = true;
        }
        if s_actions.open_cd_ripper {
            view_actions.open_cd_ripper = true;
        }
        if s_actions.open_transcode_export {
            view_actions.open_transcode_export = true;
        }
        if s_actions.open_update_check {
            view_actions.open_update_check = true;
        }
        if s_actions.toggle_crossfade_preview {
            view_actions.toggle_crossfade_preview = true;
        }
        if s_actions.crossfade_config_changed {
            view_actions.crossfade_config_changed = true;
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
        if s_actions.start_file_conversion {
            view_actions.start_file_conversion = true;
        }
        if s_actions.cancel_file_conversion {
            view_actions.cancel_file_conversion = true;
        }
        if s_actions.pick_converter_dir {
            view_actions.pick_converter_dir = true;
        }
        if s_actions.pick_converter_files {
            view_actions.pick_converter_files = true;
        }
        if s_actions.clear_converter_queue {
            view_actions.clear_converter_queue = true;
        }
        if let Some(enabled) = s_actions.recommendation_enabled_changed {
            view_actions.recommendation_enabled_changed = Some(enabled);
        }
        if let Some(batch) = s_actions.recommendation_batch_size_changed {
            view_actions.recommendation_batch_size_changed = Some(batch);
        }
        if let Some(quality) = s_actions.recommendation_analysis_quality_changed {
            view_actions.recommendation_analysis_quality_changed = Some(quality);
        }
        if s_actions.start_recommendation_analysis {
            view_actions.start_recommendation_analysis = true;
        }
        if s_actions.cancel_recommendation_analysis {
            view_actions.cancel_recommendation_analysis = true;
        }
        if s_actions.clear_recommendation_history {
            view_actions.clear_recommendation_history = true;
        }

        dsp_state.handle_settings_actions(session, &s_actions);
    });

    if dsp_state.settings_tab.biography_source_dir != biography_source_before {
        view_actions.biography_source_dir_changed =
            Some(dsp_state.settings_tab.biography_source_dir.clone());
    }
    view_actions
}

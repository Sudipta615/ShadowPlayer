use std::path::PathBuf;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use config::{
    AudioBackend, CrossfadeCurve, FallbackPolicy, LoudnessMode, PrecisionMode, ResamplerQuality,
    SampleRatePolicy, SpatialQuality, SpeedMode, TransitionMode, VolumeMode,
};
use domain::ids::ScanRootId;
use domain::models::ScanRoot;
use engine::AudioSource;
use serde::{Deserialize, Serialize};
use crate::theme::Theme;

// ─────────────────────────────────────────────────────────────────────────────
// Categories
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsCategory {
    #[default]
    Playback,
    Output,
    Spatial,
    MixBus,
    Tools,
    Library,
}

impl SettingsCategory {
    pub const ALL: [SettingsCategory; 6] = [
        SettingsCategory::Playback,
        SettingsCategory::Output,
        SettingsCategory::Spatial,
        SettingsCategory::MixBus,
        SettingsCategory::Tools,
        SettingsCategory::Library,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::Playback => "Playback",
            Self::Output => "Hardware & DAC",
            Self::Spatial => "Spatial 3D",
            Self::MixBus => "Mix Bus",
            Self::Tools => "Tools",
            Self::Library => "Library & App",
        }
    }

    pub fn draw_icon(&self, ui: &egui::Ui, rect: Rect, color: Color32) {
        match self {
            Self::Playback => crate::components::icons::draw_disc(ui, rect, color),
            Self::Output => crate::components::icons::draw_speaker(ui, rect, color, false),
            Self::Spatial => crate::components::icons::draw_bar_chart(ui, rect, color),
            Self::MixBus => crate::components::icons::draw_equalizer_sliders(ui, rect, color),
            Self::Tools => crate::components::icons::draw_gear(ui, rect, color),
            Self::Library => crate::components::icons::draw_folder(ui, rect, color),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

pub struct SettingsTabState {
    pub active_category: SettingsCategory,

    // Playback
    pub speed: f32,
    pub pitch_semitones: f32,
    pub speed_mode: SpeedMode,
    pub transition_mode: TransitionMode,
    pub crossfade_duration_secs: f32,
    pub crossfade_curve: CrossfadeCurve,
    pub loudness_mode: LoudnessMode,

    // Output & Device
    pub output_backend: AudioBackend,
    pub output_device: String,
    pub sample_rate_policy: SampleRatePolicy,
    pub volume_mode: VolumeMode,
    pub bit_perfect: bool,
    pub dither_enabled: bool,
    pub resampler_quality: ResamplerQuality,
    pub fallback_policy: FallbackPolicy,

    // Internal DSP precision (f32 vs f64 core). `None` until the user chooses,
    // so the engine's own persisted setting is never clobbered at startup.
    pub precision_mode: Option<PrecisionMode>,

    // Spatial
    pub spatial_quality: SpatialQuality,
    pub hrtf_profile: String,
    pub listener_smoothing_ms: f32,
    pub listener_max_rate: f32,
    pub room_enabled: bool,
    pub room_width: f32,
    pub room_depth: f32,
    pub room_height: f32,
    pub room_absorption: f32,
    pub room_rt60_ms: f32,
    pub room_wet: f32,
    pub air_absorption_enabled: bool,
    pub voice_budget_enabled: bool,
    pub voice_capacity: usize,
    pub screen_center_az: f32,
    pub screen_half_w: f32,
    pub screen_elevation: f32,
    pub screen_gain: f32,
    pub source_azimuth_deg: f32,
    pub source_distance_m: f32,

    // Mix Bus
    pub add_track_path: String,
    pub track_slot: u8,
    pub track_gain: f32,
    pub track_pan: f32,
    pub track_muted: bool,
    pub track_active: bool,
    pub aux_enabled: bool,
    pub aux_return_gain: f32,
    pub duck_source_slot: u8,
    pub duck_threshold_db: f32,
    pub duck_depth_db: f32,
    pub duck_attack_ms: f32,
    pub duck_release_ms: f32,

    // Capture & Tools
    pub capture_path: String,
    pub capture_device: String,
    pub stereo_enhancer_enabled: bool,
    pub loudness_tag_path: String,
    pub write_rg_path: String,
    /// When set, the library ReplayGain scan also writes the measured values
    /// into each file's tags. Off by default: the app never modifies files
    /// without explicit consent.
    pub loudness_write_tags: bool,

    // Library input
    pub new_scan_path: String,

    // Legacy collapsible section flags preserved for compatibility
    pub section_playback: bool,
    pub section_output: bool,
    pub section_spatial: bool,
    pub section_mixbus: bool,
    pub section_capture: bool,
    pub section_advanced: bool,
}

impl Default for SettingsTabState {
    fn default() -> Self {
        Self {
            active_category: SettingsCategory::Playback,

            speed: 1.0,
            pitch_semitones: 0.0,
            speed_mode: SpeedMode::Varispeed,
            transition_mode: TransitionMode::Gapless,
            crossfade_duration_secs: 3.0,
            crossfade_curve: CrossfadeCurve::ConstantPower,
            loudness_mode: LoudnessMode::Off,

            output_backend: AudioBackend::default(),
            output_device: String::new(),
            sample_rate_policy: SampleRatePolicy::FollowTrack,
            volume_mode: VolumeMode::SoftwareOnly,
            bit_perfect: false,
            dither_enabled: true,
            resampler_quality: ResamplerQuality::HighQuality,
            fallback_policy: FallbackPolicy::Allow,
            precision_mode: None,

            spatial_quality: SpatialQuality::High,
            hrtf_profile: String::new(),
            listener_smoothing_ms: 50.0,
            listener_max_rate: 0.0,
            room_enabled: false,
            room_width: 5.0,
            room_depth: 4.0,
            room_height: 2.5,
            room_absorption: 0.3,
            room_rt60_ms: 400.0,
            room_wet: 0.15,
            air_absorption_enabled: false,
            voice_budget_enabled: false,
            voice_capacity: 32,
            screen_center_az: 0.0,
            screen_half_w: 30.0,
            screen_elevation: 0.0,
            screen_gain: 1.0,
            source_azimuth_deg: 0.0,
            source_distance_m: 2.0,

            add_track_path: String::new(),
            track_slot: 2,
            track_gain: 1.0,
            track_pan: 0.0,
            track_muted: false,
            track_active: true,
            aux_enabled: false,
            aux_return_gain: 1.0,
            duck_source_slot: 0,
            duck_threshold_db: -18.0,
            duck_depth_db: 12.0,
            duck_attack_ms: 10.0,
            duck_release_ms: 200.0,

            capture_path: String::new(),
            capture_device: String::new(),
            stereo_enhancer_enabled: false,
            loudness_tag_path: String::new(),
            write_rg_path: String::new(),
            loudness_write_tags: false,

            new_scan_path: String::new(),

            section_playback: true,
            section_output: false,
            section_spatial: false,
            section_mixbus: false,
            section_capture: false,
            section_advanced: false,
        }
    }
}

/// Serde-serializable snapshot of Settings Tab state for persistent storage
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentSettingsTabState {
    pub active_category: SettingsCategory,
    pub speed: f32,
    pub pitch_semitones: f32,
    pub speed_mode: SpeedMode,
    pub transition_mode: TransitionMode,
    pub crossfade_duration_secs: f32,
    pub crossfade_curve: CrossfadeCurve,
    pub loudness_mode: LoudnessMode,

    pub output_backend: AudioBackend,
    pub output_device: String,
    pub sample_rate_policy: SampleRatePolicy,
    pub volume_mode: VolumeMode,
    pub bit_perfect: bool,
    pub dither_enabled: bool,
    pub resampler_quality: ResamplerQuality,
    pub fallback_policy: FallbackPolicy,

    #[serde(default)]
    pub precision_mode: Option<PrecisionMode>,

    pub spatial_quality: SpatialQuality,
    pub hrtf_profile: String,
    pub listener_smoothing_ms: f32,
    pub listener_max_rate: f32,
    pub room_enabled: bool,
    pub room_width: f32,
    pub room_depth: f32,
    pub room_height: f32,
    pub room_absorption: f32,
    pub room_rt60_ms: f32,
    pub room_wet: f32,
    pub air_absorption_enabled: bool,
    pub voice_budget_enabled: bool,
    pub voice_capacity: usize,
    pub stereo_enhancer_enabled: bool,
    /// Write measured ReplayGain values into the audio files during a scan.
    #[serde(default)]
    pub loudness_write_tags: bool,
}

impl From<&SettingsTabState> for PersistentSettingsTabState {
    fn from(s: &SettingsTabState) -> Self {
        Self {
            active_category: s.active_category,
            speed: s.speed,
            pitch_semitones: s.pitch_semitones,
            speed_mode: s.speed_mode,
            transition_mode: s.transition_mode,
            crossfade_duration_secs: s.crossfade_duration_secs,
            crossfade_curve: s.crossfade_curve,
            loudness_mode: s.loudness_mode,
            output_backend: s.output_backend,
            output_device: s.output_device.clone(),
            sample_rate_policy: s.sample_rate_policy.clone(),
            volume_mode: s.volume_mode,
            bit_perfect: s.bit_perfect,
            dither_enabled: s.dither_enabled,
            resampler_quality: s.resampler_quality,
            fallback_policy: s.fallback_policy,
            precision_mode: s.precision_mode,
            spatial_quality: s.spatial_quality,
            hrtf_profile: s.hrtf_profile.clone(),
            listener_smoothing_ms: s.listener_smoothing_ms,
            listener_max_rate: s.listener_max_rate,
            room_enabled: s.room_enabled,
            room_width: s.room_width,
            room_depth: s.room_depth,
            room_height: s.room_height,
            room_absorption: s.room_absorption,
            room_rt60_ms: s.room_rt60_ms,
            room_wet: s.room_wet,
            air_absorption_enabled: s.air_absorption_enabled,
            voice_budget_enabled: s.voice_budget_enabled,
            voice_capacity: s.voice_capacity,
            stereo_enhancer_enabled: s.stereo_enhancer_enabled,
            loudness_write_tags: s.loudness_write_tags,
        }
    }
}

impl PersistentSettingsTabState {
    pub fn apply_to(&self, s: &mut SettingsTabState) {
        s.active_category = self.active_category;
        s.speed = self.speed;
        s.pitch_semitones = self.pitch_semitones;
        s.speed_mode = self.speed_mode;
        s.transition_mode = self.transition_mode;
        s.crossfade_duration_secs = self.crossfade_duration_secs;
        s.crossfade_curve = self.crossfade_curve;
        s.loudness_mode = self.loudness_mode;
        s.output_backend = self.output_backend;
        s.output_device = self.output_device.clone();
        s.sample_rate_policy = self.sample_rate_policy.clone();
        s.volume_mode = self.volume_mode;
        s.bit_perfect = self.bit_perfect;
        s.dither_enabled = self.dither_enabled;
        s.resampler_quality = self.resampler_quality;
        s.fallback_policy = self.fallback_policy;
        s.precision_mode = self.precision_mode;
        s.spatial_quality = self.spatial_quality;
        s.hrtf_profile = self.hrtf_profile.clone();
        s.listener_smoothing_ms = self.listener_smoothing_ms;
        s.listener_max_rate = self.listener_max_rate;
        s.room_enabled = self.room_enabled;
        s.room_width = self.room_width;
        s.room_depth = self.room_depth;
        s.room_height = self.room_height;
        s.room_absorption = self.room_absorption;
        s.room_rt60_ms = self.room_rt60_ms;
        s.room_wet = self.room_wet;
        s.air_absorption_enabled = self.air_absorption_enabled;
        s.voice_budget_enabled = self.voice_budget_enabled;
        s.voice_capacity = self.voice_capacity;
        s.stereo_enhancer_enabled = self.stereo_enhancer_enabled;
        s.loudness_write_tags = self.loudness_write_tags;
    }
}

/// Dual-tone horizontal slider matching the design system
pub fn dual_tone_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    width: f32,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(width, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    let min = *range.start();
    let max = *range.end();

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let raw_val = min + t * (max - min);
            let stepped_val = if step > 0.0 {
                ((raw_val / step).round() * step).clamp(min, max)
            } else {
                raw_val.clamp(min, max)
            };
            if (stepped_val - *value).abs() > 0.0001 {
                *value = stepped_val;
                changed = true;
            }
        }
    }

    // Background track
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter().rect_filled(track_rect, CornerRadius::same(2), theme.bg_input);

    // Active dual-tone filled track
    let thumb_t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let thumb_x = rect.left() + thumb_t * rect.width();
    let filled_w = (thumb_x - track_rect.min.x).max(0.0);
    if filled_w > 0.0 {
        let num_slices = (filled_w / 3.0).clamp(1.0, 40.0) as usize;
        for i in 0..num_slices {
            let t0 = i as f32 / num_slices as f32;
            let t1 = (i + 1) as f32 / num_slices as f32;
            let x0 = track_rect.min.x + filled_w * t0;
            let x1 = track_rect.min.x + filled_w * t1;
            let r = (236.0 * (1.0 - t0) + 129.0 * t0) as u8;
            let g = (72.0 * (1.0 - t0) + 140.0 * t0) as u8;
            let b = (153.0 * (1.0 - t0) + 248.0 * t0) as u8;
            let col = Color32::from_rgb(r, g, b);
            let slice_rect = Rect::from_min_max(
                Pos2::new(x0, track_rect.min.y),
                Pos2::new(x1, track_rect.max.y),
            );
            ui.painter().rect_filled(slice_rect, CornerRadius::same(if i == 0 { 2 } else { 0 }), col);
        }
    }

    // White thumb knob
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let is_hovered = resp.hovered() || resp.dragged();
    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 7.5 } else { 6.0 },
        Color32::from_rgba_premultiplied(129, 140, 248, if is_hovered { 80 } else { 40 }),
    );
    ui.painter().circle_filled(thumb_pos, 5.0, Color32::WHITE);
    ui.painter().circle_stroke(thumb_pos, 5.0, Stroke::new(1.2, theme.accent_magenta));

    changed
}

// ─────────────────────────────────────────────────────────────────────────────
// Actions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SettingsTabActions {
    // Playback
    pub speed_changed: Option<f32>,
    pub pitch_changed: Option<f32>,
    pub speed_mode_changed: Option<SpeedMode>,
    pub transition_mode_changed: Option<TransitionMode>,
    pub crossfade_config_changed: bool,
    pub crossfade_curve_changed: Option<CrossfadeCurve>,
    pub loudness_mode_changed: Option<LoudnessMode>,

    // Output
    pub backend_changed: Option<AudioBackend>,
    pub device_changed: Option<String>,
    pub sample_rate_policy_changed: Option<SampleRatePolicy>,
    pub volume_mode_changed: Option<VolumeMode>,
    pub bit_perfect_changed: Option<bool>,
    pub dither_changed: Option<bool>,
    pub resampler_quality_changed: Option<ResamplerQuality>,
    pub fallback_policy_changed: Option<FallbackPolicy>,
    pub precision_mode_changed: Option<PrecisionMode>,
    pub open_asio_panel: bool,
    pub clear_output_profile: bool,

    // Spatial
    pub spatial_quality_changed: Option<SpatialQuality>,
    pub hrtf_profile_changed: Option<String>,
    pub listener_tracking_changed: bool,
    pub spatial_room_changed: bool,
    pub spatial_screen_changed: bool,
    pub voice_budget_changed: bool,

    // Mix Bus
    pub add_track: Option<AudioSource>,
    pub remove_track: Option<u8>,
    pub track_gain_changed: Option<(u8, f32)>,
    pub track_pan_changed: Option<(u8, f32)>,
    pub track_mute_changed: Option<(u8, bool)>,
    pub track_active_changed: Option<(u8, bool)>,
    pub aux_changed: bool,
    pub duck_tracks: bool,

    // Capture
    pub start_capture: bool,
    pub stop_capture: bool,

    // Advanced
    pub stereo_enhancer_changed: Option<bool>,
    pub write_loudness_tags: Option<String>,
    pub recover_stream: bool,
    /// User pressed "Scan Library" — analyse every track lacking loudness data.
    pub scan_loudness_library: bool,
    /// User pressed "Cancel" during a loudness scan.
    pub cancel_loudness_scan: bool,

    // Library & Appearance
    pub trigger_scan: Option<PathBuf>,
    pub remove_scan_root: Option<ScanRootId>,
    pub toggle_theme: bool,
    /// User pressed "Import Songs…" — open the native file picker.
    pub import_files: bool,
    /// User pressed "Import Folder…" — open the native folder picker.
    pub import_folder: bool,
    /// User pressed "Open M3U…" — open the native playlist picker.
    pub import_playlist: bool,
    /// User pressed "Find Duplicates" — open the duplicate finder.
    pub find_duplicates: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Extra Context for rich view
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SettingsExtraContext<'a> {
    pub scan_roots: &'a [ScanRoot],
    pub track_count: usize,
    pub is_scanning: bool,
    pub is_dark_theme: bool,
    /// A native import picker is currently open (disables the import buttons).
    pub import_active: bool,
    /// Directories that individual songs were imported from, newest first.
    pub imported_dirs: &'a [PathBuf],
    /// Library-wide loudness scan is in progress.
    pub loudness_running: bool,
    /// Tracks analysed so far in the current/last loudness scan.
    pub loudness_done: usize,
    /// Total tracks queued in the current loudness scan.
    pub loudness_total: usize,
    /// Current/last loudness scan status line.
    pub loudness_current: &'a str,
    /// Whether any loudness scan has finished (successfully or not).
    pub loudness_has_run: bool,
    /// Round-trip output latency reported by the backend, in milliseconds.
    pub latency_ms: f32,
    /// Mean DSP graph-rebuild cost in milliseconds.
    pub graph_build_ms: f64,
    /// Number of DSP graph rebuilds since startup.
    pub graph_builds: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Entry Points
// ─────────────────────────────────────────────────────────────────────────────

pub fn show_settings_tab(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) -> SettingsTabActions {
    show_settings_tab_extended(ui, state, theme, &SettingsExtraContext::default())
}

pub fn show_settings_tab_extended(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
) -> SettingsTabActions {
    let mut actions = SettingsTabActions::default();

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 14.0);

        // 1. Top Category Segmented Tabs
        render_category_bar(ui, state, theme);

        // 2. Active Category Content in a smooth vertical scroll
        egui::ScrollArea::vertical()
            .id_salt("settings_main_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 16.0);

                match state.active_category {
                    SettingsCategory::Playback => {
                        render_playback_category(ui, state, theme, &mut actions);
                    }
                    SettingsCategory::Output => {
                        render_output_category(ui, state, theme, &mut actions);
                    }
                    SettingsCategory::Spatial => {
                        render_spatial_category(ui, state, theme, &mut actions);
                    }
                    SettingsCategory::MixBus => {
                        render_mixbus_category(ui, state, theme, &mut actions);
                    }
                    SettingsCategory::Tools => {
                        render_tools_category(ui, state, theme, extra, &mut actions);
                    }
                    SettingsCategory::Library => {
                        render_library_category(ui, state, theme, extra, &mut actions);
                    }
                }

                ui.add_space(32.0);
            });
    });

    actions
}

// ─────────────────────────────────────────────────────────────────────────────
// Category Bar
// ─────────────────────────────────────────────────────────────────────────────

fn render_category_bar(ui: &mut egui::Ui, state: &mut SettingsTabState, theme: &Theme) {
    let container_frame = egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::symmetric(5, 5));

    container_frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(5.0, 0.0);
            let num_cats = SettingsCategory::ALL.len() as f32;
            let tab_w = ((ui.available_width() - (num_cats - 1.0) * 5.0) / num_cats).max(80.0);

            for cat in SettingsCategory::ALL {
                let is_active = state.active_category == cat;

                let (tab_rect, resp) = ui.allocate_exact_size(Vec2::new(tab_w, 34.0), Sense::click());
                if resp.clicked() && !is_active {
                    state.active_category = cat;
                }

                let p = ui.painter_at(tab_rect);
                let bg = if is_active {
                    theme.accent_violet
                } else if resp.hovered() {
                    theme.bg_pill_hover
                } else {
                    theme.bg_pill
                };
                let border = if is_active {
                    theme.accent_magenta
                } else {
                    Color32::TRANSPARENT
                };

                p.rect(tab_rect, CornerRadius::same(8), bg, Stroke::new(1.0, border), egui::StrokeKind::Inside);

                let icon_color = if is_active { Color32::WHITE } else { theme.text_secondary };
                let text_color = if is_active { Color32::WHITE } else { theme.text_primary };

                // Center icon + text horizontally
                let icon_rect = Rect::from_center_size(
                    Pos2::new(tab_rect.min.x + 14.0, tab_rect.center().y),
                    Vec2::splat(14.0),
                );
                cat.draw_icon(ui, icon_rect, icon_color);

                p.text(
                    Pos2::new(tab_rect.min.x + 26.0, tab_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    cat.title(),
                    FontId::proportional(12.0),
                    text_color,
                );
            }
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 1: Playback & Engine
// ─────────────────────────────────────────────────────────────────────────────

fn render_playback_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Speed, Pitch & Mode
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_disc(ui, r, c),
            "Playback Speed & Pitch Transposition",
            "Adjust playback rate and pitch using high-precision time-stretching or vintage varispeed",
            Some((
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { "NORMAL" } else { "MODIFIED" },
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { theme.bg_pill } else { theme.accent_violet.linear_multiply(0.3) },
                if state.speed == 1.0 && state.pitch_semitones == 0.0 { theme.text_muted } else { theme.accent_pink },
            )),
            theme,
        );

        // Speed Slider + Presets
        setting_item(
            ui,
            "Playback Speed",
            "Tempo multiplier (0.25x slow down to 4.00x speed up)",
            theme,
            |ui| {
                let mut v = state.speed;
                if dual_tone_slider(ui, &mut v, 0.25..=4.0, 0.05, 180.0, theme) {
                    state.speed = v;
                    actions.speed_changed = Some(v);
                }
                badge_label(ui, format!("{:.2}x", state.speed), theme);
                if state.speed != 1.0 && ui.add(pill_btn("1.0x", false, theme)).clicked() {
                    state.speed = 1.0;
                    actions.speed_changed = Some(1.0);
                }
            },
        );

        // Pitch Transposition
        setting_item(
            ui,
            "Pitch Transposition",
            "Shift musical key in semitone increments (-24 to +24 semitones)",
            theme,
            |ui| {
                let mut v = state.pitch_semitones;
                if dual_tone_slider(ui, &mut v, -24.0..=24.0, 0.5, 180.0, theme) {
                    state.pitch_semitones = v;
                    actions.pitch_changed = Some(v);
                }
                badge_label(ui, format!("{:+.1} st", state.pitch_semitones), theme);
                if state.pitch_semitones != 0.0 && ui.add(pill_btn("Reset", false, theme)).clicked() {
                    state.pitch_semitones = 0.0;
                    actions.pitch_changed = Some(0.0);
                }
            },
        );

        // Speed Processing Mode
        setting_item_with_info(
            ui,
            "Processing Engine",
            "Varispeed (analog tape pitch shift) vs TimeStretch (formant-preserved tempo)",
            Some(&crate::components::info_box::INFO_SPEED_PITCH),
            theme,
            |ui| {
                let options = [
                    (SpeedMode::Varispeed, "Varispeed"),
                    (SpeedMode::TimeStretch, "TimeStretch"),
                    (SpeedMode::PitchShift, "PitchShift"),
                ];
                if segment_picker(ui, &mut state.speed_mode, &options, theme) {
                    actions.speed_mode_changed = Some(state.speed_mode);
                }
            },
        );
    });

    // Card 2: Track Transitions & Crossfade
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_shuffle(ui, r, c),
            "Track Transitions & Crossfading",
            "Continuous gapless audio streaming or smooth multi-second DJ crossfading between songs",
            Some((
                match state.transition_mode {
                    TransitionMode::Gapless => "GAPLESS DIRECT",
                    TransitionMode::Crossfade => "CROSSFADE ACTIVE",
                    TransitionMode::Fade => "FADE",
                    TransitionMode::Stop => "HARD STOP",
                },
                theme.bg_pill,
                theme.accent_violet,
            )),
            theme,
        );

        setting_item_with_info(
            ui,
            "Transition Mode",
            "Gapless mode provides seamless album playback without clicks or silence gaps",
            Some(&crate::components::info_box::INFO_TRANSITION_CROSSFADE),
            theme,
            |ui| {
                let options = [
                    (TransitionMode::Gapless, "Gapless (Audiophile)"),
                    (TransitionMode::Crossfade, "Crossfade"),
                    (TransitionMode::Fade, "Fade"),
                    (TransitionMode::Stop, "Stop"),
                ];
                if segment_picker(ui, &mut state.transition_mode, &options, theme) {
                    actions.transition_mode_changed = Some(state.transition_mode);
                }
            },
        );

        if state.transition_mode == TransitionMode::Crossfade {
            ui.add_space(4.0);
            let sub_frame = egui::Frame::NONE
                .fill(theme.bg_pill.linear_multiply(0.6))
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 12));

            sub_frame.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                setting_item(
                    ui,
                    "Crossfade Duration",
                    "Overlap time in seconds between fading out song and fading in next song",
                    theme,
                    |ui| {
                        let mut v = state.crossfade_duration_secs;
                        if dual_tone_slider(ui, &mut v, 0.5..=15.0, 0.1, 160.0, theme) {
                            state.crossfade_duration_secs = v;
                            actions.crossfade_config_changed = true;
                        }
                        badge_label(ui, format!("{:.1} s", state.crossfade_duration_secs), theme);
                    },
                );

                setting_item(
                    ui,
                    "Crossfade Curve",
                    "Constant Power maintains perceived volume energy; Linear creates gradual dip",
                    theme,
                    |ui| {
                        let options = [
                            (CrossfadeCurve::ConstantPower, "Constant Power (Equal Energy)"),
                            (CrossfadeCurve::Linear, "Linear"),
                            (CrossfadeCurve::SCurve, "S-Curve"),
                        ];
                        if segment_picker(ui, &mut state.crossfade_curve, &options, theme) {
                            actions.crossfade_curve_changed = Some(state.crossfade_curve);
                            actions.crossfade_config_changed = true;
                        }
                    },
                );
            });
        }
    });

    // Card 3: Loudness Normalization
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_bar_chart(ui, r, c),
            "Loudness Normalization (ReplayGain & EBU R128)",
            "Automated gain matching prevents sudden volume jumps between quiet acoustic and loud mastered tracks",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            "Target Standard",
            "EBU R128 (-14 LUFS) or legacy ReplayGain 2.0 tags",
            Some(&crate::components::info_box::INFO_LOUDNESS_NORMALIZATION),
            theme,
            |ui| {
                let options = [
                    (LoudnessMode::Off, "Disabled (Pure)"),
                    (LoudnessMode::TrackReplayGain, "Track ReplayGain"),
                    (LoudnessMode::AlbumReplayGain, "Album ReplayGain"),
                    (LoudnessMode::EbuR128, "EBU R128 Broadcast"),
                ];
                if segment_picker(ui, &mut state.loudness_mode, &options, theme) {
                    actions.loudness_mode_changed = Some(state.loudness_mode);
                }
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 2: Hardware & Output
// ─────────────────────────────────────────────────────────────────────────────

fn render_output_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Bit-Perfect Audiophile Mode Banner Card
    let bp_border = if state.bit_perfect {
        theme.bit_perfect_green
    } else {
        theme.border_card
    };
    let bp_fill = if state.bit_perfect {
        Color32::from_rgba_premultiplied(16, 185, 129, 20)
    } else {
        theme.bg_card
    };

    let bp_frame = egui::Frame::NONE
        .fill(bp_fill)
        .stroke(Stroke::new(1.2, bp_border))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(18, 16));

    bp_frame.show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
            ui.painter_at(icon_rect).rect_filled(
                icon_rect,
                CornerRadius::same(8),
                if state.bit_perfect { theme.bit_perfect_green } else { theme.bg_pill },
            );
            crate::components::icons::draw_speaker(
                ui,
                icon_rect.shrink2(Vec2::splat(6.0)),
                if state.bit_perfect { Color32::WHITE } else { theme.accent_magenta },
                false,
            );

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                    ui.label(
                        egui::RichText::new("Audiophile Bit-Perfect Direct Stream")
                            .font(FontId::proportional(15.0))
                            .strong()
                            .color(if state.bit_perfect { theme.bit_perfect_green } else { theme.text_primary }),
                    );
                    crate::components::info_box::render_info_icon_button(
                        ui,
                        "info_bit_perfect_banner",
                        &crate::components::info_box::INFO_BIT_PERFECT,
                        theme,
                    );
                });
                ui.label(
                    egui::RichText::new(
                        if state.bit_perfect {
                            "ACTIVE: Bypassing all OS audio mixers, EQ, resampling, and digital attenuation. Audio stream delivered byte-for-byte to your DAC."
                        } else {
                            "Enable to guarantee 100% bit-exact transmission to external DACs (disables DSP, EQ, and software volume)."
                        }
                    )
                    .font(FontId::proportional(11.5))
                    .color(theme.text_muted),
                );
            });

            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 32.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let mut bp = state.bit_perfect;
                    if toggle_switch(ui, &mut bp, theme) {
                        state.bit_perfect = bp;
                        actions.bit_perfect_changed = Some(bp);
                    }
                },
            );
        });
    });

    // Card 2: Driver Backend & Device
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            "Audio Driver Backend & Hardware Device",
            "Select hardware driver architecture and specific digital-to-analog converter",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            "Driver Architecture",
            "Exclusive ALSA, WASAPI Exclusive, or cross-platform automatic detection",
            Some(&crate::components::info_box::INFO_AUDIO_BACKEND),
            theme,
            |ui| {
                let options = [
                    (AudioBackend::Auto, "Auto Detect"),
                    (AudioBackend::ExclusiveAlsa, "ALSA Exclusive"),
                    (AudioBackend::ExclusiveWasapi, "WASAPI Exclusive"),
                    (AudioBackend::ExclusiveCoreAudioHog, "CoreAudio Hog"),
                    (AudioBackend::ExclusiveAsio, "ASIO Direct"),
                ];
                if segment_picker(ui, &mut state.output_backend, &options, theme) {
                    actions.backend_changed = Some(state.output_backend);
                }
            },
        );

        setting_item(
            ui,
            "Output Endpoint",
            "Leave blank for system default, or enter specific ALSA/WASAPI endpoint string",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.output_device)
                    .hint_text("Default output device...")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
                if ui.add(pill_btn("Apply Device", false, theme)).clicked() {
                    let dev = if state.output_device.is_empty() { None } else { Some(state.output_device.clone()) };
                    actions.device_changed = Some(dev.unwrap_or_default());
                }
            },
        );

        setting_item(
            ui,
            "Fallback Policy",
            "Action to take if exclusive mode device acquisition is rejected by the OS",
            theme,
            |ui| {
                let options = [
                    (FallbackPolicy::Allow, "Allow Shared Fallback"),
                    (FallbackPolicy::Strict, "Strict (Halt if Exclusive Fails)"),
                ];
                if segment_picker(ui, &mut state.fallback_policy, &options, theme) {
                    actions.fallback_policy_changed = Some(state.fallback_policy);
                }
            },
        );

        setting_item(
            ui,
            "Hardware Actions",
            "Open external driver control panels and reset device configuration profiles",
            theme,
            |ui| {
                if ui.add(pill_btn("Open ASIO Panel", false, theme)).clicked() {
                    actions.open_asio_panel = true;
                }
                if ui.add(pill_btn("Reset Device Profile", false, theme)).clicked() {
                    actions.clear_output_profile = true;
                }
            },
        );
    });

    // Card 3: Sample Rate Policy & Resampling
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_equalizer_sliders(ui, r, c),
            "Sample Rate Policies & Resampling Precision",
            "Lock output sample rate or dynamically match input track sample rate",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            "Sample Rate Target",
            "Follow Track switches your DAC rate on song change; Fixed resamples everything",
            Some(&crate::components::info_box::INFO_SAMPLE_RATE_POLICY),
            theme,
            |ui| {
                let options = [
                    (SampleRatePolicy::FollowTrack, "Follow Track (Bit-Exact)"),
                    (SampleRatePolicy::DeviceDefault, "Device Native"),
                    (SampleRatePolicy::Fixed(44100), "44.1 kHz (CD)"),
                    (SampleRatePolicy::Fixed(48000), "48 kHz"),
                    (SampleRatePolicy::Fixed(96000), "96 kHz (Hi-Res)"),
                    (SampleRatePolicy::Fixed(192000), "192 kHz"),
                ];
                if segment_picker(ui, &mut state.sample_rate_policy, &options, theme) {
                    actions.sample_rate_policy_changed = Some(state.sample_rate_policy.clone());
                }
            },
        );

        setting_item_with_info(
            ui,
            "Resampler Engine",
            "Polyphase FIR filter quality used when rate conversion is required",
            Some(&crate::components::info_box::INFO_RESAMPLER_QUALITY),
            theme,
            |ui| {
                let options = [
                    (ResamplerQuality::Fast, "Fast (Low CPU)"),
                    (ResamplerQuality::Balanced, "Balanced"),
                    (ResamplerQuality::HighQuality, "High Quality (Linear Phase)"),
                    (ResamplerQuality::Ultra, "Ultra (Audiophile Sinc)"),
                ];
                if segment_picker(ui, &mut state.resampler_quality, &options, theme) {
                    actions.resampler_quality_changed = Some(state.resampler_quality);
                }
            },
        );

        setting_item(
            ui,
            "DSP Precision Mode",
            "Internal processing precision. Quality routes the mix chain through the f64 core for extra headroom at a small CPU cost.",
            theme,
            |ui| {
                let mut mode = state.precision_mode.unwrap_or_default();
                let options = [
                    (PrecisionMode::Performance, "Performance (f32)"),
                    (PrecisionMode::Quality, "Quality (f64 core)"),
                ];
                if segment_picker(ui, &mut mode, &options, theme) {
                    state.precision_mode = Some(mode);
                    actions.precision_mode_changed = Some(mode);
                }
            },
        );

        setting_item_with_info(
            ui,
            "Volume Attenuation Mode",
            "Software volume scaling vs Fixed 100% line-out for external preamp/headphone amp",
            Some(&crate::components::info_box::INFO_VOLUME_MODE),
            theme,
            |ui| {
                let options = [
                    (VolumeMode::SoftwareOnly, "Software Controlled"),
                    (VolumeMode::HardwarePreferred, "Hardware Preferred"),
                ];
                if segment_picker(ui, &mut state.volume_mode, &options, theme) {
                    actions.volume_mode_changed = Some(state.volume_mode);
                }
            },
        );

        setting_item_with_info(
            ui,
            "TPDF Dither",
            "Triangular PDF dithering eliminates quantization distortion when converting bit depth",
            Some(&crate::components::info_box::INFO_DITHER),
            theme,
            |ui| {
                let mut d = state.dither_enabled;
                if toggle_switch(ui, &mut d, theme) {
                    state.dither_enabled = d;
                    actions.dither_changed = Some(d);
                }
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 3: Spatial & Acoustics
// ─────────────────────────────────────────────────────────────────────────────

fn render_spatial_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Card 1: 3D Soundstage Radar Canvas
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_bar_chart(ui, r, c),
            "3D Soundstage & Spatial Radar (Interactive)",
            "Drag the virtual sound source across the circular acoustic field to position audio in 3D space",
            None,
            theme,
        );

        ui.vertical_centered(|ui| {
            let radar_resp = crate::widgets::SpatialRadarWidget::new(
                &mut state.source_azimuth_deg,
                &mut state.source_distance_m,
                state.screen_center_az,
                theme,
            )
            .room_dimensions(state.room_width, state.room_depth)
            .size(Vec2::new(ui.available_width().min(480.0), 240.0))
            .show(ui);

            if radar_resp.changed {
                state.screen_center_az = state.source_azimuth_deg;
                actions.spatial_screen_changed = true;
            }
        });

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);
            badge_label(ui, format!("Source Azimuth: {:+.1}°", state.source_azimuth_deg), theme);
            badge_label(ui, format!("Source Distance: {:.2} m", state.source_distance_m), theme);
            badge_label(ui, format!("Virtual Center: {:+.1}°", state.screen_center_az), theme);
        });
    });

    // Card 2: HRTF & Precision
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_disc(ui, r, c),
            "Binaural HRTF & Head Tracking",
            "Head-Related Transfer Function filters model human pinna and ear-canal acoustics for headphones",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            "Spatial Synthesis Quality",
            "Spherical harmonics order and convolution precision",
            Some(&crate::components::info_box::INFO_SPATIAL_HRTF),
            theme,
            |ui| {
                let options = [
                    (SpatialQuality::Low, "Low"),
                    (SpatialQuality::Medium, "Medium"),
                    (SpatialQuality::High, "High (Recommended)"),
                    (SpatialQuality::Ultra, "Ultra High-Res"),
                ];
                if segment_picker(ui, &mut state.spatial_quality, &options, theme) {
                    actions.spatial_quality_changed = Some(state.spatial_quality);
                }
            },
        );

        setting_item(
            ui,
            "HRTF Profile ID",
            "SOFA dataset profile identifier (e.g. KEMAR_diffuse, IRC_1002)",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.hrtf_profile)
                    .hint_text("Default KEMAR...")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
                if ui.add(pill_btn("Apply Profile", false, theme)).clicked() {
                    actions.hrtf_profile_changed = Some(state.hrtf_profile.clone());
                }
            },
        );

        setting_item_with_info(
            ui,
            "Listener Motion Smoothing",
            "Low-pass filter time constant for head motion tracker interpolation",
            Some(&crate::components::info_box::INFO_LISTENER_TRACKING),
            theme,
            |ui| {
                let mut v = state.listener_smoothing_ms;
                if dual_tone_slider(ui, &mut v, 0.0..=500.0, 5.0, 160.0, theme) {
                    state.listener_smoothing_ms = v;
                    actions.listener_tracking_changed = true;
                }
                badge_label(ui, format!("{:.0} ms", state.listener_smoothing_ms), theme);
            },
        );

        setting_item(
            ui,
            "Listener Max Angular Velocity",
            "Clamps extreme head rotations (0 = uncapped natural tracking)",
            theme,
            |ui| {
                let mut r = state.listener_max_rate;
                if dual_tone_slider(ui, &mut r, 0.0..=720.0, 10.0, 160.0, theme) {
                    state.listener_max_rate = r;
                    actions.listener_tracking_changed = true;
                }
                badge_label(ui, format!("{:.0} °/s", state.listener_max_rate), theme);
            },
        );
    });

    // Card 3: Virtual Room Acoustics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_folder(ui, r, c),
            "Virtual Room Acoustics Simulation",
            "Simulates listening room wall reflections, RT60 reverberation decay, and air absorption",
            Some((
                if state.room_enabled { "REVERB ACTIVE" } else { "BYPASSED" },
                theme.bg_pill,
                if state.room_enabled { theme.accent_violet } else { theme.text_muted },
            )),
            theme,
        );

        setting_item_with_info(
            ui,
            "Enable Room Acoustics",
            "Apply virtual listening room reverberation model",
            Some(&crate::components::info_box::INFO_ROOM_ACOUSTICS),
            theme,
            |ui| {
                let mut on = state.room_enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.room_enabled = on;
                    actions.spatial_room_changed = true;
                }
            },
        );

        if state.room_enabled {
            ui.add_space(4.0);
            let room_grid = egui::Frame::NONE
                .fill(theme.bg_pill.linear_multiply(0.6))
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 12));

            room_grid.show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                setting_item(
                    ui,
                    "Reverb Wet/Dry Mix",
                    "Ratio of reverberant room reflection to direct dry audio",
                    theme,
                    |ui| {
                        let mut v = state.room_wet;
                        if dual_tone_slider(ui, &mut v, 0.0..=1.0, 0.01, 160.0, theme) {
                            state.room_wet = v;
                            actions.spatial_room_changed = true;
                        }
                        badge_label(ui, format!("{:.0}%", state.room_wet * 100.0), theme);
                    },
                );

                setting_item(
                    ui,
                    "RT60 Decay Time",
                    "Time required for acoustic reflections to decay by 60 dB",
                    theme,
                    |ui| {
                        let mut v = state.room_rt60_ms;
                        if dual_tone_slider(ui, &mut v, 50.0..=3000.0, 25.0, 160.0, theme) {
                            state.room_rt60_ms = v;
                            actions.spatial_room_changed = true;
                        }
                        badge_label(ui, format!("{:.0} ms", state.room_rt60_ms), theme);
                    },
                );

                setting_item(
                    ui,
                    "Room Dimensions (W x D x H)",
                    "Physical space modeling in meters",
                    theme,
                    |ui| {
                        ui.label("W:");
                        let mut w = state.room_width;
                        if ui.add(egui::DragValue::new(&mut w).speed(0.1).range(1.0..=30.0)).changed() {
                            state.room_width = w;
                            actions.spatial_room_changed = true;
                        }
                        ui.label("D:");
                        let mut d = state.room_depth;
                        if ui.add(egui::DragValue::new(&mut d).speed(0.1).range(1.0..=30.0)).changed() {
                            state.room_depth = d;
                            actions.spatial_room_changed = true;
                        }
                        ui.label("H:");
                        let mut h = state.room_height;
                        if ui.add(egui::DragValue::new(&mut h).speed(0.1).range(1.5..=10.0)).changed() {
                            state.room_height = h;
                            actions.spatial_room_changed = true;
                        }
                    },
                );
            });
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 4: Mix Bus & DSP
// ─────────────────────────────────────────────────────────────────────────────

fn render_mixbus_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Multitrack Mixer Strip
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_equalizer_sliders(ui, r, c),
            "Multitrack Lane Mixer",
            "Adjust individual input tracks, gain attenuation, stereo panning, and mute states",
            None,
            theme,
        );

        setting_item(
            ui,
            "Add Audio Source File",
            "Ingest secondary stems, reference tracks, or voice commentary channels",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.add_track_path)
                    .hint_text("Absolute path to audio stem...")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
                if ui.add(pill_btn("+ Add Stem", false, theme)).clicked() && !state.add_track_path.is_empty() {
                    actions.add_track = Some(AudioSource::File(state.add_track_path.clone().into()));
                }
            },
        );

        setting_item(
            ui,
            "Active Channel Slot",
            "Select mixer channel strip to control",
            theme,
            |ui| {
                let mut slot = state.track_slot as f32;
                if ui.add(egui::DragValue::new(&mut slot).range(2.0..=16.0)).changed() {
                    state.track_slot = slot as u8;
                }
                badge_label(ui, format!("Slot #{}", state.track_slot), theme);
                if ui.add(pill_btn("Remove Slot", false, theme)).clicked() {
                    actions.remove_track = Some(state.track_slot);
                }
            },
        );

        setting_item(
            ui,
            "Channel Gain & Pan",
            "Fine-tune channel volume level and stereo pan balance",
            theme,
            |ui| {
                ui.label("Gain:");
                let mut g = state.track_gain;
                if dual_tone_slider(ui, &mut g, 0.0..=2.0, 0.01, 110.0, theme) {
                    state.track_gain = g;
                    actions.track_gain_changed = Some((state.track_slot, g));
                }
                badge_label(ui, format!("{:.2}x", state.track_gain), theme);

                ui.label("Pan:");
                let mut p = state.track_pan;
                if dual_tone_slider(ui, &mut p, -1.0..=1.0, 0.02, 110.0, theme) {
                    state.track_pan = p;
                    actions.track_pan_changed = Some((state.track_slot, p));
                }
                badge_label(ui, format!("{:+.2}", state.track_pan), theme);
            },
        );

        setting_item(
            ui,
            "Channel Status",
            "Mute and active playback enable state",
            theme,
            |ui| {
                let mut m = state.track_muted;
                if ui.checkbox(&mut m, "Mute").changed() {
                    state.track_muted = m;
                    actions.track_mute_changed = Some((state.track_slot, m));
                }
                let mut a = state.track_active;
                if ui.checkbox(&mut a, "Active").changed() {
                    state.track_active = a;
                    actions.track_active_changed = Some((state.track_slot, a));
                }
            },
        );
    });

    // Card 2: Sidechain Ducking Compressor
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_trending_up(ui, r, c),
            "Sidechain Ducking Compressor",
            "Automatically reduces background music level when voice or commentary channel speaks",
            None,
            theme,
        );

        setting_item(
            ui,
            "Trigger Source Slot",
            "Mixer channel monitored to trigger ducking attenuation",
            theme,
            |ui| {
                let mut slot = state.duck_source_slot as f32;
                if ui.add(egui::DragValue::new(&mut slot).range(0.0..=15.0)).changed() {
                    state.duck_source_slot = slot as u8;
                }
                badge_label(ui, format!("Source #{}", state.duck_source_slot), theme);
            },
        );

        setting_item_with_info(
            ui,
            "Threshold & Depth",
            "Level that activates ducking (dB) and total volume reduction depth (dB)",
            Some(&crate::components::info_box::INFO_DUCKING),
            theme,
            |ui| {
                ui.label("Threshold:");
                let mut t = state.duck_threshold_db;
                if ui.add(egui::DragValue::new(&mut t).speed(0.5).suffix(" dB").range(-60.0..=0.0)).changed() {
                    state.duck_threshold_db = t;
                }
                ui.label("Depth:");
                let mut d = state.duck_depth_db;
                if ui.add(egui::DragValue::new(&mut d).speed(0.5).suffix(" dB").range(0.0..=30.0)).changed() {
                    state.duck_depth_db = d;
                }
            },
        );

        setting_item(
            ui,
            "Attack & Release Envelope",
            "Reaction speed (ms) and recovery return time (ms)",
            theme,
            |ui| {
                ui.label("Attack:");
                let mut a = state.duck_attack_ms;
                if ui.add(egui::DragValue::new(&mut a).speed(1.0).suffix(" ms").range(1.0..=100.0)).changed() {
                    state.duck_attack_ms = a;
                }
                ui.label("Release:");
                let mut r = state.duck_release_ms;
                if ui.add(egui::DragValue::new(&mut r).speed(5.0).suffix(" ms").range(10.0..=1000.0)).changed() {
                    state.duck_release_ms = r;
                }
                if ui.add(pill_btn("Apply Ducking Matrix", false, theme)).clicked() {
                    actions.duck_tracks = true;
                }
            },
        );
    });

    // Card 3: Stereo Image Enhancer & Aux
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_bar_chart(ui, r, c),
            "Auxiliary Bus & Psychoacoustic Enhancer",
            "Stereo soundstage widening algorithm and parallel auxiliary return loop",
            None,
            theme,
        );

        setting_item_with_info(
            ui,
            "Stereo Field Enhancer",
            "Mid-side psychoacoustic stereo image width expander",
            Some(&crate::components::info_box::INFO_STEREO_WIDTH),
            theme,
            |ui| {
                let mut on = state.stereo_enhancer_enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.stereo_enhancer_enabled = on;
                    actions.stereo_enhancer_changed = Some(on);
                }
            },
        );

        setting_item(
            ui,
            "Auxiliary Return Bus",
            "Enable parallel effects return loop with dedicated return gain",
            theme,
            |ui| {
                let mut on = state.aux_enabled;
                if toggle_switch(ui, &mut on, theme) {
                    state.aux_enabled = on;
                    actions.aux_changed = true;
                }
                if state.aux_enabled {
                    ui.label("Return Gain:");
                    let mut v = state.aux_return_gain;
                    if dual_tone_slider(ui, &mut v, 0.0..=2.0, 0.01, 120.0, theme) {
                        state.aux_return_gain = v;
                        actions.aux_changed = true;
                    }
                    badge_label(ui, format!("{:.2}x", state.aux_return_gain), theme);
                }
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 5: Capture & Tools
// ─────────────────────────────────────────────────────────────────────────────

fn render_tools_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Direct System Audio Capture
    setting_card(ui, theme, |ui| {
        card_header_with_info(
            ui,
            |ui, r, c| crate::components::icons::draw_speaker(ui, r, c, false),
            "High-Resolution Stream Capture",
            "Record live bit-perfect audio stream directly to uncompressed PCM WAV files",
            None,
            Some(&crate::components::info_box::INFO_STREAM_CAPTURE),
            theme,
        );

        setting_item(
            ui,
            "Output Recording Path",
            "Destination WAV file on local storage",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.capture_path)
                    .hint_text("capture.wav...")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
            },
        );

        setting_item(
            ui,
            "Input / Loopback Device",
            "Capture endpoint (leave blank for loopback master monitor)",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.capture_device)
                    .hint_text("Default capture endpoint...")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
            },
        );

        setting_item(
            ui,
            "Recording Controls",
            "Start or finalize audio recording file",
            theme,
            |ui| {
                if ui.add(
                    egui::Button::new(
                        egui::RichText::new("Start Capture")
                            .font(FontId::proportional(12.0))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent_violet)
                    .corner_radius(CornerRadius::same(8)),
                ).clicked() {
                    actions.start_capture = true;
                }

                if ui.add(
                    egui::Button::new(
                        egui::RichText::new("Stop Capture")
                            .font(FontId::proportional(12.0))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(220, 38, 38))
                    .corner_radius(CornerRadius::same(8)),
                ).clicked() {
                    actions.stop_capture = true;
                }
            },
        );
    });

    // Card 2: Loudness Tag Writer & ReplayGain Scanner
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_list_icon(ui, r, c),
            "ReplayGain & Loudness Tagging Tool",
            "Batch analyze audio files and permanently write ID3/FLAC loudness normalization tags",
            None,
            theme,
        );

        setting_item(
            ui,
            "Target File Path",
            "Path to audio file to measure integrated LUFS and inject tags",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.loudness_tag_path)
                    .hint_text("/path/to/song.flac…")
                    .desired_width(180.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
                if ui.add(pill_btn("Write Tags", false, theme)).clicked() && !state.loudness_tag_path.is_empty() {
                    actions.write_loudness_tags = Some(state.loudness_tag_path.clone());
                }
            },
        );

        setting_item(
            ui,
            "Library ReplayGain Scan",
            "Analyse every track that has no loudness data (EBU R128 + ReplayGain 2.0). Results are cached for playback.",
            theme,
            |ui| {
                if extra.loudness_running {
                    let total = extra.loudness_total.max(1);
                    let frac = extra.loudness_done as f32 / total as f32;
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(150.0)
                            .text(format!("{}/{}", extra.loudness_done, extra.loudness_total)),
                    );
                    if ui.add(pill_btn("Cancel", true, theme)).clicked() {
                        actions.cancel_loudness_scan = true;
                    }
                } else {
                    let label = if extra.loudness_has_run {
                        "Rescan Library"
                    } else {
                        "Scan Library"
                    };
                    if ui.add(pill_btn(label, false, theme)).clicked() {
                        actions.scan_loudness_library = true;
                    }
                }
            },
        );

        setting_item(
            ui,
            "Write Tags Into Files",
            "Also save the measured ReplayGain / EBU R128 values into each file's tags (FLAC, MP3, M4A, WAV …). Off by default — this is the only setting that modifies your audio files; DSD and raw PCM are skipped.",
            theme,
            |ui| {
                toggle_switch(ui, &mut state.loudness_write_tags, theme);
            },
        );

        if !extra.loudness_current.is_empty() {
            ui.label(
                egui::RichText::new(extra.loudness_current)
                    .font(FontId::proportional(11.0))
                    .color(theme.text_muted),
            );
        }
    });

    // Card 3: Stream Recovery & Diagnostics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_gear(ui, r, c),
            "Audio Engine Stream Recovery",
            "Re-arm playback queues, drain ringbuffers, and recover from hardware USB/DAC disconnects",
            None,
            theme,
        );

        setting_item(
            ui,
            "Stream Recovery",
            "Force re-initializes output pipeline without restarting ShadowPlayer",
            theme,
            |ui| {
                if ui.add(
                    egui::Button::new(
                        egui::RichText::new("Recover Stream & Re-arm")
                            .font(FontId::proportional(12.0))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent_violet)
                    .corner_radius(CornerRadius::same(8)),
                ).clicked() {
                    actions.recover_stream = true;
                }
            },
        );
    });

    // Card 4: Runtime Diagnostics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_gear(ui, r, c),
            "Runtime Diagnostics",
            "Live figures measured by the audio engine's output path",
            None,
            theme,
        );

        setting_item(
            ui,
            "Output Latency",
            "Round-trip buffering between the DSP graph and the hardware device",
            theme,
            |ui| {
                badge_label(ui, format!("{:.1} ms", extra.latency_ms), theme);
            },
        );

        setting_item(
            ui,
            "DSP Graph Rebuilds",
            "Mean cost of rebuilding the processing graph (codec, sample-rate or routing change)",
            theme,
            |ui| {
                badge_label(
                    ui,
                    format!("{:.2} ms mean · {} total", extra.graph_build_ms, extra.graph_builds),
                    theme,
                );
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Category 6: Library & Appearance
// ─────────────────────────────────────────────────────────────────────────────

fn render_library_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    // Card 1: Theme & Visual Aesthetics
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_moon(ui, r, c),
            "Theme & Visual Aesthetics",
            "Customize the color scheme, dark glassmorphism effects, and interface styling",
            Some((
                if extra.is_dark_theme { "DARK GLASS" } else { "LIGHT MODE" },
                theme.bg_pill,
                theme.accent_violet,
            )),
            theme,
        );

        setting_item(
            ui,
            "Theme Palette",
            "Switch between Dark Glassmorphic and Clean Light mode",
            theme,
            |ui| {
                let lbl = if extra.is_dark_theme { "Dark Theme Active (Switch to Light)" } else { "Light Theme Active (Switch to Dark)" };
                if ui.add(
                    egui::Button::new(
                        egui::RichText::new(lbl)
                            .font(FontId::proportional(12.5))
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(theme.accent_violet)
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(160.0, 30.0)),
                ).clicked() {
                    actions.toggle_theme = true;
                }
            },
        );

        setting_item(
            ui,
            "Signature Palette",
            "Curated colors: Violet (#8B5CF6), Magenta (#EC4899), Pink (#F43F5E), Emerald (#10B981)",
            theme,
            |ui| {
                color_dot(ui, theme.accent_violet, "Violet");
                color_dot(ui, theme.accent_magenta, "Magenta");
                color_dot(ui, theme.accent_pink, "Pink");
                color_dot(ui, theme.bit_perfect_green, "Emerald");
            },
        );
    });

    // Card 2: Music Library & Monitored Directories
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_folder(ui, r, c),
            "Music Library Storage & Directories",
            "Manage monitored folders and scan for new audio files, albums, and embedded artwork",
            Some((
                if extra.is_scanning { "SCANNING..." } else { "READY" },
                if extra.is_scanning { theme.accent_magenta.linear_multiply(0.4) } else { theme.bg_pill },
                if extra.is_scanning { Color32::WHITE } else { theme.text_secondary },
            )),
            theme,
        );

        setting_item(
            ui,
            "Cataloged Tracks",
            "Indexed audio files in local SQLite library database",
            theme,
            |ui| {
                badge_label(ui, format!("{} tracks indexed", extra.track_count), theme);
            },
        );

        setting_item(
            ui,
            "Import Music",
            "Use the system file picker to add songs or whole folders to the library",
            theme,
            |ui| {
                ui.add_enabled_ui(!extra.import_active, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                        if ui.add(pill_btn("Import Songs…", true, theme)).clicked() {
                            actions.import_files = true;
                        }
                        if ui.add(pill_btn("Import Folder…", false, theme)).clicked() {
                            actions.import_folder = true;
                        }
                        if ui.add(pill_btn("Open M3U…", false, theme)).clicked() {
                            actions.import_playlist = true;
                        }
                        if extra.import_active {
                            ui.label(
                                egui::RichText::new("waiting for file picker…")
                                    .font(FontId::proportional(11.5))
                                    .color(theme.text_muted),
                            );
                        }
                    });
                });
            },
        );

        setting_item(
            ui,
            "Find Duplicate Tracks",
            "Group tracks with identical audio content (matched by fingerprint, not tags) and remove the extra copies",
            theme,
            |ui| {
                if ui.add(pill_btn("Find Duplicates…", false, theme)).clicked() {
                    actions.find_duplicates = true;
                }
            },
        );

        setting_item(
            ui,
            "Or Type a Directory",
            "Advanced: index a folder by typing its absolute path",
            theme,
            |ui| {
                let edit = egui::TextEdit::singleline(&mut state.new_scan_path)
                    .hint_text("/home/user/Music...")
                    .desired_width(200.0)
                    .font(FontId::proportional(12.0));
                ui.add(edit);
                if ui.add(pill_btn("+ Add & Scan", false, theme)).clicked() && !state.new_scan_path.is_empty() {
                    let path = PathBuf::from(state.new_scan_path.trim());
                    if path.exists() {
                        actions.trigger_scan = Some(path);
                        state.new_scan_path.clear();
                    }
                }
            },
        );

        if !extra.imported_dirs.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Imported File Locations:")
                    .font(FontId::proportional(12.0))
                    .strong()
                    .color(theme.text_secondary),
            );
            for dir in extra.imported_dirs {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    let (folder_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    crate::components::icons::draw_folder(ui, folder_rect, theme.accent_pink);
                    ui.label(
                        egui::RichText::new(dir.to_string_lossy())
                            .font(FontId::proportional(12.0))
                            .color(theme.text_primary),
                    );
                });
            }
        }

        if !extra.scan_roots.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Monitored Music Folders:")
                    .font(FontId::proportional(12.0))
                    .strong()
                    .color(theme.text_secondary),
            );

            for root in extra.scan_roots {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    let (folder_rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
                    crate::components::icons::draw_folder(ui, folder_rect, theme.accent_violet);
                    ui.label(
                        egui::RichText::new(root.path.to_string_lossy())
                            .font(FontId::proportional(12.0))
                            .color(theme.text_primary),
                    );

                    let path_clone = root.path.clone();
                    let id_clone = root.id;
                    if ui.add(pill_btn("Rescan", false, theme)).clicked() {
                        actions.trigger_scan = Some(path_clone);
                    }
                    if ui.add(pill_btn("Remove", false, theme)).clicked() {
                        actions.remove_scan_root = Some(id_clone);
                    }
                });
            }
        }
    });

    // Card 3: About ShadowPlayer
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            |ui, r, c| crate::components::icons::draw_logo(ui, r, c),
            "About ShadowPlayer",
            "Audiophile Local Desktop Music Player",
            Some((concat!("v", env!("CARGO_PKG_VERSION")), theme.bg_pill, theme.accent_violet)),
            theme,
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(16.0, 0.0);
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 48.0), Sense::hover());
            ui.painter_at(logo_rect).rect_filled(
                logo_rect,
                CornerRadius::same(12),
                theme.bg_pill,
            );
            crate::components::icons::draw_logo(ui, logo_rect.shrink2(Vec2::splat(6.0)), theme.accent_pink);

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);
                ui.label(
                    egui::RichText::new("ShadowPlayer")
                        .font(FontId::proportional(15.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new("Music for a better you - Audiophile Local Desktop Music Player")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
                ui.label(
                    egui::RichText::new("Engine: Audio Engine v1.2.1 | Framework: egui v0.37.0 / eframe | SQLite FTS5")
                        .font(FontId::proportional(11.0))
                        .color(theme.text_muted),
                );
            });
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Primitives & Card Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn setting_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let card_frame = egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(egui::Margin::symmetric(20, 16));

    card_frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 12.0);
        add_contents(ui);
    });
}

pub fn card_header(
    ui: &mut egui::Ui,
    icon_fn: impl FnOnce(&egui::Ui, Rect, Color32),
    title: &str,
    subtitle: &str,
    status_badge: Option<(&str, Color32, Color32)>,
    theme: &Theme,
) {
    card_header_with_info(ui, icon_fn, title, subtitle, status_badge, None, theme);
}

pub fn card_header_with_info(
    ui: &mut egui::Ui,
    icon_fn: impl FnOnce(&egui::Ui, Rect, Color32),
    title: &str,
    subtitle: &str,
    status_badge: Option<(&str, Color32, Color32)>,
    info: Option<&crate::components::info_box::InfoBoxContent>,
    theme: &Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
        ui.painter_at(icon_rect).rect_filled(icon_rect, CornerRadius::same(8), theme.bg_pill);
        icon_fn(ui, icon_rect.shrink2(Vec2::splat(7.0)), theme.accent_magenta);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                ui.label(
                    egui::RichText::new(title)
                        .font(FontId::proportional(14.5))
                        .strong()
                        .color(theme.text_primary),
                );
                if let Some(info_content) = info {
                    let id_str = format!("card_header_info_{}", title);
                    crate::components::info_box::render_info_icon_button(ui, id_str, info_content, theme);
                }
                if let Some((badge_text, bg, fg)) = status_badge {
                    egui::Frame::NONE
                        .fill(bg)
                        .stroke(Stroke::new(1.0, fg.linear_multiply(0.4)))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(badge_text)
                                    .font(FontId::proportional(10.5))
                                    .strong()
                                    .color(fg),
                            );
                        });
                }
            });
            if !subtitle.is_empty() {
                ui.label(
                    egui::RichText::new(subtitle)
                        .font(FontId::proportional(11.5))
                        .color(theme.text_muted),
                );
            }
        });
    });

    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

pub fn setting_item(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    theme: &Theme,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    setting_item_with_info(ui, label, description, None, theme, add_control);
}

pub fn setting_item_with_info(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    info: Option<&crate::components::info_box::InfoBoxContent>,
    theme: &Theme,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(20.0, 0.0);
        let avail_w = ui.available_width();
        let label_w = 240.0_f32.min(avail_w * 0.40);

        // Column 1: Label + info icon + description
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 24.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                    ui.label(
                        egui::RichText::new(label)
                            .font(FontId::proportional(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    if let Some(info_content) = info {
                        let id_str = format!("setting_info_{}", label);
                        crate::components::info_box::render_info_icon_button(ui, id_str, info_content, theme);
                    }
                });
                if !description.is_empty() {
                    ui.label(
                        egui::RichText::new(description)
                            .font(FontId::proportional(11.0))
                            .color(theme.text_muted),
                    );
                }
            },
        );

        // Column 2: Controls flow left-to-right naturally!
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            add_control(ui);
        });
    });
}

pub fn toggle_switch(ui: &mut egui::Ui, value: &mut bool, theme: &Theme) -> bool {
    let size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *value = !*value;
        changed = true;
    }
    let painter = ui.painter_at(rect);
    let bg_color = if *value {
        theme.accent_violet
    } else if resp.hovered() {
        theme.bg_pill.linear_multiply(1.2)
    } else {
        theme.bg_pill
    };
    let border_stroke = if *value {
        Stroke::new(1.0, theme.accent_magenta)
    } else {
        Stroke::new(1.0, theme.border_card)
    };
    painter.rect(rect, CornerRadius::same(10), bg_color, border_stroke, egui::StrokeKind::Inside);

    let radius = 7.0;
    let knob_center_x = if *value {
        rect.max.x - radius - 3.0
    } else {
        rect.min.x + radius + 3.0
    };
    let knob_pos = Pos2::new(knob_center_x, rect.center().y);
    let knob_color = if *value {
        Color32::WHITE
    } else {
        theme.text_secondary
    };
    painter.circle_filled(knob_pos, radius, knob_color);

    changed
}

pub fn segment_picker<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    selected: &mut T,
    options: &[(T, &str)],
    theme: &Theme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (val, lbl) in options {
            let is_sel = selected == val;
            let btn = egui::Button::new(
                egui::RichText::new(*lbl)
                    .font(FontId::proportional(11.8))
                    .color(if is_sel { Color32::WHITE } else { theme.text_secondary }),
            )
            .fill(if is_sel { theme.accent_violet } else { theme.bg_pill })
            .stroke(if is_sel {
                Stroke::new(1.0, theme.accent_magenta)
            } else {
                Stroke::new(1.0, theme.border_card)
            })
            .corner_radius(CornerRadius::same(7))
            .min_size(Vec2::new(0.0, 26.0));

            if ui.add(btn).clicked() && !is_sel {
                *selected = val.clone();
                changed = true;
            }
        }
    });
    changed
}

fn pill_btn(label: &str, selected: bool, theme: &Theme) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.to_owned())
            .font(FontId::proportional(11.5))
            .color(if selected { Color32::WHITE } else { theme.text_secondary }),
    )
    .fill(if selected { theme.accent_violet } else { theme.bg_pill })
    .stroke(Stroke::new(1.0, theme.border_card))
    .corner_radius(CornerRadius::same(7))
    .min_size(Vec2::new(0.0, 24.0))
}

fn badge_label(ui: &mut egui::Ui, text: impl Into<String>, theme: &Theme) {
    let t = text.into();
    egui::Frame::NONE
        .fill(theme.bg_pill)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t)
                    .font(FontId::proportional(11.5))
                    .strong()
                    .color(theme.text_primary),
            );
        });
}

fn color_dot(ui: &mut egui::Ui, color: Color32, tooltip: &str) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::hover());
    ui.painter_at(rect).circle_filled(rect.center(), 7.0, color);
    resp.on_hover_text(tooltip);
}

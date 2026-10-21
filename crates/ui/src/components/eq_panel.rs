use crate::theme::Theme;
use config::{CrossfeedProfile, GraphicEqLayout};
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2, Window};
use engine::dsp::equalizer::EqFilterType;
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Presets
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EqPreset {
    #[default]
    Flat,
    Pop,
    Rock,
    Jazz,
    Classical,
    Electronic,
    HipHop,
    VocalBoost,
    BassBoost,
    TrebleBoost,
    Custom,
}

impl EqPreset {
    pub const ALL: [EqPreset; 11] = [
        EqPreset::Flat,
        EqPreset::Pop,
        EqPreset::Rock,
        EqPreset::Jazz,
        EqPreset::Classical,
        EqPreset::Electronic,
        EqPreset::HipHop,
        EqPreset::VocalBoost,
        EqPreset::BassBoost,
        EqPreset::TrebleBoost,
        EqPreset::Custom,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            EqPreset::Flat => "Flat",
            EqPreset::Pop => "Pop",
            EqPreset::Rock => "Rock",
            EqPreset::Jazz => "Jazz",
            EqPreset::Classical => "Classical",
            EqPreset::Electronic => "Electronic",
            EqPreset::HipHop => "Hip Hop",
            EqPreset::VocalBoost => "Vocal Boost",
            EqPreset::BassBoost => "Bass Boost",
            EqPreset::TrebleBoost => "Treble Boost",
            EqPreset::Custom => "Custom",
        }
    }

    /// 10-band octave reference frequency gains in dB for [31.5, 63, 125, 250, 500, 1k, 2k, 4k, 8k, 16k]
    pub fn gains(&self) -> Option<[f32; 10]> {
        match self {
            EqPreset::Flat => Some([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            EqPreset::Pop => Some([-2.0, 0.0, 1.5, 3.0, 5.0, 4.2, 2.8, 1.2, 0.0, -1.5]),
            EqPreset::Rock => Some([4.5, 3.0, 1.5, 0.0, -1.5, 0.0, 2.0, 3.5, 4.0, 4.5]),
            EqPreset::Jazz => Some([3.0, 2.0, 1.0, 1.5, -1.0, -1.0, 0.5, 1.5, 2.5, 3.0]),
            EqPreset::Classical => Some([4.0, 3.0, 2.0, 1.5, -1.0, -1.0, 0.0, 2.0, 3.0, 3.5]),
            EqPreset::Electronic => Some([5.0, 4.0, 1.5, 0.0, -2.0, 1.5, 1.0, 2.5, 4.0, 4.5]),
            EqPreset::HipHop => Some([5.5, 4.5, 2.0, 1.0, -1.0, -1.0, 1.5, 1.0, 2.5, 3.0]),
            EqPreset::VocalBoost => Some([-2.0, -1.0, 0.0, 2.0, 4.0, 4.5, 3.5, 2.0, 0.5, -1.0]),
            EqPreset::BassBoost => Some([6.0, 5.5, 4.0, 2.5, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            EqPreset::TrebleBoost => Some([0.0, 0.0, 0.0, 0.0, 0.5, 1.5, 3.0, 4.5, 5.5, 6.0]),
            EqPreset::Custom => None,
        }
    }

    /// Interpolate gains across any frequency ladder (e.g. 10, 15, 31, 64 bands)
    pub fn gains_for_frequencies(&self, freqs: &[f32]) -> Vec<f32> {
        let base_freqs = [
            31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        match self.gains() {
            Some(base_gains) => freqs
                .iter()
                .map(|&f| interpolate_preset_gain(f, &base_freqs, &base_gains))
                .collect(),
            None => vec![0.0; freqs.len()],
        }
    }
}

/// Interpolates gain across an arbitrary frequency using log2 space
fn interpolate_preset_gain(
    target_freq: f32,
    base_freqs: &[f32; 10],
    base_gains: &[f32; 10],
) -> f32 {
    if target_freq <= base_freqs[0] {
        return base_gains[0];
    }
    if target_freq >= base_freqs[9] {
        return base_gains[9];
    }
    for i in 0..9 {
        let f0 = base_freqs[i];
        let f1 = base_freqs[i + 1];
        if target_freq >= f0 && target_freq <= f1 {
            let log_f0 = f0.log2();
            let log_f1 = f1.log2();
            let log_target = target_freq.log2();
            let t = ((log_target - log_f0) / (log_f1 - log_f0)).clamp(0.0, 1.0);
            let g = base_gains[i] + t * (base_gains[i + 1] - base_gains[i]);
            return (g * 10.0).round() / 10.0;
        }
    }
    0.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomEqPreset {
    pub name: String,
    pub gains: Vec<f32>,
    pub preamp_db: f32,
}

// ─────────────────────────────────────────────────────────────────────────────
// State & Models
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EqTab {
    #[default]
    Graphic,
    Tone,
    Advanced,
    Parametric,
    Dynamics,
    Filters,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParametricBand {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub filter_type: EqFilterType,
    pub enabled: bool,
}

impl Default for ParametricBand {
    fn default() -> Self {
        Self {
            freq: 1000.0,
            gain_db: 0.0,
            q: 0.707,
            filter_type: EqFilterType::Peaking,
            enabled: true,
        }
    }
}

pub struct EqPanelState {
    pub is_open: bool,
    /// Whether the studio was open last frame, so the keyboard focus can be
    /// moved into it exactly once, when it opens.
    pub was_open: bool,
    pub active_tab: EqTab,
    pub show_more_menu: bool,
    pub selected_preset: EqPreset,
    pub custom_presets: Vec<CustomEqPreset>,
    pub selected_custom_preset_idx: Option<usize>,
    pub is_saving_preset: bool,
    pub new_preset_name: String,
    pub extended_range: bool,         // Extended ±24 dB range toggle
    pub window_pos: Option<[f32; 2]>, // Persisted center or user placed position

    // Parametric (legacy / AutoEQ targets)
    pub eq_enabled: bool,
    pub bands: [ParametricBand; 10],
    pub preamp_db: f32,
    pub auto_headroom: bool,

    // Graphic (supports up to 64 bands)
    pub graphic_enabled: bool,
    pub graphic_layout: GraphicEqLayout,
    pub graphic_gains: [f32; 64],
    pub graphic_preamp_db: f32,

    // Tone & Acoustics
    pub bass_shelf_db: f32,
    pub treble_shelf_db: f32,
    pub midside_eq: bool,
    pub stereo_width: f32,
    pub balance: f32,
    pub dither_enabled: bool,

    // Dynamics (Limiter & Compressor)
    pub limiter_enabled: bool,
    pub limiter_mode: LimiterMode,
    pub limiter_true_peak: bool,
    pub limiter_lookahead_ms: f32,
    pub limiter_attack_ms: f32,
    pub limiter_release_ms: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_soft_clip: bool,

    pub compressor_enabled: bool,
    pub comp_threshold: [f32; 3],
    pub comp_ratio: [f32; 3],
    pub comp_attack_ms: [f32; 3],
    pub comp_release_ms: [f32; 3],
    pub comp_makeup_db: [f32; 3],
    pub comp_knee_db: [f32; 3],
    pub comp_stereo_link: [bool; 3],

    // Filters & Correction
    pub crossfeed_enabled: bool,
    pub crossfeed_profile: CrossfeedProfile,
    pub crossfeed_freq: f32,
    pub crossfeed_q: f32,
    pub crossfeed_delay_ms: f32,

    pub correction_enabled: bool,
    pub correction_depth: f32,
    pub correction_ir_path: String,
}

impl Default for EqPanelState {
    fn default() -> Self {
        let default_freqs = [
            31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        let mut bands: [ParametricBand; 10] = std::array::from_fn(|_| ParametricBand::default());
        for (i, b) in bands.iter_mut().enumerate() {
            b.freq = default_freqs[i];
        }

        Self {
            is_open: false,
            was_open: false,
            active_tab: EqTab::Graphic,
            show_more_menu: false,
            selected_preset: EqPreset::Flat,
            custom_presets: Vec::new(),
            selected_custom_preset_idx: None,
            is_saving_preset: false,
            new_preset_name: String::new(),
            extended_range: false,

            eq_enabled: false,
            bands,
            preamp_db: 0.0,
            auto_headroom: true, // Audiophile default: prevents digital clipping when positive EQ boosts are applied

            graphic_enabled: false,
            graphic_layout: GraphicEqLayout::TenBand,
            graphic_gains: [0.0; 64],
            graphic_preamp_db: 0.0,

            bass_shelf_db: 0.0,
            treble_shelf_db: 0.0,
            midside_eq: false,
            stereo_width: 1.0,
            balance: 0.0,
            dither_enabled: false,

            limiter_enabled: false,
            limiter_mode: LimiterMode::Transparent,
            limiter_true_peak: false,
            limiter_lookahead_ms: 5.0,
            limiter_attack_ms: 0.3,
            limiter_release_ms: 100.0,
            limiter_ceiling_db: -0.1,
            limiter_soft_clip: false,

            compressor_enabled: false,
            comp_threshold: [-24.0, -18.0, -18.0],
            comp_ratio: [4.0, 3.0, 3.0],
            comp_attack_ms: [10.0, 10.0, 5.0],
            comp_release_ms: [100.0, 100.0, 80.0],
            comp_makeup_db: [0.0; 3],
            comp_knee_db: [3.0; 3],
            comp_stereo_link: [true; 3],

            crossfeed_enabled: false,
            crossfeed_profile: CrossfeedProfile::Bauer,
            crossfeed_freq: 700.0,
            crossfeed_q: 0.707,
            crossfeed_delay_ms: 0.3,

            correction_enabled: false,
            correction_depth: 1.0,
            correction_ir_path: String::new(),
            window_pos: None,
        }
    }
}

/// Serde-serializable snapshot of EQ panel settings for persistent storage across sessions
///
/// `#[serde(default)]` at the struct level means a field added in a later build
/// reads back as the fresh panel's value rather than failing the whole parse —
/// which would silently discard every EQ setting the user had.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistentEqState {
    pub is_open: bool,
    pub window_pos: Option<[f32; 2]>,
    pub active_tab: EqTab,
    pub eq_enabled: bool,
    pub graphic_enabled: bool,
    pub graphic_layout: GraphicEqLayout,
    pub graphic_gains: Vec<f32>,
    pub graphic_preamp_db: f32,
    pub selected_preset: EqPreset,
    pub custom_presets: Vec<CustomEqPreset>,
    pub selected_custom_preset_idx: Option<usize>,
    pub extended_range: bool,
    pub preamp_db: f32,
    pub auto_headroom: bool,
    /// Parametric bands — the shape AutoEQ targets write into, so they must
    /// survive a restart.
    pub bands: [ParametricBand; 10],
    pub bass_shelf_db: f32,
    pub treble_shelf_db: f32,
    pub midside_eq: bool,
    pub stereo_width: f32,
    pub balance: f32,
    pub dither_enabled: bool,
    pub limiter_enabled: bool,
    pub limiter_mode: LimiterMode,
    pub limiter_true_peak: bool,
    pub limiter_lookahead_ms: f32,
    pub limiter_attack_ms: f32,
    pub limiter_release_ms: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_soft_clip: bool,
    pub compressor_enabled: bool,
    pub comp_threshold: [f32; 3],
    pub comp_ratio: [f32; 3],
    pub comp_attack_ms: [f32; 3],
    pub comp_release_ms: [f32; 3],
    pub comp_makeup_db: [f32; 3],
    pub comp_knee_db: [f32; 3],
    pub comp_stereo_link: [bool; 3],
    pub crossfeed_enabled: bool,
    pub crossfeed_profile: CrossfeedProfile,
    pub crossfeed_freq: f32,
    pub crossfeed_q: f32,
    pub crossfeed_delay_ms: f32,
    pub correction_enabled: bool,
    pub correction_depth: f32,
    pub correction_ir_path: String,
}

impl From<&EqPanelState> for PersistentEqState {
    fn from(s: &EqPanelState) -> Self {
        Self {
            is_open: s.is_open,
            window_pos: s.window_pos,
            active_tab: s.active_tab,
            eq_enabled: s.eq_enabled,
            graphic_enabled: s.graphic_enabled,
            graphic_layout: s.graphic_layout.clone(),
            graphic_gains: s.graphic_gains.to_vec(),
            graphic_preamp_db: s.graphic_preamp_db,
            selected_preset: s.selected_preset,
            custom_presets: s.custom_presets.clone(),
            selected_custom_preset_idx: s.selected_custom_preset_idx,
            extended_range: s.extended_range,
            preamp_db: s.preamp_db,
            auto_headroom: s.auto_headroom,
            bands: s.bands.clone(),
            bass_shelf_db: s.bass_shelf_db,
            treble_shelf_db: s.treble_shelf_db,
            midside_eq: s.midside_eq,
            stereo_width: s.stereo_width,
            balance: s.balance,
            dither_enabled: s.dither_enabled,
            limiter_enabled: s.limiter_enabled,
            limiter_mode: s.limiter_mode,
            limiter_true_peak: s.limiter_true_peak,
            limiter_lookahead_ms: s.limiter_lookahead_ms,
            limiter_attack_ms: s.limiter_attack_ms,
            limiter_release_ms: s.limiter_release_ms,
            limiter_ceiling_db: s.limiter_ceiling_db,
            limiter_soft_clip: s.limiter_soft_clip,
            compressor_enabled: s.compressor_enabled,
            comp_threshold: s.comp_threshold,
            comp_ratio: s.comp_ratio,
            comp_attack_ms: s.comp_attack_ms,
            comp_release_ms: s.comp_release_ms,
            comp_makeup_db: s.comp_makeup_db,
            comp_knee_db: s.comp_knee_db,
            comp_stereo_link: s.comp_stereo_link,
            crossfeed_enabled: s.crossfeed_enabled,
            crossfeed_profile: s.crossfeed_profile,
            crossfeed_freq: s.crossfeed_freq,
            crossfeed_q: s.crossfeed_q,
            crossfeed_delay_ms: s.crossfeed_delay_ms,
            correction_enabled: s.correction_enabled,
            correction_depth: s.correction_depth,
            correction_ir_path: s.correction_ir_path.clone(),
        }
    }
}

impl Default for PersistentEqState {
    fn default() -> Self {
        Self::from(&EqPanelState::default())
    }
}

impl PersistentEqState {
    pub fn apply_to(&self, state: &mut EqPanelState) {
        state.is_open = self.is_open;
        state.window_pos = self.window_pos;
        state.active_tab = self.active_tab;
        state.eq_enabled = self.eq_enabled;
        state.graphic_enabled = self.graphic_enabled;
        state.graphic_layout = self.graphic_layout.clone();
        for (i, &g) in self.graphic_gains.iter().take(64).enumerate() {
            state.graphic_gains[i] = g;
        }
        state.graphic_preamp_db = self.graphic_preamp_db;
        state.selected_preset = self.selected_preset;
        state.custom_presets = self.custom_presets.clone();
        state.selected_custom_preset_idx = self.selected_custom_preset_idx;
        state.extended_range = self.extended_range;
        state.preamp_db = self.preamp_db;
        state.auto_headroom = self.auto_headroom;
        for (i, band) in self.bands.iter().take(10).enumerate() {
            state.bands[i] = band.clone();
        }
        state.bass_shelf_db = self.bass_shelf_db;
        state.treble_shelf_db = self.treble_shelf_db;
        state.midside_eq = self.midside_eq;
        state.stereo_width = self.stereo_width;
        state.balance = self.balance;
        state.dither_enabled = self.dither_enabled;
        state.limiter_enabled = self.limiter_enabled;
        state.limiter_mode = self.limiter_mode;
        state.limiter_true_peak = self.limiter_true_peak;
        state.limiter_lookahead_ms = self.limiter_lookahead_ms;
        state.limiter_attack_ms = self.limiter_attack_ms;
        state.limiter_release_ms = self.limiter_release_ms;
        state.limiter_ceiling_db = self.limiter_ceiling_db;
        state.limiter_soft_clip = self.limiter_soft_clip;
        state.compressor_enabled = self.compressor_enabled;
        state.comp_threshold = self.comp_threshold;
        state.comp_ratio = self.comp_ratio;
        state.comp_attack_ms = self.comp_attack_ms;
        state.comp_release_ms = self.comp_release_ms;
        state.comp_makeup_db = self.comp_makeup_db;
        state.comp_knee_db = self.comp_knee_db;
        state.comp_stereo_link = self.comp_stereo_link;
        state.crossfeed_enabled = self.crossfeed_enabled;
        state.crossfeed_profile = self.crossfeed_profile;
        state.crossfeed_freq = self.crossfeed_freq;
        state.crossfeed_q = self.crossfeed_q;
        state.crossfeed_delay_ms = self.crossfeed_delay_ms;
        state.correction_enabled = self.correction_enabled;
        state.correction_depth = self.correction_depth;
        state.correction_ir_path = self.correction_ir_path.clone();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Actions (returned to dsp_state and app each frame)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct EqPanelActions {
    // Equalizer On/Off
    pub eq_enabled_changed: Option<bool>,
    pub graphic_enabled_changed: Option<bool>,

    // Sliders & Preamp
    pub graphic_slider_changed: Option<usize>,
    pub graphic_preamp_changed: Option<f32>,
    pub eq_preamp_changed: Option<f32>,
    pub eq_band_changed: Option<usize>,
    pub eq_auto_headroom_changed: Option<bool>,
    pub eq_reset: bool,
    pub graphic_reset: bool,

    // Tone & Acoustics
    pub bass_shelf_changed: Option<f32>,
    pub treble_shelf_changed: Option<f32>,
    pub midside_eq_changed: Option<bool>,
    pub stereo_width_changed: Option<f32>,
    pub balance_changed: Option<f32>,
    pub dither_enabled_changed: Option<bool>,

    // External Modals & Presets
    pub open_autoeq_dialog: bool,
    pub graphic_layout_changed: Option<GraphicEqLayout>,

    // Dynamics (limiter/comp)
    pub limiter_enabled_changed: Option<bool>,
    pub limiter_mode_changed: Option<LimiterMode>,
    pub limiter_true_peak_changed: Option<bool>,
    pub limiter_params_changed: bool,
    pub compressor_enabled_changed: Option<bool>,
    pub comp_band_params_changed: Option<usize>,
    pub comp_band_features_changed: Option<usize>,

    // Filters
    pub crossfeed_enabled_changed: Option<bool>,
    pub crossfeed_profile_changed: Option<CrossfeedProfile>,
    pub crossfeed_custom_changed: bool,
    pub correction_enabled_changed: Option<bool>,
    pub correction_depth_changed: Option<f32>,
    pub load_correction_ir: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Floating Resizable Equalizer Window
// ─────────────────────────────────────────────────────────────────────────────

pub fn show_eq_panel(
    ctx: &egui::Context,
    state: &mut EqPanelState,
    theme: &Theme,
) -> EqPanelActions {
    let mut actions = EqPanelActions::default();

    if !state.is_open {
        state.was_open = false;
        return actions;
    }

    let screen = ctx
        .input(|i| i.raw.screen_rect)
        .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 830.0)));
    let default_w = 760.0_f32.min((screen.width() - 40.0).max(400.0));
    let default_h = 560.0_f32.min((screen.height() - 40.0).max(350.0));
    let default_x = ((screen.width() - default_w) / 2.0).max(10.0);
    let default_y = ((screen.height() - default_h) / 2.0).max(10.0);
    let initial_pos = state
        .window_pos
        .map(|[x, y]| Pos2::new(x, y))
        .unwrap_or_else(|| Pos2::new(default_x, default_y));

    // Floating studio panel styled directly with active design system tokens
    let win_response = Window::new("equalizer_floating_studio")
        .resizable(true)
        .movable(true)
        .collapsible(false)
        .title_bar(false)
        .default_pos(initial_pos)
        .default_size(Vec2::new(default_w, default_h))
        .min_size(crate::dialog::min_dialog_size())
        .max_size(Vec2::new(880.0, (screen.height() - 40.0).max(500.0)))
        .frame(
            egui::Frame::window(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0_f32, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_xl))
                .inner_margin(Margin::symmetric(
                    theme.space_5xl as i8,
                    theme.space_3xl as i8,
                )),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

            // ── 1. Top Header Bar (Always Enabled) ───────────────────────────
            render_header(ui, state, &mut actions, theme);

            // ── 2. Tab Switcher: [ 🎚 Graphic Equalizer ]   [ 🎛 Tone & Acoustics ]   [ ⚙ Advanced ]
            render_tab_navigation(ui, state, theme);

            // ── 3. Enabled UI Wrapper with Visual Greying-Out ─────────────────
            let eq_active = state.graphic_enabled || state.eq_enabled;
            let content_start_y = ui.cursor().top();

            ui.add_enabled_ui(eq_active, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

                egui::ScrollArea::vertical()
                    .id_salt("eq_tabs_scroll_area")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);
                        match state.active_tab {
                            EqTab::Graphic => {
                                render_graphic_eq_tab(ui, state, &mut actions, theme);
                            }
                            EqTab::Tone => {
                                render_tone_acoustics_tab(ui, state, &mut actions, theme);
                            }
                            EqTab::Advanced => {
                                render_advanced_tab(ui, state, &mut actions, theme);
                            }
                            _ => {}
                        }
                    });
            });

            // If EQ is bypassed, draw subtle darkening overlay & informative badge
            if !eq_active {
                let rect = ui.min_rect();
                let overlay_rect = Rect::from_min_max(
                    Pos2::new(rect.left(), content_start_y),
                    Pos2::new(rect.right(), rect.bottom()),
                );
                ui.painter().rect_filled(
                    overlay_rect,
                    CornerRadius::same(theme.radius_xl),
                    theme.overlay_scrim,
                );

                let badge_pos = overlay_rect.center();
                let badge_text = "Equalizer Bypassed (Toggle Switch in Header to Enable)";
                let font = crate::theme::font(12.5);
                let galley =
                    ui.painter()
                        .layout_no_wrap(badge_text.to_string(), font, theme.text_muted);
                let badge_rect =
                    Rect::from_center_size(badge_pos, galley.size() + Vec2::new(26.0, 12.0));

                ui.painter().rect_filled(
                    badge_rect,
                    CornerRadius::same(theme.radius_lg),
                    theme.bg_pill,
                );
                ui.painter().rect_stroke(
                    badge_rect,
                    CornerRadius::same(theme.radius_lg),
                    Stroke::new(1.0, theme.border_card),
                    egui::StrokeKind::Inside,
                );
                ui.painter().galley(
                    badge_rect.min + Vec2::new(13.0, 6.0),
                    galley,
                    theme.text_secondary,
                );
            }
        });

    if let Some(inner) = win_response {
        let rect = inner.response.rect;
        if let Some(pos) = state.window_pos {
            if (pos[0] - rect.min.x).abs() > 1.0 || (pos[1] - rect.min.y).abs() > 1.0 {
                state.window_pos = Some([rect.min.x, rect.min.y]);
            }
        } else if (rect.min.x - default_x).abs() > 2.0 || (rect.min.y - default_y).abs() > 2.0 {
            // User moved window away from initial center position
            state.window_pos = Some([rect.min.x, rect.min.y]);
        }
    }

    actions
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Header Bar
// ─────────────────────────────────────────────────────────────────────────────

fn render_header(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);

        // Icon (Sliders)
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::hover());
        crate::components::icons::draw_equalizer_sliders(ui, icon_rect, theme.accent_warm);

        // Title
        ui.label(
            egui::RichText::new("Equalizer")
                .font(crate::theme::font(17.5))
                .strong()
                .color(theme.text_primary),
        );

        ui.add_space(8.0);

        // Master Toggle Switch: Enable Equalizer
        let mut eq_active = state.graphic_enabled || state.eq_enabled;
        if custom_toggle_switch(ui, &mut eq_active, "Enable Equalizer", theme) {
            state.graphic_enabled = eq_active;
            state.eq_enabled = eq_active;
            actions.graphic_enabled_changed = Some(eq_active);
            actions.eq_enabled_changed = Some(eq_active);
        }

        // Right-aligned actions: AutoEQ badge, More (⋮), and Close (✕)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            // Close button (✕)
            let (close_rect, close_resp) =
                ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::click());
            // The studio takes the keyboard focus when it opens, so the
            // keyboard works straight away instead of staying wherever it
            // was in the library behind.
            if !state.was_open {
                ctx.memory_mut(|m| m.request_focus(close_resp.id));
            }
            close_resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, "Close the DSP studio")
            });
            let _ = close_resp.clone().on_hover_text("Close the DSP studio");
            let cp = ui.painter_at(close_rect);
            let close_bg = if close_resp.hovered() {
                theme.destructive.gamma_multiply(0.2)
            } else {
                theme.bg_pill
            };
            cp.rect_filled(close_rect, CornerRadius::same(theme.radius_sm), close_bg);
            let icon_col = if close_resp.hovered() {
                theme.destructive
            } else {
                theme.text_secondary
            };
            crate::components::icons::draw_window_close(
                ui,
                close_rect.shrink2(Vec2::splat(6.0)),
                icon_col,
            );
            if close_resp
                .on_hover_text("Close Equalizer Panel (Esc)")
                .clicked()
            {
                state.is_open = false;
            }
        });
    });

    // Remember that it was open, so the focus is moved into it once, on the
    // frame it opens.
    state.was_open = state.is_open;
}

// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// 3. Tab Switcher Navigation
// ─────────────────────────────────────────────────────────────────────────────

fn render_tab_navigation(ui: &mut egui::Ui, state: &mut EqPanelState, theme: &Theme) {
    ui.vertical_centered(|ui| {
        let tab_items = [
            (EqTab::Graphic, "Graphic Equalizer"),
            (EqTab::Tone, "Tone & Acoustics"),
            (EqTab::Advanced, "Advanced"),
        ];

        let tab_w = 142.0;
        let tab_h = 32.0;
        let spacing = 6.0;
        let total_w =
            (tab_items.len() as f32 * tab_w) + ((tab_items.len() - 1) as f32 * spacing) + 10.0;

        ui.allocate_ui_with_layout(
            Vec2::new(total_w, tab_h + 8.0),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border_card))
                    .corner_radius(CornerRadius::same(theme.radius_lg))
                    .inner_margin(Margin::symmetric(
                        theme.space_sm as i8,
                        theme.space_xs as i8,
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(spacing, 0.0);

                            for (tab, label) in tab_items {
                                let is_sel = state.active_tab == tab;
                                let (bg, text_col, font) = if is_sel {
                                    (theme.accent, Color32::WHITE, crate::theme::font(12.5))
                                } else {
                                    (
                                        Color32::TRANSPARENT,
                                        theme.text_secondary,
                                        crate::theme::font(12.0),
                                    )
                                };

                                let btn = egui::Button::new(
                                    egui::RichText::new(label)
                                        .font(font)
                                        .strong()
                                        .color(text_col),
                                )
                                .fill(bg)
                                .stroke(Stroke::NONE)
                                .corner_radius(CornerRadius::same(theme.radius_lg))
                                .min_size(Vec2::new(tab_w, tab_h));

                                if ui.add(btn).clicked() {
                                    state.active_tab = tab;
                                }
                            }
                        });
                    });
            },
        );
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Tab 1: Graphic Equalizer (Bands, Canvas, Spline, Presets, Save)
// ─────────────────────────────────────────────────────────────────────────────

fn render_graphic_eq_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    // ── Sub-bar: Band Count Selector (Left) & Preset Dropdown + Save Button (Right) ─────────
    ui.horizontal(|ui| {
        // 1. Left side: Band Count Buttons (Secondary filter chips: visually distinct from main tabs)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            let band_options = [
                ("10 Bands", GraphicEqLayout::TenBand),
                ("15 Bands", GraphicEqLayout::FifteenBand),
                ("31 Bands", GraphicEqLayout::ThirtyOneBand),
                ("64 Bands", GraphicEqLayout::SixtyFourBand),
            ];

            for (lbl, layout) in band_options {
                let is_active = matches!(
                    (&state.graphic_layout, &layout),
                    (GraphicEqLayout::TenBand, GraphicEqLayout::TenBand)
                        | (GraphicEqLayout::FifteenBand, GraphicEqLayout::FifteenBand)
                        | (
                            GraphicEqLayout::ThirtyOneBand,
                            GraphicEqLayout::ThirtyOneBand
                        )
                        | (
                            GraphicEqLayout::SixtyFourBand,
                            GraphicEqLayout::SixtyFourBand
                        )
                );

                let (bg, text_col, stroke) = if is_active {
                    (theme.accent_warm, Color32::WHITE, Stroke::NONE)
                } else {
                    (
                        theme.bg_pill,
                        theme.text_secondary,
                        Stroke::new(1.0, theme.border_card),
                    )
                };

                let btn = egui::Button::new(
                    egui::RichText::new(lbl)
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(text_col),
                )
                .fill(bg)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(60.0, 24.0));

                if ui.add(btn).clicked() && !is_active {
                    state.graphic_layout = layout.clone();
                    actions.graphic_layout_changed = Some(layout);
                    apply_active_preset(state, actions);
                }
            }

            ui.add_space(2.0);
            crate::components::info_box::render_info_icon_button(
                ui,
                "eq_graphic_band_topology",
                &crate::components::info_box::INFO_GRAPHIC_EQ_BANDS,
                theme,
            );
        });

        // 2. Right side: Preset Dropdown & Save Button (No "Preset:" label, moved to right, UI color)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            // Save Preset Button
            let save_btn = egui::Button::new(
                egui::RichText::new("Save")
                    .font(crate::theme::font(11.5))
                    .color(theme.text_primary),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(44.0, 24.0));

            if ui
                .add(save_btn)
                .on_hover_text("Save current EQ curve as a custom preset")
                .clicked()
            {
                state.is_saving_preset = !state.is_saving_preset;
                if state.is_saving_preset && state.new_preset_name.is_empty() {
                    state.new_preset_name = format!("Preset {}", state.custom_presets.len() + 1);
                }
            }

            // Delete Custom Preset Button (if custom preset active)
            if let Some(idx) = state.selected_custom_preset_idx {
                let del_btn = egui::Button::new(
                    egui::RichText::new("Delete")
                        .font(crate::theme::font(11.0))
                        .color(theme.destructive),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(44.0, 24.0));

                if ui
                    .add(del_btn)
                    .on_hover_text("Delete selected custom preset")
                    .clicked()
                    && idx < state.custom_presets.len()
                {
                    state.custom_presets.remove(idx);
                    state.selected_custom_preset_idx = None;
                    state.selected_preset = EqPreset::Flat;
                    apply_active_preset(state, actions);
                }
            }

            // Preset Dropdown ComboBox (with UI colors applied via scope)
            let current_preset_name = if let Some(idx) = state.selected_custom_preset_idx {
                state
                    .custom_presets
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Custom".to_string())
            } else {
                state.selected_preset.name().to_string()
            };

            ui.scope(|ui| {
                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.weak_bg_fill = theme.bg_pill;
                visuals.widgets.inactive.bg_fill = theme.bg_pill;
                visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                visuals.widgets.inactive.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.widgets.hovered.weak_bg_fill = theme.bg_card_hover;
                visuals.widgets.hovered.bg_fill = theme.bg_card_hover;
                visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                visuals.widgets.hovered.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.widgets.open.weak_bg_fill = theme.bg_card_hover;
                visuals.widgets.open.bg_fill = theme.bg_card_hover;
                visuals.widgets.open.bg_stroke = Stroke::NONE;
                visuals.widgets.open.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.window_fill = theme.bg_card;
                visuals.window_stroke = Stroke::new(1.0, theme.border_card);
                visuals.menu_corner_radius = CornerRadius::same(theme.radius_md);

                egui::ComboBox::from_id_salt("eq_presets_combo")
                    .selected_text(
                        egui::RichText::new(current_preset_name)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_primary),
                    )
                    .width(115.0)
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Factory Presets")
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                        for preset in EqPreset::ALL {
                            if preset == EqPreset::Custom {
                                continue;
                            }
                            let is_sel = state.selected_preset == preset
                                && state.selected_custom_preset_idx.is_none();
                            if ui.selectable_label(is_sel, preset.name()).clicked() {
                                state.selected_preset = preset;
                                state.selected_custom_preset_idx = None;
                                apply_active_preset(state, actions);
                            }
                        }

                        if !state.custom_presets.is_empty() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new("Saved Presets")
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_muted),
                            );
                            for (idx, cp) in state.custom_presets.iter().enumerate() {
                                let is_sel = state.selected_custom_preset_idx == Some(idx);
                                if ui.selectable_label(is_sel, &cp.name).clicked() {
                                    state.selected_preset = EqPreset::Custom;
                                    state.selected_custom_preset_idx = Some(idx);
                                    let num = state.graphic_layout.num_bands();
                                    for i in 0..num {
                                        let g = cp.gains.get(i).copied().unwrap_or(0.0);
                                        state.graphic_gains[i] = g;
                                        actions.graphic_slider_changed = Some(i);
                                    }
                                }
                            }
                        }
                    });
            });

            crate::components::info_box::render_info_icon_button(
                ui,
                "eq_preset_curves",
                &crate::components::info_box::INFO_EQ_PRESETS,
                theme,
            );
        });
    });

    // ── Inline Save Box (when Save toggled) ──────────────────────────────────
    if state.is_saving_preset {
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.accent_highlight))
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(Margin::symmetric(
                theme.space_xl as i8,
                theme.space_sm as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    ui.label(
                        egui::RichText::new("Preset Name:")
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_preset_name).desired_width(160.0),
                    );

                    if ui
                        .button(egui::RichText::new("Confirm Save").color(Color32::WHITE))
                        .clicked()
                    {
                        let trimmed = state.new_preset_name.trim();
                        if !trimmed.is_empty() {
                            let num = state.graphic_layout.num_bands();
                            let gains = state.graphic_gains[0..num].to_vec();
                            state.custom_presets.push(CustomEqPreset {
                                name: trimmed.to_string(),
                                gains,
                                preamp_db: state.graphic_preamp_db,
                            });
                            state.selected_custom_preset_idx = Some(state.custom_presets.len() - 1);
                            state.selected_preset = EqPreset::Custom;
                            state.new_preset_name.clear();
                            state.is_saving_preset = false;
                        }
                    }

                    if ui.button("Cancel").clicked() {
                        state.is_saving_preset = false;
                    }
                });
            });
    }

    // ── Spline Canvas with Vertical Stems & Interactive Curve ────────────────
    render_spline_eq_canvas(ui, state, actions, theme);

    // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────────
    render_footer_controls(ui, state, actions, theme, EqTab::Graphic);
}

fn apply_active_preset(state: &mut EqPanelState, actions: &mut EqPanelActions) {
    let freqs = state.graphic_layout.frequencies();
    let num = freqs.len();
    if let Some(idx) = state.selected_custom_preset_idx {
        if let Some(cp) = state.custom_presets.get(idx) {
            for i in 0..num {
                let g = cp.gains.get(i).copied().unwrap_or(0.0);
                state.graphic_gains[i] = g;
                actions.graphic_slider_changed = Some(i);
            }
            return;
        }
    }
    let new_gains = state.selected_preset.gains_for_frequencies(&freqs);
    for i in 0..num {
        let g = new_gains.get(i).copied().unwrap_or(0.0);
        state.graphic_gains[i] = g;
        actions.graphic_slider_changed = Some(i);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Spline EQ Canvas (Scales to 10, 15, 31, 64 Bands)
// ─────────────────────────────────────────────────────────────────────────────

fn render_spline_eq_canvas(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let avail_h = ui.available_height();
    // Dynamic height scaling with plenty of space in the dedicated tab
    let canvas_h = (avail_h - 90.0).clamp(190.0, 500.0);

    let (canvas_rect, canvas_resp) =
        ui.allocate_exact_size(Vec2::new(avail_w, canvas_h), Sense::click_and_drag());
    let painter = ui.painter_at(canvas_rect);

    // Pill Background & Subtle Border
    painter.rect_filled(
        canvas_rect,
        CornerRadius::same(theme.radius_lg),
        theme.bg_pill,
    );
    painter.rect_stroke(
        canvas_rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let pad_left = 46.0;
    let pad_right = 16.0;
    let pad_top = 18.0;
    let pad_bottom = 24.0;

    let plot_rect = Rect::from_min_max(
        Pos2::new(canvas_rect.left() + pad_left, canvas_rect.top() + pad_top),
        Pos2::new(
            canvas_rect.right() - pad_right,
            canvas_rect.bottom() - pad_bottom,
        ),
    );

    // Horizontal Reference dB Lines (+24, +12, 0, -12, -24 if extended; else +12, +6, 0, -6, -12)
    let max_db = if state.extended_range {
        24.0_f32
    } else {
        12.0_f32
    };
    let grid_dbs: &[f32] = if state.extended_range {
        &[24.0, 12.0, 0.0, -12.0, -24.0]
    } else {
        &[12.0, 6.0, 0.0, -6.0, -12.0]
    };
    let y_for_db = |db: f32| -> f32 {
        let norm = (db - (-max_db)) / (2.0 * max_db);
        plot_rect.bottom() - norm * plot_rect.height()
    };

    for &db in grid_dbs {
        let y = y_for_db(db);
        let is_zero = db.abs() < 0.01;

        // dB Label on the left
        let lbl_text = if is_zero {
            "0dB".to_string()
        } else {
            format!("{:+0.0}dB", db)
        };
        painter.text(
            Pos2::new(canvas_rect.left() + 10.0, y),
            egui::Align2::LEFT_CENTER,
            lbl_text,
            crate::theme::font(10.5),
            if is_zero {
                theme.text_secondary
            } else {
                theme.text_muted
            },
        );

        // Gridline across the plot
        let stroke_color = if is_zero {
            theme.meter_grid
        } else {
            theme.meter_grid.gamma_multiply(0.55)
        };
        let stroke = Stroke::new(if is_zero { 1.2 } else { 1.0 }, stroke_color);
        draw_dashed_line(
            &painter,
            Pos2::new(plot_rect.left(), y),
            Pos2::new(plot_rect.right(), y),
            stroke,
        );
    }

    let freqs = state.graphic_layout.frequencies();
    let num_bands = freqs.len().clamp(1, 64);
    let col_w = plot_rect.width() / num_bands as f32;

    let mut node_positions = Vec::with_capacity(num_bands);
    let mut hovered_band: Option<(usize, f32, f32, Pos2)> = None;

    let pointer_pos = ui.input(|i| i.pointer.interact_pos());

    for i in 0..num_bands {
        let center_x = plot_rect.left() + (i as f32 + 0.5) * col_w;
        let band_rect = Rect::from_min_max(
            Pos2::new(center_x - col_w * 0.5, plot_rect.top()),
            Pos2::new(center_x + col_w * 0.5, plot_rect.bottom()),
        );

        // Interactive Drag / Click for this band
        if canvas_resp.dragged() || canvas_resp.clicked() {
            if let Some(pos) = pointer_pos {
                if band_rect.contains(Pos2::new(pos.x, plot_rect.center().y)) {
                    let norm = ((plot_rect.bottom() - pos.y) / plot_rect.height()).clamp(0.0, 1.0);
                    let new_gain = (norm * (2.0 * max_db) - max_db).clamp(-max_db, max_db);
                    let rounded_gain = (new_gain * 10.0).round() / 10.0;
                    if (state.graphic_gains[i] - rounded_gain).abs() > 0.05 {
                        state.graphic_gains[i] = rounded_gain;
                        state.selected_preset = EqPreset::Custom;
                        state.selected_custom_preset_idx = None;
                        actions.graphic_slider_changed = Some(i);
                    }
                }
            }
        }

        // Double-click resets band to 0.0 dB
        if canvas_resp.double_clicked() {
            if let Some(pos) = pointer_pos {
                if band_rect.contains(Pos2::new(pos.x, plot_rect.center().y)) {
                    state.graphic_gains[i] = 0.0;
                    state.selected_preset = EqPreset::Custom;
                    state.selected_custom_preset_idx = None;
                    actions.graphic_slider_changed = Some(i);
                }
            }
        }

        let gain = state.graphic_gains[i];
        let node_y = y_for_db(gain);
        let node_pos = Pos2::new(center_x, node_y);
        node_positions.push(node_pos);

        // Background vertical track line
        painter.line_segment(
            [
                Pos2::new(center_x, plot_rect.top()),
                Pos2::new(center_x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, theme.border_subtle),
        );

        // Vertical stem line from node down to bottom track with vibrant gradient
        let stem_start = Pos2::new(center_x, node_y);
        let stem_end = Pos2::new(center_x, plot_rect.bottom());
        if stem_start.y < stem_end.y {
            let stem_thickness = if num_bands <= 15 {
                2.5
            } else if num_bands <= 32 {
                1.8
            } else {
                1.2
            };
            draw_vertical_gradient_line(
                &painter,
                stem_start,
                stem_end,
                theme.accent_highlight,
                theme.accent,
                stem_thickness,
            );
        }

        // Frequency Label below the plot (condensed for high band counts)
        let show_label = if num_bands <= 15 {
            true
        } else if num_bands <= 32 {
            i % 2 == 0 || i == num_bands - 1
        } else {
            i % 6 == 0 || i == num_bands - 1
        };

        if show_label {
            let lbl_y = canvas_rect.bottom() - 10.0;
            let freq_val = freqs.get(i).copied().unwrap_or(1000.0);
            let freq_str = format_frequency_label(freq_val);
            painter.text(
                Pos2::new(center_x, lbl_y),
                egui::Align2::CENTER_CENTER,
                freq_str,
                crate::theme::font(if num_bands > 15 { 9.5 } else { 10.5 }),
                theme.text_secondary,
            );
        }

        // Hover detection for tooltip
        if let Some(pos) = pointer_pos {
            let hit_radius = if num_bands <= 15 { 14.0 } else { 8.0 };
            if (pos - node_pos).length() < hit_radius || band_rect.contains(pos) {
                let freq_val = freqs.get(i).copied().unwrap_or(1000.0);
                hovered_band = Some((i, freq_val, gain, node_pos));
            }
        }
    }

    // Continuous Catmull-Rom Spline Curve passing through all nodes
    let segments_per_band = if num_bands <= 15 {
        16
    } else if num_bands <= 32 {
        8
    } else {
        4
    };
    let spline_pts = catmull_rom_spline(&node_positions, segments_per_band);

    for w in spline_pts.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(2.4, theme.accent_highlight));
    }

    // Draw Node Thumbs (white circular nodes with glow and border)
    let base_node_r = if num_bands <= 15 {
        5.5
    } else if num_bands <= 32 {
        4.0
    } else {
        2.8
    };
    for (i, &pos) in node_positions.iter().enumerate() {
        let is_hovered = hovered_band.map(|(b, _, _, _)| b == i).unwrap_or(false);

        // Glow halo
        let halo_r = if is_hovered {
            base_node_r + 4.5
        } else {
            base_node_r + 2.5
        };
        painter.circle_filled(
            pos,
            halo_r,
            crate::theme::with_alpha(theme.accent_highlight, if is_hovered { 90 } else { 45 }),
        );

        // Center Node
        let node_r = if is_hovered {
            base_node_r + 1.2
        } else {
            base_node_r
        };
        painter.circle_filled(pos, node_r, Color32::WHITE);
        painter.circle_stroke(pos, node_r, Stroke::new(1.6, theme.accent_highlight));
    }

    // Floating Gain Tooltip on hovered node
    if let Some((_, freq, gain, pos)) = hovered_band {
        let tip_y = if pos.y > canvas_rect.top() + 35.0 {
            pos.y - 18.0
        } else {
            pos.y + 18.0
        };
        let tip_pos = Pos2::new(pos.x, tip_y);
        let tip_str = format!("{}: {:+.1} dB", format_exact_frequency(freq), gain);

        let font = crate::theme::font(11.0);
        let galley = painter.layout_no_wrap(tip_str, font, theme.text_primary);
        let tip_rect = Rect::from_center_size(tip_pos, galley.size() + Vec2::new(10.0, 4.0));

        painter.rect_filled(tip_rect, CornerRadius::same(theme.radius_sm), theme.bg_card);
        painter.rect_stroke(
            tip_rect,
            CornerRadius::same(theme.radius_sm),
            Stroke::new(1.0, theme.accent_highlight),
            egui::StrokeKind::Inside,
        );
        painter.galley(
            tip_rect.min + Vec2::new(5.0, 2.0),
            galley,
            theme.text_primary,
        );
    }
}

fn format_frequency_label(f: f32) -> String {
    if f >= 10000.0 {
        format!("{:.0}k", f / 1000.0)
    } else if f >= 1000.0 {
        if (f % 1000.0).abs() < 10.0 {
            format!("{:.0}k", f / 1000.0)
        } else {
            format!("{:.1}k", f / 1000.0)
        }
    } else {
        format!("{:.0}", f)
    }
}

fn format_exact_frequency(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1} kHz", f / 1000.0)
    } else {
        format!("{:.0} Hz", f)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab 2: Tone & Acoustics (Bass, Treble, Stereo Width, Balance + Extras)
// ─────────────────────────────────────────────────────────────────────────────

fn render_tone_acoustics_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let two_col = avail_w >= 540.0;
    let card_w = if two_col {
        (avail_w - 12.0) * 0.5
    } else {
        avail_w
    };

    let tone_max = if state.extended_range { 24.0 } else { 12.0 };
    let tone_range = -tone_max..=tone_max;
    let min_lbl = if state.extended_range {
        "-24dB"
    } else {
        "-12dB"
    };
    let max_lbl = if state.extended_range {
        "+24dB"
    } else {
        "+12dB"
    };

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        // Row 1: Bass & Treble
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 1: Bass Shelf (80-100 Hz)
            render_slider_card(
                ui,
                card_w,
                "Bass Shelf",
                &crate::components::info_box::INFO_BASS_SHELF,
                &mut state.bass_shelf_db,
                tone_range.clone(),
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.bass_shelf_changed = Some(v),
                theme,
            );

            // Card 2: Treble Shelf (7.5-10 kHz)
            if two_col {
                render_slider_card(
                    ui,
                    card_w,
                    "Treble Shelf",
                    &crate::components::info_box::INFO_TREBLE_SHELF,
                    &mut state.treble_shelf_db,
                    tone_range.clone(),
                    0.0,
                    min_lbl,
                    "0dB",
                    max_lbl,
                    |v| format!("{:+.1} dB", v),
                    |v| actions.treble_shelf_changed = Some(v),
                    theme,
                );
            }
        });

        if !two_col {
            render_slider_card(
                ui,
                card_w,
                "Treble Shelf",
                &crate::components::info_box::INFO_TREBLE_SHELF,
                &mut state.treble_shelf_db,
                tone_range,
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.treble_shelf_changed = Some(v),
                theme,
            );
        }

        // Row 2: Stereo Width & Balance
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 3: Stereo Width (0% to 200%)
            render_slider_card(
                ui,
                card_w,
                "Stereo Width",
                &crate::components::info_box::INFO_STEREO_WIDTH,
                &mut state.stereo_width,
                0.0..=2.0,
                1.0,
                "0 %",
                "100 %",
                "200 %",
                |v| format!("{:.0} %", v * 100.0),
                |v| actions.stereo_width_changed = Some(v),
                theme,
            );

            // Card 4: Balance (-1.00 to +1.00)
            if two_col {
                render_slider_card(
                    ui,
                    card_w,
                    "Balance",
                    &crate::components::info_box::INFO_BALANCE,
                    &mut state.balance,
                    -1.0..=1.0,
                    0.0,
                    "Left",
                    "Center",
                    "Right",
                    |v| {
                        if v.abs() < 0.01 {
                            "Center (0.00)".to_string()
                        } else if v < 0.0 {
                            format!("L {:.2}", -v)
                        } else {
                            format!("R {:.2}", v)
                        }
                    },
                    |v| actions.balance_changed = Some(v),
                    theme,
                );
            }
        });

        if !two_col {
            render_slider_card(
                ui,
                card_w,
                "Balance",
                &crate::components::info_box::INFO_BALANCE,
                &mut state.balance,
                -1.0..=1.0,
                0.0,
                "Left",
                "Center",
                "Right",
                |v| {
                    if v.abs() < 0.01 {
                        "Center (0.00)".to_string()
                    } else if v < 0.0 {
                        format!("L {:.2}", -v)
                    } else {
                        format!("R {:.2}", v)
                    }
                },
                |v| actions.balance_changed = Some(v),
                theme,
            );
        }

        // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────
        render_footer_controls(ui, state, actions, theme, EqTab::Tone);
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab 3: Advanced Settings (Max Dynamic Range, Headroom, M/S, Dither, AutoEQ)
// ─────────────────────────────────────────────────────────────────────────────

fn render_advanced_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let two_col = avail_w >= 540.0;
    let card_w = if two_col {
        (avail_w - 12.0) * 0.5
    } else {
        avail_w
    };

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        // Row 1: Extended Gain Range & Auto Headroom
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 1: Extended Dynamic Range (±24 dB Max Values Mode)
            render_advanced_toggle_card(
                ui,
                card_w,
                "Extended Gain Range",
                &crate::components::info_box::INFO_EXTENDED_RANGE,
                "Unlocks full ±24.0 dB boost/cut across Graphic EQ bands, Tone shelves, and Preamp.",
                if state.extended_range { "±24 dB Unlocked" } else { "±12 dB Standard" },
                state.extended_range,
                &mut state.extended_range,
                |_| {},
                theme,
            );

            // Card 2: Automatic Headroom Management
            if two_col {
                let mut ah = state.auto_headroom;
                render_advanced_toggle_card(
                    ui,
                    card_w,
                    "Auto Headroom Management",
                    &crate::components::info_box::INFO_AUTO_HEADROOM,
                    "Dynamically lowers master digital gain when EQ boosts exceed 0 dBFS to prevent clipping.",
                    if ah { "Auto Attenuation ON" } else { "Headroom Bypass" },
                    ah,
                    &mut ah,
                    |v| {
                        state.auto_headroom = v;
                        actions.eq_auto_headroom_changed = Some(v);
                    },
                    theme,
                );
            }
        });

        if !two_col {
            let mut ah = state.auto_headroom;
            render_advanced_toggle_card(
                ui,
                card_w,
                "Auto Headroom Management",
                &crate::components::info_box::INFO_AUTO_HEADROOM,
                "Dynamically lowers master digital gain when EQ boosts exceed 0 dBFS to prevent clipping.",
                if ah { "Auto Attenuation ON" } else { "Headroom Bypass" },
                ah,
                &mut ah,
                |v| {
                    state.auto_headroom = v;
                    actions.eq_auto_headroom_changed = Some(v);
                },
                theme,
            );
        }

        // Row 2: Mid/Side Processing & TPDF Dithering
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 3: Mid/Side Processing (M/S Mode)
            let mut ms = state.midside_eq;
            render_advanced_toggle_card(
                ui,
                card_w,
                "Mid/Side Processing (M/S)",
                &crate::components::info_box::INFO_MIDSIDE_EQ,
                "Splits audio into Mid (center) and Side (stereo field) channels for independent acoustic shaping.",
                if ms { "M/S Matrix Active" } else { "Stereo L/R Mode" },
                ms,
                &mut ms,
                |v| {
                    state.midside_eq = v;
                    actions.midside_eq_changed = Some(v);
                },
                theme,
            );

            // Card 4: TPDF Triangular Dithering
            if two_col {
                let mut dit = state.dither_enabled;
                render_advanced_toggle_card(
                    ui,
                    card_w,
                    "TPDF Triangular Dithering",
                    &crate::components::info_box::INFO_DITHER,
                    "High-resolution noise shaping that eliminates low-level quantization distortion on 32-bit float output.",
                    if dit { "32-bit TPDF Active" } else { "Truncation Mode" },
                    dit,
                    &mut dit,
                    |v| {
                        state.dither_enabled = v;
                        actions.dither_enabled_changed = Some(v);
                    },
                    theme,
                );
            }
        });

        if !two_col {
            let mut dit = state.dither_enabled;
            render_advanced_toggle_card(
                ui,
                card_w,
                "TPDF Triangular Dithering",
                &crate::components::info_box::INFO_DITHER,
                "High-resolution noise shaping that eliminates low-level quantization distortion on 32-bit float output.",
                if dit { "32-bit TPDF Active" } else { "Truncation Mode" },
                dit,
                &mut dit,
                |v| {
                    state.dither_enabled = v;
                    actions.dither_enabled_changed = Some(v);
                },
                theme,
            );
        }

        // Row 3: Headphone Calibration (AutoEQ Curve Database) Banner Card
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(Margin::symmetric(theme.space_3xl as i8, theme.space_xl as i8))
            .show(ui, |ui| {
                ui.set_width(avail_w - 32.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_2xl, 0.0);

                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::hover());
                    crate::components::icons::draw_headphones(ui, icon_rect, theme.accent_highlight);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                            ui.label(
                                egui::RichText::new("AutoEQ Headphone Target Calibration")
                                    .font(crate::theme::font(13.5))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            crate::components::info_box::render_info_icon_button(
                                ui,
                                "autoeq_calibration_card",
                                &crate::components::info_box::INFO_AUTOEQ,
                                theme,
                            );
                        });
                        ui.label(
                            egui::RichText::new("Calibrate your headphones or IEMs using Harman/Diffuse-field acoustic targets from 4,000+ lab measurements.")
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new("Open AutoEQ")
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(theme.radius_md))
                        .min_size(Vec2::new(110.0, 30.0));

                        if ui.add(btn).on_hover_text("Open headphone calibration search dialog").clicked() {
                            actions.open_autoeq_dialog = true;
                        }
                    });
                });
            });

        // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────
        render_footer_controls(ui, state, actions, theme, EqTab::Advanced);
    });
}

#[allow(clippy::too_many_arguments)]
fn render_advanced_toggle_card(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    info: &crate::components::info_box::InfoBoxContent,
    description: &str,
    badge_text: &str,
    is_active: bool,
    value: &mut bool,
    mut on_changed: impl FnMut(bool),
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(
            1.0,
            if is_active {
                theme.accent.linear_multiply(0.4)
            } else {
                theme.border_card
            },
        ))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_xl as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);

                // Row 1: Title, Clickable Info Icon, and Toggle Switch
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(ui, title, info, theme);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if custom_toggle_switch(ui, value, title, theme) {
                            on_changed(*value);
                        }
                    });
                });

                // Row 2: Status Pill Badge
                ui.horizontal(|ui| {
                    let badge_bg = if is_active {
                        theme.accent.linear_multiply(0.2)
                    } else {
                        theme.bg_pill
                    };
                    let badge_fg = if is_active {
                        theme.accent_warm
                    } else {
                        theme.text_muted
                    };
                    egui::Frame::NONE
                        .fill(badge_bg)
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(
                            theme.space_sm as i8,
                            theme.space_xxs as i8,
                        ))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(badge_text)
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(badge_fg),
                            );
                        });
                });

                // Row 3: Description text
                ui.label(
                    egui::RichText::new(description)
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn render_slider_card(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    info: &crate::components::info_box::InfoBoxContent,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_val: f32,
    left_lbl: &str,
    center_lbl: &str,
    right_lbl: &str,
    format_val: impl Fn(f32) -> String,
    mut on_changed: impl FnMut(f32),
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_lg as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);

                // Row 1: Title + Clickable Info Icon
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(13.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(ui, title, info, theme);
                });

                // Row 2: Range Labels (-12dB ... 0dB ... +12dB or -24dB ... 0dB ... +24dB)
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(left_lbl)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                    let avail_mid = (ui.available_width() - 48.0).max(40.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(avail_mid, 14.0),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new(center_lbl)
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_muted),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(right_lbl)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                    });
                });

                // Row 3: Gradient Slider
                ui.add_space(2.0);
                if gradient_horizontal_slider(ui, value, range, default_val, theme) {
                    on_changed(*value);
                }

                // Row 4: Numeric Readout Centered Below Slider
                ui.add_space(1.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format_val(*value))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Footer Controls: Preamp Slider & Tab-Specific Reset Button
// ─────────────────────────────────────────────────────────────────────────────

fn render_footer_controls(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
) {
    let avail_w = ui.available_width();
    let reset_btn_w = 95.0;
    let preamp_card_w = (avail_w - reset_btn_w - 12.0).max(200.0);

    let preamp_max = if state.extended_range { 24.0 } else { 12.0 };
    let min_lbl = if state.extended_range {
        "-24dB"
    } else {
        "-12dB"
    };
    let max_lbl = if state.extended_range {
        "+24dB"
    } else {
        "+12dB"
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

        // Preamp Card (All-in-one control shared across tabs)
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(Margin::symmetric(
                theme.space_2xl as i8,
                theme.space_md as i8,
            ))
            .show(ui, |ui| {
                ui.set_width(preamp_card_w - 28.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);

                    // Row 1: Label + Info Button
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                        ui.label(
                            egui::RichText::new("Preamp (Master)")
                                .font(crate::theme::font(12.5))
                                .strong()
                                .color(theme.text_primary),
                        );
                        crate::components::info_box::render_info_icon_button(
                            ui,
                            "eq_footer_master_preamp",
                            &crate::components::info_box::INFO_MASTER_PREAMP,
                            theme,
                        );
                    });

                    // Row 2: Range Labels & Slider
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                        ui.label(
                            egui::RichText::new(min_lbl)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );

                        let slider_w = (ui.available_width() - 50.0).max(80.0);
                        let mut p = state.graphic_preamp_db;
                        ui.allocate_ui_with_layout(
                            Vec2::new(slider_w, 20.0),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                if gradient_horizontal_slider(
                                    ui,
                                    &mut p,
                                    -preamp_max..=preamp_max,
                                    0.0,
                                    theme,
                                ) {
                                    state.graphic_preamp_db = p;
                                    state.preamp_db = p;
                                    actions.graphic_preamp_changed = Some(p);
                                    actions.eq_preamp_changed = Some(p);
                                }
                            },
                        );

                        ui.label(
                            egui::RichText::new(max_lbl)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                    });

                    // Row 3: Readout
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:+.1} dB", state.graphic_preamp_db))
                                .font(crate::theme::font(11.5))
                                .color(theme.text_secondary),
                        );
                    });
                });
            });

        // Tab-Specific Reset Button
        let (reset_rect, reset_resp) =
            ui.allocate_exact_size(Vec2::new(reset_btn_w, 58.0), Sense::click());
        reset_resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Reset this section")
        });
        let _ = reset_resp.clone().on_hover_text("Reset this section");
        let rp = ui.painter_at(reset_rect);
        let bg = if reset_resp.hovered() {
            theme.accent
        } else {
            theme.bg_card_hover
        };
        rp.rect_filled(reset_rect, CornerRadius::same(theme.radius_lg), bg);
        rp.rect_stroke(
            reset_rect,
            CornerRadius::same(theme.radius_lg),
            Stroke::new(1.0, theme.border_card),
            egui::StrokeKind::Inside,
        );

        // High quality SVG Reset icon and Reset text
        let icon_rect = Rect::from_center_size(
            Pos2::new(reset_rect.center().x + 22.0, reset_rect.center().y),
            Vec2::splat(16.0),
        );
        crate::components::icons::draw_reset(ui, icon_rect, theme.accent_highlight);

        rp.text(
            Pos2::new(reset_rect.center().x - 12.0, reset_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "Reset",
            crate::theme::font(13.0),
            theme.text_primary,
        );

        let tooltip = match tab {
            EqTab::Graphic => "Reset Graphic EQ band sliders to 0.0 dB flat",
            EqTab::Tone => "Reset Bass, Treble, Stereo Width, and Balance to defaults",
            EqTab::Advanced => "Reset Advanced dynamic range and DSP processing modes to defaults",
            _ => "Reset settings for this tab",
        };

        if reset_resp.on_hover_text(tooltip).clicked() {
            match tab {
                EqTab::Graphic => {
                    // Reset Graphic EQ bands and master preamp to 0.0 dB flat
                    let num = state.graphic_layout.num_bands();
                    for i in 0..num {
                        state.graphic_gains[i] = 0.0;
                        actions.graphic_slider_changed = Some(i);
                    }
                    state.graphic_preamp_db = 0.0;
                    actions.graphic_preamp_changed = Some(0.0);
                    state.selected_preset = EqPreset::Flat;
                    state.selected_custom_preset_idx = None;
                    actions.graphic_reset = true;
                }
                EqTab::Tone => {
                    // Reset Tone shelves, stereo width, balance, and master preamp
                    state.bass_shelf_db = 0.0;
                    actions.bass_shelf_changed = Some(0.0);

                    state.treble_shelf_db = 0.0;
                    actions.treble_shelf_changed = Some(0.0);

                    state.stereo_width = 1.0;
                    actions.stereo_width_changed = Some(1.0);

                    state.balance = 0.0;
                    actions.balance_changed = Some(0.0);

                    state.graphic_preamp_db = 0.0;
                    actions.graphic_preamp_changed = Some(0.0);
                }
                EqTab::Advanced => {
                    // Reset Advanced settings to audiophile defaults
                    state.extended_range = false;
                    state.auto_headroom = true;
                    actions.eq_auto_headroom_changed = Some(true);
                    state.midside_eq = false;
                    actions.midside_eq_changed = Some(false);
                    state.dither_enabled = false;
                    actions.dither_enabled_changed = Some(false);
                }
                _ => {}
            }
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Primitives: Gradient Slider, Toggle Switch, Spline Math, Vector Icons
// ─────────────────────────────────────────────────────────────────────────────

fn gradient_horizontal_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_val: f32,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(ui.available_width(), 18.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    let min = *range.start();
    let max = *range.end();

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new_val = min + t * (max - min);
            if (new_val - *value).abs() > 0.001 {
                *value = new_val;
                changed = true;
            }
        }
    }

    if resp.double_clicked() {
        *value = default_val;
        changed = true;
    }

    // Paint background track using theme token (clearly visible in both dark & light modes)
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(theme.radius_xs),
        theme.slider_track,
    );

    // Paint active track up to thumb (single jade hue).
    let thumb_t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let thumb_x = rect.left() + thumb_t * rect.width();
    let filled_w = (thumb_x - track_rect.min.x).max(0.0);
    if filled_w > 0.0 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(track_rect.min.x, track_rect.min.y),
            Pos2::new(track_rect.min.x + filled_w, track_rect.max.y),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(theme.radius_xs),
            theme.accent_highlight,
        );
    }

    // Paint circular thumb
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let is_hovered = resp.hovered() || resp.dragged();

    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 8.5 } else { 7.0 },
        theme
            .accent_highlight
            .gamma_multiply(if is_hovered { 0.35 } else { 0.18 }),
    );
    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 5.8 } else { 5.0 },
        Color32::WHITE,
    );
    ui.painter().circle_stroke(
        thumb_pos,
        if is_hovered { 5.8 } else { 5.0 },
        Stroke::new(1.6, theme.accent_highlight),
    );

    changed
}

fn custom_toggle_switch(ui: &mut egui::Ui, value: &mut bool, tooltip: &str, theme: &Theme) -> bool {
    let desired_size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());
    let mut changed = false;

    if resp.clicked() {
        *value = !*value;
        changed = true;
    }

    let p = ui.painter_at(rect);
    let bg = if *value {
        theme.accent
    } else {
        theme.border_card
    };
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);

    let thumb_x = if *value {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    p.circle_filled(thumb_pos, 7.5, Color32::WHITE);

    resp.on_hover_text(tooltip);
    changed
}

fn draw_dashed_line(painter: &egui::Painter, start: Pos2, end: Pos2, stroke: Stroke) {
    let total_len = (end - start).length();
    let dash_len = 5.0;
    let gap_len = 4.0;
    let dir = (end - start) / total_len;

    let mut cur = 0.0;
    while cur < total_len {
        let p1 = start + dir * cur;
        let p2 = start + dir * (cur + dash_len).min(total_len);
        painter.line_segment([p1, p2], stroke);
        cur += dash_len + gap_len;
    }
}

fn draw_vertical_gradient_line(
    painter: &egui::Painter,
    start: Pos2,
    end: Pos2,
    top_col: Color32,
    bottom_col: Color32,
    thickness: f32,
) {
    let steps = 14;
    let total_h = end.y - start.y;
    if total_h <= 0.0 {
        return;
    }

    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let y0 = start.y + t0 * total_h;
        let y1 = start.y + t1 * total_h;

        let col = crate::theme::lerp_color(top_col, bottom_col, t0);

        painter.line_segment(
            [Pos2::new(start.x, y0), Pos2::new(start.x, y1)],
            Stroke::new(thickness, col),
        );
    }
}

fn catmull_rom_spline(points: &[Pos2], segments: usize) -> Vec<Pos2> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let n = points.len();
    let mut result = Vec::with_capacity(n * segments);

    for i in 0..n - 1 {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n {
            points[i + 2]
        } else {
            points[i + 1]
        };

        for step in 0..segments {
            let t = step as f32 / segments as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);

            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);

            result.push(Pos2::new(x, y));
        }
    }
    result.push(*points.last().unwrap());
    result
}

//! The floating DSP/EQ studio.
//!
//! The panel is split into focused modules:
//!
//! - [`presets`] — factory/user presets and gain interpolation.
//! - [`parametric`] — the parametric-band model used by AutoEQ and chains.
//! - [`curve`] — the interactive frequency-response canvas and spline math.
//! - [`graphic`] — the Graphic Equalizer tab.
//! - [`tone`] — the Tone & Acoustics tab.
//! - [`advanced`] — the Advanced tab and its setting cards.
//! - [`controls`] — shared footer, slider, and toggle primitives.
//!
//! `mod.rs` owns the panel state, the action sink, and the window shell.

use crate::theme::Theme;
use config::{CrossfeedProfile, GraphicEqLayout};
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2, Window};
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

mod advanced;
mod controls;
mod curve;
mod graphic;
mod parametric;
mod presets;
mod tone;

use advanced::render_advanced_tab;
use graphic::render_graphic_eq_tab;
use tone::render_tone_acoustics_tab;

pub use controls::custom_toggle_switch;
pub use parametric::ParametricBand;
pub use presets::{CustomEqPreset, EqPreset};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EqTab {
    #[default]
    Graphic,
    Tone,
    Advanced,
    Parametric,
    Dynamics,
    Filters,
    /// The creative effects: chorus, flanger, phaser, delay and saturation.
    Effects,
}

pub struct EqPanelState {
    pub is_open: bool,
    /// Whether the studio was open last frame, so the keyboard focus can be
    /// moved into it exactly once, when it opens.
    pub was_open: bool,
    pub active_tab: EqTab,
    /// The graphic-EQ band the keyboard is currently editing. Not persisted:
    /// it describes focus, not a preference.
    pub keyboard_band: usize,
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

    // Creative effects (chorus, flanger, phaser, delay, saturation).
    //
    /// One struct, not five: the engine's insert is configured in one go, so
    /// the panel cannot describe a state the engine cannot run.
    pub fx: config::FxConfig,
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
            keyboard_band: 0,
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
            fx: config::FxConfig::default(),
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
    /// The creative-effects settings.
    #[serde(default)]
    pub fx: config::FxConfig,
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
            fx: s.fx,
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
        state.fx = self.fx;
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

    // Creative effects
    /// The creative-effects settings changed.
    pub fx_changed: bool,

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
            // The effects tab is not the equalizer, so it stays live even
            // when the equalizer is bypassed.
            let eq_active = state.graphic_enabled || state.eq_enabled;
            let fx_tab = state.active_tab == EqTab::Effects;
            let content_start_y = ui.cursor().top();

            ui.add_enabled_ui(eq_active || fx_tab, |ui| {
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
                            EqTab::Effects => {
                                actions.fx_changed |= crate::components::fx_panel::render_fx_tab(
                                    ui,
                                    &mut state.fx,
                                    theme,
                                );
                            }
                            _ => {}
                        }
                    });
            });

            // If EQ is bypassed, draw subtle darkening overlay & informative badge
            if !eq_active && !fx_tab {
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
            egui::RichText::new(tr!("Equalizer"))
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
            let _ = close_resp
                .clone()
                .on_hover_text(tr!(Msg::S35EC68DAC50375CB));
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
                .on_hover_text(tr!(Msg::S99DFAE3E6EE98739))
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
            (EqTab::Graphic, tr!("Graphic Equalizer")),
            (EqTab::Tone, tr!("Tone & Acoustics")),
            (EqTab::Advanced, tr!("Advanced")),
            (EqTab::Effects, tr!("Effects")),
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

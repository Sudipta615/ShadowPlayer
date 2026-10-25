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
//! - [`dynamics`] — the limiter and compressor tab.
//! - [`filters`] — crossfeed and correction controls.
//! - [`controls`] — shared footer, slider, and toggle primitives.
//! - [`actions`] — the typed action sink returned to the dispatcher.
//! - [`persistence`] — the serde snapshot used across launches.
//!
//! `mod.rs` owns the panel state and the window shell.

use crate::theme::Theme;
use config::{CrossfeedProfile, EqMode, GraphicEqLayout};
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2, Window};
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

mod actions;
mod advanced;
mod controls;
mod curve;
mod dynamics;
mod filters;
mod graphic;
mod parametric;
mod persistence;
mod preset_library;
mod presets;
mod tone;

use advanced::render_advanced_tab;
use dynamics::render_dynamics_tab;
use filters::render_filters_tab;
use graphic::render_graphic_eq_tab;
use tone::render_tone_acoustics_tab;

pub use actions::{EqPanelActions, PresetBindRequest, PresetSaveRequest};
pub use persistence::PersistentEqState;

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
    /// The single master switch. The graphic and parametric layers are both
    /// kept, but only the layer named by [`Self::eq_mode`] is audible.
    pub eq_enabled: bool,
    /// Which curve owns the engine's EQ stage.
    pub eq_mode: EqMode,
    /// The active parametric curve. AutoEQ can write 64 bands here; the
    /// summary view edits/opens the curve as a whole rather than truncating it
    /// to a fixed row count.
    pub bands: Vec<ParametricBand>,
    pub preamp_db: f32,
    pub auto_headroom: bool,

    /// Durable, DB-backed presets. The legacy `custom_presets` above predate
    /// the library and remain graphic-only; new saves go here so a curve can
    /// carry provenance and a device binding.
    pub library: crate::state::PresetLibraryState,

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
    /// Which compressor band the Dynamics tab is editing. Scratch state.
    pub comp_band: usize,

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
        let bands: Vec<ParametricBand> = default_freqs
            .iter()
            .map(|&freq| ParametricBand {
                freq,
                ..ParametricBand::default()
            })
            .collect();

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
            eq_mode: EqMode::Graphic,
            bands,
            preamp_db: 0.0,
            // Static auto-headroom is opt-in. It lowers the *whole* signal by
            // the curve's peak boost, so enabling it silently turns a +6 dB
            // bass slider into 0 dB at the centre and −6 dB everywhere else.
            auto_headroom: false,
            library: crate::state::PresetLibraryState::default(),

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

            // The output guard is on by default. Static auto-headroom is off,
            // so an aggressive boost keeps its level and is caught by the
            // look-ahead limiter only when it would actually clip.
            limiter_enabled: true,
            limiter_mode: LimiterMode::Transparent,
            limiter_true_peak: true,
            limiter_lookahead_ms: 5.0,
            limiter_attack_ms: 0.3,
            limiter_release_ms: 100.0,
            limiter_ceiling_db: -1.0,
            limiter_soft_clip: false,

            compressor_enabled: false,
            comp_threshold: [-24.0, -18.0, -18.0],
            comp_ratio: [4.0, 3.0, 3.0],
            comp_attack_ms: [10.0, 10.0, 5.0],
            comp_release_ms: [100.0, 100.0, 80.0],
            comp_makeup_db: [0.0; 3],
            comp_knee_db: [3.0; 3],
            comp_stereo_link: [true; 3],
            comp_band: 0,

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

impl EqPanelState {
    /// Switch band topology while preserving the visible curve.
    ///
    /// The old implementation changed the layout and then re-applied the
    /// selected preset, which destroyed a custom curve and left the engine's
    /// old bands active when the layout shrank. Interpolating the current
    /// gains onto the new ladder keeps the shape the user is looking at; the
    /// engine receives every new slider in the same atomic snapshot.
    pub fn set_graphic_layout(&mut self, layout: GraphicEqLayout) {
        let old_freqs = self.graphic_layout.frequencies();
        let old_count = old_freqs.len().min(self.graphic_gains.len());
        let old_gains = self.graphic_gains[..old_count].to_vec();
        let new_freqs = layout.frequencies();
        for (index, freq) in new_freqs.iter().enumerate().take(64) {
            self.graphic_gains[index] =
                presets::interpolate_curve_gain(*freq, &old_freqs[..old_count], &old_gains);
        }
        for slot in self.graphic_gains.iter_mut().skip(new_freqs.len().min(64)) {
            *slot = 0.0;
        }
        self.graphic_layout = layout;
    }

    /// Flatten the graphic curve and its shared preamp.
    pub fn reset_graphic_eq(&mut self) {
        self.graphic_gains.fill(0.0);
        self.preamp_db = 0.0;
        self.graphic_preamp_db = 0.0;
        self.selected_preset = EqPreset::Flat;
        self.selected_custom_preset_idx = None;
    }

    /// The master preamp. The graphic mirror exists only for old saved state.
    ///
    /// The engine can represent ±30 dB. Clamping here keeps the readout, the
    /// slider and the audible gain identical instead of showing a value the
    /// DSP silently truncated.
    pub fn set_preamp(&mut self, db: f32) {
        if !db.is_finite() {
            return;
        }
        let db = db.clamp(-30.0, 30.0);
        self.preamp_db = db;
        self.graphic_preamp_db = db;
    }

    /// Enter a mode and keep the legacy flags consistent with it.
    pub fn set_eq_mode(&mut self, mode: EqMode) {
        self.eq_mode = mode;
        self.graphic_enabled = self.eq_enabled && mode == EqMode::Graphic;
    }

    /// Whether the visible/curved values need the ±24 dB range.
    ///
    /// Extended range is a display preference, but the engine always applies
    /// the stored value. Disabling it while a curve exceeds ±12 dB would draw
    /// a clamped picture of a boost that is still audible, so every restore
    /// path keeps the range open when the curve needs it.
    pub fn needs_extended_range(&self) -> bool {
        let bands = self
            .graphic_layout
            .num_bands()
            .min(self.graphic_gains.len());
        self.preamp_db.abs() > 12.0
            || self.graphic_gains[..bands]
                .iter()
                .any(|gain| gain.abs() > 12.0)
    }

    /// Install a curve read from a text file.
    ///
    /// Returns the mode the curve installed: `true` for parametric, `false`
    /// for graphic. A parametric file replaces the whole band bank (never a
    /// prefix), and a graphic file is interpolated onto the ladder the user is
    /// currently looking at, so an AutoEQ `GraphicEQ.txt` does not move every
    /// slider when the layout is 31 or 64 bands.
    ///
    /// The preamp travels with the curve: it is what keeps the imported shape
    /// from clipping, and dropping it made an imported +6 dB bass shelf play
    /// 6 dB louder than the file asked for. Static auto-headroom is switched
    /// off for the same reason the AutoEQ dialog does it — the curve already
    /// carries its own level, and attenuating it twice is not "safer", it is
    /// quieter than intended.
    pub fn apply_imported_curve(&mut self, curve: &config::ImportedCurve) -> bool {
        self.eq_enabled = true;
        if curve.is_graphic() {
            self.eq_mode = EqMode::Graphic;
            self.graphic_enabled = true;
            let freqs = self.graphic_layout.frequencies();
            let (source_freqs, source_gains): (Vec<f32>, Vec<f32>) =
                curve.graphic.iter().copied().unzip();
            for (index, freq) in freqs.iter().enumerate().take(64) {
                self.graphic_gains[index] =
                    presets::interpolate_curve_gain(*freq, &source_freqs, &source_gains);
            }
            self.graphic_gains[freqs.len().min(64)..].fill(0.0);
            self.set_preamp(curve.preamp_db);
            self.selected_preset = EqPreset::Custom;
            self.selected_custom_preset_idx = None;
            if self.needs_extended_range() {
                self.extended_range = true;
            }
            false
        } else {
            self.eq_mode = EqMode::Parametric;
            self.graphic_enabled = false;
            self.bands = curve
                .bands
                .iter()
                .map(|band| ParametricBand {
                    freq: band.frequency,
                    gain_db: band.gain_db,
                    q: band.q,
                    filter_type: band.filter_type.into(),
                    enabled: band.enabled,
                })
                .collect();
            self.set_preamp(curve.preamp_db);
            self.auto_headroom = false;
            true
        }
    }

    /// Snapshot the audible curve as text and a suggested filename.
    ///
    /// Parametric mode exports every band; graphic mode exports the active
    /// ladder so the file reproduces exactly what the engine is playing.
    pub fn export_curve_text(&self) -> (String, String) {
        if self.eq_mode == EqMode::Parametric && !self.bands.is_empty() {
            let preset = config::EqPreset {
                name: "ShadowPlayer parametric EQ".to_string(),
                output_device_pattern: None,
                preamp_db: self.preamp_db,
                bands: self
                    .bands
                    .iter()
                    .map(|band| config::EqBandConfig {
                        enabled: band.enabled,
                        filter_type: band.filter_type.into(),
                        frequency: band.freq,
                        gain_db: band.gain_db,
                        q: band.q,
                    })
                    .collect(),
            };
            (
                "shadowplayer-parametric.txt".to_string(),
                config::write_parametric(&preset),
            )
        } else {
            let freqs = self.graphic_layout.frequencies();
            let count = freqs.len().min(self.graphic_gains.len());
            (
                "shadowplayer-graphic.txt".to_string(),
                config::write_graphic(&freqs[..count], &self.graphic_gains[..count]),
            )
        }
    }
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
            // The effects, dynamics and filters tabs are not the equalizer:
            // their stages have independent masters and can still be active
            // while the EQ curve is bypassed. Greying them out behind an
            // "Equalizer Bypassed" overlay hid live processing.
            let eq_active = state.eq_enabled;
            let independent_tab = matches!(
                state.active_tab,
                EqTab::Effects | EqTab::Dynamics | EqTab::Filters
            );
            let content_start_y = ui.cursor().top();

            ui.add_enabled_ui(eq_active || independent_tab, |ui| {
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
                            EqTab::Parametric => {
                                // The explicit mode selector lives in the
                                // Graphic tab; an old saved tab id still lands
                                // on a usable editor instead of a blank page.
                                render_graphic_eq_tab(ui, state, &mut actions, theme);
                            }
                            EqTab::Dynamics => {
                                render_dynamics_tab(ui, state, &mut actions, theme);
                            }
                            EqTab::Filters => {
                                render_filters_tab(ui, state, &mut actions, theme);
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
                        }
                    });
            });

            // Only EQ-owned tabs get the bypass overlay.
            if !eq_active && !independent_tab {
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

        // Master Toggle Switch: Enable Equalizer. This is the one enable
        // state; the legacy `graphic_enabled` flag mirrors the selected mode so
        // old chain presets keep loading correctly.
        let mut eq_active = state.eq_enabled;
        if custom_toggle_switch(ui, &mut eq_active, "Enable Equalizer", theme) {
            state.eq_enabled = eq_active;
            state.graphic_enabled = eq_active && state.eq_mode == EqMode::Graphic;
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
            (EqTab::Dynamics, tr!("Dynamics")),
            (EqTab::Filters, tr!("Filters")),
            (EqTab::Effects, tr!("Effects")),
            (EqTab::Advanced, tr!("Advanced")),
        ];

        let spacing = 5.0;
        let tab_h = 32.0;
        // Six tabs must still fit the default 760 px studio. The old fixed
        // 142 px buttons would have forced a horizontal scrollbar that hid
        // Dynamics and Filters entirely.
        let tab_w =
            ((ui.available_width() - 26.0 - spacing * (tab_items.len().saturating_sub(1)) as f32)
                / tab_items.len() as f32)
                .clamp(80.0, 128.0);
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
                                    (theme.accent, Color32::WHITE, crate::theme::font(11.5))
                                } else {
                                    (
                                        Color32::TRANSPARENT,
                                        theme.text_secondary,
                                        crate::theme::font(11.0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use engine::dsp::equalizer::EqFilterType;

    #[test]
    fn layout_change_interpolates_the_visible_curve_and_clears_stale_bands() {
        let mut state = EqPanelState::default();
        state.graphic_gains[..10]
            .copy_from_slice(&[4.0, 3.0, 2.5, 0.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0]);
        state.graphic_gains[10] = 99.0; // stale slot outside the 10-band layout

        state.set_graphic_layout(GraphicEqLayout::ThirtyOneBand);

        assert_eq!(state.graphic_layout, GraphicEqLayout::ThirtyOneBand);
        let freqs = state.graphic_layout.frequencies();
        for (freq, expected) in [
            (31.5, 4.0),
            (63.0, 3.0),
            (125.0, 2.5),
            (500.0, -1.0),
            (1000.0, -2.0),
            (16000.0, 4.0),
        ] {
            let index = freqs.iter().position(|f| (*f - freq).abs() < 1e-3).unwrap();
            assert!(
                (state.graphic_gains[index] - expected).abs() < 0.05,
                "{freq} Hz should survive the layout change, got {} dB",
                state.graphic_gains[index]
            );
        }
        assert!(
            state.graphic_gains[31..].iter().all(|g| *g == 0.0),
            "bands beyond the new layout must not keep stale gains"
        );

        state.set_graphic_layout(GraphicEqLayout::TenBand);
        assert_eq!(
            &state.graphic_gains[..10],
            &[4.0, 3.0, 2.5, 0.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0]
        );
        assert!(state.graphic_gains[10..].iter().all(|g| *g == 0.0));
    }

    #[test]
    fn legacy_persisted_state_infers_graphic_only_from_the_legacy_flag() {
        let legacy = PersistentEqState {
            eq_enabled: true,
            eq_mode: None,
            graphic_enabled: true,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        legacy.apply_to(&mut state);
        assert_eq!(state.eq_mode, EqMode::Graphic);
        assert!(state.graphic_enabled);

        let active_legacy_graphic = PersistentEqState {
            eq_enabled: false,
            eq_mode: None,
            graphic_enabled: true,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        active_legacy_graphic.apply_to(&mut state);
        assert!(state.eq_enabled, "the legacy graphic layer was audible");
        assert!(state.graphic_enabled);

        let parametric = PersistentEqState {
            eq_enabled: true,
            eq_mode: None,
            graphic_enabled: false,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        parametric.apply_to(&mut state);
        assert_eq!(state.eq_mode, EqMode::Parametric);
        assert!(!state.graphic_enabled);

        // An explicit mode is authoritative even if the old layer flag still
        // says the graphic EQ used to be active.
        let explicit = PersistentEqState {
            eq_enabled: true,
            eq_mode: Some(EqMode::Parametric),
            graphic_enabled: true,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        explicit.apply_to(&mut state);
        assert_eq!(state.eq_mode, EqMode::Parametric);
        assert!(!state.graphic_enabled);
    }

    #[test]
    fn persisted_extreme_graphic_gain_widens_the_visible_range() {
        let saved = PersistentEqState {
            eq_mode: Some(EqMode::Graphic),
            graphic_gains: vec![18.0; 10],
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        saved.apply_to(&mut state);
        assert!(state.extended_range);
        assert!((state.graphic_gains[0] - 18.0).abs() < 1e-6);
    }

    #[test]
    fn persisted_preamp_prefers_the_shared_field_and_backfills_the_mirror() {
        let saved = PersistentEqState {
            eq_mode: Some(EqMode::Parametric),
            preamp_db: -3.0,
            graphic_preamp_db: -8.0,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        saved.apply_to(&mut state);
        assert_eq!(state.preamp_db, -3.0);
        assert_eq!(state.graphic_preamp_db, -3.0);

        let legacy_graphic = PersistentEqState {
            eq_mode: None,
            graphic_enabled: true,
            preamp_db: 0.0,
            graphic_preamp_db: -6.0,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        legacy_graphic.apply_to(&mut state);
        assert_eq!(state.eq_mode, EqMode::Graphic);
        assert_eq!(state.preamp_db, -6.0);
        assert_eq!(state.graphic_preamp_db, -6.0);

        let inert_graphic = PersistentEqState {
            eq_mode: None,
            preamp_db: 0.0,
            graphic_preamp_db: -6.0,
            ..Default::default()
        };
        let mut state = EqPanelState::default();
        inert_graphic.apply_to(&mut state);
        assert_eq!(state.eq_mode, EqMode::Parametric);
        assert_eq!(state.preamp_db, 0.0);
    }

    #[test]
    fn preamp_is_clamped_to_the_range_the_engine_can_represent() {
        let mut state = EqPanelState::default();
        state.set_preamp(45.0);
        assert_eq!(state.preamp_db, 30.0);
        assert_eq!(state.graphic_preamp_db, 30.0);
        state.set_preamp(-45.0);
        assert_eq!(state.preamp_db, -30.0);
        state.set_preamp(f32::NAN);
        assert_eq!(state.preamp_db, -30.0);
    }

    #[test]
    fn parametric_import_installs_every_band_its_preamp_and_the_mode() {
        let text = "\
Preamp: -5.0 dB
Filter 1: ON LSC Fc 105 Hz Gain 4.0 dB Q 0.70
Filter 2: ON PK Fc 1000 Hz Gain -3.0 dB Q 1.20
Filter 3: ON HSC Fc 10000 Hz Gain 2.0 dB Q 0.70
";
        let curve = config::parse_curve("imported", text).unwrap();
        let mut state = EqPanelState {
            eq_enabled: false,
            auto_headroom: true,
            ..Default::default()
        };

        assert!(state.apply_imported_curve(&curve));

        assert!(state.eq_enabled);
        assert_eq!(state.eq_mode, EqMode::Parametric);
        assert!(!state.graphic_enabled);
        assert_eq!(state.bands.len(), 3);
        assert_eq!(state.bands[0].filter_type, EqFilterType::LowShelf);
        assert!((state.preamp_db + 5.0).abs() < 1e-6);
        assert!(
            !state.auto_headroom,
            "an imported curve carries its own level; attenuating it twice is wrong"
        );
    }

    #[test]
    fn graphic_import_lands_on_the_active_ladder_without_dropping_points() {
        let text = "GraphicEQ: 31.5 4.0; 1000 -2.0; 16000 3.0";
        let curve = config::parse_curve("graphic", text).unwrap();
        let mut state = EqPanelState {
            graphic_layout: GraphicEqLayout::ThirtyOneBand,
            ..Default::default()
        };

        assert!(!state.apply_imported_curve(&curve));

        assert_eq!(state.eq_mode, EqMode::Graphic);
        let freqs = state.graphic_layout.frequencies();
        let low = freqs.iter().position(|f| (*f - 31.5).abs() < 0.01).unwrap();
        let mid = freqs
            .iter()
            .position(|f| (*f - 1000.0).abs() < 0.01)
            .unwrap();
        assert!((state.graphic_gains[low] - 4.0).abs() < 0.1);
        assert!((state.graphic_gains[mid] + 2.0).abs() < 0.1);
    }

    #[test]
    fn exported_parametric_text_reimports_to_the_same_curve() {
        let text = "Preamp: -2.0 dB\nFilter 1: ON PK Fc 500 Hz Gain -4.0 dB Q 1.00";
        let curve = config::parse_curve("exported", text).unwrap();
        let mut state = EqPanelState::default();
        state.apply_imported_curve(&curve);

        let (name, exported) = state.export_curve_text();
        assert!(name.ends_with(".txt"));
        let reparsed = config::parse_curve("reimported", &exported).unwrap();
        assert_eq!(reparsed.bands.len(), 1);
        assert!((reparsed.preamp_db + 2.0).abs() < 1e-6);
        assert!((reparsed.bands[0].gain_db + 4.0).abs() < 1e-6);
    }
}

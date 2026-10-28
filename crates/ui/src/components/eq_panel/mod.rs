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
use eframe::egui::{self, CornerRadius, Margin, Pos2, Rect, Sense, Stroke, Vec2, Window};
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

mod actions;
mod advanced;
mod controls;
pub mod crossfeed;
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
pub use crossfeed::{CalibratedCrossfeedPreset, CrossfeedViewMode};
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
    pub extended_range: bool, // Extended ±24 dB range toggle
    /// User-placed window origin, persisted across launches.
    pub window_pos: Option<[f32; 2]>,
    /// User-resized window size, persisted across launches. Without this a
    /// shrunk panel reopened at the default size and egui's content-driven
    /// auto-expansion made it look like the resize had been ignored.
    pub window_size: Option<[f32; 2]>,

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
    pub crossfeed_view_mode: CrossfeedViewMode,

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
            crossfeed_view_mode: CrossfeedViewMode::default(),

            correction_enabled: false,
            correction_depth: 1.0,
            correction_ir_path: String::new(),
            fx: config::FxConfig::default(),
            window_pos: None,
            window_size: None,
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
            // The tab owns the model it renders; importing a curve of the other
            // kind must move the user to the page that now describes the sound.
            self.active_tab = EqTab::Graphic;
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
            self.active_tab = EqTab::Parametric;
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
    let min_size = crate::dialog::min_dialog_size();
    let max_size = Vec2::new(
        880.0_f32.min((screen.width() - 20.0).max(min_size.x)),
        (screen.height() - 40.0).max(min_size.y),
    );
    let default_w = 760.0_f32.min(max_size.x);
    let default_h = 560.0_f32.min(max_size.y);
    // Restore the size the user last dragged to. Clamp it to the current
    // screen so a monitor change cannot open the panel off-screen.
    let initial_size = state
        .window_size
        .map(|[w, h]| {
            Vec2::new(
                w.clamp(min_size.x, max_size.x),
                h.clamp(min_size.y, max_size.y),
            )
        })
        .unwrap_or(Vec2::new(default_w, default_h));
    let default_x = ((screen.width() - initial_size.x) / 2.0).max(10.0);
    let default_y = ((screen.height() - initial_size.y) / 2.0).max(10.0);
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
        .default_size(initial_size)
        .min_size(min_size)
        .max_size(max_size)
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

            // ── 2. Tab Switcher ──────────────────────────────────────────────
            controls::render_tab_navigation(ui, state, &mut actions, theme);

            // ── 3. Enabled UI Wrapper with Visual Greying-Out ─────────────────
            // The master switch is the one enable state for the entire studio.
            // Every tab describes a stage that runs through the EQ graph, so a
            // bypassed EQ greys all of them out together instead of leaving
            // Dynamics, Filters and Effects looking editable.
            let eq_active = state.eq_enabled;
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
                            EqTab::Parametric => {
                                parametric::render_parametric_tab(ui, state, &mut actions, theme);
                            }
                            EqTab::Tone => {
                                render_tone_acoustics_tab(ui, state, &mut actions, theme);
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

            // The whole studio is bypassed, so cover the content and say why.
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

        // Remember a user resize. This is what makes the next launch open at
        // the width the user chose instead of the default 760 px.
        let current_size = rect.size();
        let changed = state
            .window_size
            .map(|[w, h]| (w - current_size.x).abs() > 1.0 || (h - current_size.y).abs() > 1.0)
            .unwrap_or(true);
        if changed {
            state.window_size = Some([current_size.x, current_size.y]);
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

#[cfg(test)]
mod tests;

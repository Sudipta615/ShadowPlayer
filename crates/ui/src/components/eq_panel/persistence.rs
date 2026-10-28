//! Serialisable snapshot of the EQ studio for app persistence.

use super::{CrossfeedViewMode, CustomEqPreset, EqPanelState, EqPreset, EqTab, ParametricBand};
use config::{CrossfeedProfile, EqMode, GraphicEqLayout};
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

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
    /// The last size the user dragged the studio to. `None` means the default.
    pub window_size: Option<[f32; 2]>,
    pub active_tab: EqTab,
    pub eq_enabled: bool,
    /// `None` in state written before the mode existed; `apply_to` infers it
    /// from the legacy `graphic_enabled` flag rather than resetting the user
    /// to parametric bands they never chose. The field-level default matters:
    /// the container default is a fresh panel, whose mode is already Graphic.
    #[serde(default)]
    pub eq_mode: Option<EqMode>,
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
    /// Parametric bands — AutoEQ targets can write up to 64, so the complete
    /// bank must survive a restart and a chain-preset round trip.
    pub bands: Vec<ParametricBand>,
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
    #[serde(default)]
    pub crossfeed_view_mode: CrossfeedViewMode,
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
            window_size: s.window_size,
            active_tab: s.active_tab,
            eq_enabled: s.eq_enabled,
            eq_mode: Some(s.eq_mode),
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
            crossfeed_view_mode: s.crossfeed_view_mode,
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
        state.window_pos = self
            .window_pos
            .filter(|[x, y]| x.is_finite() && y.is_finite());
        state.window_size = self
            .window_size
            .filter(|[w, h]| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0);
        state.active_tab = self.active_tab;
        // Before the explicit mode existed the header master was the OR of
        // the two layer flags. A persisted graphic layer that was on is
        // therefore audible state even if the parametric flag was off.
        let legacy_graphic = self.eq_mode.is_none() && self.graphic_enabled;
        state.eq_enabled = self.eq_enabled || legacy_graphic;
        state.eq_mode = self.eq_mode.unwrap_or(if self.graphic_enabled {
            EqMode::Graphic
        } else {
            EqMode::Parametric
        });
        state.graphic_enabled = state.eq_enabled && state.eq_mode == EqMode::Graphic;
        // Graphic and Parametric are the two tabs that own a curve mode. A
        // snapshot written before the mode existed can pair the wrong tab with
        // the audible stage; reconcile it so the selected page always describes
        // what the engine is playing.
        match (state.active_tab, state.eq_mode) {
            (EqTab::Graphic, EqMode::Parametric) => state.active_tab = EqTab::Parametric,
            (EqTab::Parametric, EqMode::Graphic) => state.active_tab = EqTab::Graphic,
            _ => {}
        }
        state.graphic_layout = self.graphic_layout.clone();
        for (i, &g) in self.graphic_gains.iter().take(64).enumerate() {
            state.graphic_gains[i] = g;
        }
        state.selected_preset = self.selected_preset;
        state.custom_presets = self.custom_presets.clone();
        state.selected_custom_preset_idx = self.selected_custom_preset_idx;
        state.extended_range = self.extended_range;
        if state.needs_extended_range() {
            state.extended_range = true;
        }
        // Two persisted preamp fields grew from the old ownership split. The
        // mode tells us which one the user could hear; a nonzero value on the
        // inactive stage must not become the new shared preamp.
        let preamp = match state.eq_mode {
            EqMode::Graphic => self.graphic_preamp_db,
            EqMode::Parametric => self.preamp_db,
        };
        state.set_preamp(preamp);
        state.auto_headroom = self.auto_headroom;
        state.bands = self.bands.clone();
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
        state.crossfeed_view_mode = self.crossfeed_view_mode;
        state.correction_enabled = self.correction_enabled;
        state.correction_depth = self.correction_depth;
        state.correction_ir_path = self.correction_ir_path.clone();
        state.fx = self.fx;
    }
}

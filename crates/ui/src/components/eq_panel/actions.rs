//! Typed action sink returned by the EQ studio.

use config::CrossfeedProfile;
use engine::dsp::limiter::LimiterMode;

/// Save the active curve under `name`, optionally binding it to a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetSaveRequest {
    pub name: String,
    /// Case-insensitive substring of an output device name.
    pub output_pattern: Option<String>,
    pub is_default: bool,
}

/// Set or clear one preset's device pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetBindRequest {
    pub id: String,
    /// `None` clears the binding.
    pub output_pattern: Option<String>,
}

#[derive(Default)]
pub struct EqPanelActions {
    // Equalizer On/Off
    /// The master switch changed.
    pub eq_enabled_changed: Option<bool>,
    /// The graphic/parametric mode changed.
    pub eq_mode_changed: bool,
    /// Any part of the graphic model changed: layout, one or more sliders,
    /// or the shared preamp. The dispatcher sends the complete state as one
    /// command, so a preset can never lose all but its last band.
    pub graphic_eq_changed: bool,

    // Sliders & Preamp
    pub eq_band_changed: Option<usize>,
    /// Replace the whole parametric bank from the panel's stored curve
    /// (currently a flat reset). The dispatcher sends one atomic command, so
    /// a 64-band AutoEQ curve cannot leave its tail audible.
    pub eq_bands_changed: bool,
    pub eq_auto_headroom_changed: Option<bool>,

    // Tone & Acoustics
    pub bass_shelf_changed: Option<f32>,
    pub treble_shelf_changed: Option<f32>,
    pub midside_eq_changed: Option<bool>,
    pub stereo_width_changed: Option<f32>,
    pub balance_changed: Option<f32>,
    pub dither_enabled_changed: Option<bool>,

    // External Modals & Presets
    pub open_autoeq_dialog: bool,
    /// Read a text curve (AutoEQ / APO / REW / GraphicEQ) from disk.
    pub import_curve: bool,
    /// Write the active curve as text.
    pub export_curve: bool,
    /// Save the active curve into the durable preset library.
    pub preset_save: Option<PresetSaveRequest>,
    /// Apply a library preset by id.
    pub preset_apply: Option<String>,
    pub preset_delete: Option<String>,
    /// Make a library preset the single global default.
    pub preset_default: Option<String>,
    /// Change (or clear) a preset's output-device pattern.
    pub preset_bind: Option<PresetBindRequest>,
    /// Point the current track at a preset.
    pub preset_bind_track: Option<String>,
    /// Remove the current track's preset pointer.
    pub preset_unbind_track: bool,

    // Dynamics (limiter/comp)
    pub limiter_enabled_changed: Option<bool>,
    pub limiter_mode_changed: Option<LimiterMode>,
    pub limiter_true_peak_changed: Option<bool>,
    pub limiter_params_changed: bool,
    pub compressor_enabled_changed: Option<bool>,
    pub comp_band_params_changed: Option<usize>,
    pub comp_band_features_changed: Option<usize>,
    /// Reset all three compressor bands in one frame.
    pub comp_params_reset: bool,

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
    /// Open the acoustic alignment and speaker setup wizard.
    pub open_speaker_setup: bool,
}

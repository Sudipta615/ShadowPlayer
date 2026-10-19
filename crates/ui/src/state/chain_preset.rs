//! Named, persistent snapshots of the entire DSP chain.
//!
//! The EQ panel can already save a *curve* (`CustomEqPreset`), but an audiophile
//! setup is rarely just a curve: a headphone correction profile is a parametric
//! curve plus a crossfeed setting plus a preamp, and a speaker setup is a
//! crossover plus LFE trim plus an up/down-mix choice. Rebuilding that by hand
//! on every switch is the reason people keep notes.
//!
//! A [`ChainPreset`] captures the whole signal path in one named object:
//!
//! - Equalizer: parametric bands, graphic sliders, preamp, auto headroom
//! - Tone: bass/treble shelves, mid/side, stereo width, balance
//! - Dynamics: limiter and multiband compressor
//! - Filters: crossfeed, room/headphone correction
//! - Mix bus: bass management crossover, LFE, up/down mixing
//!
//! It deliberately does **not** capture device selection, resampler policy or
//! spatial rendering: those describe the hardware and the room, not the chain,
//! and swapping them mid-track would restart the output device.
//!
//! Applying a preset only writes into the two state structs; the caller pushes
//! the result to the audio session with `DspState::sync_chain_to_session`.

use config::{
    BassManagementConfig, ChannelMixConfig, CrossfeedProfile, GraphicEqLayout, LfeConfig,
};
use engine::dsp::limiter::LimiterMode;
use serde::{Deserialize, Serialize};

use crate::components::eq_panel::{EqPanelState, ParametricBand};
use crate::components::settings_tab::SettingsTabState;

/// Parametric bands exposed by the EQ panel.
const PARAMETRIC_BANDS: usize = 10;

fn copy_f32s(dst: &mut [f32], src: &[f32]) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d = *s;
    }
}

fn copy_bools(dst: &mut [bool], src: &[bool]) {
    for (d, s) in dst.iter_mut().zip(src.iter()) {
        *d = *s;
    }
}

/// The equalizer side of the chain: everything the EQ panel owns that changes
/// the signal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChainEq {
    // ── Equalizer ────────────────────────────────────────────────
    pub eq_enabled: bool,
    pub bands: Vec<ParametricBand>,
    pub preamp_db: f32,
    pub auto_headroom: bool,
    pub graphic_enabled: bool,
    pub graphic_layout: GraphicEqLayout,
    pub graphic_gains: Vec<f32>,
    pub graphic_preamp_db: f32,

    // ── Tone ─────────────────────────────────────────────────────
    pub bass_shelf_db: f32,
    pub treble_shelf_db: f32,
    pub midside_eq: bool,
    pub stereo_width: f32,
    pub balance: f32,
    pub dither_enabled: bool,

    // ── Dynamics ─────────────────────────────────────────────────
    pub limiter_enabled: bool,
    pub limiter_mode: LimiterMode,
    pub limiter_true_peak: bool,
    pub limiter_lookahead_ms: f32,
    pub limiter_attack_ms: f32,
    pub limiter_release_ms: f32,
    pub limiter_ceiling_db: f32,
    pub limiter_soft_clip: bool,

    pub compressor_enabled: bool,
    pub comp_threshold: Vec<f32>,
    pub comp_ratio: Vec<f32>,
    pub comp_attack_ms: Vec<f32>,
    pub comp_release_ms: Vec<f32>,
    pub comp_makeup_db: Vec<f32>,
    pub comp_knee_db: Vec<f32>,
    pub comp_stereo_link: Vec<bool>,

    // ── Filters & correction ─────────────────────────────────────
    pub crossfeed_enabled: bool,
    pub crossfeed_profile: CrossfeedProfile,
    pub crossfeed_freq: f32,
    pub crossfeed_q: f32,
    pub crossfeed_delay_ms: f32,

    pub correction_enabled: bool,
    pub correction_depth: f32,
    pub correction_ir_path: String,
}

impl Default for ChainEq {
    /// A chain preset with no fields present deserialises to the panel's own
    /// defaults, so an older or partial preset never invents odd values.
    fn default() -> Self {
        Self::capture(&EqPanelState::default())
    }
}

impl ChainEq {
    /// Snapshot the signal-affecting state of the EQ panel.
    pub fn capture(panel: &EqPanelState) -> Self {
        Self {
            eq_enabled: panel.eq_enabled,
            bands: panel.bands.to_vec(),
            preamp_db: panel.preamp_db,
            auto_headroom: panel.auto_headroom,
            graphic_enabled: panel.graphic_enabled,
            graphic_layout: panel.graphic_layout.clone(),
            graphic_gains: panel.graphic_gains.to_vec(),
            graphic_preamp_db: panel.graphic_preamp_db,

            bass_shelf_db: panel.bass_shelf_db,
            treble_shelf_db: panel.treble_shelf_db,
            midside_eq: panel.midside_eq,
            stereo_width: panel.stereo_width,
            balance: panel.balance,
            dither_enabled: panel.dither_enabled,

            limiter_enabled: panel.limiter_enabled,
            limiter_mode: panel.limiter_mode,
            limiter_true_peak: panel.limiter_true_peak,
            limiter_lookahead_ms: panel.limiter_lookahead_ms,
            limiter_attack_ms: panel.limiter_attack_ms,
            limiter_release_ms: panel.limiter_release_ms,
            limiter_ceiling_db: panel.limiter_ceiling_db,
            limiter_soft_clip: panel.limiter_soft_clip,

            compressor_enabled: panel.compressor_enabled,
            comp_threshold: panel.comp_threshold.to_vec(),
            comp_ratio: panel.comp_ratio.to_vec(),
            comp_attack_ms: panel.comp_attack_ms.to_vec(),
            comp_release_ms: panel.comp_release_ms.to_vec(),
            comp_makeup_db: panel.comp_makeup_db.to_vec(),
            comp_knee_db: panel.comp_knee_db.to_vec(),
            comp_stereo_link: panel.comp_stereo_link.to_vec(),

            crossfeed_enabled: panel.crossfeed_enabled,
            crossfeed_profile: panel.crossfeed_profile,
            crossfeed_freq: panel.crossfeed_freq,
            crossfeed_q: panel.crossfeed_q,
            crossfeed_delay_ms: panel.crossfeed_delay_ms,

            correction_enabled: panel.correction_enabled,
            correction_depth: panel.correction_depth,
            correction_ir_path: panel.correction_ir_path.clone(),
        }
    }

    /// Write this snapshot back into the EQ panel.
    ///
    /// Length mismatches are tolerated by copying only what lines up, so a
    /// preset saved against a different band count still restores the rest.
    pub fn apply_to(&self, panel: &mut EqPanelState) {
        panel.eq_enabled = self.eq_enabled;
        let bands = self.bands.len().min(PARAMETRIC_BANDS);
        for i in 0..bands {
            panel.bands[i] = self.bands[i].clone();
        }
        panel.preamp_db = self.preamp_db;
        panel.auto_headroom = self.auto_headroom;
        panel.graphic_enabled = self.graphic_enabled;
        panel.graphic_layout = self.graphic_layout.clone();
        copy_f32s(&mut panel.graphic_gains, &self.graphic_gains);
        panel.graphic_preamp_db = self.graphic_preamp_db;

        panel.bass_shelf_db = self.bass_shelf_db;
        panel.treble_shelf_db = self.treble_shelf_db;
        panel.midside_eq = self.midside_eq;
        panel.stereo_width = self.stereo_width;
        panel.balance = self.balance;
        panel.dither_enabled = self.dither_enabled;

        panel.limiter_enabled = self.limiter_enabled;
        panel.limiter_mode = self.limiter_mode;
        panel.limiter_true_peak = self.limiter_true_peak;
        panel.limiter_lookahead_ms = self.limiter_lookahead_ms;
        panel.limiter_attack_ms = self.limiter_attack_ms;
        panel.limiter_release_ms = self.limiter_release_ms;
        panel.limiter_ceiling_db = self.limiter_ceiling_db;
        panel.limiter_soft_clip = self.limiter_soft_clip;

        panel.compressor_enabled = self.compressor_enabled;
        copy_f32s(&mut panel.comp_threshold, &self.comp_threshold);
        copy_f32s(&mut panel.comp_ratio, &self.comp_ratio);
        copy_f32s(&mut panel.comp_attack_ms, &self.comp_attack_ms);
        copy_f32s(&mut panel.comp_release_ms, &self.comp_release_ms);
        copy_f32s(&mut panel.comp_makeup_db, &self.comp_makeup_db);
        copy_f32s(&mut panel.comp_knee_db, &self.comp_knee_db);
        copy_bools(&mut panel.comp_stereo_link, &self.comp_stereo_link);

        panel.crossfeed_enabled = self.crossfeed_enabled;
        panel.crossfeed_profile = self.crossfeed_profile;
        panel.crossfeed_freq = self.crossfeed_freq;
        panel.crossfeed_q = self.crossfeed_q;
        panel.crossfeed_delay_ms = self.crossfeed_delay_ms;

        panel.correction_enabled = self.correction_enabled;
        panel.correction_depth = self.correction_depth;
        panel.correction_ir_path = self.correction_ir_path.clone();
    }

    /// Short human-readable summary of what is engaged.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.graphic_enabled {
            parts.push(format!("{} graphic", self.graphic_layout.num_bands()));
        }
        if self.eq_enabled {
            parts.push("parametric EQ".to_string());
        }
        if self.bass_shelf_db.abs() > 0.05 || self.treble_shelf_db.abs() > 0.05 {
            parts.push(format!(
                "tone {:+.1}/{:+}",
                self.bass_shelf_db, self.treble_shelf_db
            ));
        }
        if self.limiter_enabled {
            parts.push("limiter".to_string());
        }
        if self.compressor_enabled {
            parts.push("compressor".to_string());
        }
        if self.crossfeed_enabled {
            parts.push("crossfeed".to_string());
        }
        if self.correction_enabled {
            parts.push("correction".to_string());
        }
        if parts.is_empty() {
            parts.push("flat".to_string());
        }
        parts.join(" · ")
    }
}

/// One named snapshot of the whole DSP chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainPreset {
    pub name: String,
    /// Optional free-form note, e.g. which headphones it was measured for.
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub eq: ChainEq,
    #[serde(default)]
    pub bass_management: BassManagementConfig,
    #[serde(default)]
    pub lfe: LfeConfig,
    #[serde(default)]
    pub channel_mix: ChannelMixConfig,
}

impl ChainPreset {
    /// Capture the current chain under a name.
    pub fn capture(
        name: impl Into<String>,
        note: impl Into<String>,
        eq_panel: &EqPanelState,
        settings: &SettingsTabState,
    ) -> Self {
        Self {
            name: name.into(),
            note: note.into(),
            eq: ChainEq::capture(eq_panel),
            bass_management: settings.bass_management.clone(),
            lfe: settings.lfe.clone(),
            channel_mix: settings.channel_mix.clone(),
        }
    }

    /// Write this preset into the live EQ panel and settings state.
    pub fn apply(&self, eq_panel: &mut EqPanelState, settings: &mut SettingsTabState) {
        self.eq.apply_to(eq_panel);
        settings.bass_management = self.bass_management.clone();
        settings.lfe = self.lfe.clone();
        settings.channel_mix = self.channel_mix.clone();
    }

    /// One-line description for the preset list.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.eq.summary()];
        if self.bass_management.enabled {
            parts.push(format!("sub {:.0}Hz", self.bass_management.crossover_hz));
        }
        if self.lfe.enabled {
            parts.push("LFE".to_string());
        }
        if self.channel_mix.enabled {
            parts.push("remix".to_string());
        }
        parts.join(" · ")
    }
}

/// Insert or replace a preset by name, returning its index.
///
/// Names are the user-facing identity, so saving the same name twice
/// overwrites rather than accumulating near-duplicates.
pub fn upsert(presets: &mut Vec<ChainPreset>, preset: ChainPreset) -> usize {
    if let Some(idx) = presets
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(&preset.name))
    {
        presets[idx] = preset;
        idx
    } else {
        presets.push(preset);
        presets.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::dsp::equalizer::EqFilterType;

    fn engaged_panel() -> EqPanelState {
        let mut panel = EqPanelState {
            eq_enabled: true,
            graphic_enabled: true,
            preamp_db: -6.5,
            auto_headroom: false,
            bass_shelf_db: 3.0,
            treble_shelf_db: -2.0,
            stereo_width: 1.4,
            balance: -0.2,
            limiter_enabled: true,
            limiter_mode: LimiterMode::Saturate,
            limiter_ceiling_db: -0.3,
            compressor_enabled: true,
            crossfeed_enabled: true,
            crossfeed_profile: CrossfeedProfile::Jmeier,
            crossfeed_freq: 620.0,
            correction_enabled: true,
            correction_depth: 0.75,
            correction_ir_path: "/irs/room.wav".to_string(),
            ..EqPanelState::default()
        };
        panel.comp_threshold[0] = -30.0;
        panel.comp_ratio[2] = 6.0;
        panel.comp_stereo_link[1] = false;
        panel.bands[3] = ParametricBand {
            freq: 250.0,
            gain_db: -4.5,
            q: 1.8,
            filter_type: EqFilterType::Notch,
            enabled: false,
        };
        panel.graphic_gains[5] = 7.5;
        panel
    }

    fn engaged_settings() -> SettingsTabState {
        let mut settings = SettingsTabState::default();
        settings.bass_management.enabled = true;
        settings.bass_management.crossover_hz = 95.0;
        settings.bass_management.sub_delay_ms = 12.5;
        settings.lfe.enabled = true;
        settings.lfe.gain_db = -4.0;
        settings.lfe.crossover_hz = Some(120.0);
        settings.channel_mix.enabled = true;
        settings
    }

    #[test]
    fn capture_then_apply_restores_the_whole_chain() {
        let panel = engaged_panel();
        let settings = engaged_settings();
        let preset = ChainPreset::capture("Headphones", "HD650", &panel, &settings);

        // Wipe everything, then apply: every engaged field must come back.
        let mut panel2 = EqPanelState::default();
        let mut settings2 = SettingsTabState::default();
        preset.apply(&mut panel2, &mut settings2);

        assert_eq!(panel2.preamp_db, -6.5);
        assert!(!panel2.auto_headroom);
        assert_eq!(panel2.bass_shelf_db, 3.0);
        assert_eq!(panel2.balance, -0.2);
        assert_eq!(panel2.limiter_mode, LimiterMode::Saturate);
        assert_eq!(panel2.comp_threshold[0], -30.0);
        assert_eq!(panel2.comp_ratio[2], 6.0);
        assert!(!panel2.comp_stereo_link[1]);
        assert_eq!(panel2.crossfeed_profile, CrossfeedProfile::Jmeier);
        assert_eq!(panel2.crossfeed_freq, 620.0);
        assert_eq!(panel2.correction_depth, 0.75);
        assert_eq!(panel2.correction_ir_path, "/irs/room.wav");
        assert_eq!(panel2.bands[3].filter_type, EqFilterType::Notch);
        assert_eq!(panel2.bands[3].gain_db, -4.5);
        assert!(!panel2.bands[3].enabled);
        assert_eq!(panel2.graphic_gains[5], 7.5);

        assert!(settings2.bass_management.enabled);
        assert_eq!(settings2.bass_management.crossover_hz, 95.0);
        assert_eq!(settings2.bass_management.sub_delay_ms, 12.5);
        assert_eq!(settings2.lfe.gain_db, -4.0);
        assert_eq!(settings2.lfe.crossover_hz, Some(120.0));
        assert!(settings2.channel_mix.enabled);

        // And a re-capture must be byte-identical.
        let again = ChainPreset::capture("Headphones", "HD650", &panel2, &settings2);
        assert_eq!(again, preset);
    }

    #[test]
    fn applying_never_mutates_untouched_panel_fields() {
        let preset = ChainPreset::capture(
            "Flat",
            "",
            &EqPanelState::default(),
            &SettingsTabState::default(),
        );
        let mut panel = EqPanelState {
            is_open: true,
            window_pos: Some([10.0, 20.0]),
            active_tab: crate::components::eq_panel::EqTab::Dynamics,
            selected_custom_preset_idx: Some(2),
            extended_range: true,
            ..EqPanelState::default()
        };

        preset.apply(&mut panel, &mut SettingsTabState::default());

        // Presentation state is not part of the chain and must survive.
        assert!(panel.is_open);
        assert_eq!(panel.window_pos, Some([10.0, 20.0]));
        assert_eq!(panel.selected_custom_preset_idx, Some(2));
        assert!(panel.extended_range);
    }

    #[test]
    fn preset_json_tolerates_missing_fields() {
        // A preset written by an older build: only a name and a preamp.
        let json = r#"{"name":"Legacy","eq":{"preamp_db":-3.0}}"#;
        let preset: ChainPreset = serde_json::from_str(json).unwrap();
        assert_eq!(preset.name, "Legacy");
        assert_eq!(preset.eq.preamp_db, -3.0);
        // Missing pieces fall back to the panel defaults, not to zeros.
        let defaults = EqPanelState::default();
        assert_eq!(preset.eq.auto_headroom, defaults.auto_headroom);
        assert_eq!(preset.eq.bands.len(), defaults.bands.len());
        assert_eq!(preset.channel_mix, ChannelMixConfig::default());
    }

    #[test]
    fn round_trips_through_json() {
        let preset = ChainPreset::capture(
            "Speakers",
            "living room",
            &engaged_panel(),
            &engaged_settings(),
        );
        let json = serde_json::to_string(&preset).unwrap();
        let back: ChainPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, preset);
    }

    #[test]
    fn upsert_replaces_case_insensitively() {
        let mut presets = Vec::new();
        let mut p = ChainPreset::capture(
            "Speakers",
            "",
            &EqPanelState::default(),
            &SettingsTabState::default(),
        );
        let idx = upsert(&mut presets, p.clone());
        assert_eq!(idx, 0);

        p.eq.preamp_db = -4.0;
        p.name = "speakers".to_string();
        let idx = upsert(&mut presets, p.clone());
        assert_eq!(idx, 0, "same name should overwrite");
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].eq.preamp_db, -4.0);
    }

    /// The EQ snapshot must survive a save/restore cycle in full: the bands in
    /// particular are what AutoEQ targets, and losing them meant a measured
    /// correction silently vanished on restart.
    #[test]
    fn eq_persistence_round_trips_the_whole_chain() {
        use crate::components::eq_panel::PersistentEqState;

        let panel = engaged_panel();
        let json = serde_json::to_string(&PersistentEqState::from(&panel)).unwrap();
        let parsed: PersistentEqState = serde_json::from_str(&json).unwrap();

        let mut restored = EqPanelState::default();
        parsed.apply_to(&mut restored);

        assert_eq!(restored.bands[3].freq, 250.0);
        assert_eq!(restored.bands[3].gain_db, -4.5);
        assert_eq!(restored.bands[3].filter_type, EqFilterType::Notch);
        assert!(!restored.bands[3].enabled);
        assert_eq!(restored.limiter_mode, LimiterMode::Saturate);
        assert_eq!(restored.limiter_ceiling_db, -0.3);
        assert_eq!(restored.comp_ratio[2], 6.0);
        assert!(!restored.comp_stereo_link[1]);
        assert_eq!(restored.crossfeed_freq, 620.0);
        assert_eq!(restored.correction_ir_path, "/irs/room.wav");
    }

    /// A payload written before the chain fields existed must not fail to
    /// parse: `serde_json::from_str` errors are swallowed at the call site, so
    /// a hard failure would silently reset every EQ setting the user had.
    #[test]
    fn eq_persistence_tolerates_a_legacy_payload() {
        use crate::components::eq_panel::PersistentEqState;

        let mut value =
            serde_json::to_value(PersistentEqState::from(&EqPanelState::default())).unwrap();
        let obj = value.as_object_mut().unwrap();
        for key in [
            "bands",
            "limiter_lookahead_ms",
            "comp_threshold",
            "comp_stereo_link",
            "crossfeed_freq",
            "correction_depth",
        ] {
            obj.remove(key);
        }

        let parsed: PersistentEqState =
            serde_json::from_value(value).expect("legacy payload must still load");

        let mut restored = EqPanelState::default();
        parsed.apply_to(&mut restored);

        // Missing fields fall back to the fresh panel's values, not to zero.
        let fresh = EqPanelState::default();
        assert_eq!(restored.limiter_lookahead_ms, fresh.limiter_lookahead_ms);
        assert_eq!(restored.crossfeed_freq, fresh.crossfeed_freq);
        assert_eq!(restored.correction_depth, fresh.correction_depth);
        assert_eq!(restored.bands[0].freq, fresh.bands[0].freq);
        assert_eq!(restored.bands[9].freq, fresh.bands[9].freq);
        assert_eq!(restored.comp_threshold, fresh.comp_threshold);
    }

    #[test]
    fn summary_names_the_engaged_blocks() {
        let preset = ChainPreset::capture("x", "", &engaged_panel(), &engaged_settings());
        let s = preset.summary();
        for needle in [
            "graphic",
            "parametric EQ",
            "limiter",
            "compressor",
            "crossfeed",
            "sub",
        ] {
            assert!(s.contains(needle), "summary {s:?} missing {needle:?}");
        }

        let flat = ChainPreset::capture(
            "flat",
            "",
            &EqPanelState::default(),
            &SettingsTabState::default(),
        );
        assert_eq!(flat.summary(), "flat");
    }
}

//! Factory and user EQ presets, and the interpolation that maps a preset's
//! ten octave gains onto an arbitrary band layout.

use super::{EqPanelActions, EqPanelState};
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

/// Write the selected preset's gains into the active graphic-EQ layout.
pub(super) fn apply_active_preset(state: &mut EqPanelState, actions: &mut EqPanelActions) {
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

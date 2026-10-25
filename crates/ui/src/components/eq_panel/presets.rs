//! Factory and user EQ presets, and the interpolation that maps a preset's
//! ten octave gains onto an arbitrary band layout.

use super::{EqPanelActions, EqPanelState};
use config::GraphicEqLayout;
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

/// Interpolates gain across an arbitrary frequency using log2 space.
pub(super) fn interpolate_curve_gain(target_freq: f32, freqs: &[f32], gains: &[f32]) -> f32 {
    if freqs.is_empty() || gains.is_empty() {
        return 0.0;
    }
    if target_freq <= freqs[0] {
        return gains[0];
    }
    let last = freqs.len().min(gains.len()) - 1;
    if target_freq >= freqs[last] {
        return gains[last];
    }
    for i in 0..last {
        let f0 = freqs[i];
        let f1 = freqs[i + 1];
        if target_freq >= f0 && target_freq <= f1 {
            let log_f0 = f0.log2();
            let log_f1 = f1.log2();
            let log_target = target_freq.log2();
            let t = ((log_target - log_f0) / (log_f1 - log_f0)).clamp(0.0, 1.0);
            let g = gains[i] + t * (gains[i + 1] - gains[i]);
            return (g * 10.0).round() / 10.0;
        }
    }
    0.0
}

/// Interpolates gain across an arbitrary frequency using log2 space
fn interpolate_preset_gain(
    target_freq: f32,
    base_freqs: &[f32; 10],
    base_gains: &[f32; 10],
) -> f32 {
    interpolate_curve_gain(target_freq, base_freqs, base_gains)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomEqPreset {
    pub name: String,
    pub gains: Vec<f32>,
    pub preamp_db: f32,
    /// The ladder the gains were captured on. Without it a curve saved at 31
    /// bands was later copied index-for-index onto a 10-band layout, moving
    /// every frequency. Older presets have no field and are treated as the
    /// 10-band ladder they were most commonly saved from.
    #[serde(default)]
    pub layout: Option<GraphicEqLayout>,
}

/// Write the selected preset's gains into the active graphic-EQ layout.
pub(super) fn apply_active_preset(state: &mut EqPanelState, actions: &mut EqPanelActions) {
    let freqs = state.graphic_layout.frequencies();
    let num = freqs.len().min(state.graphic_gains.len());
    if let Some(idx) = state.selected_custom_preset_idx {
        if let Some(cp) = state.custom_presets.get(idx) {
            let source_freqs = cp
                .layout
                .as_ref()
                .map(GraphicEqLayout::frequencies)
                .unwrap_or_else(|| GraphicEqLayout::TenBand.frequencies());
            for (i, freq) in freqs.iter().enumerate().take(num) {
                state.graphic_gains[i] = interpolate_curve_gain(*freq, &source_freqs, &cp.gains);
            }
            state.graphic_gains[num..].fill(0.0);
            // A saved curve's preamp is part of the curve: it is what keeps
            // the saved shape from clipping. Ignoring it made a saved +6 dB
            // bass boost play 6 dB louder than when it was saved.
            state.set_preamp(cp.preamp_db);
            // Restore the range the curve was saved in. Drawing a +18 dB
            // preset on a ±12 dB canvas would visually clamp a boost the
            // engine is still applying.
            if state.needs_extended_range() {
                state.extended_range = true;
            }
            // One flag for the whole curve. The old per-band `Option<usize>`
            // survived exactly one assignment, so every preset applied only
            // its final band — the Bass Boost preset ended at 0 dB.
            actions.graphic_eq_changed = true;
            return;
        }
    }
    let new_gains = state.selected_preset.gains_for_frequencies(&freqs);
    for i in 0..num {
        state.graphic_gains[i] = new_gains.get(i).copied().unwrap_or(0.0);
    }
    state.graphic_gains[num..].fill(0.0);
    // A factory preset is the whole curve: its reference level is unity. A
    // preamp left over from a previous custom preset would otherwise make the
    // newly selected curve play at the wrong level.
    state.set_preamp(0.0);
    if state.needs_extended_range() {
        state.extended_range = true;
    }
    actions.graphic_eq_changed = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::eq_panel::EqPanelActions;
    use config::GraphicEqLayout;

    #[test]
    fn factory_preset_writes_every_band_on_a_64_band_layout() {
        let mut state = EqPanelState {
            graphic_layout: GraphicEqLayout::SixtyFourBand,
            selected_preset: EqPreset::Rock,
            ..Default::default()
        };
        let mut actions = EqPanelActions::default();

        apply_active_preset(&mut state, &mut actions);

        let expected = EqPreset::Rock.gains_for_frequencies(&state.graphic_layout.frequencies());
        assert_eq!(expected.len(), 64);
        assert_eq!(
            &state.graphic_gains[..64],
            expected.as_slice(),
            "a preset must install the whole curve, not just its final band"
        );
        assert!(actions.graphic_eq_changed);
    }

    #[test]
    fn custom_preset_restores_its_curve_and_preamp() {
        let mut state = EqPanelState::default();
        state.custom_presets.push(CustomEqPreset {
            name: "saved".into(),
            gains: vec![6.0, 5.0, 3.0, 1.0, 0.0, -1.0, -2.0, -1.0, 0.0, 1.0],
            preamp_db: -5.5,
            layout: Some(GraphicEqLayout::TenBand),
        });
        state.selected_custom_preset_idx = Some(0);
        let mut actions = EqPanelActions::default();

        apply_active_preset(&mut state, &mut actions);

        assert_eq!(
            &state.graphic_gains[..10],
            &[6.0, 5.0, 3.0, 1.0, 0.0, -1.0, -2.0, -1.0, 0.0, 1.0]
        );
        assert_eq!(state.preamp_db, -5.5);
        assert_eq!(state.graphic_preamp_db, -5.5);
        assert!(actions.graphic_eq_changed);
    }

    #[test]
    fn factory_preset_clears_a_custom_presets_saved_preamp() {
        let mut state = EqPanelState {
            preamp_db: -5.5,
            graphic_preamp_db: -5.5,
            selected_preset: EqPreset::Rock,
            ..Default::default()
        };
        let mut actions = EqPanelActions::default();

        apply_active_preset(&mut state, &mut actions);

        assert_eq!(state.preamp_db, 0.0);
        assert_eq!(state.graphic_preamp_db, 0.0);
        assert_eq!(
            &state.graphic_gains[..10],
            EqPreset::Rock
                .gains_for_frequencies(&GraphicEqLayout::TenBand.frequencies())
                .as_slice()
        );
        assert!(actions.graphic_eq_changed);
    }

    #[test]
    fn legacy_custom_preset_without_a_layout_loads_as_ten_band() {
        let saved: CustomEqPreset = serde_json::from_str(
            r#"{"name":"old","gains":[1,2,3,4,5,6,7,8,9,10],"preamp_db":-2.0}"#,
        )
        .expect("old custom presets must keep parsing");
        assert_eq!(saved.layout, None);
        assert_eq!(saved.preamp_db, -2.0);

        let mut state = EqPanelState {
            custom_presets: vec![saved],
            selected_custom_preset_idx: Some(0),
            ..Default::default()
        };
        let mut actions = EqPanelActions::default();
        apply_active_preset(&mut state, &mut actions);
        assert_eq!(
            &state.graphic_gains[..10],
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]
        );
        assert_eq!(state.preamp_db, -2.0);
    }

    #[test]
    fn custom_preset_restores_the_range_its_boost_needs() {
        let mut state = EqPanelState {
            extended_range: false,
            selected_preset: EqPreset::Custom,
            selected_custom_preset_idx: Some(0),
            custom_presets: vec![CustomEqPreset {
                name: "Extreme".into(),
                gains: vec![18.0; 10],
                preamp_db: 0.0,
                layout: Some(GraphicEqLayout::TenBand),
            }],
            ..Default::default()
        };
        let mut actions = EqPanelActions::default();
        apply_active_preset(&mut state, &mut actions);
        assert!(state.extended_range);
        assert!((state.graphic_gains[0] - 18.0).abs() < 1e-6);
        assert!(actions.graphic_eq_changed);
    }

    #[test]
    fn custom_preset_maps_its_saved_ladder_onto_a_different_layout() {
        let mut state = EqPanelState::default();
        state.custom_presets.push(CustomEqPreset {
            name: "saved".into(),
            gains: vec![6.0, 5.0, 3.0, 1.0, 0.0, -1.0, -2.0, -1.0, 0.0, 1.0],
            preamp_db: -5.5,
            layout: Some(GraphicEqLayout::TenBand),
        });
        state.selected_custom_preset_idx = Some(0);
        state.set_graphic_layout(GraphicEqLayout::ThirtyOneBand);
        let mut actions = EqPanelActions::default();

        apply_active_preset(&mut state, &mut actions);

        let freqs = state.graphic_layout.frequencies();
        for (freq, expected) in [(31.5, 6.0), (1000.0, -1.0), (16000.0, 1.0)] {
            let index = freqs.iter().position(|f| (*f - freq).abs() < 1e-3).unwrap();
            assert!(
                (state.graphic_gains[index] - expected).abs() < 0.1,
                "{freq} Hz should map to {expected} dB, got {}",
                state.graphic_gains[index]
            );
        }
        assert_eq!(state.preamp_db, -5.5);
    }

    #[test]
    fn interpolation_round_trips_a_ten_band_curve_through_31_bands() {
        let freqs = [
            31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
        ];
        let gains = [4.0, 3.0, 2.5, 0.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0];

        for (freq, gain) in freqs.into_iter().zip(gains) {
            let interpolated = interpolate_curve_gain(freq, &freqs, &gains);
            assert!(
                (interpolated - gain).abs() < 1e-3,
                "{freq} Hz should keep {gain} dB, got {interpolated}"
            );
        }
    }
}

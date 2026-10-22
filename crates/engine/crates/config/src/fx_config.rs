//! Creative-effect configuration: chorus, flanger, phaser, delay, saturation.
//!
//! These are the *artistic* effects — the sound-design layer rather than the
//! corrective layer (EQ, dynamics, filters). They live in the config crate so
//! the engine, the player and the persisted settings all speak the same
//! vocabulary: one struct describes one effect's state, and
//! [`FxConfig`] describes the insert as a whole.

use serde::{Deserialize, Serialize};

/// The creative effects the engine can run, in the order they are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FxEffect {
    Chorus,
    Flanger,
    Phaser,
    Delay,
    Distortion,
}

impl FxEffect {
    pub const ALL: [FxEffect; 5] = [
        FxEffect::Chorus,
        FxEffect::Flanger,
        FxEffect::Phaser,
        FxEffect::Delay,
        FxEffect::Distortion,
    ];

    /// The effect's name, as the settings and the diagnostics call it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chorus => "Chorus",
            Self::Flanger => "Flanger",
            Self::Phaser => "Phaser",
            Self::Delay => "Delay",
            Self::Distortion => "Distortion",
        }
    }
}

/// The chorus: a multi-voice modulated delay.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChorusConfig {
    pub enabled: bool,
    /// LFO rate in Hz.
    pub rate_hz: f32,
    /// Modulation depth in milliseconds.
    pub depth_ms: f32,
    /// The delay the modulation swings around, in milliseconds.
    pub base_delay_ms: f32,
    /// How much of the delayed signal is fed back into the line.
    pub feedback: f32,
    /// The wet (effect) level.
    pub wet: f32,
    /// The dry (original) level.
    pub dry: f32,
}

impl Default for ChorusConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_hz: 1.2,
            depth_ms: 3.5,
            base_delay_ms: 15.0,
            feedback: 0.2,
            wet: 0.5,
            dry: 1.0,
        }
    }
}

/// The flanger: a short modulated comb filter.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FlangerConfig {
    pub enabled: bool,
    pub rate_hz: f32,
    pub depth_ms: f32,
    /// The fixed offset the modulation is added to, in milliseconds.
    pub manual_ms: f32,
    pub feedback: f32,
    pub wet: f32,
    pub dry: f32,
}

impl Default for FlangerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_hz: 0.3,
            depth_ms: 2.0,
            manual_ms: 1.0,
            feedback: 0.7,
            wet: 0.7,
            dry: 0.7,
        }
    }
}

/// The phaser: an allpass ladder swept by an LFO.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhaserConfig {
    pub enabled: bool,
    pub rate_hz: f32,
    /// The frequency the sweep starts from, in Hz.
    pub base_freq_hz: f32,
    /// How many octaves the sweep covers.
    pub sweep_octaves: f32,
    pub feedback: f32,
    /// How many allpass stages the ladder runs (2–8).
    pub stages: u8,
    pub wet: f32,
    pub dry: f32,
}

impl Default for PhaserConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_hz: 0.5,
            base_freq_hz: 500.0,
            sweep_octaves: 3.0,
            feedback: 0.6,
            stages: 6,
            wet: 0.5,
            dry: 0.7,
        }
    }
}

/// The stereo ping-pong delay.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DelayConfig {
    pub enabled: bool,
    /// The delay time in milliseconds.
    pub time_ms: f32,
    /// How much of the delayed signal is fed back into the line.
    pub feedback: f32,
    /// The one-pole damping of the repeats, 0 = none.
    pub damping: f32,
    pub wet: f32,
    pub dry: f32,
}

impl Default for DelayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            time_ms: 250.0,
            feedback: 0.35,
            damping: 0.2,
            wet: 0.4,
            dry: 1.0,
        }
    }
}

/// The shape of the distortion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionShape {
    SoftClipTanh,
    SoftClipAtan,
    SoftClipCubic,
    HardClip,
    TapeSaturation,
    TubeSaturation,
    Wavefolder,
}

impl DistortionShape {
    pub const ALL: [DistortionShape; 7] = [
        DistortionShape::SoftClipTanh,
        DistortionShape::SoftClipAtan,
        DistortionShape::SoftClipCubic,
        DistortionShape::HardClip,
        DistortionShape::TapeSaturation,
        DistortionShape::TubeSaturation,
        DistortionShape::Wavefolder,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::SoftClipTanh => "Soft clip",
            Self::SoftClipAtan => "Progressive",
            Self::SoftClipCubic => "Cubic",
            Self::HardClip => "Hard clip",
            Self::TapeSaturation => "Tape",
            Self::TubeSaturation => "Tube",
            Self::Wavefolder => "Wavefolder",
        }
    }
}

/// The saturation / distortion stage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DistortionConfig {
    pub enabled: bool,
    pub shape: DistortionShape,
    /// How hard the signal is driven into the shape.
    pub drive: f32,
    /// A DC offset added before the shape.
    pub bias: f32,
    /// The output ceiling.
    pub ceiling: f32,
    pub wet: f32,
    pub dry: f32,
}

impl Default for DistortionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            shape: DistortionShape::SoftClipTanh,
            drive: 1.0,
            bias: 0.0,
            ceiling: 1.0,
            wet: 1.0,
            dry: 0.0,
        }
    }
}

/// The creative-effects insert as a whole.
///
/// Every effect is off by default: an insert that changes the sound the user
/// did not ask for is a bug, not a feature.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FxConfig {
    pub chorus: ChorusConfig,
    pub flanger: FlangerConfig,
    pub phaser: PhaserConfig,
    pub delay: DelayConfig,
    pub distortion: DistortionConfig,
}

impl FxConfig {
    /// Whether any effect is engaged.
    pub fn any_enabled(&self) -> bool {
        self.chorus.enabled
            || self.flanger.enabled
            || self.phaser.enabled
            || self.delay.enabled
            || self.distortion.enabled
    }

    /// The settings of one effect.
    pub fn effect(&self, effect: FxEffect) -> Option<&dyn EffectSettings> {
        match effect {
            FxEffect::Chorus => Some(&self.chorus),
            FxEffect::Flanger => Some(&self.flanger),
            FxEffect::Phaser => Some(&self.phaser),
            FxEffect::Delay => Some(&self.delay),
            FxEffect::Distortion => Some(&self.distortion),
        }
    }
}

/// What every effect's settings can answer.
pub trait EffectSettings {
    /// Whether the effect is engaged.
    fn enabled(&self) -> bool;
}

impl EffectSettings for ChorusConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
}
impl EffectSettings for FlangerConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
}
impl EffectSettings for PhaserConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
}
impl EffectSettings for DelayConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
}
impl EffectSettings for DistortionConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_is_off_by_default() {
        let config = FxConfig::default();
        assert!(!config.any_enabled());
        for effect in FxEffect::ALL {
            assert!(
                !config
                    .effect(effect)
                    .expect("every effect has settings")
                    .enabled(),
                "{} should be off by default",
                effect.name()
            );
        }
    }

    #[test]
    fn an_old_payload_gets_the_defaults() {
        // A settings blob written before the effects existed must still load.
        let config: FxConfig = serde_json::from_str("{}").expect("an empty blob loads");
        assert!(!config.any_enabled());
    }

    #[test]
    fn a_config_round_trips() {
        let mut config = FxConfig::default();
        config.chorus.enabled = true;
        config.delay.time_ms = 375.0;
        let json = serde_json::to_string(&config).expect("serialisable");
        let back: FxConfig = serde_json::from_str(&json).expect("deserialisable");
        assert_eq!(config, back);
    }
}

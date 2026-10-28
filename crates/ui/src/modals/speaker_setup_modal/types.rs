//! Types and domain models for acoustic alignment and speaker setup.

use config::{CrossoverFilterType, CrossoverSlope};
use std::ops::RangeInclusive;

/// Speed of sound in standard dry room air at 20°C (meters per second).
pub const SPEED_OF_SOUND_MPS: f32 = 343.2;
/// Speed of sound in standard dry room air at 20°C (feet per second).
pub const SPEED_OF_SOUND_FPS: f32 = 1126.0;

/// Distance measurement unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceUnit {
    #[default]
    Meters,
    Feet,
}

impl DistanceUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Meters => "m",
            Self::Feet => "ft",
        }
    }

    pub fn speed_of_sound(self) -> f32 {
        match self {
            Self::Meters => SPEED_OF_SOUND_MPS,
            Self::Feet => SPEED_OF_SOUND_FPS,
        }
    }

    pub fn slider_range(self) -> RangeInclusive<f32> {
        match self {
            Self::Meters => 0.5..=10.0,
            Self::Feet => 1.5..=33.0,
        }
    }

    pub fn step(self) -> f32 {
        match self {
            Self::Meters => 0.05,
            Self::Feet => 0.1,
        }
    }

    pub fn format_distance(self, distance: f32) -> String {
        match self {
            Self::Meters => format!("{:.2} m", distance),
            Self::Feet => format!("{:.1} ft", distance),
        }
    }

    pub fn convert_to(self, distance: f32, target: DistanceUnit) -> f32 {
        match (self, target) {
            (Self::Meters, Self::Feet) => distance * 3.28084,
            (Self::Feet, Self::Meters) => distance / 3.28084,
            _ => distance,
        }
    }
}

/// Semantic speaker roles mapped to standard channel indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeakerRole {
    #[default]
    FrontLeft,
    FrontRight,
    Center,
    Subwoofer,
    SurroundLeft,
    SurroundRight,
    RearLeft,
    RearRight,
}

impl SpeakerRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::FrontLeft => "FL",
            Self::FrontRight => "FR",
            Self::Center => "C",
            Self::Subwoofer => "SUB",
            Self::SurroundLeft => "SL",
            Self::SurroundRight => "SR",
            Self::RearLeft => "RL",
            Self::RearRight => "RR",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::FrontLeft => "Front Left",
            Self::FrontRight => "Front Right",
            Self::Center => "Center",
            Self::Subwoofer => "Subwoofer (LFE)",
            Self::SurroundLeft => "Surround Left",
            Self::SurroundRight => "Surround Right",
            Self::RearLeft => "Rear Left",
            Self::RearRight => "Rear Right",
        }
    }

    pub fn default_channel(self) -> usize {
        match self {
            Self::FrontLeft => 0,
            Self::FrontRight => 1,
            Self::Center => 2,
            Self::Subwoofer => 3,
            Self::SurroundLeft => 4,
            Self::SurroundRight => 5,
            Self::RearLeft => 6,
            Self::RearRight => 7,
        }
    }

    pub fn polar_angle_deg(self) -> f32 {
        match self {
            Self::FrontLeft => -30.0,
            Self::FrontRight => 30.0,
            Self::Center => 0.0,
            Self::Subwoofer => -45.0,
            Self::SurroundLeft => -105.0,
            Self::SurroundRight => 105.0,
            Self::RearLeft => -150.0,
            Self::RearRight => 150.0,
        }
    }

    pub fn is_subwoofer(self) -> bool {
        matches!(self, Self::Subwoofer)
    }
}

/// Surround and multichannel layout topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeakerLayout {
    Stereo20,
    #[default]
    Stereo21,
    Lcr31,
    Surround51,
    Surround71,
}

impl SpeakerLayout {
    pub fn roles(self) -> &'static [SpeakerRole] {
        match self {
            Self::Stereo20 => &[SpeakerRole::FrontLeft, SpeakerRole::FrontRight],
            Self::Stereo21 => &[
                SpeakerRole::FrontLeft,
                SpeakerRole::FrontRight,
                SpeakerRole::Subwoofer,
            ],
            Self::Lcr31 => &[
                SpeakerRole::FrontLeft,
                SpeakerRole::Center,
                SpeakerRole::FrontRight,
                SpeakerRole::Subwoofer,
            ],
            Self::Surround51 => &[
                SpeakerRole::FrontLeft,
                SpeakerRole::Center,
                SpeakerRole::FrontRight,
                SpeakerRole::Subwoofer,
                SpeakerRole::SurroundLeft,
                SpeakerRole::SurroundRight,
            ],
            Self::Surround71 => &[
                SpeakerRole::FrontLeft,
                SpeakerRole::Center,
                SpeakerRole::FrontRight,
                SpeakerRole::Subwoofer,
                SpeakerRole::SurroundLeft,
                SpeakerRole::SurroundRight,
                SpeakerRole::RearLeft,
                SpeakerRole::RearRight,
            ],
        }
    }
}

/// Per-channel alignment settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeakerChannelConfig {
    pub role: SpeakerRole,
    pub channel: usize,
    pub distance: f32,
    pub trim_db: f32,
    pub invert: bool,
}

impl SpeakerChannelConfig {
    pub fn new(role: SpeakerRole, unit: DistanceUnit) -> Self {
        let base_m = match role {
            SpeakerRole::FrontLeft | SpeakerRole::FrontRight | SpeakerRole::Center => 2.50,
            SpeakerRole::Subwoofer => 2.80,
            SpeakerRole::SurroundLeft | SpeakerRole::SurroundRight => 2.10,
            SpeakerRole::RearLeft | SpeakerRole::RearRight => 2.30,
        };
        let distance = DistanceUnit::Meters.convert_to(base_m, unit);
        Self {
            role,
            channel: role.default_channel(),
            distance,
            trim_db: 0.0,
            invert: false,
        }
    }
}

/// Quick crossover presets based on acoustic speaker driver sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpeakerSizePreset {
    Floorstander,
    #[default]
    Bookshelf,
    Satellite,
    Custom,
}

/// Modal dialog state for speaker setup & alignment.
pub struct SpeakerSetupModalState {
    pub is_open: bool,
    pub layout: SpeakerLayout,
    pub unit: DistanceUnit,
    pub speakers: Vec<SpeakerChannelConfig>,
    pub selected_idx: usize,
    pub live_preview: bool,

    // Crossover & Bass Management
    pub bass_management_enabled: bool,
    pub speaker_size: SpeakerSizePreset,
    pub crossover_hz: f32,
    pub slope: CrossoverSlope,
    pub filter_type: CrossoverFilterType,
    pub filter_q: f32,
    pub mains_highpass: bool,
    pub sub_phase_deg: f32,
    pub sub_polarity_invert: bool,
    pub sub_fine_delay_ms: f32,

    pub status_banner: Option<String>,
}

impl Default for SpeakerSetupModalState {
    fn default() -> Self {
        let layout = SpeakerLayout::Stereo21;
        let unit = DistanceUnit::Meters;
        let speakers = layout
            .roles()
            .iter()
            .map(|&r| SpeakerChannelConfig::new(r, unit))
            .collect();

        Self {
            is_open: false,
            layout,
            unit,
            speakers,
            selected_idx: 0,
            live_preview: false,

            bass_management_enabled: true,
            speaker_size: SpeakerSizePreset::Bookshelf,
            crossover_hz: 80.0,
            slope: CrossoverSlope::Db24,
            filter_type: CrossoverFilterType::LinkwitzRiley,
            filter_q: std::f32::consts::FRAC_1_SQRT_2,
            mains_highpass: true,
            sub_phase_deg: 0.0,
            sub_polarity_invert: false,
            sub_fine_delay_ms: 0.0,

            status_banner: None,
        }
    }
}

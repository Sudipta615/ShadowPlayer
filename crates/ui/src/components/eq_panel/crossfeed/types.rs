//! Types, preset definitions, and acoustic physics for headphone crossfeed.

use config::CrossfeedProfile;
use serde::{Deserialize, Serialize};

/// Calibrated acoustic presets for headphone crossfeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CalibratedCrossfeedPreset {
    #[default]
    Bauer,
    ChuMoy,
    Jmeier,
    Subtle,
    Pronounced,
    StudioMonitor,
    Custom,
}

impl CalibratedCrossfeedPreset {
    pub const ALL: [CalibratedCrossfeedPreset; 7] = [
        CalibratedCrossfeedPreset::Bauer,
        CalibratedCrossfeedPreset::ChuMoy,
        CalibratedCrossfeedPreset::Jmeier,
        CalibratedCrossfeedPreset::Subtle,
        CalibratedCrossfeedPreset::Pronounced,
        CalibratedCrossfeedPreset::StudioMonitor,
        CalibratedCrossfeedPreset::Custom,
    ];

    pub fn label(self) -> String {
        match self {
            Self::Bauer => tr!("Bauer"),
            Self::ChuMoy => tr!("Chu Moy"),
            Self::Jmeier => tr!("Jmeier"),
            Self::Subtle => tr!("Subtle (Acoustic)"),
            Self::Pronounced => tr!("Pronounced (Vintage)"),
            Self::StudioMonitor => tr!("Studio Monitor 60°"),
            Self::Custom => tr!("Custom"),
        }
    }

    /// Default parameters: `(cutoff_frequency_hz, q_factor, delay_ms)`.
    pub fn params(self) -> (f32, f32, f32) {
        match self {
            Self::Bauer => (700.0, 0.50, 0.30),
            Self::ChuMoy => (700.0, 0.707, 0.25),
            Self::Jmeier => (600.0, 0.60, 0.35),
            Self::Subtle => (700.0, 0.707, 0.30),
            Self::Pronounced => (850.0, 0.707, 0.30),
            Self::StudioMonitor => (600.0, 0.707, 0.28),
            Self::Custom => (700.0, 0.707, 0.30),
        }
    }

    pub fn engine_profile(self) -> CrossfeedProfile {
        match self {
            Self::Bauer => CrossfeedProfile::Bauer,
            Self::ChuMoy => CrossfeedProfile::ChuMoy,
            Self::Jmeier => CrossfeedProfile::Jmeier,
            _ => CrossfeedProfile::Custom,
        }
    }

    pub fn description(self) -> String {
        match self {
            Self::Bauer => {
                tr!("1965 Benjamin Bauer circuit. Smooth 700 Hz lowpass blend with 300 µs ITD.")
            }
            Self::ChuMoy => {
                tr!("Classic DIY audiophile crossfeed with maximally flat Butterworth filter.")
            }
            Self::Jmeier => {
                tr!("Dr. Jan Meier natural soundstage with gentle bass warmth and 350 µs ITD.")
            }
            Self::Subtle => {
                tr!("Minimal crossfeed; preserves separation while relieving harsh ping-pong stereo.")
            }
            Self::Pronounced => {
                tr!("Higher 850 Hz cutoff for early 1960s hard-panned stereo recordings.")
            }
            Self::StudioMonitor => {
                tr!("Simulates an acoustically treated nearfield studio at 60° equilateral triangle.")
            }
            Self::Custom => {
                tr!("Freely adjust cutoff frequency, filter Q factor, and interaural delay.")
            }
        }
    }

    pub fn detect(profile: CrossfeedProfile, freq: f32, q: f32, delay_ms: f32) -> Self {
        match profile {
            CrossfeedProfile::Bauer => Self::Bauer,
            CrossfeedProfile::ChuMoy => Self::ChuMoy,
            CrossfeedProfile::Jmeier => Self::Jmeier,
            CrossfeedProfile::Custom => {
                if (freq - 700.0).abs() < 5.0
                    && (q - 0.707).abs() < 0.05
                    && (delay_ms - 0.30).abs() < 0.02
                {
                    Self::Subtle
                } else if (freq - 850.0).abs() < 5.0
                    && (q - 0.707).abs() < 0.05
                    && (delay_ms - 0.30).abs() < 0.02
                {
                    Self::Pronounced
                } else if (freq - 600.0).abs() < 5.0
                    && (q - 0.707).abs() < 0.05
                    && (delay_ms - 0.28).abs() < 0.02
                {
                    Self::StudioMonitor
                } else {
                    Self::Custom
                }
            }
        }
    }
}

/// Visualizer view mode: Soundstage Geometry vs Acoustic Frequency Response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CrossfeedViewMode {
    #[default]
    SoundstageGeometry,
    TransferSpectrum,
}

/// Speed of sound in room air: 343.2 m/s = 34.32 cm/ms.
pub const SPEED_OF_SOUND_CM_PER_MS: f32 = 34.32;

/// Interaural time delay in microseconds.
pub fn calculate_itd_delay_us(delay_ms: f32) -> f32 {
    delay_ms * 1000.0
}

/// Equivalent acoustic path offset around cranial circumference in cm.
pub fn calculate_acoustic_path_cm(delay_ms: f32) -> f32 {
    delay_ms * SPEED_OF_SOUND_CM_PER_MS
}

/// Psychoacoustic listening fatigue reduction index (percentage 0..100).
pub fn calculate_fatigue_reduction_pct(freq_hz: f32, delay_ms: f32) -> u32 {
    let f_term = (freq_hz / 700.0).clamp(0.4, 1.6);
    let d_term = (delay_ms / 0.30).clamp(0.3, 1.7);
    let score = (35.0 * f_term + 55.0 * d_term).clamp(10.0, 98.0);
    score.round() as u32
}

/// 2nd-order analog/digital lowpass biquad power response in dB, normalized with -6.0 dB cross-blend.
pub fn biquad_crossfeed_gain_db(f: f32, fc: f32, q: f32) -> f32 {
    let ratio = (f / fc.max(20.0)).max(0.001);
    let ratio_sq = ratio * ratio;
    let denom = (1.0 - ratio_sq) * (1.0 - ratio_sq) + (ratio_sq / (q * q).max(0.01));
    let mag_sq = 1.0 / denom.max(1e-12);
    let db = 10.0 * mag_sq.log10() - 6.02;
    db.clamp(-36.0, 6.0)
}

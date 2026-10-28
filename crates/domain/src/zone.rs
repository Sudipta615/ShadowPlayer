//! Multi-zone playback models, clock drift compensation, and ZoneSwitch routing rules.
//!
//! Provides the core data types for autonomous playback zones (`ZoneConfig`), master/slave
//! linking with millisecond timing offsets and quartz clock drift micro-adjustments ($\pm 0.05\%$),
//! and metadata-driven automatic output routing (`ZoneSwitchRule`).

use serde::{Deserialize, Serialize};

use crate::ids::ZoneId;
use crate::models::Track;
use crate::smart::SmartOp;

/// Maximum allowable clock drift micro-adjustment percentage (±0.05%).
///
/// Hardware quartz oscillators typically drift by 10–50 parts per million (ppm).
/// A ±0.05% (500 ppm) resampler adjustment window allows continuous, inaudible
/// clock phase-locking without causing detectable pitch variation.
pub const MAX_CLOCK_DRIFT_PCT: f32 = 0.05;

/// Lower bound of relative timing alignment offset in milliseconds.
pub const MIN_TIMING_OFFSET_MS: f32 = -50.0;

/// Upper bound of relative timing alignment offset in milliseconds.
pub const MAX_TIMING_OFFSET_MS: f32 = 50.0;

/// Configuration and persistence state for an independent audio zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneConfig {
    pub id: ZoneId,
    pub name: String,
    pub output_device: Option<String>,
    pub output_backend: Option<String>,
    pub volume: f32,
    pub muted: bool,
    pub dsp_preset: Option<String>,
    pub linked_master_id: Option<ZoneId>,
    pub timing_offset_ms: f32,
    pub clock_drift_pct: f32,
    pub is_default: bool,
    pub created_at_unix: u64,
}

impl ZoneConfig {
    /// Create a new default primary zone ("Main Zone").
    pub fn main_default() -> Self {
        Self {
            id: ZoneId::new(),
            name: "Main Zone".to_string(),
            output_device: None,
            output_backend: None,
            volume: 1.0,
            muted: false,
            dsp_preset: None,
            linked_master_id: None,
            timing_offset_ms: 0.0,
            clock_drift_pct: 0.0,
            is_default: true,
            created_at_unix: 0,
        }
    }

    /// Create a new named custom zone.
    pub fn new_custom(name: impl Into<String>, device: Option<String>) -> Self {
        Self {
            id: ZoneId::new(),
            name: name.into(),
            output_device: device,
            output_backend: None,
            volume: 1.0,
            muted: false,
            dsp_preset: None,
            linked_master_id: None,
            timing_offset_ms: 0.0,
            clock_drift_pct: 0.0,
            is_default: false,
            created_at_unix: 0,
        }
    }

    /// Clamp and set the relative timing offset in milliseconds ([-50.0 ms, +50.0 ms]).
    pub fn set_timing_offset(&mut self, ms: f32) {
        self.timing_offset_ms = ms.clamp(MIN_TIMING_OFFSET_MS, MAX_TIMING_OFFSET_MS);
    }

    /// Clamp and set the clock drift micro-adjustment percentage ([-0.05%, +0.05%]).
    pub fn set_clock_drift(&mut self, pct: f32) {
        self.clock_drift_pct = pct.clamp(-MAX_CLOCK_DRIFT_PCT, MAX_CLOCK_DRIFT_PCT);
    }
}

/// Metadata field evaluated by ZoneSwitch conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneMatchField {
    Format,
    SampleRate,
    BitDepth,
    Channels,
    Genre,
    Artist,
    IsLossless,
}

impl ZoneMatchField {
    pub const ALL: [ZoneMatchField; 7] = [
        ZoneMatchField::Format,
        ZoneMatchField::SampleRate,
        ZoneMatchField::BitDepth,
        ZoneMatchField::Channels,
        ZoneMatchField::Genre,
        ZoneMatchField::Artist,
        ZoneMatchField::IsLossless,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Format => "Format",
            Self::SampleRate => "Sample Rate (Hz)",
            Self::BitDepth => "Bit Depth",
            Self::Channels => "Channels",
            Self::Genre => "Genre",
            Self::Artist => "Artist",
            Self::IsLossless => "Is Lossless",
        }
    }
}

/// A single predicate condition within a [`ZoneSwitchRule`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneSwitchCondition {
    pub field: ZoneMatchField,
    pub op: SmartOp,
    pub value: String,
}

impl ZoneSwitchCondition {
    pub fn new(field: ZoneMatchField, op: SmartOp, value: impl Into<String>) -> Self {
        Self {
            field,
            op,
            value: value.into(),
        }
    }

    /// Evaluate this condition against a given [`Track`].
    pub fn matches(&self, track: &Track) -> bool {
        match self.field {
            ZoneMatchField::Format => match_text(&track.format, self.op, &self.value),
            ZoneMatchField::Genre => {
                let genre = track.genre.as_deref().unwrap_or("");
                match_text(genre, self.op, &self.value)
            }
            ZoneMatchField::Artist => match_text(&track.artist, self.op, &self.value),
            ZoneMatchField::SampleRate => {
                let target: u32 = self.value.trim().parse().unwrap_or(0);
                match_number(track.sample_rate as f64, self.op, target as f64)
            }
            ZoneMatchField::BitDepth => {
                let current = track.bit_depth.unwrap_or(16) as f64;
                let target: f64 = self.value.trim().parse().unwrap_or(0.0);
                match_number(current, self.op, target)
            }
            ZoneMatchField::Channels => {
                let current = track.channels as f64;
                let target: f64 = self.value.trim().parse().unwrap_or(2.0);
                match_number(current, self.op, target)
            }
            ZoneMatchField::IsLossless => match self.op {
                SmartOp::IsTrue => track.is_lossless,
                SmartOp::IsFalse => !track.is_lossless,
                _ => true,
            },
        }
    }
}

fn match_text(haystack: &str, op: SmartOp, needle: &str) -> bool {
    let h = haystack.to_lowercase();
    let n = needle.trim().to_lowercase();
    match op {
        SmartOp::Equals => h == n,
        SmartOp::NotEquals => h != n,
        SmartOp::Contains => h.contains(&n),
        SmartOp::NotContains => !h.contains(&n),
        SmartOp::IsEmpty => haystack.trim().is_empty(),
        SmartOp::IsNotEmpty => !haystack.trim().is_empty(),
        _ => false,
    }
}

fn match_number(current: f64, op: SmartOp, target: f64) -> bool {
    match op {
        SmartOp::Equals => (current - target).abs() < 0.001,
        SmartOp::NotEquals => (current - target).abs() >= 0.001,
        SmartOp::GreaterThan => current > target,
        SmartOp::LessThan => current < target,
        _ => false,
    }
}

/// Metadata routing rule that redirects playback of matching tracks to a dedicated zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneSwitchRule {
    pub id: String,
    pub name: String,
    pub target_zone_id: ZoneId,
    pub conditions: Vec<ZoneSwitchCondition>,
    pub priority: i32,
    pub enabled: bool,
    pub created_at_unix: u64,
}

impl ZoneSwitchRule {
    pub fn new(
        name: impl Into<String>,
        target_zone_id: ZoneId,
        conditions: Vec<ZoneSwitchCondition>,
        priority: i32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            target_zone_id,
            conditions,
            priority,
            enabled: true,
            created_at_unix: 0,
        }
    }

    /// Check if all conditions of this rule match the specified track.
    pub fn matches(&self, track: &Track) -> bool {
        if !self.enabled || self.conditions.is_empty() {
            return false;
        }
        self.conditions.iter().all(|cond| cond.matches(track))
    }
}

/// Quartz clock drift compensation and master-slave resampler micro-adjustment manager.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockDriftManager {
    current_drift_pct: f32,
}

impl Default for ClockDriftManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockDriftManager {
    pub fn new() -> Self {
        Self {
            current_drift_pct: 0.0,
        }
    }

    pub fn with_drift_pct(pct: f32) -> Self {
        let mut mgr = Self::new();
        mgr.set_drift_pct(pct);
        mgr
    }

    pub fn drift_pct(&self) -> f32 {
        self.current_drift_pct
    }

    pub fn set_drift_pct(&mut self, pct: f32) {
        self.current_drift_pct = pct.clamp(-MAX_CLOCK_DRIFT_PCT, MAX_CLOCK_DRIFT_PCT);
    }

    /// Calculate the fractional resampler playback rate multiplier:
    ///
    /// $$R = 1.0 + \frac{\text{drift\_pct}}{100.0}$$
    pub fn resampler_rate_ratio(&self) -> f64 {
        1.0 + (self.current_drift_pct as f64 / 100.0)
    }

    /// Compute dynamic micro-adjustment from measured inter-zone buffer lag in milliseconds.
    ///
    /// If the slave buffer is lagging by $\Delta \tau > 0$ ms, rate is micro-increased;
    /// if leading ($\Delta \tau < 0$ ms), rate is micro-decreased.
    pub fn update_from_phase_error(&mut self, phase_error_ms: f32) -> f32 {
        // Proportional micro-gain: 1.0 ms error produces 0.005% rate shift
        let correction = (phase_error_ms * 0.005).clamp(-MAX_CLOCK_DRIFT_PCT, MAX_CLOCK_DRIFT_PCT);
        self.set_drift_pct(correction);
        self.current_drift_pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TrackId;
    use std::path::PathBuf;

    fn sample_track(
        format: &str,
        sample_rate: u32,
        channels: usize,
        bit_depth: Option<u32>,
    ) -> Track {
        Track {
            id: TrackId::new(),
            path: PathBuf::from(format!("/music/track.{format}")),
            title: "Test Track".into(),
            artist: "Test Artist".into(),
            album: "Test Album".into(),
            genre: Some("Classical".into()),
            year: Some(2024),
            track_number: Some(1),
            disc_number: Some(1),
            duration_secs: 240.0,
            bpm: None,
            sample_rate,
            channels,
            format: format.into(),
            bitrate_kbps: Some(1411),
            bit_depth,
            is_lossless: format != "MP3",
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 50_000_000,
            mtime_unix_secs: 1_700_000_000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 5,
            has_lyrics: false,
            play_count: 10,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
            sort_artist_folded: None,
            sort_title_folded: None,
            sort_album_folded: None,
        }
    }

    #[test]
    fn zone_config_bounds_enforced() {
        let mut cfg = ZoneConfig::main_default();
        assert!(cfg.is_default);

        cfg.set_timing_offset(100.0);
        assert_eq!(cfg.timing_offset_ms, 50.0);

        cfg.set_timing_offset(-120.0);
        assert_eq!(cfg.timing_offset_ms, -50.0);

        cfg.set_clock_drift(1.5);
        assert_eq!(cfg.clock_drift_pct, 0.05);

        cfg.set_clock_drift(-0.8);
        assert_eq!(cfg.clock_drift_pct, -0.05);
    }

    #[test]
    fn clock_drift_resampler_ratio() {
        let mut mgr = ClockDriftManager::new();
        assert_eq!(mgr.resampler_rate_ratio(), 1.0);

        mgr.set_drift_pct(0.05);
        assert!((mgr.resampler_rate_ratio() - 1.0005).abs() < 1e-7);

        mgr.set_drift_pct(-0.05);
        assert!((mgr.resampler_rate_ratio() - 0.9995).abs() < 1e-7);

        // Phase error update
        mgr.update_from_phase_error(2.0);
        assert!((mgr.drift_pct() - 0.01).abs() < 1e-4);
    }

    #[test]
    fn zone_switch_rule_evaluates_format_and_channels() {
        let dsd_dac_id = ZoneId::new();
        let rule_dsd = ZoneSwitchRule::new(
            "Route DSD",
            dsd_dac_id,
            vec![ZoneSwitchCondition::new(
                ZoneMatchField::Format,
                SmartOp::Equals,
                "DSD",
            )],
            10,
        );

        let dsd_track = sample_track("DSD", 2822400, 2, Some(1));
        let flac_track = sample_track("FLAC", 96000, 2, Some(24));

        assert!(rule_dsd.matches(&dsd_track));
        assert!(!rule_dsd.matches(&flac_track));

        let avr_id = ZoneId::new();
        let rule_multichannel = ZoneSwitchRule::new(
            "Route Multichannel",
            avr_id,
            vec![ZoneSwitchCondition::new(
                ZoneMatchField::Channels,
                SmartOp::GreaterThan,
                "2",
            )],
            5,
        );

        let multichannel_track = sample_track("FLAC", 48000, 6, Some(24));
        assert!(rule_multichannel.matches(&multichannel_track));
        assert!(!rule_multichannel.matches(&flac_track));
    }
}

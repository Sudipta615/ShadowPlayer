//! The creative-effects insert: chorus, flanger, phaser, delay, saturation.
//!
//! These are the sound-design effects rather than the corrective ones. They
//! sit after the stereo stage and before the transport stages (timestretch,
//! volume, seek fade), so a listener hears the modulated signal at the
//! volume they asked for.
//!
//! The node is one arena slot rather than five: the effects run in a fixed
//! order, share one stereo front pair, and are engaged one at a time by the
//! user. Keeping them in one slot keeps the chain's shape stable — a new
//! effect does not renumber the stages after it.

use crate::dsp::{
    graph2::prod::arena::node::DspNode,
    pipeline::{DspStageCapability, StageChannelSupport, StagePrecision},
};
use crate::fx::delay::PingPongDelay;
use crate::fx::distortion::{DistortionType, Saturator};
use crate::fx::modulation::{Chorus, Flanger, Phaser};

/// The longest delay the insert can hold, in milliseconds.
///
/// The delay line is allocated once, when the node is built, and never grows:
/// a longer delay would mean an allocation on the audio thread.
const MAX_DELAY_MS: f32 = 2000.0;

/// The creative-effects node.
pub struct FxNode {
    /// The chorus.
    pub chorus: Chorus,
    /// The flanger.
    pub flanger: Flanger,
    /// The phaser.
    pub phaser: Phaser,
    /// The stereo ping-pong delay.
    pub delay: PingPongDelay,
    /// The saturation stage.
    pub saturator: Saturator,

    /// Which effects are engaged.
    pub chorus_enabled: bool,
    pub flanger_enabled: bool,
    pub phaser_enabled: bool,
    pub delay_enabled: bool,
    pub distortion_enabled: bool,

    /// The settings the node was last configured with.
    ///
    /// Kept so the engine's state report can say what the insert is doing
    /// without the node having to be read field by field.
    applied: config::FxConfig,

    /// The delay time in milliseconds, as the user set it.
    ///
    /// The processor holds the delay in samples, so the configured time is
    /// kept here and converted whenever the line is read.
    delay_time_ms: f32,

    /// The rate the node was prepared for.
    sample_rate: f32,
}

impl Default for FxNode {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl FxNode {
    /// Build the node for a sample rate.
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(1.0);
        let max_delay_samples = (MAX_DELAY_MS * 0.001 * sr) as usize + 64;
        Self {
            chorus: Chorus::new(sr),
            flanger: Flanger::new(sr),
            phaser: Phaser::new(sr),
            delay: PingPongDelay::new(max_delay_samples, 1, 0.0),
            saturator: Saturator::new(DistortionType::SoftClipTanh, 1.0),
            chorus_enabled: false,
            flanger_enabled: false,
            phaser_enabled: false,
            delay_enabled: false,
            distortion_enabled: false,
            applied: config::FxConfig::default(),
            delay_time_ms: 250.0,
            sample_rate: sr,
        }
    }

    /// Whether any effect is engaged.
    pub fn is_engaged(&self) -> bool {
        self.chorus_enabled
            || self.flanger_enabled
            || self.phaser_enabled
            || self.delay_enabled
            || self.distortion_enabled
    }

    /// The delay time in samples, clamped to the line's capacity.
    fn delay_samples(&self) -> usize {
        let sr = self.sample_rate.max(1.0);
        let samples = (self.delay_time_ms * 0.001 * sr).round() as usize;
        samples.clamp(1, self.delay_capacity().saturating_sub(2))
    }

    /// The delay line's capacity in samples.
    fn delay_capacity(&self) -> usize {
        self.delay.buffer_len()
    }

    /// The settings the insert is running with.
    pub fn config(&self) -> config::FxConfig {
        self.applied
    }

    /// Apply the insert's settings.
    ///
    /// The whole insert is configured in one go, so the settings can never
    /// disagree with each other (a chorus at one rate and a delay at
    /// another would be a state the UI cannot produce).
    pub fn apply_config(&mut self, config: &config::FxConfig) {
        self.applied = *config;
        let chorus = &config.chorus;
        self.chorus_enabled = chorus.enabled;
        self.chorus.rate_hz = chorus.rate_hz;
        self.chorus.depth_ms = chorus.depth_ms;
        self.chorus.base_delay_ms = chorus.base_delay_ms;
        self.chorus.feedback = chorus.feedback;
        self.chorus.wet = chorus.wet;
        self.chorus.dry = chorus.dry;

        let flanger = &config.flanger;
        self.flanger_enabled = flanger.enabled;
        self.flanger.rate_hz = flanger.rate_hz;
        self.flanger.depth_ms = flanger.depth_ms;
        self.flanger.manual_ms = flanger.manual_ms;
        self.flanger.feedback = flanger.feedback;
        self.flanger.wet = flanger.wet;
        self.flanger.dry = flanger.dry;

        let phaser = &config.phaser;
        self.phaser_enabled = phaser.enabled;
        self.phaser.rate_hz = phaser.rate_hz;
        self.phaser.base_freq_hz = phaser.base_freq_hz;
        self.phaser.sweep_octaves = phaser.sweep_octaves;
        self.phaser.feedback = phaser.feedback;
        self.phaser.stages_count = (phaser.stages as usize).clamp(2, 8);
        self.phaser.wet = phaser.wet;
        self.phaser.dry = phaser.dry;

        let delay = &config.delay;
        self.delay_enabled = delay.enabled;
        self.delay_time_ms = delay.time_ms;
        self.delay.feedback = delay.feedback;
        self.delay.damping = delay.damping;
        self.delay.wet = delay.wet;
        self.delay.dry = delay.dry;

        let distortion = &config.distortion;
        self.distortion_enabled = distortion.enabled;
        self.saturator.distortion_type = match distortion.shape {
            config::DistortionShape::SoftClipTanh => DistortionType::SoftClipTanh,
            config::DistortionShape::SoftClipAtan => DistortionType::SoftClipAtan,
            config::DistortionShape::SoftClipCubic => DistortionType::SoftClipCubic,
            config::DistortionShape::HardClip => DistortionType::HardClip,
            config::DistortionShape::TapeSaturation => DistortionType::TapeSaturation,
            config::DistortionShape::TubeSaturation => DistortionType::TubeSaturation,
            config::DistortionShape::Wavefolder => DistortionType::Wavefolder,
        };
        self.saturator.drive = distortion.drive;
        self.saturator.bias = distortion.bias;
        self.saturator.ceiling = distortion.ceiling;
        self.saturator.wet = distortion.wet;
        self.saturator.dry = distortion.dry;
    }
}

impl DspNode for FxNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "creative_fx",
            channel_support: StageChannelSupport::StereoOnly,
            position: "post-stereo",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: true,
            precision: StagePrecision::Any,
        }
    }

    fn is_active(&self) -> bool {
        self.is_engaged()
    }

    /// The insert adds no *latency*: the delay effect's repeat time is part
    /// of its sound, not a pipeline delay the transport has to compensate for.
    fn latency_samples(&self) -> usize {
        0
    }

    fn reset(&mut self) {
        self.chorus.reset();
        self.flanger.reset();
        self.phaser.reset();
        self.delay.reset();
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        let sr = sample_rate.max(1.0);
        if (sr - self.sample_rate).abs() < f32::EPSILON {
            return;
        }
        // The processors hold their own rate (their delay lines and LFOs are
        // built for it), so a rate change rebuilds them. The settings live in
        // the config, not in the processors, so nothing is lost.
        let chorus = Chorus::new(sr);
        let flanger = Flanger::new(sr);
        let phaser = Phaser::new(sr);
        let max_delay_samples = (MAX_DELAY_MS * 0.001 * sr) as usize + 64;
        self.chorus = chorus;
        self.flanger = flanger;
        self.phaser = phaser;
        self.delay = PingPongDelay::new(max_delay_samples, 1, 0.0);
        self.sample_rate = sr;
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        if planes.len() < 2 || !self.is_engaged() {
            return;
        }
        let frames = planes[0].len().min(planes[1].len());
        if frames == 0 {
            return;
        }
        let (left, right) = planes.split_at_mut(1);
        let left = &mut left[0][..frames];
        let right = &mut right[0][..frames];

        if self.chorus_enabled {
            self.chorus.process_stereo(left, right);
        }
        if self.flanger_enabled {
            self.flanger.process_plane(left);
            self.flanger.process_plane(right);
        }
        if self.phaser_enabled {
            self.phaser.process_plane(left);
            self.phaser.process_plane(right);
        }
        if self.delay_enabled {
            self.delay.delay_samples = self.delay_samples();
            self.delay.process_stereo(left, right);
        }
        if self.distortion_enabled {
            self.saturator.process_plane(left);
            self.saturator.process_plane(right);
        }
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        if planes.len() < 2 || !self.is_engaged() {
            return;
        }
        let frames = planes[0].len().min(planes[1].len());
        if frames == 0 || frames > crate::buffer::MAX_AUDIO_BLOCK_FRAMES {
            return;
        }
        // The effect DSP is f32-only, so the f64 path demotes the front pair
        // onto the stack, processes, and promotes back. Fixed-size scratch —
        // no allocation on the audio path.
        let mut left = [0f32; crate::buffer::MAX_AUDIO_BLOCK_FRAMES];
        let mut right = [0f32; crate::buffer::MAX_AUDIO_BLOCK_FRAMES];
        for (dst, src) in left.iter_mut().zip(planes[0].iter()) {
            *dst = *src as f32;
        }
        for (dst, src) in right.iter_mut().zip(planes[1].iter()) {
            *dst = *src as f32;
        }
        {
            let mut pair: [&mut [f32]; 2] = [&mut left[..frames], &mut right[..frames]];
            self.process_block_f32(&mut pair);
        }
        for (dst, src) in planes[0].iter_mut().zip(left[..frames].iter()) {
            *dst = *src as f64;
        }
        for (dst, src) in planes[1].iter_mut().zip(right[..frames].iter()) {
            *dst = *src as f64;
        }
    }
}

//! Talkback / duplex monitor mix.
//!
//! This is the engine half of a talkback control: one capture endpoint is
//! opened, its input is rate-converted to the output clock with ring-fill
//! drift feedback, and the result is added to the **front L/R** channels of
//! the primary output. The program DSP chain, loudness meters, and endpoint
//! fan-out are not modified — a talkback microphone is a monitor source, not
//! a second program lane.
//!
//! # Boundaries, stated up front
//!
//! - **PCM only.** Native DSD, DoP, and PCM→SDM transports bypass this mixer;
//!   there is no PCM monitor path through a bitstream.
//! - **Primary output only.** Physical endpoint workers receive the unmodified
//!   program. Sending a control-room microphone to every zone is not a
//!   default this engine will invent.
//! - **No automatic feedback suppression or ducking.** The input is live only
//!   while [`config::TalkbackConfig::enabled`] is true (push-to-talk is the
//!   intended host control). A monitor speaker that can hear the microphone
//!   can still feed back; suppressing that automatically would require a
//!   model of the room and the playback material, which this stage does not
//!   claim.
//! - **No per-endpoint talkback routing yet.** Gain is one monitor level; the
//!   input is downmixed to mono by averaging its channels.
//!
//! # Why the drift loop exists
//!
//! An input device and an output device run on independent crystals. Even when
//! both report 48 kHz they differ by tens of ppm, so a fixed-ratio converter
//! slowly fills or drains its FIFO. [`DriftController`] steers the conversion
//! ratio from the FIFO fill, exactly as the endpoint path does for a second
//! DAC. The FIFO is deliberately small (a few tens of milliseconds): talkback
//! is a latency-sensitive monitor, not a recording buffer.
//!
//! The conversion itself is linear interpolation between input frames. That
//! is a deliberate quality choice for a speech monitor: it is cheap, bounded,
//! and has no filter-bank latency. It is **not** a mastering converter and is
//! not used for the program path.

use std::collections::VecDeque;
use std::time::Instant;

use crate::buffer::{PlaybackState, MAX_AUDIO_BLOCK_FRAMES};
use crate::output::capture::{open_input_capture, CaptureError, SystemCapture};
use crate::output::DriftController;

use super::AudioEngine;

/// Capture ring capacity: ~200 ms at 48 kHz. The monitor drains it on every
/// tick; the ring exists to absorb scheduling jitter, not to build latency.
const CAPTURE_RING_FRAMES: usize = 9_600;

/// Monitor FIFO capacity in frames. Half of this (≈21 ms at 48 kHz) is the
/// drift controller's target. Small enough to keep the monitor responsive,
/// large enough to ride out a missed tick.
const FIFO_CAPACITY: usize = 2_048;

/// One drain chunk from the capture ring. Fixed so the scratch is allocated
/// once, when the monitor opens.
const DRAIN_CHUNK_FRAMES: usize = 1_024;

/// The pure monitor mixer: input frames in, front-pair monitor samples out.
///
/// Kept separate from the device handle so the gain, downmix, rate conversion
/// and drift logic can be tested without an audio device.
pub(crate) struct MonitorMixer {
    fifo_l: VecDeque<f32>,
    fifo_r: VecDeque<f32>,
    /// Fractional read position from the front of the FIFO.
    pos: f64,
    /// Nominal input frames consumed per output frame (`input_rate / output_rate`).
    base_step: f64,
    drift: DriftController,
    gain_db: f32,
    gain_current: f32,
    mix: Vec<f32>,
    underrun_frames: u64,
    overrun_frames: u64,
}

impl MonitorMixer {
    pub(crate) fn new(input_rate: u32, output_rate: u32, gain_db: f32) -> Self {
        let base_step = f64::from(input_rate.max(1)) / f64::from(output_rate.max(1));
        let gain_db = clamp_gain_db(gain_db);
        let gain = db_to_gain(gain_db);
        let mut drift = DriftController::new_adaptive(true, FIFO_CAPACITY);
        // The endpoint controller's loss-of-clock detector treats a fill that
        // does not move as a stalled device. A monitor FIFO can legitimately
        // sit at its cap while the input clock runs fast, so that heuristic
        // would drop the correction exactly when it is needed.
        drift.set_loss_of_clock_detection(None);
        Self {
            fifo_l: VecDeque::with_capacity(FIFO_CAPACITY),
            fifo_r: VecDeque::with_capacity(FIFO_CAPACITY),
            pos: 0.0,
            base_step,
            drift,
            gain_db,
            gain_current: gain,
            mix: vec![0.0; MAX_AUDIO_BLOCK_FRAMES * crate::buffer::MAX_CHANNELS],
            underrun_frames: 0,
            overrun_frames: 0,
        }
    }

    pub(crate) fn set_gain_db(&mut self, gain_db: f32) {
        self.gain_db = clamp_gain_db(gain_db);
    }

    pub(crate) fn gain_db(&self) -> f32 {
        self.gain_db
    }

    pub(crate) fn underrun_frames(&self) -> u64 {
        self.underrun_frames
    }

    pub(crate) fn overrun_frames(&self) -> u64 {
        self.overrun_frames
    }

    /// Append interleaved capture frames, downmixed to mono by averaging.
    ///
    /// A full FIFO drops its oldest frame and counts an overrun: keeping a
    /// microphone ever further behind the output is worse than losing a
    /// sample of speech.
    pub(crate) fn push_input(&mut self, samples: &[f32], channels: usize) {
        let channels = channels.max(1);
        let frames = samples.len() / channels;
        for i in 0..frames {
            let mut mono = 0.0;
            for c in 0..channels {
                mono += samples[i * channels + c];
            }
            mono /= channels as f32;
            if self.fifo_l.len() >= FIFO_CAPACITY {
                self.fifo_l.pop_front();
                self.fifo_r.pop_front();
                self.overrun_frames = self.overrun_frames.saturating_add(1);
            }
            self.fifo_l.push_back(mono);
            // The input is downmixed, so both monitor channels carry the same
            // signal. Keeping two queues (rather than one) mirrors the
            // stereo-shaped output surface and leaves room for a per-channel
            // monitor law without changing the mixing call.
            self.fifo_r.push_back(mono);
        }
    }

    /// Mix the monitor signal into `program`, returning the rendered block.
    ///
    /// `silent` renders a monitor-only block (the transport is not playing).
    /// The caller must pass blocks no larger than
    /// [`MAX_AUDIO_BLOCK_FRAMES`] frames and `channels` no larger than
    /// [`crate::buffer::MAX_CHANNELS`]; the scratch is preallocated for that
    /// contract.
    pub(crate) fn render(&mut self, program: &[f32], channels: usize, silent: bool) -> &[f32] {
        let len = program.len();
        self.render_block(Some(program), len, channels, silent)
    }

    /// Render a monitor-only block of `frames` frames, without a program
    /// source to copy. Used by the idle pump when transport is not playing.
    pub(crate) fn render_idle(&mut self, frames: usize, channels: usize) -> &[f32] {
        let channels = channels.clamp(1, crate::buffer::MAX_CHANNELS);
        let frames = frames.min(MAX_AUDIO_BLOCK_FRAMES);
        self.render_block(None, frames * channels, channels, true)
    }

    fn render_block(
        &mut self,
        program: Option<&[f32]>,
        len: usize,
        channels: usize,
        silent: bool,
    ) -> &[f32] {
        let channels = channels.clamp(1, crate::buffer::MAX_CHANNELS);
        if len > self.mix.len() {
            // A caller broke the block contract. Grow once rather than
            // truncating audio; this cannot happen on the engine's own path.
            self.mix.resize(len, 0.0);
        }
        if silent {
            self.mix[..len].fill(0.0);
        } else {
            let program = program.unwrap_or(&[]);
            self.mix[..len].copy_from_slice(&program[..len]);
        }
        let frames = len / channels;
        // Sample the FIFO before draining it: the fill the controller sees is
        // the backlog that accumulated since the previous block.
        self.drift.update(self.fifo_l.len());
        if frames == 0 || self.fifo_l.len() < 2 {
            if frames > 0 {
                self.underrun_frames = self.underrun_frames.saturating_add(frames as u64);
            }
            return &self.mix[..len];
        }

        // The drift controller is written for a resampler whose ratio scales
        // its *output* production: a full ring must produce less. This
        // interpolator steps through *input* frames per output frame, so the
        // inverse applies — a full FIFO consumes more input, which drains it.
        let step = self.base_step / self.drift.ratio().max(0.5);
        let target = db_to_gain(self.gain_db);
        let start = self.gain_current;
        let increment = (target - start) / frames as f32;
        let mut generated = 0usize;

        while generated < frames {
            if self.fifo_l.len() < 2 {
                break;
            }
            let index = self.pos.floor() as usize;
            if index + 1 >= self.fifo_l.len() {
                break;
            }
            let frac = (self.pos - index as f64) as f32;
            let sample = self.fifo_l[index] * (1.0 - frac) + self.fifo_l[index + 1] * frac;
            let gain = start + increment * (generated + 1) as f32;
            let base = generated * channels;
            self.mix[base] += sample * gain;
            if channels >= 2 {
                // Front L/R. Centre, surrounds, and LFE are deliberately left
                // to the program: a talkback mic belongs in the control-room
                // pair, not spread across the room.
                self.mix[base + 1] += sample * gain;
            }
            generated += 1;
            self.pos += step;
            let drop = self.pos.floor() as usize;
            if drop > 0 {
                let drop = drop.min(self.fifo_l.len());
                self.fifo_l.drain(..drop);
                self.fifo_r.drain(..drop);
                self.pos -= drop as f64;
            }
        }

        if generated < frames {
            self.underrun_frames = self
                .underrun_frames
                .saturating_add((frames - generated) as u64);
        }
        // Finish the gain ramp even on a short block, so the next block starts
        // from the level the host asked for rather than a stale one.
        self.gain_current = target;
        &self.mix[..len]
    }

    #[cfg(test)]
    fn fill_frames(&self) -> usize {
        self.fifo_l.len()
    }

    #[cfg(test)]
    fn drift_ratio(&self) -> f64 {
        self.drift.ratio()
    }
}

/// An open talkback capture plus its monitor mixer.
pub(crate) struct TalkbackMonitor {
    capture: Box<dyn SystemCapture>,
    mixer: MonitorMixer,
    input_channels: usize,
    output_rate: u32,
    /// The device selection the monitor was opened with, so a config change
    /// from one name to another reopens instead of silently keeping the old
    /// device.
    requested_device: Option<String>,
    scratch: Vec<f32>,
    device_name: String,
    last_idle_pump: Option<Instant>,
}

impl TalkbackMonitor {
    /// Open the selected input device and start the monitor. The output rate
    /// is the rate of the stream that will carry the monitor mix.
    pub(crate) fn open(
        config: &config::TalkbackConfig,
        output_rate: u32,
    ) -> Result<Self, CaptureError> {
        let mut capture = open_input_capture(config.device.as_deref(), CAPTURE_RING_FRAMES)?;
        let input_rate = capture.sample_rate();
        let input_channels = capture.channels().max(1) as usize;
        let device_name = capture.device_name();
        capture.start()?;
        log::info!(
            "talkback: monitoring '{}' ({} Hz / {} ch) at {} Hz output, {:.1} dB",
            device_name,
            input_rate,
            input_channels,
            output_rate,
            clamp_gain_db(config.gain_db)
        );
        Ok(Self {
            capture,
            mixer: MonitorMixer::new(input_rate, output_rate, config.gain_db),
            input_channels,
            output_rate,
            requested_device: config.device.clone(),
            scratch: vec![0.0; DRAIN_CHUNK_FRAMES * input_channels],
            device_name,
            last_idle_pump: None,
        })
    }

    pub(crate) fn requested_device(&self) -> Option<&str> {
        self.requested_device.as_deref()
    }

    pub(crate) fn device_name(&self) -> &str {
        &self.device_name
    }

    pub(crate) fn output_rate(&self) -> u32 {
        self.output_rate
    }

    pub(crate) fn gain_db(&self) -> f32 {
        self.mixer.gain_db()
    }

    pub(crate) fn underrun_frames(&self) -> u64 {
        self.mixer.underrun_frames()
    }

    pub(crate) fn overrun_frames(&self) -> u64 {
        self.mixer.overrun_frames()
    }

    pub(crate) fn set_gain_db(&mut self, gain_db: f32) {
        self.mixer.set_gain_db(gain_db);
    }

    /// Drain everything the capture thread has produced into the mixer FIFO.
    pub(crate) fn pump_capture(&mut self) {
        let buffer = self.capture.buffer();
        loop {
            let frames = buffer.pop_frames_interleaved(&mut self.scratch, self.input_channels);
            if frames == 0 {
                break;
            }
            self.mixer.push_input(
                &self.scratch[..frames * self.input_channels],
                self.input_channels,
            );
        }
    }

    /// Render a program block with the monitor mixed into its front pair.
    pub(crate) fn render(&mut self, program: &[f32], channels: usize, silent: bool) -> &[f32] {
        self.pump_capture();
        self.mixer.render(program, channels, silent)
    }

    /// Render a monitor-only block (no program). The transport is not
    /// playing; the idle pump feeds the primary output so push-to-talk is
    /// audible without a loaded track.
    pub(crate) fn render_idle(&mut self, frames: usize, channels: usize) -> &[f32] {
        self.pump_capture();
        self.mixer.render_idle(frames, channels)
    }

    /// Frames to generate for an idle (transport not playing) monitor block,
    /// paced by elapsed time so a monitor-only stream does not overfill the
    /// output ring.
    pub(crate) fn idle_block_frames(&mut self, output_rate: u32) -> usize {
        let now = Instant::now();
        let elapsed = self
            .last_idle_pump
            .map(|last| now.duration_since(last).as_secs_f64())
            .unwrap_or(0.005)
            .clamp(0.001, 0.050);
        self.last_idle_pump = Some(now);
        ((elapsed * f64::from(output_rate.max(1))).round() as usize)
            .clamp(64, MAX_AUDIO_BLOCK_FRAMES)
    }
}

impl Drop for TalkbackMonitor {
    fn drop(&mut self) {
        self.capture.stop();
        log::info!("talkback: monitor stopped");
    }
}

impl AudioEngine {
    /// Bring the live monitor in line with `config.talkback` and the active
    /// output rate. Idempotent: gain-only changes are applied to the running
    /// mixer, while an enable/disable, device change, or output-rate change
    /// opens or closes the capture endpoint.
    ///
    /// Opening the microphone is a control-thread operation (it enumerates
    /// and negotiates a device), so this is never called from the sample
    /// path. A failure is recorded in `talkback_error` and surfaced through
    /// [`EngineSettings`](crate::engine::EngineSettings); it never fails the
    /// surrounding config apply or stops playback.
    pub(crate) fn sync_talkback(&mut self) {
        let desired = self.config.talkback.clone();
        if !self.is_running() || !desired.enabled {
            if self.talkback.take().is_some() {
                log::info!("talkback: monitor disabled");
            }
            self.talkback_error = None;
            self.sync_monitor_transport();
            return;
        }

        if let Some(monitor) = self.talkback.as_mut() {
            if monitor.output_rate() == self.output_sample_rate
                && monitor.requested_device() == desired.device.as_deref()
            {
                monitor.set_gain_db(desired.gain_db);
                self.talkback_error = None;
                return;
            }
        }

        // Device or output rate moved: the old conversion ratio is invalid.
        self.talkback = None;
        match TalkbackMonitor::open(&desired, self.output_sample_rate) {
            Ok(monitor) => {
                self.talkback = Some(monitor);
                self.talkback_error = None;
            }
            Err(error) => {
                let message = error.to_string();
                log::warn!("talkback: {message}");
                self.talkback_error = Some(message);
            }
        }
        self.sync_monitor_transport();
    }

    /// Keep the primary stream draining while a monitor is live.
    ///
    /// Pause/stop normally pauses the output so no silence is written to the
    /// DAC. A live monitor is different: push-to-talk must work without a
    /// loaded track, so the stream stays running and the buffered program
    /// tail is discarded before the monitor takes over — otherwise resuming
    /// would replay a stale fragment of the last track before the microphone.
    pub(crate) fn sync_monitor_transport(&self) {
        if self.current_state() == PlaybackState::Playing {
            return;
        }
        if self.talkback.is_some() {
            if let Some(output) = self.audio_output.as_ref() {
                output.reset_buffer();
                output.resume();
            } else {
                self.sample_sink.reset();
            }
        } else if let Some(output) = self.audio_output.as_ref() {
            output.pause();
        }
    }

    /// Feed the primary output a monitor-only block while transport is not
    /// playing. Pacing uses wall time plus a small ring-fill target, so the
    /// idle stream neither starves nor accumulates monitor latency.
    pub(crate) fn pump_talkback_idle(&mut self) {
        if self.current_state() == PlaybackState::Playing
            || self.dsd.native_dsd_active
            || self.dsd.dop_active
            || self.dsd.sdm_active
        {
            return;
        }
        let Some(output) = self.audio_output.as_ref() else {
            return;
        };
        let channels = (output.channels() as usize).clamp(1, crate::buffer::MAX_CHANNELS);
        let Some(monitor) = self.talkback.as_mut() else {
            return;
        };
        let rate = self.output_sample_rate.max(1);
        let elapsed_frames = monitor.idle_block_frames(rate);
        // Hold ~50 ms in the primary ring: enough to ride out a scheduler
        // hiccup without making push-to-talk audibly lag.
        let target_fill = (rate as usize / 20).max(256);
        let buffered = self.sample_sink.buffered_frames();
        let frames = elapsed_frames
            .saturating_add(target_fill.saturating_sub(buffered))
            .min(MAX_AUDIO_BLOCK_FRAMES);
        let block = monitor.render_idle(frames, channels);
        let _ = self.sample_sink.push_interleaved(block, channels);
    }
}

pub(crate) fn clamp_gain_db(gain_db: f32) -> f32 {
    if gain_db.is_finite() {
        gain_db.clamp(-60.0, 12.0)
    } else {
        0.0
    }
}

fn db_to_gain(gain_db: f32) -> f32 {
    let clamped = clamp_gain_db(gain_db);
    if clamped <= -60.0 {
        0.0
    } else {
        10.0_f32.powf(clamped / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    #[test]
    fn the_monitor_is_mixed_into_the_front_pair_only() {
        let mut mixer = MonitorMixer::new(RATE, RATE, 0.0);
        // A constant input is the easiest signal to reason about through the
        // interpolator: every output sample should be the input level.
        mixer.push_input(&[0.5f32; 512], 1);

        let silent = [0.0f32; 256 * 6];
        let out = mixer.render(&silent, 6, true);
        for frame in out.chunks(6) {
            assert!(
                (frame[0] - 0.5).abs() < 1e-5,
                "monitor must reach front left: {}",
                frame[0]
            );
            assert!(
                (frame[1] - 0.5).abs() < 1e-5,
                "monitor must reach front right: {}",
                frame[1]
            );
            for (channel, sample) in frame.iter().enumerate().skip(2) {
                assert!(
                    sample.abs() < 1e-7,
                    "channel {channel} must stay untouched, got {sample}"
                );
            }
        }
    }

    #[test]
    fn the_monitor_gain_is_applied_in_db() {
        let mut mixer = MonitorMixer::new(RATE, RATE, -6.0);
        mixer.push_input(&[1.0f32; 512], 1);
        let silent = [0.0f32; 256 * 2];
        let out = mixer.render(&silent, 2, true);
        let expected = 10.0f32.powf(-6.0 / 20.0);
        assert!(
            (out[0] - expected).abs() < 1e-5,
            "expected {expected}, got {}",
            out[0]
        );
    }

    #[test]
    fn a_stereo_input_is_downmixed_to_mono_by_averaging() {
        let mut mixer = MonitorMixer::new(RATE, RATE, 0.0);
        // L = +1, R = -1 averages to silence, which proves the downmix is a
        // real average rather than "take the left channel".
        mixer.push_input(&[1.0f32, -1.0].repeat(256), 2);
        let silent = [0.0f32; 128 * 2];
        let out = mixer.render(&silent, 2, true);
        for sample in out {
            assert!(sample.abs() < 1e-6, "downmix must cancel, got {sample}");
        }
    }

    #[test]
    fn a_missing_input_leaves_the_program_untouched_and_counts_underruns() {
        let mut mixer = MonitorMixer::new(RATE, RATE, 0.0);
        let program = [0.25f32; 64 * 2];
        let out = mixer.render(&program, 2, false);
        assert_eq!(out, program);
        assert!(mixer.underrun_frames() > 0);
    }

    #[test]
    fn a_fuller_fifo_steers_the_conversion_slower() {
        let mut mixer = MonitorMixer::new(RATE, RATE, 0.0);
        // Fill past the controller's midpoint. The controller's ratio is an
        // output-production scale: positive fill error must make it less than
        // unity. The mixer inverts that into an input step, so a full FIFO is
        // consumed faster and drains.
        mixer.push_input(&[0.1f32; FIFO_CAPACITY], 1);
        assert_eq!(mixer.fill_frames(), FIFO_CAPACITY);
        // Drain less than the FIFO holds, so every update still samples a
        // fill above the target. (An empty FIFO correctly steers the other
        // way, so draining it all would test the opposite sign.)
        for _ in 0..8 {
            let silent = [0.0f32; 64 * 2];
            let _ = mixer.render(&silent, 2, true);
        }
        assert!(mixer.fill_frames() > FIFO_CAPACITY / 2);
        assert!(
            mixer.drift_ratio() < 1.0,
            "a full FIFO must slow output production, ratio was {}",
            mixer.drift_ratio()
        );
    }
}

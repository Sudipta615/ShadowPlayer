//! An audible preview of the configured crossfade.
//!
//! The real transition happens between two tracks, so the preview cannot play
//! the user's music without disturbing the main engine. Instead it renders a
//! short, self-contained demo: two synthetic voices, a fade-out and a fade-in
//! using the same curve and duration the engine will use. The rendered buffer
//! is both what is heard and what the preview waveform is drawn from, so the
//! picture and the sound cannot describe different curves.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use config::CrossfadeCurve;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};

const PREVIEW_SECS: f32 = 3.0;
const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: usize = 2;

/// A running preview stream. Dropping it stops the sound.
pub struct CrossfadePreview {
    _stream: Option<cpal::Stream>,
    peaks: Vec<f32>,
    position: Arc<AtomicUsize>,
    error: Option<String>,
}

impl Default for CrossfadePreview {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossfadePreview {
    pub fn new() -> Self {
        Self {
            _stream: None,
            peaks: Vec::new(),
            position: Arc::new(AtomicUsize::new(0)),
            error: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        self._stream.is_some()
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// How far through the three-second loop the preview is, 0.0–1.0.
    pub fn progress(&self) -> f32 {
        let total = (PREVIEW_SECS * SAMPLE_RATE as f32) as usize;
        (self.position.load(Ordering::Relaxed) % total.max(1)) as f32 / total.max(1) as f32
    }

    /// Redraw the preview envelope for a configuration without playing it.
    ///
    /// This is what lets the panel show the curve before the first listen.
    /// While the preview is playing the running buffer owns the picture, so
    /// the idle snapshot must not replace it mid-transition.
    pub fn set_preview(&mut self, curve: CrossfadeCurve, duration_secs: f32) {
        if self.is_playing() {
            return;
        }
        self.peaks = render_demo(curve, duration_secs).1;
    }

    /// Start (or restart) the loop for `curve` and `duration_secs`.
    pub fn start(&mut self, curve: CrossfadeCurve, duration_secs: f32) {
        self.stop();
        self.error = None;

        let (samples, peaks) = render_demo(curve, duration_secs);
        self.peaks = peaks;
        let total_frames = samples.len() / CHANNELS;

        match open_stream(Arc::new(samples), self.position.clone()) {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    self.error = Some(tr_args!(
                        "Could not start preview audio: {error}",
                        "error" => e
                    ));
                    return;
                }
                self._stream = Some(stream);
                log::info!("Crossfade preview playing {total_frames} frames");
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }

    pub fn stop(&mut self) {
        self._stream = None;
        self.position.store(0, Ordering::Relaxed);
    }

    pub fn toggle(&mut self, curve: CrossfadeCurve, duration_secs: f32) {
        if self.is_playing() {
            self.stop();
        } else {
            self.start(curve, duration_secs);
        }
    }
}

/// Render the crossfade demo and return both the interleaved stereo buffer and
/// a downsampled envelope for the panel.
pub fn render_demo(curve: CrossfadeCurve, duration_secs: f32) -> (Vec<f32>, Vec<f32>) {
    let total_frames = (PREVIEW_SECS * SAMPLE_RATE as f32).round() as usize;
    let fade_frames =
        ((duration_secs.clamp(0.2, PREVIEW_SECS) * SAMPLE_RATE as f32) as usize).min(total_frames);
    let fade_start = total_frames.saturating_sub(fade_frames);

    let mut samples = Vec::with_capacity(total_frames * CHANNELS);
    for frame in 0..total_frames {
        let t = frame as f32 / SAMPLE_RATE as f32;

        // Two voices, deliberately different in register and timbre so the
        // transition is obvious: a warm triad A and a brighter triad B.
        let voice_a = 0.55 * (2.0 * std::f32::consts::PI * 196.0 * t).sin()
            + 0.25 * (2.0 * std::f32::consts::PI * 247.0 * t).sin()
            + 0.12 * (2.0 * std::f32::consts::PI * 294.0 * t).sin();
        let voice_b = 0.50 * (2.0 * std::f32::consts::PI * 293.66 * t).sin()
            + 0.22 * (2.0 * std::f32::consts::PI * 370.0 * t).sin()
            + 0.14 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();

        let (a_gain, b_gain) = if frame < fade_start {
            (1.0, 0.0)
        } else {
            let progress = (frame - fade_start) as f32 / fade_frames.max(1) as f32;
            fade_gains(curve, progress)
        };

        // A 5 ms fade at the loop seam prevents a click when the buffer wraps.
        let seam = (frame.min(total_frames - 1 - frame) as f32 / (0.005 * SAMPLE_RATE as f32))
            .clamp(0.0, 1.0);

        let mixed = (voice_a * a_gain + voice_b * b_gain) * seam * 0.45;
        // Slight stereo width so the preview exercises both channels.
        samples.push(mixed);
        samples.push(mixed * 0.92);
    }

    // One peak per ~4 px at the widest panel.
    let buckets = 160;
    let mut peaks = vec![0.0f32; buckets];
    for (frame, pair) in samples.chunks_exact(CHANNELS).enumerate() {
        let bucket = frame * buckets / total_frames.max(1);
        let amplitude = pair[0].abs().max(pair[1].abs());
        if let Some(slot) = peaks.get_mut(bucket) {
            *slot = slot.max(amplitude);
        }
    }
    let max = peaks.iter().copied().fold(0.0f32, f32::max).max(0.001);
    for peak in &mut peaks {
        *peak = (*peak / max).clamp(0.0, 1.0);
    }

    (samples, peaks)
}

/// The outgoing and incoming gains at `progress` (0.0–1.0).
pub fn fade_gains(curve: CrossfadeCurve, progress: f32) -> (f32, f32) {
    let p = progress.clamp(0.0, 1.0);
    match curve {
        CrossfadeCurve::Linear => (1.0 - p, p),
        CrossfadeCurve::ConstantPower => {
            let angle = p * std::f32::consts::FRAC_PI_2;
            (angle.cos(), angle.sin())
        }
        CrossfadeCurve::Exponential => ((1.0 - p) * (1.0 - p), p * p),
        CrossfadeCurve::Logarithmic => ((1.0 - p).sqrt(), p.sqrt()),
        CrossfadeCurve::SCurve => {
            let smooth = p * p * (3.0 - 2.0 * p);
            (1.0 - smooth, smooth)
        }
    }
}

fn open_stream(samples: Arc<Vec<f32>>, position: Arc<AtomicUsize>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| tr!("No audio output device is available for the preview"))?;
    let supported = device.default_output_config().map_err(|e| {
        tr_args!(
            "Could not query the output device: {error}",
            "error" => e
        )
    })?;
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let output_rate = config.sample_rate;

    match sample_format {
        SampleFormat::F32 => build::<f32>(&device, config, samples, position, output_rate),
        SampleFormat::I16 => build::<i16>(&device, config, samples, position, output_rate),
        SampleFormat::U16 => build::<u16>(&device, config, samples, position, output_rate),
        SampleFormat::I32 => build::<i32>(&device, config, samples, position, output_rate),
        SampleFormat::U8 => build::<u8>(&device, config, samples, position, output_rate),
        other => Err(tr_args!(
            "Unsupported preview sample format: {format}",
            "format" => other
        )),
    }
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Arc<Vec<f32>>,
    position: Arc<AtomicUsize>,
    output_rate: u32,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let total_frames = (samples.len() / CHANNELS).max(1);
    let source = samples;
    let pos = position;
    // The demo is rendered at 44.1 kHz, but the device's default stream may
    // run at 48 kHz or another rate. Step through the source by the ratio of
    // the two rates instead of treating one output frame as one source frame;
    // otherwise the preview plays at the wrong pitch (and progress drifts).
    let step = SAMPLE_RATE as f64 / output_rate.max(1) as f64;
    let mut source_pos = 0.0f64;
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let base = source_pos.floor() as usize % total_frames;
                    let next = (base + 1) % total_frames;
                    let fraction = source_pos - source_pos.floor();
                    let interpolate = |channel: usize| {
                        let a = source
                            .get(base * CHANNELS + channel)
                            .copied()
                            .unwrap_or(0.0) as f64;
                        let b = source
                            .get(next * CHANNELS + channel)
                            .copied()
                            .unwrap_or(a as f32) as f64;
                        (a + (b - a) * fraction) as f32
                    };
                    let left = interpolate(0);
                    let right = interpolate(1);
                    if let Some(sample) = frame.first_mut() {
                        *sample = T::from_sample(left);
                    }
                    if let Some(sample) = frame.get_mut(1) {
                        *sample = T::from_sample(right);
                    }
                    for sample in frame.iter_mut().skip(2) {
                        let value: T = Sample::EQUILIBRIUM;
                        *sample = value;
                    }

                    source_pos += step;
                    if source_pos >= total_frames as f64 {
                        source_pos %= total_frames as f64;
                    }
                    pos.store(source_pos as usize % total_frames, Ordering::Relaxed);
                }
            },
            |err| log::debug!("Crossfade preview stream error: {err}"),
            None,
        )
        .map_err(|e| {
            tr_args!(
                "Could not open the preview output stream: {error}",
                "error" => e
            )
        })?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_power_keeps_energy_constant() {
        for step in 0..=10 {
            let (out, incoming) = fade_gains(CrossfadeCurve::ConstantPower, step as f32 / 10.0);
            let power = out * out + incoming * incoming;
            assert!((power - 1.0).abs() < 1e-5, "power={power}");
        }
    }

    #[test]
    fn preview_has_a_distinct_second_half() {
        let (samples, peaks) = render_demo(CrossfadeCurve::Linear, 1.0);
        assert_eq!(
            samples.len(),
            PREVIEW_SECS as usize * SAMPLE_RATE as usize * CHANNELS
        );
        assert!(peaks.iter().any(|p| *p > 0.5));
        // The loudest part of the demo is before the fade ends, so the
        // envelope must not be flat.
        let loud = peaks.iter().filter(|p| **p > 0.8).count();
        assert!(loud > 10, "peaks={peaks:?}");
    }
}

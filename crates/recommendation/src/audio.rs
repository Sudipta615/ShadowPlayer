use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex, OnceLock};

/// A single decoded window. `samples` are interleaved PCM frames.
///
/// The decoder adapter should seek and decode only the requested interval, not
/// decode the whole file. The analyser downmixes this window and resamples it.
#[derive(Clone, Debug)]
pub struct PcmWindow {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl PcmWindow {
    /// Number of complete interleaved frames in the window.
    #[inline]
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Adapter boundary for the application's existing decoder (for example Symphonia).
/// For multichannel files, a channel-layout-aware decoder-side downmix to mono
/// is preferred: it reduces peak memory and avoids treating an LFE channel as a
/// full-band channel. `downmix_and_resample` still supports interleaved input as
/// a safe fallback when the host cannot provide a mono analysis mix.
pub trait AudioWindowSource {
    /// Duration of the track in seconds. Return a finite positive value when known.
    fn duration_seconds(&self) -> f64;

    /// Seek and decode one interval. `start_seconds` is clamped by the analyser.
    fn read_window(
        &mut self,
        start_seconds: f64,
        duration_seconds: f64,
    ) -> Result<PcmWindow, AudioReadError>;
}

#[derive(Debug, Clone)]
pub struct AudioReadError(pub String);

impl AudioReadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for AudioReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for AudioReadError {}

/// Downmix interleaved PCM to mono and apply a windowed-sinc low-pass resampler.
///
/// The resampler uses a rate-adaptive Blackman-windowed sinc filter and a
/// 128-phase coefficient table. Its filter support grows when downsampling
/// aggressively, while trigonometric coefficients are computed only once per
/// rate conversion rather than once per output sample.
///
/// The coefficient table is cached by `(input_rate, output_rate)` so analyzing
/// a library of tracks does not rebuild the same filter for every file.
pub fn downmix_and_resample(
    pcm: &PcmWindow,
    target_sample_rate: u32,
) -> Result<Vec<f32>, AudioReadError> {
    let mut cache = None;
    let mut mono = Vec::new();
    let mut output = Vec::new();
    downmix_and_resample_into(pcm, target_sample_rate, &mut cache, &mut mono, &mut output)?;
    Ok(output)
}

/// Reusable resampling plan. The plan is immutable after construction and is
/// shared through the process-wide cache below.
pub(crate) struct ResamplePlan {
    input_rate: u32,
    output_rate: u32,
    ratio: f64,
    radius: usize,
    tap_count: usize,
    coefficients: Vec<f32>,
    /// Sum of each phase's coefficients. Used to normalize interior samples
    /// without re-summing every tap for every output sample.
    phase_sums: Vec<f32>,
}

const PHASES: usize = 128;
const OUTPUT_RADIUS: f64 = 16.0;
const MAX_SOURCE_RADIUS: usize = 2_048;
const MAX_CACHED_PLANS: usize = 8;

impl ResamplePlan {
    fn new(input_rate: u32, output_rate: u32) -> Self {
        // Keep the filter support approximately constant in output-sample units.
        // A fixed 32-input-sample filter is too short when downsampling 192 kHz
        // to 8–11 kHz: its transition band becomes too wide and aliases into
        // features. The phase table avoids per-output trigonometric work.
        let ratio = output_rate as f64 / input_rate as f64;
        // Nyquist of the output is ratio / 2 cycles per input sample.
        // Applying the factor of 0.5 here is essential to suppress aliasing.
        let cutoff = ratio.min(1.0) * 0.5 * 0.94;
        let radius = (OUTPUT_RADIUS / ratio.min(1.0)).ceil() as usize;
        let radius = radius.clamp(16, MAX_SOURCE_RADIUS);
        let tap_count = radius * 2;
        let first_tap = -(radius as isize) + 1;
        let last_tap = radius as isize;
        let mut coefficients = vec![0.0_f32; PHASES * tap_count];
        let mut phase_sums = vec![0.0_f32; PHASES];

        for (phase_index, phase_sum) in phase_sums.iter_mut().enumerate() {
            let fraction = phase_index as f64 / PHASES as f64;
            let row_start = phase_index * tap_count;
            let mut sum = 0.0_f64;
            for (tap_index, tap) in (first_tap..=last_tap).enumerate() {
                let distance = tap as f64 - fraction;
                let normalized_distance = distance.abs() / radius as f64;
                if normalized_distance >= 1.0 {
                    continue;
                }
                let scaled = cutoff * distance;
                let sinc = if scaled.abs() < 1.0e-12 {
                    1.0
                } else {
                    (std::f64::consts::PI * scaled).sin() / (std::f64::consts::PI * scaled)
                };
                let window = 0.42
                    + 0.5 * (std::f64::consts::PI * normalized_distance).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * normalized_distance).cos();
                let coefficient = cutoff * sinc * window;
                coefficients[row_start + tap_index] = coefficient as f32;
                sum += coefficient;
            }
            *phase_sum = sum as f32;
        }

        Self {
            input_rate,
            output_rate,
            ratio,
            radius,
            tap_count,
            coefficients,
            phase_sums,
        }
    }

    fn resample_into(&self, input: &[f32], output: &mut Vec<f32>) {
        output.clear();
        if input.is_empty() {
            return;
        }
        if self.input_rate == self.output_rate {
            output.extend_from_slice(input);
            return;
        }

        let out_len = ((input.len() as f64 * self.ratio).round() as usize).max(1);
        if output.capacity() < out_len {
            output.reserve_exact(out_len - output.capacity());
        }
        let first_tap = -(self.radius as isize) + 1;
        let last_tap = self.radius as isize;
        let input_len = input.len() as isize;

        for out_index in 0..out_len {
            let center = out_index as f64 / self.ratio;
            let center_i = center.floor() as isize;
            let fraction = center - center_i as f64;
            let phase = ((fraction * PHASES as f64) as usize).min(PHASES - 1);
            let row_start = phase * self.tap_count;
            let source_start = center_i + first_tap;
            let source_end = center_i + last_tap;

            let (weighted, weight_sum) = if source_start >= 0 && source_end < input_len {
                // Interior samples have every tap in range. The precomputed
                // phase sum avoids a second accumulation loop.
                let base = source_start as usize;
                let mut acc = 0.0_f32;
                let coefficients = &self.coefficients[row_start..row_start + self.tap_count];
                for (tap_index, coefficient) in coefficients.iter().enumerate() {
                    acc += input[base + tap_index] * *coefficient;
                }
                (acc, self.phase_sums[phase])
            } else {
                // At the edges, normalize only over the taps that exist. This
                // preserves DC and avoids amplifying the zero-padded tail.
                let mut acc = 0.0_f32;
                let mut sum = 0.0_f32;
                for (tap_index, tap) in (first_tap..=last_tap).enumerate() {
                    let source_index = center_i + tap;
                    if source_index < 0 || source_index >= input_len {
                        continue;
                    }
                    let coefficient = self.coefficients[row_start + tap_index];
                    acc += input[source_index as usize] * coefficient;
                    sum += coefficient;
                }
                (acc, sum)
            };

            let sample = if weight_sum.abs() > 1.0e-12 {
                weighted / weight_sum
            } else {
                0.0
            };
            output.push(sample.clamp(-4.0, 4.0));
        }
    }
}

type PlanCache = Mutex<Vec<(u32, u32, Arc<ResamplePlan>)>>;

fn plan_cache() -> &'static PlanCache {
    static CACHE: OnceLock<PlanCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::with_capacity(MAX_CACHED_PLANS)))
}

fn get_plan(input_rate: u32, output_rate: u32) -> Arc<ResamplePlan> {
    let mut cache = plan_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((_, _, plan)) = cache
        .iter()
        .find(|(input, output, _)| *input == input_rate && *output == output_rate)
    {
        return Arc::clone(plan);
    }
    let plan = Arc::new(ResamplePlan::new(input_rate, output_rate));
    if cache.len() >= MAX_CACHED_PLANS {
        cache.remove(0);
    }
    cache.push((input_rate, output_rate, Arc::clone(&plan)));
    plan
}

/// Downmix and resample into caller-owned buffers. Reusing `mono` and `output`
/// across the three analysis windows removes the largest per-window allocations.
pub(crate) fn downmix_and_resample_into(
    pcm: &PcmWindow,
    target_sample_rate: u32,
    cache: &mut Option<Arc<ResamplePlan>>,
    mono: &mut Vec<f32>,
    output: &mut Vec<f32>,
) -> Result<(), AudioReadError> {
    if pcm.sample_rate == 0 {
        return Err(AudioReadError::new("source sample rate is zero"));
    }
    if target_sample_rate == 0 {
        return Err(AudioReadError::new("target sample rate is zero"));
    }
    if pcm.channels == 0 {
        return Err(AudioReadError::new("source channel count is zero"));
    }
    let channels = pcm.channels as usize;
    if pcm.samples.len() % channels != 0 {
        return Err(AudioReadError::new(
            "interleaved PCM length is not divisible by channel count",
        ));
    }
    if pcm.samples.is_empty() {
        mono.clear();
        output.clear();
        return Ok(());
    }

    downmix_into(pcm, mono);

    if pcm.sample_rate == target_sample_rate {
        output.clear();
        output.extend_from_slice(mono);
        return Ok(());
    }

    let cache_matches = cache.as_ref().is_some_and(|plan| {
        plan.input_rate == pcm.sample_rate && plan.output_rate == target_sample_rate
    });
    if !cache_matches {
        *cache = Some(get_plan(pcm.sample_rate, target_sample_rate));
    }
    let plan = cache.as_ref().expect("resample plan was just initialized");
    plan.resample_into(mono, output);
    Ok(())
}

/// Downmix interleaved PCM to mono. For stereo, mid and side signals are both
/// evaluated and the higher-energy one is used. This prevents an anti-phase
/// stereo master from cancelling to near-silence in the analyser while keeping
/// ordinary correlated stereo unchanged.
fn downmix_into(pcm: &PcmWindow, mono: &mut Vec<f32>) {
    let channels = pcm.channels as usize;
    let frames = pcm.samples.len() / channels;
    mono.clear();
    if mono.capacity() < frames {
        mono.reserve_exact(frames - mono.capacity());
    }

    if channels == 1 {
        mono.extend(
            pcm.samples
                .iter()
                .map(|sample| if sample.is_finite() { *sample } else { 0.0 }),
        );
        return;
    }

    if channels == 2 {
        let mut mid_energy = 0.0_f64;
        let mut side_energy = 0.0_f64;
        for frame in pcm.samples.chunks_exact(2) {
            let left = sanitize(frame[0]);
            let right = sanitize(frame[1]);
            let mid = 0.5 * (left + right);
            let side = 0.5 * (left - right);
            mid_energy += (mid as f64) * (mid as f64);
            side_energy += (side as f64) * (side as f64);
        }
        // Only switch to the side signal when mid is almost completely
        // cancelled. A modest margin avoids changing the downmix for ordinary,
        // slightly decorrelated stereo material.
        let use_side = mid_energy <= 1.0e-12 || side_energy > mid_energy * 8.0;
        for frame in pcm.samples.chunks_exact(2) {
            let left = sanitize(frame[0]);
            let right = sanitize(frame[1]);
            mono.push(if use_side {
                0.5 * (left - right)
            } else {
                0.5 * (left + right)
            });
        }
        return;
    }

    let scale = 1.0 / channels as f64;
    for frame in pcm.samples.chunks_exact(channels) {
        let mut sum = 0.0_f64;
        for sample in frame {
            sum += sanitize(*sample) as f64;
        }
        mono.push((sum * scale) as f32);
    }
}

#[inline]
fn sanitize(sample: f32) -> f32 {
    if sample.is_finite() {
        sample
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(samples: Vec<f32>, sample_rate: u32, channels: u16) -> PcmWindow {
        PcmWindow {
            samples,
            sample_rate,
            channels,
        }
    }

    #[test]
    fn downmix_averages_channels() {
        let pcm = window(vec![1.0, 1.0, 0.5, 0.5], 8_000, 2);
        let mono = downmix_and_resample(&pcm, 8_000).unwrap();
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 1.0).abs() < 1.0e-6);
        assert!((mono[1] - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn downmix_preserves_anti_phase_stereo_energy() {
        let sample_rate = 8_000_u32;
        let samples: Vec<f32> = (0..sample_rate)
            .flat_map(|i| {
                let value =
                    (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin();
                [value, -value]
            })
            .collect();
        let pcm = window(samples, sample_rate, 2);
        let mono = downmix_and_resample(&pcm, sample_rate).unwrap();
        let rms = (mono.iter().map(|value| value * value).sum::<f32>() / mono.len() as f32).sqrt();
        assert!(rms > 0.6, "anti-phase stereo was cancelled: RMS {rms}");
    }

    #[test]
    fn downmix_rejects_invalid_pcm() {
        assert!(downmix_and_resample(&window(vec![0.0; 4], 0, 2), 8_000).is_err());
        assert!(downmix_and_resample(&window(vec![0.0; 4], 8_000, 0), 8_000).is_err());
        assert!(downmix_and_resample(&window(vec![0.0; 3], 8_000, 2), 8_000).is_err());
        assert!(downmix_and_resample(&window(vec![0.0; 4], 8_000, 2), 0).is_err());
    }

    #[test]
    fn resampler_preserves_constant_signal_away_from_edges() {
        let pcm = window(vec![0.25; 48_000], 48_000, 1);
        let output = downmix_and_resample(&pcm, 11_025).unwrap();
        assert_eq!(output.len(), 11_025);
        let middle = &output[100..output.len() - 100];
        let mean = middle.iter().map(|value| *value as f64).sum::<f64>() / middle.len() as f64;
        assert!((mean - 0.25).abs() < 0.01);
    }

    #[test]
    fn downsampler_suppresses_tones_above_target_nyquist() {
        let sample_rate = 48_000_u32;
        let duration_seconds = 1.5_f32;
        let sample_count = (sample_rate as f32 * duration_seconds) as usize;
        let samples: Vec<f32> = (0..sample_count)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 10_000.0 * t).sin() * 0.5
            })
            .collect();
        let pcm = window(samples, sample_rate, 1);
        let output = downmix_and_resample(&pcm, 11_025).unwrap();
        let rms = (output
            .iter()
            .map(|sample| (*sample as f64).powi(2))
            .sum::<f64>()
            / output.len() as f64)
            .sqrt();
        assert!(
            rms < 0.08,
            "aliased 10 kHz tone remains too strong: RMS {rms}"
        );
    }

    #[test]
    fn resampler_output_length_tracks_input_duration() {
        let pcm = window(vec![0.0; 44_100 * 3], 44_100, 1);
        let output = downmix_and_resample(&pcm, 11_025).unwrap();
        assert_eq!(output.len(), 11_025 * 3);
    }

    #[test]
    fn resampler_handles_short_and_non_finite_input() {
        let pcm = window(vec![f32::NAN, f32::INFINITY, 0.5], 48_000, 1);
        let output = downmix_and_resample(&pcm, 11_025).unwrap();
        assert!(!output.is_empty());
        assert!(output.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn cached_plan_is_reused_for_same_rates() {
        let first = get_plan(44_100, 11_025);
        let second = get_plan(44_100, 11_025);
        assert!(Arc::ptr_eq(&first, &second));
    }
}

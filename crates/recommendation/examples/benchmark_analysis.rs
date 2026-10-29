//! Synthetic audio-analysis microbenchmark. It measures downmix, resampling,
//! feature extraction, tempo estimation, and chroma analysis on an in-memory
//! signal. It does not measure decoder seeking or disk I/O. Run in release mode:
//!
//! cargo run --release --example benchmark_analysis
//! cargo run --release --example benchmark_analysis -- 5

use shadow_recommender::{
    analyze_track, AnalysisConfig, AnalysisProfile, AudioReadError, AudioWindowSource, PcmWindow,
};
use std::hint::black_box;
use std::time::Instant;

const SAMPLE_RATE: u32 = 44_100;
const CHANNELS: u16 = 2;
const DURATION_SECONDS: f64 = 24.0;

/// A deterministic, stereo test signal with a bass line, harmonics, a simple
/// amplitude envelope, and a 120 BPM click track for tempo/chroma coverage.
struct SyntheticSource {
    samples: Vec<f32>,
    frames: usize,
}

impl SyntheticSource {
    fn new() -> Self {
        let frames = (SAMPLE_RATE as f64 * DURATION_SECONDS).round() as usize;
        let mut samples = Vec::with_capacity(frames * CHANNELS as usize);
        for frame in 0..frames {
            let t = frame as f64 / SAMPLE_RATE as f64;
            let beat_phase = (t * 2.0).fract();
            let click = if beat_phase < 0.02 {
                (1.0 - beat_phase / 0.02) * 0.35
            } else {
                0.0
            };
            let envelope = 0.55 + 0.20 * (2.0 * std::f64::consts::PI * 0.13 * t).sin();
            let bass = (2.0 * std::f64::consts::PI * 110.0 * t).sin() * 0.30;
            let body = (2.0 * std::f64::consts::PI * 220.0 * t).sin() * 0.18
                + (2.0 * std::f64::consts::PI * 330.0 * t).sin() * 0.12
                + (2.0 * std::f64::consts::PI * 440.0 * t).sin() * 0.08;
            let shimmer = (2.0 * std::f64::consts::PI * 1_760.0 * t).sin() * 0.03;
            let left = ((bass + body + shimmer) * envelope + click * 0.8).clamp(-1.0, 1.0);
            let right = ((bass * 0.9 + body + shimmer * 0.7) * envelope + click).clamp(-1.0, 1.0);
            samples.push(left as f32);
            samples.push(right as f32);
        }
        Self { samples, frames }
    }
}

impl AudioWindowSource for SyntheticSource {
    fn duration_seconds(&self) -> f64 {
        DURATION_SECONDS
    }

    fn read_window(
        &mut self,
        start_seconds: f64,
        duration_seconds: f64,
    ) -> Result<PcmWindow, AudioReadError> {
        let start_frame =
            ((start_seconds.max(0.0) * SAMPLE_RATE as f64).round() as usize).min(self.frames);
        let requested_frames = (duration_seconds.max(0.0) * SAMPLE_RATE as f64).round() as usize;
        let frame_count = requested_frames.min(self.frames - start_frame);
        let begin = start_frame * CHANNELS as usize;
        let end = begin + frame_count * CHANNELS as usize;
        Ok(PcmWindow {
            samples: self.samples[begin..end].to_vec(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
        })
    }
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}

fn main() {
    let iterations = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5)
        .clamp(1, 100);
    let mut source = SyntheticSource::new();

    println!(
        "signal={DURATION_SECONDS:.0}s stereo {SAMPLE_RATE}Hz iterations={iterations} \
         (synthetic in-memory analysis; excludes decoder seek/IO)"
    );
    for profile in [
        AnalysisProfile::LowResource,
        AnalysisProfile::Balanced,
        AnalysisProfile::HighQuality,
    ] {
        let config = AnalysisConfig::for_profile(profile);
        let mut samples = Vec::with_capacity(iterations);
        let mut summary = None;

        // Warm caches, branch predictors, and the process-wide resample plan.
        if let Ok(result) = analyze_track(&mut source, &config) {
            black_box(result);
        }
        for _ in 0..iterations {
            let start = Instant::now();
            let result = analyze_track(&mut source, &config);
            let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
            match result {
                Ok(result) => {
                    black_box(&result);
                    summary = Some(result);
                }
                Err(error) => {
                    println!("{profile:?}: analysis failed: {error}");
                    break;
                }
            }
            samples.push(elapsed);
        }
        if samples.is_empty() {
            continue;
        }
        samples.sort_by(f64::total_cmp);
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let Some(result) = summary else {
            continue;
        };
        let tempo_bpm = if result.features.is_valid(7) {
            result.features.values[7] * 150.0 + 50.0
        } else {
            0.0
        };
        println!(
            "{profile:?}: p50={:.2}ms p95={:.2}ms mean={:.2}ms windows={}/{} \
             sampled={:.2}s tempo={:.1}bpm conf={:.2} tuning={:+.1}c valid={}",
            percentile(&samples, 0.50),
            percentile(&samples, 0.95),
            mean,
            result.windows_analyzed,
            result.windows_analyzed + result.windows_failed,
            result.sampled_seconds,
            tempo_bpm,
            result.tempo_confidence,
            result.tuning_cents,
            result.features.valid_count(),
        );
    }
}

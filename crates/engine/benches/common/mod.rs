//! Shared deterministic test-signal generators for the engine's Criterion benches.
//!
//! Every file in `benches/` is compiled as its **own crate**, so this module is
//! compiled independently into each of them (`mod common;` resolves to
//! `benches/common/mod.rs` from every bench file — Cargo's bench auto-discovery
//! only looks at `benches/*.rs` and `benches/*/main.rs`, so this file is never
//! mistaken for a benchmark target of its own).
//!
//! It contains **only** signal generation, deliberately:
//!
//! - No audio files, no binary assets, no network. Every bench builds its
//!   stimulus in memory, so `cargo bench` is reproducible on a clean checkout.
//! - Deterministic output: the noise generator is a fixed-seed LCG, so two runs
//!   of the same bench measure the same signal.
//! - No engine types are imported here, so a bench can use the helpers without
//!   inheriting behaviour it did not ask to measure.

// Each bench crate compiles its own copy of this module and uses only the
// helpers it needs, so "never used" is the normal case for the rest.
#![allow(dead_code)]

/// The engine's native output rate (48 kHz), used as the default sample rate
/// for every bench unless the bench is specifically about rate conversion.
pub const SAMPLE_RATE: f32 = 48_000.0;

/// A mono sine wave of `frames` frames at `freq_hz`.
///
/// The phase is continuous across calls only if the caller keeps the returned
/// buffer; each call starts at phase 0, which is fine for a bench (the
/// stimulus is stationary either way).
pub fn sine(frames: usize, freq_hz: f32, sample_rate: f32) -> Vec<f32> {
    let step = 2.0 * std::f32::consts::PI * freq_hz / sample_rate;
    (0..frames).map(|i| (step * i as f32).sin() * 0.5).collect()
}

/// A stereo pair of sines at `left_hz` and `right_hz`.
///
/// Two *different* frequencies on the two channels (rather than the same tone
/// duplicated) so that a stage which accidentally collapses the stereo image
/// (a mono downmix, a mis-wired panner) changes the output instead of looking
/// identical to the input.
pub fn stereo_sine(
    frames: usize,
    left_hz: f32,
    right_hz: f32,
    sample_rate: f32,
) -> (Vec<f32>, Vec<f32>) {
    let left = sine(frames, left_hz, sample_rate);
    let right = sine(frames, right_hz, sample_rate);
    (left, right)
}

/// Deterministic white-ish noise from a fixed-seed linear congruential
/// generator.
///
/// Deterministic on purpose: the engine's decoders are content-sensitive (a
/// DSD decimator's output depends on the exact bit pattern), so the stimulus
/// has to be reproducible for a benchmark to be comparable between runs.
pub fn noise(frames: usize, seed: u64) -> Vec<f32> {
    let mut state = seed | 1;
    (0..frames)
        .map(|_| {
            // Numerical Recipes LCG constants.
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let unit = ((state >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0;
            (unit * 0.5) as f32
        })
        .collect()
}

/// A single unit impulse at the first sample, silence elsewhere.
///
/// The canonical probe for a linear time-invariant stage: the output is the
/// stage's impulse response, so a bench can use it to assert that a stage is
/// actually doing work.
pub fn impulse(frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames];
    if !out.is_empty() {
        out[0] = 1.0;
    }
    out
}

/// A logarithmic sine sweep from 20 Hz to 20 kHz over `frames` frames.
///
/// Used where a bench needs a stimulus with energy across the whole band
/// (a filter bank, a convolution kernel, a loudness meter) rather than a
/// single tone that would only excite one part of the spectrum.
pub fn sweep(frames: usize, f0: f32, f1: f32, sample_rate: f32) -> Vec<f32> {
    if frames == 0 {
        return Vec::new();
    }
    let total = frames as f64;
    let ratio = ((f1 / f0) as f64).ln();
    let mut phase = 0.0f64;
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = i as f64 / total;
        // Instantaneous frequency of an exponential sweep: f(t) = f0·(f1/f0)^t.
        let freq = f0 as f64 * (ratio * t).exp();
        phase += 2.0 * std::f64::consts::PI * freq / sample_rate as f64;
        out.push((phase.sin() as f32) * 0.5);
    }
    out
}

/// An exponentially-decaying noise burst — a room-impulse-response-like
/// kernel, deterministic and bounded.
pub fn decaying_noise(frames: usize, seed: u64, decay: f32) -> Vec<f32> {
    let raw = noise(frames, seed);
    raw.into_iter()
        .enumerate()
        .map(|(i, s)| s * (-(i as f32) * decay).exp())
        .collect()
}

/// Interleave a stereo pair into the interleaved `f32` layout the engine's
/// ring buffers and decoders use.
pub fn interleave(left: &[f32], right: &[f32]) -> Vec<f32> {
    let n = left.len().min(right.len());
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        out.push(left[i]);
        out.push(right[i]);
    }
    out
}

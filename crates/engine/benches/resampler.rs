//! # Resampler throughput per quality tier
//!
//! Measures the engine's Rubato-backed resampler
//! ([`engine::dsp::resampler::AudioResampler`]) across the four quality tiers
//! the engine exposes (`config::ResamplerQuality`: Fast, Balanced,
//! HighQuality, Ultra).
//!
//! What the numbers mean:
//!
//! - Each iteration feeds **100 ms of input** (4410 frames at 44.1 kHz) and
//!   drains every output frame the resampler produced. `Throughput::Elements`
//!   is the number of *input* frames, so Criterion reports input frames per
//!   second.
//! - **CPU per second of audio** is the number to read: the iteration
//!   represents 100 ms of audio, so a reported time of 1.0 ms means the
//!   resampler used 1 % of the CPU it is given. Multiply the reported
//!   microseconds by 10 to get the percentage of one core.
//! - The tier's **group delay** (the latency the engine reports and adds to
//!   its latency budget) is printed once per tier, because it is the number a
//!   host has to budget for, and it is the reason the tiers differ in cost:
//!   the measured tap ladder is 320 / 640 / 1120 / 2240 taps for Fast /
//!   Balanced / HighQuality / Ultra.
//!
//! What is actually being measured:
//!
//! - The engine's own `AudioResampler` wrapper around `rubato::Fft`, driven
//!   through its public per-sample `feed`/`read` API — the same API the
//!   decode loop uses. The input is a 441 Hz sine (left) and a 443 Hz sine
//!   (right) so a channel-swap bug would show up.
//! - 44.1 kHz → 48 kHz, the conversion a CD-sourced file needs on a 48 kHz
//!   output device.
//! - The resampler is warmed with one chunk before the measured region, so
//!   the first-chunk filter fill is not charged to the steady state.
//! - The measured time covers **both** halves of the API: the `feed` calls
//!   (which include the per-call rebuild check and the amortised FFT chunk
//!   processing) **and** the `read` calls that drain the output. That
//!   per-sample bookkeeping is a real cost the engine pays, and it is large
//!   enough relative to the FFT work that the four tiers land closer
//!   together than the 320 / 640 / 1120 / 2240-tap filter ladder alone would
//!   suggest. The group delays printed per tier (160 / 320 / 560 / 1120 output
//!   frames) show that the tiers really are different filters.
//! - The f64 variant (`AudioResampler<f64>`) is measured for the two expensive
//!   tiers only; it is the same code with a wider sample type.
//!
//! `black_box` is `std::hint::black_box`: Criterion 0.8 deprecated its own
//! re-export in favour of the standard-library one.

use std::hint::black_box;

use config::ResamplerQuality;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::dsp::resampler::AudioResampler;

mod common;

/// Source rate (a CD-sourced file).
const SRC_RATE: f32 = 44_100.0;
/// Output rate (a 48 kHz device).
const DST_RATE: f32 = 48_000.0;
/// Input frames per iteration: 100 ms at 44.1 kHz.
const CHUNK: usize = 4_410;

fn bench_resampler(c: &mut Criterion) {
    let (pl, pr) = common::stereo_sine(CHUNK, 441.0, 443.0, SRC_RATE);

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    let mut group = c.benchmark_group("resampler/f32_44k1_to_48k");
    group.throughput(Throughput::Elements(CHUNK as u64));
    for tier in [
        ResamplerQuality::Fast,
        ResamplerQuality::Balanced,
        ResamplerQuality::HighQuality,
        ResamplerQuality::Ultra,
    ] {
        let mut resampler = AudioResampler::<f32>::new(tier, SRC_RATE, DST_RATE)
            .expect("44.1 -> 48 kHz is a supported conversion");
        // Warm up: the first chunk fills the filter and is not representative
        // of the steady state.
        for i in 0..CHUNK {
            resampler.feed(pl[i], pr[i]);
        }
        while resampler.read().is_some() {}
        eprintln!(
            "[resampler] {tier:?}: effective tier {:?}, group delay {} output frames \
             ({:.2} ms)",
            resampler.effective_quality(),
            resampler.latency_samples(),
            resampler.latency_ms(),
        );
        group.bench_with_input(BenchmarkId::from_parameter(tier.as_str()), &tier, |b, _| {
            b.iter(|| {
                for i in 0..CHUNK {
                    resampler.feed(black_box(pl[i]), black_box(pr[i]));
                }
                let mut frames = 0usize;
                let mut acc = 0.0f32;
                while let Some((l, r)) = resampler.read() {
                    acc += l + r;
                    frames += 1;
                }
                black_box((frames, acc));
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("resampler/f64_44k1_to_48k");
    group.throughput(Throughput::Elements(CHUNK as u64));
    for tier in [ResamplerQuality::HighQuality, ResamplerQuality::Ultra] {
        let mut resampler = AudioResampler::<f64>::new(tier, SRC_RATE, DST_RATE)
            .expect("44.1 -> 48 kHz is a supported conversion");
        for i in 0..CHUNK {
            resampler.feed(pl[i] as f64, pr[i] as f64);
        }
        while resampler.read().is_some() {}
        group.bench_with_input(BenchmarkId::from_parameter(tier.as_str()), &tier, |b, _| {
            b.iter(|| {
                for i in 0..CHUNK {
                    resampler.feed(black_box(pl[i] as f64), black_box(pr[i] as f64));
                }
                let mut frames = 0usize;
                let mut acc = 0.0f64;
                while let Some((l, r)) = resampler.read() {
                    acc += l + r;
                    frames += 1;
                }
                black_box((frames, acc));
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_resampler);
criterion_main!(benches);

//! # Partitioned FFT convolution
//!
//! Measures the engine's partitioned overlap-add FFT convolver
//! ([`engine::dsp::convolution::ConvolutionEngine`]) — the stage behind the
//! convolution insert, the room-correction filter and the acoustic IR path.
//!
//! What the numbers mean:
//!
//! - Each iteration processes one **512-frame stereo block** (10.67 ms at
//!   48 kHz). `Throughput::Elements` is 512 frames.
//! - The engine's convolver only does an FFT round trip once per *partition*,
//!   so the block size shows up directly in the cost: a 64-sample partition
//!   does eight FFT round trips per 512-frame block, a 512-sample partition
//!   does one.
//!
//! What is actually being measured, and one honest limitation:
//!
//! - `partition_size` varies the **partition (FFT block) size**. The engine
//!   derives it from the constructor's `max_ir_length`:
//!   `block_size = min(DEFAULT_PARTITION_SIZE = 512, next_pow2(max_ir_length)).max(64)`.
//!   That makes **64, 128, 256 and 512 reachable**, and makes **1024 and 2048
//!   unreachable**: `DEFAULT_PARTITION_SIZE` clamps the partition to 512 and
//!   the engine exposes no public setter for it (`block_size()` is read-only).
//!   Rather than fabricate a 1024/2048 row the bench cannot produce, this
//!   file measures the partition sizes the engine can actually build and says
//!   so here.
//! - `partitions` therefore measures the *other* axis of partitioned
//!   convolution — the number of partitions (1, 2, 4, 8, 16) at the maximum
//!   partition size of 512, i.e. IR lengths of 512 … 8192 samples. This is
//!   the axis that grows with impulse-response length, and it is where the
//!   cost of a long room IR actually lands.
//! - `precision` measures the same convolution in the f64 path
//!   (`process_block_f64`), which the engine uses in Quality mode.
//!
//! The impulse responses are deterministic, exponentially-decaying noise
//! bursts (a room-impulse-response-like kernel). The input is an in-memory
//! sine pair.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::dsp::convolution::ConvolutionEngine;

mod common;

/// Block size of the audio block fed to the convolver in every measurement.
const BLOCK: usize = 512;

/// Build a convolver whose partition size is `partition` and load an IR of
/// `ir_len` samples into it.
///
/// `max_ir_length` is what actually selects the partition size (see the
/// module docs); the IR length then selects the partition *count*.
fn convolver(partition: usize, ir_len: usize, sample_rate: f32) -> ConvolutionEngine {
    let mut engine = ConvolutionEngine::new(sample_rate, partition);
    let ir: Vec<(f32, f32)> = common::decaying_noise(ir_len, 0xC0FFEE, 0.002)
        .into_iter()
        .map(|s| (s, s))
        .collect();
    engine
        .load_ir_from_samples(&ir)
        .expect("the synthetic IR is a valid kernel");
    engine.set_enabled(true);
    engine.set_wet_mix(1.0);
    engine
}

fn bench_convolution(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;
    let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    // ── Partition (FFT block) size ──────────────────────────────────────────
    let mut group = c.benchmark_group("convolution/partition_size");
    group.throughput(Throughput::Elements(BLOCK as u64));
    for partition in [64usize, 128, 256, 512] {
        let mut engine = convolver(partition, partition, sample_rate);
        eprintln!(
            "[convolution] partition {partition}: engine reports block_size {}, \
             {} partition(s), partition latency {} samples",
            engine.block_size(),
            engine.num_partitions(),
            engine.partition_latency_samples(),
        );
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_with_input(
            BenchmarkId::from_parameter(partition),
            &partition,
            |b, _| {
                b.iter(|| {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    engine.process_block(black_box(&mut l), black_box(&mut r));
                    black_box(&l);
                });
            },
        );
    }
    group.finish();

    // ── Partition count (impulse-response length) at the 512-sample partition ─
    let mut group = c.benchmark_group("convolution/partitions");
    group.throughput(Throughput::Elements(BLOCK as u64));
    for partitions in [1usize, 2, 4, 8, 16] {
        let ir_len = partitions * 512;
        let mut engine = convolver(512, ir_len, sample_rate);
        eprintln!(
            "[convolution] {partitions} partition(s) (IR {ir_len} samples): block_size {}, \
             algorithmic latency {} samples ({:.2} ms)",
            engine.block_size(),
            engine.algorithmic_latency_samples(),
            engine.latency_ms(),
        );
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_with_input(
            BenchmarkId::from_parameter(partitions),
            &partitions,
            |b, _| {
                b.iter(|| {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    engine.process_block(black_box(&mut l), black_box(&mut r));
                    black_box(&l);
                });
            },
        );
    }
    group.finish();

    // ── The f64 path (Quality mode) ─────────────────────────────────────────
    let mut group = c.benchmark_group("convolution/precision");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let mut engine = convolver(512, 4096, sample_rate);
        let (mut l, mut r): (Vec<f64>, Vec<f64>) = (
            pl.iter().map(|&s| s as f64).collect(),
            pr.iter().map(|&s| s as f64).collect(),
        );
        group.bench_function("f64_8_partitions", |b| {
            b.iter(|| {
                engine.process_block_f64(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    {
        let mut engine = convolver(512, 4096, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("f32_8_partitions", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                engine.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_convolution);
criterion_main!(benches);

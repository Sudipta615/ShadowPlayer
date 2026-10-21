//! # Output backend: the null sink and the master ring
//!
//! Measures the engine's output path with **no hardware**: the
//! [`engine::output::NullOutput`] backend and the SPSC ring
//! ([`engine::buffer::FixedFrameBuffer`]) it drains.
//!
//! What the numbers mean:
//!
//! - `output/ring_push` and `output/ring_pop` are the producer/consumer
//!   halves of the master ring, measured in **frames per second** of
//!   interleaved stereo audio. They are the data-movement cost every sample
//!   the engine plays pays, on top of the DSP itself.
//! - `output/null_end_to_end` is the **end-to-end latency** of the null
//!   backend: the wall-clock time from pushing a block into the master ring
//!   to the sink's drain thread reporting it consumed. This number is
//!   dominated by the backend's *deliberate* real-time pacing (the drain loop
//!   paces against a wall-clock deadline derived from the frame count, exactly
//!   as a device callback would) and by its 512-frame block quantisation — a
//!   sink that consumed faster than real time would hide starvation, which is
//!   the whole reason the backend exists. A reading near 10.7 ms (one 512-frame
//!   block period at 48 kHz) is the expected floor; a reading near 21 ms means
//!   the push landed just after the drain thread's block boundary.
//! - `output/null_capture_tail` is the cost of draining the sink's capture
//!   tail — the buffer the null backend keeps of what it actually consumed,
//!   which is what a test asserts on.
//!
//! What is actually being measured:
//!
//! - The engine's real [`FixedFrameBuffer`] (the SPSC ring the output matrix
//!   drains) and the engine's real [`NullOutput`] (the hardware-free sink that
//!   runs the same paced drain loop a device callback runs).
//! - The null backend is the one the engine's own CI uses to exercise the
//!   whole output path without a DAC, so this row measures the path CI
//!   measures.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use engine::buffer::FixedFrameBuffer;
use engine::output::{NullOutput, Output};

mod common;

/// Frames per block: the null backend's drain block is 512 frames, and the
/// engine's typical realtime block is the same order.
const BLOCK: usize = 512;
/// The backend's negotiated rate.
const RATE: u32 = 48_000;
/// Stereo channels.
const CHANNELS: usize = 2;

fn bench_output(c: &mut Criterion) {
    let (l, r) = common::stereo_sine(BLOCK, 997.0, 1009.0, RATE as f32);
    let interleaved = common::interleave(&l, &r);

    // ── The master ring: producer side ─────────────────────────────────────
    let mut group = c.benchmark_group("output/ring_push");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let buffer = FixedFrameBuffer::new(1 << 16).expect("the ring allocates");
        group.bench_function("512_frames", |b| {
            b.iter(|| {
                // Reset outside the measured work so the ring never fills (a
                // full ring exercises the drop path, not the copy path).
                buffer.reset();
                black_box(buffer.push_frames_interleaved(black_box(&interleaved), CHANNELS));
            });
        });
    }
    group.finish();

    // ── The master ring: consumer side (what the drain loop does) ──────────
    let mut group = c.benchmark_group("output/ring_pop");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let buffer = FixedFrameBuffer::new(1 << 16).expect("the ring allocates");
        let mut scratch = vec![0.0f32; BLOCK * CHANNELS];
        group.bench_function("512_frames", |b| {
            b.iter(|| {
                buffer.reset();
                buffer.push_frames_interleaved(black_box(&interleaved), CHANNELS);
                black_box(buffer.pop_frames_interleaved(black_box(&mut scratch), CHANNELS));
                black_box(&scratch);
            });
        });
    }
    group.finish();

    // ── The null backend end to end ────────────────────────────────────────
    {
        let buffer = Arc::new(FixedFrameBuffer::new(1 << 16).expect("the ring allocates"));
        let mut sink = NullOutput::new(buffer.clone(), RATE, CHANNELS as u16, RATE as usize)
            .expect("the null backend constructs");
        sink.start().expect("the drain thread starts");
        // Let the drain loop reach its first block boundary before measuring.
        std::thread::sleep(Duration::from_millis(30));

        let mut group = c.benchmark_group("output/null_end_to_end");
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_function("push_to_consumed", |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let before = sink.frames_played();
                    buffer.push_frames_interleaved(black_box(&interleaved), CHANNELS);
                    let start = Instant::now();
                    // Wait for the drain thread to report the frames it took.
                    while sink.frames_played() == before {
                        std::thread::sleep(Duration::from_micros(100));
                        if start.elapsed() > Duration::from_secs(2) {
                            break;
                        }
                    }
                    total += start.elapsed();
                }
                total
            });
        });
        group.finish();

        // ── Draining the capture tail ──────────────────────────────────────
        let mut group = c.benchmark_group("output/null_capture_tail");
        group.throughput(Throughput::Elements(1));
        group.bench_function("take_all_captured", |b| {
            b.iter(|| black_box(sink.take_all_captured()));
        });
        group.finish();

        // Report the sink's own accounting, so the frame counts are visible.
        eprintln!(
            "[output] null backend consumed {} frames, {} underrun(s) reported",
            sink.frames_played(),
            sink.take_underruns()
        );
        sink.stop();
    }

    // ── The null backend's constructor (the control-path cost of opening) ──
    let mut group = c.benchmark_group("output/null_backend_construction");
    group.throughput(Throughput::Elements(1));
    group.bench_function("new", |b| {
        b.iter(|| {
            let buffer = Arc::new(FixedFrameBuffer::new(1 << 16).expect("the ring allocates"));
            black_box(
                NullOutput::new(buffer, RATE, CHANNELS as u16, RATE as usize)
                    .expect("the null backend constructs"),
            )
        });
    });
    group.finish();
}

criterion_group!(benches, bench_output);
criterion_main!(benches);

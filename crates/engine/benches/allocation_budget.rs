//! # Realtime allocation budget
//!
//! The engine claims **"0 heap allocations in steady state"** on the audio hot
//! path (README § Real-Time Safety). This bench turns that claim into a
//! **measured budget** rather than a boolean: a counting global allocator
//! records every allocation the benchmark thread makes, and the benches below
//! report **allocations per processed block**.
//!
//! How the measurement works:
//!
//! - A [`CountingAlloc`] global allocator wraps the system allocator and
//!   increments an atomic counter on every `alloc` / `alloc_zeroed` /
//!   `realloc`. The counter is only *armed* around the measured region, so
//!   allocations Criterion itself makes (and the ones the setup code makes)
//!   are not counted.
//! - Each hot path runs through [`Bencher::iter_custom`], whose returned
//!   `Duration` is **the number of allocations observed**, expressed in
//!   nanoseconds. Criterion divides the returned value by the iteration count
//!   and prints it on its time axis, so:
//!
//!   > **the reported "time" of these benches is the allocation count per
//!   > processed block — 1.00 ns means one heap allocation per block.**
//!
//!   Reading the number: a value printed in **ps** (picoseconds) is *below one
//!   allocation per block* — i.e. zero allocations, the claim holding. A value
//!   printed in **ns** (nanoseconds) is one or more allocations per block —
//!   the claim being violated, with the number being the size of the
//!   violation. (A one-nanosecond floor is applied to the *batch total* only,
//!   because Criterion refuses to report a measurement of exactly zero; after
//!   the per-iteration division it lands in the picosecond range, which is
//!   where "zero allocations" is printed.)
//!
//! How to read the numbers:
//!
//! - `allocation_budget/allocations_per_block/*` — allocations per block.
//! - `allocation_budget/wall_time/*` — the *same* paths measured in ordinary
//!   wall-clock time, so the allocation numbers have a cost context.
//! - A summary table of the measured budgets is printed at the end of the
//!   run, so the numbers are visible even if Criterion's output is not read.
//! - A **self-test** runs first: the counter is armed, a `Vec` is allocated
//!   on purpose, and the counter is checked to have seen it. A budget that
//!   cannot detect a violation is not a budget.
//!
//! What is actually being measured (all of it is the engine's real audio
//! path):
//!
//! - `dsp_pipeline` — [`engine::dsp::pipeline::DspPipeline::process_block`],
//!   the reference chain (pre-mix → EQ → compressor → convolution → … → volume
//!   → seek fade).
//! - `graph2_arena` — [`engine::dsp::graph2::prod::Graph2Engine::process_block`],
//!   the **production** arena graph the engine actually runs, on the plan
//!   lowered from the canonical Graph 2.0 topology.
//! - `parametric_eq` — [`engine::dsp::equalizer::ParametricEq::process_block`].
//! - `convolution` — [`engine::dsp::convolution::ConvolutionEngine::process_block`].
//! - `master_ring_push` — [`engine::buffer::FixedFrameBuffer::push_frames_interleaved`],
//!   the producer side of the SPSC ring the output matrix drains.
//! - `resampler_feed` — [`engine::dsp::resampler::AudioResampler::feed`], the
//!   decode-loop's sample-at-a-time resampler path.
//!
//! The warm-up matters: a first call legitimately allocates (filter state,
//! FFT plans, scratch buffers), and the engine's contract is about the
//! **steady state**. Every path is therefore run for a warm-up phase before
//! the counter is armed.
//!
//! The counting technique is the one the engine's own
//! `tests/realtime_alloc_budget.rs` uses to *hold* the contract as a test;
//! this bench turns the same measurement into a number. The counter is armed
//! **per thread** (a `const`-initialised thread-local flag), so an allocation
//! made by any other thread in the process cannot be mistaken for the engine's
//! behaviour.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use config::EngineConfig;
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use engine::buffer::FixedFrameBuffer;
use engine::dsp::convolution::ConvolutionEngine;
use engine::dsp::equalizer::{EqBandParams, EqFilterType, ParametricEq};
use engine::dsp::graph2::prod::Graph2Engine;
use engine::dsp::pipeline::DspPipeline;
use engine::dsp::resampler::AudioResampler;

mod common;

/// Total allocations observed while the counter is armed.
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Total bytes requested while the counter is armed.
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Whether *this thread's* allocations are counted.
    ///
    /// A `const`-initialised thread local: reading it must not allocate, or
    /// the allocator would count its own bookkeeping.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// A global allocator that counts every allocation the armed thread makes.
struct CountingAlloc;

impl CountingAlloc {
    #[inline]
    fn count(layout: &Layout) {
        if ARMED.try_with(|armed| armed.get()).unwrap_or(false) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
    }
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count(&layout);
        // SAFETY: the layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count(&layout);
        // SAFETY: the layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::count(&layout);
        // SAFETY: the pointer and layout are forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout are forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Bytes requested while the counter was last armed.
fn last_alloc_bytes() -> usize {
    ALLOC_BYTES.load(Ordering::Relaxed)
}

/// Prove the counting instrument works before trusting its verdict.
///
/// A budget that cannot detect a violation is not a budget: this arms the
/// counter, allocates on purpose, and checks that the counter saw it. If this
/// ever fails, every "0 allocations" reading below it is meaningless.
fn counter_self_test() {
    let (bytes, count) = count_allocations(|| {
        let v: Vec<u8> = vec![0u8; 4096];
        black_box(&v);
        v.len()
    });
    assert!(
        count >= 1,
        "the counting allocator did not observe a deliberate allocation"
    );
    eprintln!(
        "[allocation_budget] counter self-test: {count} allocation(s) / {bytes} bytes \
         observed for one deliberate Vec"
    );
}

/// Run `f` with the allocation counter armed and return the number of
/// allocations it performed.
fn count_allocations<R>(f: impl FnOnce() -> R) -> (R, u64) {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    ARMED.with(|armed| armed.set(true));
    let out = f();
    ARMED.with(|armed| armed.set(false));
    (out, ALLOC_COUNT.load(Ordering::Relaxed) as u64)
}

/// Turn a batch allocation total into the `Duration` Criterion reports.
///
/// One nanosecond == one allocation. The `max(1)` is the floor Criterion
/// needs to be able to print a measurement at all; after Criterion divides by
/// the iteration count it is far below the display resolution.
fn as_measurement(total: u64) -> Duration {
    Duration::from_nanos(total.max(1))
}

/// Block size used for every measurement (512 stereo frames at 48 kHz).
const BLOCK: usize = 512;

fn bench_allocation_budget(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;
    let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    // Prove the instrument before trusting it.
    counter_self_test();

    // ── The allocation-count measurement ────────────────────────────────────
    // NOTE: no `Throughput` here — a throughput derived from an allocation
    // count would be a meaningless number, and the unit of this measurement is
    // "allocations per block", which Criterion's time axis carries.
    let mut group = c.benchmark_group("allocation_budget/allocations_per_block");

    {
        let mut pipeline = DspPipeline::from_config(&EngineConfig::default(), sample_rate);
        // Warm-up: the first block legitimately allocates filter state.
        let (mut wl, mut wr) = (pl.clone(), pr.clone());
        for _ in 0..8 {
            pipeline.process_block(&mut wl, &mut wr);
        }
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("dsp_pipeline", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    let ((), allocs) = count_allocations(|| {
                        pipeline.process_block(black_box(&mut l), black_box(&mut r));
                    });
                    total += allocs;
                    black_box(&l);
                }
                as_measurement(total)
            });
        });
    }

    {
        let mut engine = Graph2Engine::from_config(&EngineConfig::default(), sample_rate);
        let (mut wl, mut wr) = (pl.clone(), pr.clone());
        for _ in 0..8 {
            engine.process_block(&mut wl, &mut wr);
        }
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("graph2_arena", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    let ((), allocs) = count_allocations(|| {
                        engine.process_block(black_box(&mut l), black_box(&mut r));
                    });
                    total += allocs;
                    black_box(&l);
                }
                as_measurement(total)
            });
        });
    }

    {
        let mut eq = ParametricEq::new(10, sample_rate);
        for i in 0..10 {
            eq.set_band(
                i,
                EqBandParams {
                    frequency: 31.25 * 2.0f32.powi(i as i32),
                    gain_db: 2.0,
                    q: 1.4,
                    filter_type: if i == 0 {
                        EqFilterType::LowShelf
                    } else {
                        EqFilterType::Peaking
                    },
                    enabled: true,
                },
            );
        }
        eq.set_enabled(true);
        let (mut wl, mut wr) = (pl.clone(), pr.clone());
        for _ in 0..8 {
            eq.process_block(&mut wl, &mut wr);
        }
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("parametric_eq", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    let ((), allocs) = count_allocations(|| {
                        eq.process_block(black_box(&mut l), black_box(&mut r));
                    });
                    total += allocs;
                    black_box(&l);
                }
                as_measurement(total)
            });
        });
    }

    {
        let mut engine = ConvolutionEngine::new(sample_rate, 2048);
        let ir: Vec<(f32, f32)> = common::decaying_noise(2048, 0xC0FFEE, 0.002)
            .into_iter()
            .map(|s| (s, s))
            .collect();
        let _ = engine.load_ir_from_samples(&ir);
        engine.set_enabled(true);
        let (mut wl, mut wr) = (pl.clone(), pr.clone());
        for _ in 0..8 {
            engine.process_block(&mut wl, &mut wr);
        }
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("convolution", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    l.copy_from_slice(&pl);
                    r.copy_from_slice(&pr);
                    let ((), allocs) = count_allocations(|| {
                        engine.process_block(black_box(&mut l), black_box(&mut r));
                    });
                    total += allocs;
                    black_box(&l);
                }
                as_measurement(total)
            });
        });
    }

    {
        let buffer = FixedFrameBuffer::new(1 << 16).expect("the ring allocates");
        let interleaved = common::interleave(&pl, &pr);
        group.bench_function("master_ring_push", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    let ((), allocs) = count_allocations(|| {
                        buffer.push_frames_interleaved(black_box(&interleaved), 2);
                    });
                    total += allocs;
                }
                as_measurement(total)
            });
        });
    }

    {
        let mut resampler =
            AudioResampler::<f32>::new(config::ResamplerQuality::Balanced, 44_100.0, 48_000.0)
                .expect("44.1 -> 48 kHz is supported");
        for i in 0..1024 {
            resampler.feed(pl[i % BLOCK], pr[i % BLOCK]);
        }
        while resampler.read().is_some() {}
        group.bench_function("resampler_feed", |b| {
            b.iter_custom(|iters| {
                let mut total = 0u64;
                for _ in 0..iters {
                    let ((), allocs) = count_allocations(|| {
                        for i in 0..BLOCK {
                            resampler.feed(black_box(pl[i]), black_box(pr[i]));
                        }
                    });
                    total += allocs;
                }
                as_measurement(total)
            });
        });
    }

    group.finish();

    // ── The same paths, measured in wall-clock time ─────────────────────────
    // The allocation rows above report a count on Criterion's time axis. These
    // rows report the ordinary cost of the same work, so the two can be read
    // together.
    let mut group = c.benchmark_group("allocation_budget/wall_time");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let mut pipeline = DspPipeline::from_config(&EngineConfig::default(), sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("dsp_pipeline", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                pipeline.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    {
        let mut engine = Graph2Engine::from_config(&EngineConfig::default(), sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("graph2_arena", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                engine.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    {
        let buffer = FixedFrameBuffer::new(1 << 16).expect("the ring allocates");
        let interleaved = common::interleave(&pl, &pr);
        group.bench_function("master_ring_push", |b| {
            b.iter(|| {
                buffer.push_frames_interleaved(black_box(&interleaved), 2);
            });
        });
    }
    group.finish();

    // ── The measured budget, printed ────────────────────────────────────────
    // One measurement per hot path, in the same steady state the benches
    // above use, reported as a plain "allocations per block" table.
    eprintln!("[allocation_budget] steady-state heap allocations per {BLOCK}-frame block:");
    let mut pipeline = DspPipeline::from_config(&EngineConfig::default(), sample_rate);
    let (mut l, mut r) = (pl.clone(), pr.clone());
    for _ in 0..8 {
        pipeline.process_block(&mut l, &mut r);
    }
    let (_, allocs) = count_allocations(|| {
        pipeline.process_block(black_box(&mut l), black_box(&mut r));
    });
    eprintln!(
        "  dsp_pipeline     : {allocs} ({} bytes)",
        last_alloc_bytes()
    );

    let mut engine = Graph2Engine::from_config(&EngineConfig::default(), sample_rate);
    let (mut wl, mut wr) = (pl.clone(), pr.clone());
    for _ in 0..8 {
        engine.process_block(&mut wl, &mut wr);
    }
    let (mut l, mut r) = (pl.clone(), pr.clone());
    let (_, allocs) = count_allocations(|| {
        engine.process_block(black_box(&mut l), black_box(&mut r));
    });
    eprintln!("  graph2_arena     : {allocs}");

    let mut eq = ParametricEq::new(10, sample_rate);
    for i in 0..10 {
        eq.set_band(
            i,
            EqBandParams {
                frequency: 31.25 * 2.0f32.powi(i as i32),
                gain_db: 2.0,
                q: 1.4,
                filter_type: EqFilterType::Peaking,
                enabled: true,
            },
        );
    }
    eq.set_enabled(true);
    let (mut wl, mut wr) = (pl.clone(), pr.clone());
    for _ in 0..8 {
        eq.process_block(&mut wl, &mut wr);
    }
    let (mut l, mut r) = (pl.clone(), pr.clone());
    let (_, allocs) = count_allocations(|| {
        eq.process_block(black_box(&mut l), black_box(&mut r));
    });
    eprintln!("  parametric_eq    : {allocs}");

    let mut conv = ConvolutionEngine::new(sample_rate, 2048);
    let ir: Vec<(f32, f32)> = common::decaying_noise(2048, 0xC0FFEE, 0.002)
        .into_iter()
        .map(|s| (s, s))
        .collect();
    let _ = conv.load_ir_from_samples(&ir);
    conv.set_enabled(true);
    let (mut wl, mut wr) = (pl.clone(), pr.clone());
    for _ in 0..8 {
        conv.process_block(&mut wl, &mut wr);
    }
    let (mut l, mut r) = (pl.clone(), pr.clone());
    let (_, allocs) = count_allocations(|| {
        conv.process_block(black_box(&mut l), black_box(&mut r));
    });
    eprintln!("  convolution      : {allocs}");

    let ring = FixedFrameBuffer::new(1 << 16).expect("the ring allocates");
    let interleaved = common::interleave(&pl, &pr);
    let (_, allocs) = count_allocations(|| {
        ring.push_frames_interleaved(black_box(&interleaved), 2);
    });
    eprintln!("  master_ring_push : {allocs}");

    let mut resampler =
        AudioResampler::<f32>::new(config::ResamplerQuality::Balanced, 44_100.0, 48_000.0)
            .expect("44.1 -> 48 kHz is supported");
    for i in 0..1024 {
        resampler.feed(pl[i % BLOCK], pr[i % BLOCK]);
    }
    while resampler.read().is_some() {}
    let (_, allocs) = count_allocations(|| {
        for i in 0..BLOCK {
            resampler.feed(black_box(pl[i]), black_box(pr[i]));
        }
    });
    eprintln!("  resampler_feed   : {allocs}");
}

criterion_group!(benches, bench_allocation_budget);
criterion_main!(benches);

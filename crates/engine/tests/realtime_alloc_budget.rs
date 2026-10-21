//! The zero-allocation hot-path contract, as a black-box test.
//!
//! The engine's rule is that the audio thread may not allocate: no heap, no
//! locks, no blocking I/O. This suite checks the heap half of that rule from
//! the outside, by installing a counting global allocator in *this* test
//! binary and measuring the allocations the hot path makes.
//!
//! The allocator counts only the allocations made by the thread that armed
//! it, so the test harness's own bookkeeping on other threads cannot be
//! mistaken for the engine's behaviour.
//!
//! A global allocator can only be defined once per binary, which is why this
//! file owns one and no other suite in `tests/` does.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use engine::buffer::{declick_ramp_frames, UnderrunState};
use engine::dsp::biquad::{BiquadCoeffs, BiquadState};
use engine::dsp::crossfeed::Crossfeed;
use engine::dsp::equalizer::{EqBandParams, EqFilterType, ParametricEq};
use engine::dsp::pipeline::DspPipeline;

/// Number of allocations made while the counter was armed.
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

/// Number of bytes requested while the counter was armed.
static BYTES: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Whether *this* thread's allocations are counted.
    ///
    /// A `const`-initialised thread local: reading it must not allocate, or
    /// the allocator would count its own bookkeeping.
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// A global allocator that counts what the armed thread allocates.
struct CountingAllocator;

impl CountingAllocator {
    #[inline]
    fn count(layout: &Layout) {
        if ARMED.try_with(|armed| armed.get()).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::count(&layout);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::count(&layout);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        Self::count(&layout);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Run `body` with the allocation counter armed and report how many
/// allocations it made.
fn allocations_during(body: impl FnOnce()) -> usize {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    ARMED.with(|armed| armed.set(true));
    body();
    ARMED.with(|armed| armed.set(false));
    let count = ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(
        BYTES.load(Ordering::Relaxed),
        0,
        "the byte counter must agree with the allocation counter"
    );
    count
}

/// A bypassed pipeline: every user stage off, which is the bit-perfect path
/// and the one that must be cheapest of all.
fn bypassed_pipeline() -> DspPipeline {
    DspPipeline::builder(48_000.0)
        .with_eq(false)
        .with_limiter_disabled()
        .with_crossfeed(false, config::CrossfeedProfile::Bauer)
        .with_stereo_width(1.0)
        .build()
}

#[test]
fn the_bypassed_pipeline_allocates_nothing_while_processing_blocks() {
    let mut pipeline = bypassed_pipeline();
    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];

    // Warm up: the first passes may lazily allocate (filter scratch, the
    // precision-mode promotion buffers, …). The contract is about the steady
    // state, which is what an audio callback ever sees.
    for _ in 0..8 {
        pipeline.process_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            pipeline.process_block(&mut left, &mut right);
        }
    });
    assert_eq!(
        allocations, 0,
        "the bypassed pipeline must not allocate on the hot path"
    );
}

#[test]
fn a_pipeline_with_an_eq_and_a_crossfeed_allocates_nothing_either() {
    // The two stages a user is most likely to have on. They are the ones that
    // would allocate if any stage were going to.
    let mut pipeline = DspPipeline::builder(48_000.0)
        .with_eq(true)
        .with_limiter_disabled()
        .with_crossfeed(true, config::CrossfeedProfile::Bauer)
        .with_stereo_width(1.0)
        .build();
    pipeline.eq.set_band(
        0,
        EqBandParams {
            enabled: true,
            filter_type: EqFilterType::Peaking,
            frequency: 1_000.0,
            gain_db: 3.0,
            q: 1.0,
        },
    );

    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];

    for _ in 0..8 {
        pipeline.process_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            pipeline.process_block(&mut left, &mut right);
        }
    });
    assert_eq!(
        allocations, 0,
        "the EQ and crossfeed stages must not allocate on the hot path"
    );
}

#[test]
fn the_crossfeed_stage_allocates_nothing_while_processing_blocks() {
    let mut crossfeed = Crossfeed::new(48_000.0);
    crossfeed.set_enabled(true);
    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];

    for _ in 0..4 {
        crossfeed.process_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            crossfeed.process_block(&mut left, &mut right);
        }
    });
    assert_eq!(allocations, 0, "the crossfeed must not allocate");
}

#[test]
fn the_parametric_eq_allocates_nothing_while_processing_blocks() {
    let mut eq = ParametricEq::new(10, 48_000.0);
    eq.set_enabled(true);
    eq.set_band(
        0,
        EqBandParams {
            enabled: true,
            filter_type: EqFilterType::Peaking,
            frequency: 1_000.0,
            gain_db: 3.0,
            q: 1.0,
        },
    );
    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];

    for _ in 0..4 {
        eq.process_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            eq.process_block(&mut left, &mut right);
        }
    });
    assert_eq!(allocations, 0, "the parametric EQ must not allocate");
}

#[test]
fn the_biquad_stage_allocates_nothing_while_processing_samples() {
    let coeffs = BiquadCoeffs::<f32>::peaking(48_000.0, 1_000.0, 3.0, 1.0);
    let mut state = BiquadState::<f32>::default();

    for _ in 0..8 {
        let _ = state.process(0.25, &coeffs);
    }

    let allocations = allocations_during(|| {
        for _ in 0..4096 {
            let _ = state.process(0.25, &coeffs);
        }
    });
    assert_eq!(allocations, 0, "the biquad must not allocate");
}

#[test]
fn the_underrun_concealment_allocates_nothing() {
    // The concealment path runs on the audio thread when the device starves,
    // which is the worst possible moment to allocate.
    let mut state = UnderrunState::new(48_000);
    let mut block = vec![1.0f32; 512];

    for _ in 0..4 {
        state.declick(&mut block, 2, 256);
    }

    let allocations = allocations_during(|| {
        for _ in 0..256 {
            state.declick(&mut block, 2, 256);
        }
    });
    assert_eq!(allocations, 0, "the underrun concealment must not allocate");
    assert_eq!(declick_ramp_frames(48_000), 48);
}

#[test]
fn the_counter_itself_does_not_allocate() {
    // If measuring an allocation allocated one, the measurement would be
    // useless. This asserts the harness is sound: arming and disarming the
    // counter, and reading it, must not allocate.
    let allocations = allocations_during(|| {
        let mut sink = 0u64;
        for i in 0..1000u64 {
            sink = sink.wrapping_add(i);
        }
        assert_eq!(sink, 499_500);
    });
    assert_eq!(
        allocations, 0,
        "the allocation counter must not allocate while it is counting"
    );
}

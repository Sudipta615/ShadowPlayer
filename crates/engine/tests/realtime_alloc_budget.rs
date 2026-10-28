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
fn the_svf_stage_allocates_nothing_while_processing_samples() {
    let mut svf = engine::dsp::equalizer::SvfFilter::<f32>::new(48_000.0);
    svf.set_params(1_000.0, 1.0);
    svf.set_gain_db(3.0);

    for _ in 0..8 {
        let _ = svf.process(0.25, engine::dsp::equalizer::SvfMode::Peaking);
    }

    let allocations = allocations_during(|| {
        for _ in 0..4096 {
            let _ = svf.process(0.25, engine::dsp::equalizer::SvfMode::Peaking);
        }
    });
    assert_eq!(allocations, 0, "the SVF filter must not allocate");
}

#[test]
fn the_bass_enhancer_stage_allocates_nothing_while_processing_blocks() {
    let mut enhancer = engine::dsp::bass_enhancer::BassEnhancer::new(48_000.0);
    enhancer.set_enabled(true);
    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];

    for _ in 0..4 {
        enhancer.process_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            enhancer.process_block(&mut left, &mut right);
        }
    });
    assert_eq!(
        allocations, 0,
        "the psychoacoustic bass enhancer must not allocate"
    );
}

#[test]
fn the_critically_damped_smoother_allocates_nothing() {
    let mut smoother = engine::dsp::smoother::CriticallyDampedSmoother::new(0.0, 20.0, 48_000.0);
    smoother.set_target(1.0);

    for _ in 0..8 {
        let _ = smoother.next_value();
    }

    let allocations = allocations_during(|| {
        for _ in 0..4096 {
            let _ = smoother.next_value();
        }
    });
    assert_eq!(
        allocations, 0,
        "the critically damped smoother must not allocate"
    );
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

/// The output ultrasonic stage is inline-state only: enabling it must not add
/// a single allocation to the output-domain hot path.
#[test]
fn the_ultrasonic_stage_allocates_nothing_while_processing_blocks() {
    let mut config = config::EngineConfig::default();
    config.limiter.enabled = false;
    config.ultrasonic_filter = config::UltrasonicFilter::Cut20k;
    let mut pipeline = DspPipeline::from_config(&config, 192_000.0);

    let mut left = vec![0.25f32; 512];
    let mut right = vec![-0.25f32; 512];
    for _ in 0..8 {
        pipeline.process_final_limiter_block(&mut left, &mut right);
    }

    let allocations = allocations_during(|| {
        for _ in 0..128 {
            pipeline.process_final_limiter_block(&mut left, &mut right);
        }
    });
    assert_eq!(
        allocations, 0,
        "the ultrasonic stage must not allocate on the hot path"
    );

    // The multichannel endpoint is the same contract.
    let mut interleaved = vec![0.1f32; 512 * 6];
    for _ in 0..8 {
        pipeline.process_final_limiter_multichannel(&mut interleaved, 6);
    }
    let allocations = allocations_during(|| {
        for _ in 0..128 {
            pipeline.process_final_limiter_multichannel(&mut interleaved, 6);
        }
    });
    assert_eq!(
        allocations, 0,
        "the multichannel ultrasonic path must not allocate"
    );
}

/// PCM→SDM runs on the audio thread: interpolation, modulation, and packing
/// must all be allocation-free once the stage is built.
#[test]
fn the_pcm_to_sdm_stage_allocates_nothing_while_processing_blocks() {
    use engine::decode::dsd::DsdWireFormat;
    use engine::dsp::sdm::{SdmOutput, SdmRatePlan};

    for (multiple, stage_factor) in [(64u32, 8u32), (256, 32)] {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: 352_800,
            dsd_rate: 44_100 * multiple,
            multiple,
            stage_factor,
            family: 44_100,
        };
        for order in [5usize, 7] {
            let mut stage = SdmOutput::new(plan, 2, DsdWireFormat::U8, order).expect("sdm stage");
            let frames = 512usize;
            let input = vec![0.1f32; frames * 2];
            // The caller owns the packed buffer; size it for the block.
            let mut out = Vec::with_capacity(frames * 2 * stage_factor as usize / 8 + 64);
            for _ in 0..4 {
                stage.process_block(&input, &mut out);
            }

            let allocations = allocations_during(|| {
                for _ in 0..64 {
                    stage.process_block(&input, &mut out);
                }
            });
            assert_eq!(
                allocations, 0,
                "SDM order {order} at DSD{multiple} must not allocate"
            );
        }
    }
}

/// The polyphase engine (P1.2 filter shapes) must meet the same realtime
/// budget as every other DSP stage: its history is sized at construction and
/// trimmed in place, so no chunk may touch the heap.
#[cfg(feature = "resample")]
#[test]
fn the_polyphase_resampler_allocates_nothing_while_processing_chunks() {
    use engine::dsp::resampler::AudioResampler;

    let mut resampler = AudioResampler::<f32>::new_with_filter(
        config::ResamplerQuality::Fast,
        config::ResamplerFilter::Kaiser,
        44_100.0,
        48_000.0,
    )
    .expect("polyphase resampler");

    let left = vec![0.125f32; 1024];
    let right = vec![-0.125f32; 1024];

    // Warm up: the first chunks exercise the branch table and the output
    // ring, not the steady state an audio callback sees.
    for _ in 0..8 {
        resampler.feed_block(&left, &right);
        while resampler.read().is_some() {}
    }

    let allocations = allocations_during(|| {
        for _ in 0..64 {
            resampler.feed_block(&left, &right);
            while resampler.read().is_some() {}
        }
    });
    assert_eq!(
        allocations, 0,
        "the polyphase resampler allocated {allocations} times in the steady state"
    );
}

#[test]
fn the_high_order_dither_path_allocates_nothing_while_converting_blocks() {
    use engine::dsp::dither::DitherType;
    use engine::output::format_converter::{AudioFormatConverter, TargetFormat};

    for dither_type in [DitherType::Ns5, DitherType::Ns9, DitherType::Lns15] {
        for rate in [44_100u32, 96_000] {
            // Construction may allocate; the per-sample conversion may not.
            let mut converter =
                AudioFormatConverter::new_at_rate(TargetFormat::I24Le, dither_type, rate);
            converter.set_dither_seed(0x1234_5678_9ABC_DEF0);
            let mut phase = 0.0f32;
            let allocations = allocations_during(|| {
                for _ in 0..4_096 {
                    phase += 0.01;
                    let sample = (phase * 997.0).sin() * 0.25;
                    let _ = converter.convert_stereo_to_i24le(sample, -sample);
                }
            });
            assert_eq!(
                allocations,
                0,
                "{} at {rate} Hz allocated on the conversion path",
                dither_type.as_str()
            );
        }
    }
}

/// Write a minimal 16-bit mono WAV for the matrix-FIR allocation test.
fn write_delta_wav(path: &std::path::Path) {
    let samples = [1.0f32, 0.0, 0.0, 0.0];
    let mut data = Vec::new();
    for sample in samples {
        data.extend_from_slice(&((sample * 32_767.0) as i16).to_le_bytes());
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn the_matrix_fir_stage_allocates_nothing_while_processing_blocks() {
    use engine::dsp::channel_trim::ChannelTrimmer;

    let path = std::env::temp_dir().join(format!(
        "shadowplayer_matrix_fir_alloc_{}.wav",
        std::process::id()
    ));
    write_delta_wav(&path);
    let mut trimmer = ChannelTrimmer::new(48_000.0);
    trimmer.set_config(&config::ChannelTrimConfig::default(), 48_000.0);
    let path = path.display().to_string();
    trimmer.set_routing(&config::ChannelRoutingConfig {
        enabled: true,
        matrix: Vec::new(),
        fir_routes: (0..4)
            .map(|route| config::MatrixFirRouteConfig {
                source: route % 2,
                destination: 2 + route % 2,
                ir_path: Some(path.clone()),
                gain_db: -3.0,
                delay_ms: route as f32 * 0.5,
            })
            .collect(),
    });
    assert!(trimmer.is_active(4));

    let mut planes: Vec<Vec<f32>> = (0..4)
        .map(|channel| {
            (0..1_024)
                .map(|frame| {
                    if channel < 2 {
                        (frame as f32 * 0.001).sin()
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .collect();

    let allocations = allocations_during(|| {
        for _ in 0..4 {
            trimmer.process_planes(&mut planes, 4, 1_024);
        }
    });
    assert_eq!(allocations, 0, "matrix FIR allocated on the audio path");
    let _ = std::fs::remove_file(path);
}

#[test]
fn the_apodizing_detector_allocates_nothing_while_analyzing_blocks() {
    use engine::dsp::apodizing::ApodizingDetector;

    let mut detector = ApodizingDetector::new(48_000.0);
    let block: Vec<f32> = (0..512)
        .flat_map(|frame| {
            let x = (frame as f32 * 0.05).sin() * 0.25;
            [x, -x]
        })
        .collect();
    // Warm the biquad/history state outside the measured region.
    detector.analyze_interleaved(&block, 2, 48_000.0);

    let allocations = allocations_during(|| {
        for _ in 0..256 {
            detector.analyze_interleaved(&block, 2, 48_000.0);
        }
    });
    assert_eq!(
        allocations, 0,
        "the apodizing detector allocated while analysing the decode path"
    );
}

#[test]
fn the_dac_correction_bank_allocates_nothing_while_processing_blocks() {
    use engine::dsp::graph2::prod::{CorrectionNode, DspNode};

    let path =
        std::env::temp_dir().join(format!("shadowplayer_dac_alloc_{}.wav", std::process::id()));
    write_delta_wav(&path);
    let mut node = CorrectionNode::new(48_000.0);
    node.apply_dac_config(
        &config::DacCorrectionConfig {
            enabled: true,
            profiles: vec![config::DacCorrectionProfileConfig {
                dac: "test-dac".to_string(),
                sample_rate: 48_000,
                ir_paths: vec![path.display().to_string()],
                latency_ms: 0.0,
                depth: 0.8,
            }],
        },
        Some("test-dac"),
        48_000.0,
    );
    assert!(node.dac_info().active);
    let mut plane = vec![0.0f32; 512];
    for block in 0..4 {
        plane[0] = if block == 0 { 0.5 } else { 0.0 };
        let mut planes = [&mut plane[..]];
        node.process_block_f32(&mut planes);
    }

    let allocations = allocations_during(|| {
        for _ in 0..64 {
            let mut planes = [&mut plane[..]];
            node.process_block_f32(&mut planes);
        }
    });
    assert_eq!(allocations, 0, "DAC correction allocated on the audio path");
    let _ = std::fs::remove_file(path);
}

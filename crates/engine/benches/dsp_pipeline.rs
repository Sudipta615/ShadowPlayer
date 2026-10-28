//! # DSP pipeline: full-chain cost and per-stage attribution
//!
//! Measures the engine's **reference DSP chain**
//! ([`engine::dsp::pipeline::DspPipeline`], the frozen bit-exactness oracle)
//! so that the cost of the whole chain and of each of its stages is
//! attributable.
//!
//! What the numbers mean:
//!
//! - Every measurement processes one **512-frame stereo block** at 48 kHz
//!   (10.67 ms of audio). `Throughput::Elements` is set to 512, so Criterion
//!   also reports frames/second. A stage whose reported time is close to
//!   10.67 µs is free; one that is close to 10.67 ms is not keeping up with
//!   real time.
//!
//! What is actually being measured:
//!
//! - `full_chain` — `process_block` **plus** `process_final_limiter_block`,
//!   i.e. the whole audio-path chain the engine runs: output preamp →
//!   loudness normalisation → 10-band EQ → 3-band multiband compressor →
//!   partitioned-FFT convolution → balance → crossfeed → stereo enhancer →
//!   time-stretch → volume → seek-fade → output-domain lookahead limiter.
//!   Every optional stage is switched **on and given real work** (a 10-band
//!   EQ curve with gains, a loaded convolution IR, a crossfeed profile, a
//!   widened stereo image, EBU R128 normalisation). A disabled stage
//!   short-circuits, so benchmarking the default configuration would measure
//!   almost nothing.
//! - `chain_sections` — the three public block entry points of the same
//!   pipeline, so the cost of the pre-mix section (`process_outgoing_block`),
//!   the post-mix section (`process_post_mix_block`) and the output-domain
//!   limiter (`process_final_limiter_block`) is visible separately.
//!   The engine's **mix bus is not one of these**: it is a separate node
//!   (`TrackMixer`) fed by the graph's mix-bus node, not by `process_block`,
//!   so it is deliberately not claimed to be covered here.
//! - `stage_attribution` — the same `process_block` measured with one extra
//!   stage enabled at a time on top of an all-off baseline. The deltas
//!   between the rows are the attributable cost of each stage.
//! - `precision` — the same full chain in `PrecisionMode::Performance` (f32)
//!   and `PrecisionMode::Quality` (f64). Quality mode promotes the block to
//!   f64 once, runs the whole chain in f64, and demotes once.
//!
//! Warm-up: every pipeline is fed one second of audio before the measured
//! region, so the EQ's coefficient ramps have settled and the compressor's
//! envelope is in steady state — otherwise the numbers would describe the
//! *transient* of switching the stage on, not its running cost.
//!
//! Two things worth knowing when reading the numbers:
//!
//! - **Flush-to-zero / denormals-are-zero are enabled**, exactly as the
//!   engine's realtime thread has them
//!   (`dsp_utils::enable_flush_zero_denormals_on_current_thread`). Without
//!   them a stage whose internal state decays towards zero pays denormal
//!   penalties the engine never pays in production.
//! - **The output-domain limiter dominates the chain.** Its default
//!   `TruePeakMode::Fir4x` detector is a 4× polyphase FIR (400 MACs per
//!   sample per channel), which is why `chain_sections/final_limiter` costs
//!   more than every other section combined. The section rows do not add up
//!   to the `full_chain` row exactly: the limiter's cost depends on the
//!   signal it is given, and in the full chain that signal has already been
//!   through the EQ, the compressor and the convolution.
//!
//! `black_box` is `std::hint::black_box`: Criterion 0.8 deprecated its own
//! re-export in favour of the standard-library one.

use std::hint::black_box;

use config::EngineConfig;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::dsp::equalizer::{EqBandParams, EqFilterType};
use engine::dsp::loudness::LoudnessMode;
use engine::dsp::pipeline::{DspPipeline, PrecisionMode};

mod common;

/// Block size used for every measurement: 512 stereo frames = 10.67 ms at
/// 48 kHz.
const BLOCK: usize = 512;

/// A closure that switches one optional stage of the pipeline on.
type StageEnable = fn(&mut DspPipeline);

/// A pipeline with every optional stage **off** — the attribution baseline.
fn baseline_pipeline(sample_rate: f32) -> DspPipeline {
    let mut p = DspPipeline::from_config(&EngineConfig::default(), sample_rate);
    p.set_eq_enabled(false);
    p.set_compressor_enabled(false);
    p.convolution.set_enabled(false);
    p.set_crossfeed_enabled(false);
    p.set_stereo_enhancer_enabled(false);
    p.set_limiter_enabled(false);
    p.set_loudness_mode(LoudnessMode::Off);
    p
}

/// Give the pipeline a working 10-band EQ (enabled bands, non-zero gains).
fn enable_eq(p: &mut DspPipeline) {
    p.set_eq_enabled(true);
    for i in 0..10 {
        let t = i as f32 / 9.0;
        let filter_type = match i {
            0 => EqFilterType::LowShelf,
            9 => EqFilterType::HighShelf,
            _ => EqFilterType::Peaking,
        };
        p.eq.set_band(
            i,
            EqBandParams {
                frequency: 31.25 * 2.0f32.powi(i as i32),
                gain_db: 2.0 * (t * std::f32::consts::TAU).sin(),
                q: 1.4,
                filter_type,
                enabled: true,
            },
        );
    }
    p.eq.snap_gain_stages();
}

/// Load a synthetic 2048-sample room-ish IR into the pipeline's convolution
/// engine and enable it. The IR is identical on both channels, so the engine
/// treats it as a mono kernel (one partition set, not two).
fn enable_convolution(p: &mut DspPipeline) {
    let ir: Vec<(f32, f32)> = common::decaying_noise(2048, 0x5EED, 0.002)
        .into_iter()
        .map(|s| (s, s))
        .collect();
    let _ = p.convolution.load_ir_from_samples(&ir);
    p.convolution.set_enabled(true);
    p.convolution.set_wet_mix(1.0);
}

/// The full production chain: every optional stage on.
fn full_pipeline(sample_rate: f32) -> DspPipeline {
    let mut p = DspPipeline::from_config(&EngineConfig::default(), sample_rate);
    enable_eq(&mut p);
    p.set_compressor_enabled(true);
    enable_convolution(&mut p);
    p.set_crossfeed_enabled(true);
    p.set_stereo_enhancer_enabled(true);
    p.set_stereo_width(1.4);
    p.set_limiter_enabled(true);
    p.set_loudness_mode(LoudnessMode::EbuR128);
    p
}

/// Feed one second of audio so ramps settle and envelopes reach steady state.
fn warm_up(p: &mut DspPipeline, sample_rate: f32) {
    let (mut l, mut r) = common::stereo_sine(48_000, 997.0, 1009.0, sample_rate);
    p.process_block(&mut l, &mut r);
    p.process_final_limiter_block(&mut l, &mut r);
}

/// Write a minimal 16-bit mono PCM WAV for the matrix-FIR bench.
fn write_mono_wav(path: &std::path::Path, sample_rate: u32, samples: &[f32]) {
    let mut data = Vec::with_capacity(samples.len() * 2);
    for &sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0).round() as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut bytes = Vec::with_capacity(44 + data.len());
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    std::fs::write(path, bytes).unwrap();
}

fn bench_dsp_pipeline(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`,
    // which the null backend's drain thread also calls). Without it, a stage
    // whose internal state decays towards zero pays denormal penalties that
    // the engine never pays in production, so the bench enables it too.
    let ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();
    eprintln!("[dsp_pipeline] flush-to-zero / denormals-are-zero enabled: {ftz}");

    // ── The whole chain, and the same chain in the other precision tier ────
    let mut group = c.benchmark_group("dsp_pipeline/full_chain");
    group.throughput(Throughput::Elements(BLOCK as u64));
    for (name, precision) in [
        ("f32_performance", PrecisionMode::Performance),
        ("f64_quality", PrecisionMode::Quality),
    ] {
        let mut p = full_pipeline(sample_rate);
        p.set_precision_mode(precision);
        warm_up(&mut p, sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function(name, |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_block(black_box(&mut l), black_box(&mut r));
                p.process_final_limiter_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();

    // ── The pipeline's own sections ─────────────────────────────────────────
    let mut group = c.benchmark_group("dsp_pipeline/chain_sections");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let mut p = full_pipeline(sample_rate);
        warm_up(&mut p, sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("pre_mix_outgoing", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_outgoing_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    {
        let mut p = full_pipeline(sample_rate);
        warm_up(&mut p, sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("post_mix", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_post_mix_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    {
        let mut p = full_pipeline(sample_rate);
        warm_up(&mut p, sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("final_limiter", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_final_limiter_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();

    // ── Per-stage attribution ───────────────────────────────────────────────
    // Each row is the *same* `process_block` call with one extra stage turned
    // on, so the difference from `baseline` is that stage's cost.
    let stages: [(&str, StageEnable); 6] = [
        ("+eq", |p| enable_eq(p)),
        ("+compressor", |p| p.set_compressor_enabled(true)),
        ("+convolution", |p| enable_convolution(p)),
        ("+crossfeed", |p| p.set_crossfeed_enabled(true)),
        ("+stereo_enhancer", |p| {
            p.set_stereo_enhancer_enabled(true);
            p.set_stereo_width(1.4);
        }),
        ("+loudness", |p| p.set_loudness_mode(LoudnessMode::EbuR128)),
    ];

    let mut group = c.benchmark_group("dsp_pipeline/stage_attribution");
    group.throughput(Throughput::Elements(BLOCK as u64));
    {
        let mut p = baseline_pipeline(sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function("baseline_all_off", |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    for (name, enable) in stages {
        let mut p = baseline_pipeline(sample_rate);
        enable(&mut p);
        warm_up(&mut p, sample_rate);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();

    // ── Output ultrasonic filter ────────────────────────────────────────────
    // The limiter is disabled so the row isolates the ultrasonic stage's own
    // running cost (a 4th-order cascade at 192 kHz) against the passthrough.
    let mut group = c.benchmark_group("dsp_pipeline/ultrasonic");
    group.throughput(Throughput::Elements(BLOCK as u64));
    for (name, filter) in [
        ("off", config::UltrasonicFilter::Off),
        ("20khz", config::UltrasonicFilter::Cut20k),
    ] {
        let mut cfg = EngineConfig::default();
        cfg.limiter.enabled = false;
        cfg.ultrasonic_filter = filter;
        let mut p = DspPipeline::from_config(&cfg, 192_000.0);
        let (pl, pr) = common::stereo_sine(BLOCK, 997.0, 1009.0, 192_000.0);
        let (mut l, mut r) = (pl.clone(), pr.clone());
        group.bench_function(name, |b| {
            b.iter(|| {
                l.copy_from_slice(&pl);
                r.copy_from_slice(&pr);
                p.process_final_limiter_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();
}

/// PCM→SDM: one 512-frame stereo block at the 352.8 kHz working rate,
/// interpolated to the DSD rate and modulated by the real one-bit loop.
///
/// `Throughput::Elements` is the number of *input* frames, so the reported
/// time is directly comparable with the 1.45 µs of audio in a 512-frame
/// block at 352.8 kHz (not the 10.67 µs of the 48 kHz pipeline rows).
fn bench_sdm(c: &mut Criterion) {
    use engine::decode::dsd::DsdWireFormat;
    use engine::dsp::sdm::{SdmOutput, SdmRatePlan};

    const WORKING: u32 = 352_800;
    let mut group = c.benchmark_group("dsp_pipeline/sdm");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let input: Vec<f32> = (0..BLOCK * 2)
        .map(|i| 0.4 * (i as f32 * 0.01).sin())
        .collect();

    for (multiple, order) in [(64u32, 5usize), (64, 7), (256, 5), (256, 7)] {
        let plan = SdmRatePlan {
            pcm_rate: 44_100,
            working_rate: WORKING,
            dsd_rate: 44_100 * multiple,
            multiple,
            stage_factor: multiple / 8,
            family: 44_100,
        };
        let mut stage = SdmOutput::new(plan, 2, DsdWireFormat::U8, order).expect("sdm stage");
        let mut out = Vec::with_capacity(BLOCK * 2 * (multiple as usize / 8) + 64);
        for _ in 0..8 {
            stage.process_block(&input, &mut out);
        }
        let label = format!("order{order}_DSD{multiple}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let block = stage.process_block(black_box(&input), &mut out);
                black_box(block);
            });
        });
    }
    group.finish();
}

/// Isolate the modulator from the interpolation, so a slow SDM row can be
/// attributed. Processes the same number of DSD samples as a 512-frame
/// stereo block at the given multiple.
fn bench_sdm_modulator(c: &mut Criterion) {
    use engine::dsp::sdm::SdmModulator;

    let mut group = c.benchmark_group("dsp_pipeline/sdm_modulator");
    for (multiple, order) in [(64u32, 5usize), (64, 7), (256, 5), (256, 7)] {
        let samples = BLOCK * (multiple as usize / 8) * 2;
        let mut modulator = SdmModulator::new(order, multiple).expect("modulator");
        for _ in 0..8 {
            for _ in 0..samples {
                black_box(modulator.process_sample(0.1));
            }
        }
        let label = format!("order{order}_DSD{multiple}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                for _ in 0..samples {
                    black_box(modulator.process_sample(black_box(0.1)));
                }
            });
        });
    }
    group.finish();
}

/// Sparse matrix FIR: one and four routes processing an eight-channel block.
///
/// The routing stage owns one partitioned convolver per configured route, so
/// the 4-route row is the cost that matters for a real crossover/room profile;
/// the delta from 1→4 routes is the marginal per-route FFT cost. The IR is
/// 2048 samples (four 512-sample partitions), the same order as a short
/// crossover/driver-alignment kernel. A longer IR costs proportionally more
/// partitions. The 16-route 64k row is deliberately the worst supported
/// matrix configuration: it is the case a multicore requirement is measured
/// against, not a typical profile.
fn bench_matrix_fir(c: &mut Criterion) {
    use engine::dsp::channel_trim::ChannelTrimmer;

    let short_ir: Vec<f32> = common::decaying_noise(2048, 0x5EED, 0.002);
    let long_ir: Vec<f32> = common::decaying_noise(65_536, 0x5EED, 0.0001);
    let short_path = std::env::temp_dir().join(format!(
        "shadowplayer_matrix_fir_bench_short_{}.wav",
        std::process::id()
    ));
    let long_path = std::env::temp_dir().join(format!(
        "shadowplayer_matrix_fir_bench_long_{}.wav",
        std::process::id()
    ));
    write_mono_wav(&short_path, common::SAMPLE_RATE as u32, &short_ir);
    write_mono_wav(&long_path, common::SAMPLE_RATE as u32, &long_ir);

    let mut group = c.benchmark_group("matrix_fir");
    group.throughput(Throughput::Elements(BLOCK as u64));

    for (label, routes, path) in [
        ("routes_1", 1usize, &short_path),
        ("routes_4", 4, &short_path),
        ("routes_16", 16, &short_path),
        ("route_1_ir64k", 1, &long_path),
        ("routes_16_ir64k", 16, &long_path),
    ] {
        let route_configs: Vec<config::MatrixFirRouteConfig> = (0..routes)
            .map(|route| config::MatrixFirRouteConfig {
                source: route % 8,
                destination: (route + 4) % 8,
                ir_path: Some(path.display().to_string()),
                gain_db: 0.0,
                delay_ms: 0.0,
            })
            .collect();
        let mut trimmer = ChannelTrimmer::new(common::SAMPLE_RATE);
        trimmer.set_config(&config::ChannelTrimConfig::default(), common::SAMPLE_RATE);
        trimmer.set_routing(&config::ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: route_configs,
        });
        assert!(trimmer.is_active(8));
        let mut planes: Vec<Vec<f32>> = (0..8)
            .map(|channel| {
                (0..BLOCK)
                    .map(|frame| ((channel * BLOCK + frame) as f32 * 0.01).sin() * 0.1)
                    .collect()
            })
            .collect();
        // One second of audio: the convolver's FIFO and partition state are
        // in steady state before the measured region.
        let mut warm = vec![vec![0.0f32; BLOCK]; 8];
        for _ in 0..94 {
            trimmer.process_planes(&mut warm, 8, BLOCK);
        }
        group.bench_function(label, |b| {
            b.iter(|| {
                trimmer.process_planes(black_box(&mut planes), 8, BLOCK);
                black_box(&planes);
            });
        });
    }
    group.finish();
    let _ = std::fs::remove_file(short_path);
    let _ = std::fs::remove_file(long_path);
}

/// Source apodizing-need detector on a decoded stereo block. The detector is
/// a per-sample biquad plus a burst counter, so 512 frames is well below the
/// decode budget; the row makes that visible rather than assuming it.
fn bench_apodizing(c: &mut Criterion) {
    use engine::dsp::apodizing::ApodizingDetector;

    let block: Vec<f32> = (0..BLOCK)
        .flat_map(|frame| {
            let x = (frame as f32 * 0.05).sin() * 0.25;
            [x, -x]
        })
        .collect();
    let mut detector = ApodizingDetector::new(common::SAMPLE_RATE);
    detector.analyze_interleaved(&block, 2, common::SAMPLE_RATE);
    let mut group = c.benchmark_group("apodizing");
    group.throughput(Throughput::Elements(BLOCK as u64));
    group.bench_function("stereo_block", |b| {
        b.iter(|| {
            detector.analyze_interleaved(black_box(&block), 2, black_box(common::SAMPLE_RATE));
            black_box(detector.report());
        });
    });
    group.finish();
}

fn bench_dither(c: &mut Criterion) {
    use engine::dsp::dither::{Dither, DitherType};

    let mut group = c.benchmark_group("dither_high_order");
    for rate in [44_100u32, 96_000] {
        for dither_type in [DitherType::Ns5, DitherType::Ns9, DitherType::Lns15] {
            let mut dither = Dither::with_sample_rate(dither_type, 24, rate);
            let mut left = vec![0.1f32; 512];
            let mut right = vec![-0.1f32; 512];
            group.throughput(Throughput::Elements(512));
            group.bench_with_input(
                BenchmarkId::new(dither_type.as_str(), rate),
                &rate,
                |b, _| {
                    b.iter(|| {
                        dither.process_block(&mut left, &mut right);
                        black_box(left[0]);
                    });
                },
            );
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_dsp_pipeline,
    bench_sdm,
    bench_sdm_modulator,
    bench_matrix_fir,
    bench_apodizing,
    bench_dither
);
criterion_main!(benches);

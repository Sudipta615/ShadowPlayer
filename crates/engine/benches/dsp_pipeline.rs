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
}

criterion_group!(benches, bench_dsp_pipeline);
criterion_main!(benches);

//! # Spatial rendering: HRTF, ambisonics and VBAP
//!
//! Measures the engine's spatial layer — the three independent renderers the
//! graph's `SpatialNode` chooses between, plus the ambisonic encode/decode/
//! rotate primitives they are built from.
//!
//! What the numbers mean:
//!
//! - Every render measurement processes one **512-frame block** and reports
//!   `Throughput::Elements(512)`. A renderer whose reported time is close to
//!   10.67 µs is free at 48 kHz; one that is close to 10.67 ms is not keeping
//!   up with real time.
//! - The encode/decode/rotate measurements count **frames**, not blocks,
//!   because those primitives are per-frame APIs in the engine.
//!
//! What is actually being measured:
//!
//! - `binaural_render` — [`BinauralRenderer`] (the HRTF path): the analytic
//!   head model (Woodworth ITD + Duda–Martens head shadow + pinna elevation
//!   notch) applied to a scene of 4 positioned objects, rendered to two ears.
//!   **No measured HRTF dataset is loaded**, so this measures the *analytic*
//!   head model, not FIR convolution against a measured IR. The dataset path
//!   is a different code path (`use_dataset`) and is not claimed here.
//! - `vbap_render_5_1` — [`VbapRenderer`]: 3-triplet VBAP with the 2D
//!   reduction, rendered onto a 5.1 layout (the multichannel path the graph
//!   uses).
//! - `basic_panner_stereo` — [`BasicPanner`]: the equal-power panner, the
//!   cheapest of the three.
//! - `ambisonic_encode` — [`AmbisonicEncoder::encode`]: one plane-wave
//!   encoding into the ACN/SN3D bus, per order.
//! - `ambisonic_decode` — [`AmbisonicDecoder::process_bus`]: the bus →
//!   speakers decode matrix.
//! - `ambisonic_rotate` — [`rotate_bus_frame_n`]: the exact Wigner-d bus
//!   rotation. Only orders 1–3 have exact blocks; above that the engine
//!   deliberately does **not** rotate, and `rotation_supported` says so. The
//!   bench therefore measures orders 1, 2 and 3 only.
//! - `ambisonic_render` — [`AmbisonicRenderer`]: the whole ambisonic path in
//!   one call — bus rotation followed by decode onto the layout.
//!
//! The scene, the listener and the object positions are fixed and
//! deterministic; the audio is an in-memory sine per object.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::spatial::ambisonic::AmbisonicEncoder;
use engine::spatial::{
    channel_count, rotate_bus_frame_n, AmbisonicDecoder, AmbisonicRenderer, BasicPanner,
    BinauralRenderer, DecoderPolicy, SpatialRenderer, SpatialScene, SpeakerLayout, VbapRenderer,
    Vec3,
};

mod common;

/// Block size for every render measurement (512 frames = 10.67 ms at 48 kHz).
const BLOCK: usize = 512;

/// Number of positioned objects in the test scene.
const OBJECTS: usize = 4;

/// A scene with `OBJECTS` objects spread around the listener, plus one mono
/// audio plane per object.
fn scene_and_planes(sample_rate: u32) -> (SpatialScene, Vec<Vec<f32>>) {
    let mut scene = SpatialScene::new(sample_rate);
    let positions = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, 2.0, 0.0),
        Vec3::new(0.0, 0.0, 1.5),
    ];
    for p in positions {
        scene.create_audio_object(p);
    }
    let planes = (0..OBJECTS)
        .map(|i| common::sine(BLOCK, 220.0 * (i as f32 + 1.0), sample_rate as f32))
        .collect();
    (scene, planes)
}

fn bench_spatial(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;
    let sample_rate_u32 = sample_rate as u32;

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();
    let stereo = SpeakerLayout::stereo();
    let five_one = SpeakerLayout::five_point_one();

    // ── Binaural (HRTF head model) ──────────────────────────────────────────
    {
        let mut renderer = BinauralRenderer::new(5.0);
        renderer
            .prepare(&stereo, sample_rate_u32)
            .expect("the stereo layout is a valid binaural layout");
        let (scene, planes) = scene_and_planes(sample_rate_u32);
        let inputs: Vec<&[f32]> = planes.iter().map(|p| p.as_slice()).collect();
        let mut out = vec![0.0f32; 2 * BLOCK];
        let mut group = c.benchmark_group("spatial/binaural_render");
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_function("4_objects_analytic_hrtf", |b| {
            b.iter(|| {
                renderer
                    .process_block(
                        black_box(&scene),
                        black_box(&inputs),
                        BLOCK,
                        black_box(&mut out),
                    )
                    .expect("prepared renderer");
                black_box(&out);
            });
        });
        group.finish();
    }

    // ── VBAP on 5.1 ─────────────────────────────────────────────────────────
    {
        let mut renderer = VbapRenderer::new();
        renderer
            .prepare(&five_one, sample_rate_u32)
            .expect("the 5.1 layout is a valid VBAP layout");
        let (scene, planes) = scene_and_planes(sample_rate_u32);
        let inputs: Vec<&[f32]> = planes.iter().map(|p| p.as_slice()).collect();
        let mut out = vec![0.0f32; 6 * BLOCK];
        let mut group = c.benchmark_group("spatial/vbap_render_5_1");
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_function("4_objects", |b| {
            b.iter(|| {
                renderer
                    .process_block(
                        black_box(&scene),
                        black_box(&inputs),
                        BLOCK,
                        black_box(&mut out),
                    )
                    .expect("prepared renderer");
                black_box(&out);
            });
        });
        group.finish();
    }

    // ── Equal-power panner on stereo ────────────────────────────────────────
    {
        let mut renderer = BasicPanner::new(5.0);
        renderer
            .prepare(&stereo, sample_rate_u32)
            .expect("the stereo layout is a valid panner layout");
        let (scene, planes) = scene_and_planes(sample_rate_u32);
        let inputs: Vec<&[f32]> = planes.iter().map(|p| p.as_slice()).collect();
        let mut out = vec![0.0f32; 2 * BLOCK];
        let mut group = c.benchmark_group("spatial/basic_panner_stereo");
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_function("4_objects", |b| {
            b.iter(|| {
                renderer
                    .process_block(
                        black_box(&scene),
                        black_box(&inputs),
                        BLOCK,
                        black_box(&mut out),
                    )
                    .expect("prepared renderer");
                black_box(&out);
            });
        });
        group.finish();
    }

    // ── Ambisonic encode ────────────────────────────────────────────────────
    {
        let dir = Vec3::new(1.0, 0.0, 0.0);
        let mut group = c.benchmark_group("spatial/ambisonic_encode");
        group.throughput(Throughput::Elements(1));
        for order in [1u8, 2, 3] {
            let encoder = AmbisonicEncoder::new(order);
            let mut bus = vec![0.0f32; encoder.channels()];
            group.bench_with_input(BenchmarkId::from_parameter(order), &order, |b, _order| {
                b.iter(|| {
                    encoder.encode(black_box(dir), black_box(0.5), black_box(&mut bus));
                    black_box(&bus);
                });
            });
        }
        group.finish();
    }

    // ── Ambisonic decode ────────────────────────────────────────────────────
    {
        let mut group = c.benchmark_group("spatial/ambisonic_decode");
        for order in [1u8, 2, 3] {
            let mut decoder = AmbisonicDecoder::with_order(DecoderPolicy::Basic, order);
            decoder
                .prepare(&five_one, sample_rate_u32)
                .expect("the 5.1 layout decodes");
            let bus_width = decoder.bus_width();
            let bus = common::sine(bus_width * BLOCK, 440.0, sample_rate);
            let mut out = vec![0.0f32; 6 * BLOCK];
            group.throughput(Throughput::Elements(BLOCK as u64));
            group.bench_with_input(BenchmarkId::from_parameter(order), &order, |b, _order| {
                b.iter(|| {
                    decoder.process_bus(black_box(&bus), BLOCK, black_box(&mut out[..6 * BLOCK]));
                    black_box(&out);
                });
            });
        }
        group.finish();
    }

    // ── Ambisonic bus rotation (exact for orders 1..=3 only) ───────────────
    {
        let q = engine::spatial::Quat::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 0.3);
        let mut group = c.benchmark_group("spatial/ambisonic_rotate");
        group.throughput(Throughput::Elements(1));
        for order in [1u8, 2, 3] {
            let width = engine::spatial::channel_count(order);
            let mut frame = vec![0.25f32; width];
            group.bench_with_input(BenchmarkId::from_parameter(order), &order, |b, _order| {
                b.iter(|| {
                    rotate_bus_frame_n(black_box(q), order, black_box(&mut frame));
                    black_box(&frame);
                });
            });
        }
        group.finish();
    }

    // ── Ambisonic render (rotate + decode) ──────────────────────────────────
    {
        let mut renderer = AmbisonicRenderer::with_order(DecoderPolicy::Basic, 1);
        renderer
            .prepare(&five_one, sample_rate_u32)
            .expect("the 5.1 layout decodes");
        let bus_width = channel_count(renderer.order());
        let planes: Vec<Vec<f32>> = (0..bus_width)
            .map(|c| common::sine(BLOCK, 220.0 * (c as f32 + 1.0), sample_rate))
            .collect();
        let inputs: Vec<&[f32]> = planes.iter().map(|p| p.as_slice()).collect();
        let scene = SpatialScene::new(sample_rate_u32);
        let mut out = vec![0.0f32; 6 * BLOCK];
        let mut group = c.benchmark_group("spatial/ambisonic_render");
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_function("order1_to_5_1", |b| {
            b.iter(|| {
                renderer
                    .process_block(
                        black_box(&scene),
                        black_box(&inputs),
                        BLOCK,
                        black_box(&mut out),
                    )
                    .expect("prepared renderer");
                black_box(&out);
            });
        });
        group.finish();
    }
}

criterion_group!(benches, bench_spatial);
criterion_main!(benches);

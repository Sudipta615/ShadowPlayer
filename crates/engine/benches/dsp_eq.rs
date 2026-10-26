//! # Parametric EQ throughput
//!
//! Measures the cost of the engine's real multi-band parametric equaliser
//! ([`engine::dsp::equalizer::ParametricEq`]) as a **block-processing
//! throughput** number.
//!
//! What the numbers mean:
//!
//! - `time` is the wall-clock cost of one `process_block` call over a 512-frame
//!   stereo block at 48 kHz (10.67 ms of audio). Divide 512 by the reported
//!   nanoseconds and you get the real-time factor of the EQ alone: a factor
//!   of 1.0 means the EQ consumes exactly the audio it processes.
//! - `Throughput::Elements` is set to the number of **stereo frames** per
//!   iteration, so Criterion also reports the implied frames/second.
//!
//! What is being measured, precisely:
//!
//! - **Band counts 10 / 31 / 64** are the engine's own constructors
//!   (`default_10_band`, `standard_31_band`, `standard_64_band`) with every band
//!   enabled and given a non-zero gain, i.e. a *working* EQ curve rather than
//!   64 bypassed bands. A disabled band short-circuits, so benchmarking an
//!   all-disabled EQ would measure nothing.
//! - **Precision tiers** are the two public entry points of the same
//!   processor: `process_block` (f32 in / f32 out) and `process_block_f64`
//!   (f64 in / f64 out). The engine's `ParametricEq` keeps its biquad cores in
//!   native f64 *regardless* of the entry point (see the struct-level precision
//!   note in `src/dsp/equalizer/core.rs`): the f32 path promotes the block to
//!   f64 once, cascades every band in f64, and demotes once. The two numbers
//!   therefore differ by the promotion/demotion copy, **not** by the filter
//!   arithmetic. That is reported here rather than hidden, because a reader
//!   would otherwise conclude the EQ is "twice as slow in f64" for a reason
//!   that is not the one that matters.
//! - **Settled filters.** `set_band` starts a ~1.5 ms coefficient ramp
//!   (`SmoothedBiquad::smooth_steps`). While a ramp is in flight the EQ runs
//!   the per-sample smoothing path instead of the vectorised settled path. Each
//!   benchmark therefore warms the EQ with one second of audio *before* the
//!   measured region, so the number reported is the steady-state cost a
//!   playback session actually pays.
//! - `from_preset` measures the *control-path* cost of building an EQ from a
//!   `config::EqPreset` (the AutoEQ/preset path a host runs when the user
//!   picks a curve) — a different question from the audio-path cost above, and
//!   reported separately for that reason.
//!
//! `black_box` is `std::hint::black_box`: Criterion 0.8 deprecated its own
//! re-export in favour of the standard-library one.

use std::hint::black_box;

use config::{EqBandConfig, EqPreset, FilterType};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::dsp::biquad::BiquadCoeffs;
use engine::dsp::equalizer::{EqBandParams, EqFilterType, ParametricEq};

mod common;

/// Block size used for every measurement: 512 stereo frames = 10.67 ms at
/// 48 kHz, which is a typical realtime block period.
const BLOCK: usize = 512;

/// Build an EQ with `num_bands` **enabled** bands spread logarithmically from
/// 20 Hz to 20 kHz, each with a non-zero gain so no band short-circuits.
fn enabled_eq(num_bands: usize, sample_rate: f32) -> ParametricEq {
    let mut eq = ParametricEq::new(num_bands, sample_rate);
    let last = num_bands.saturating_sub(1);
    for i in 0..num_bands {
        let t = if last == 0 {
            0.0
        } else {
            i as f32 / last as f32
        };
        let frequency = 20.0 * 2.0f32.powf(t * 10.0);
        let gain_db = 3.0 * (t * std::f32::consts::TAU).sin();
        let filter_type = if i == 0 {
            EqFilterType::LowShelf
        } else if i == last {
            EqFilterType::HighShelf
        } else {
            EqFilterType::Peaking
        };
        eq.set_band(
            i,
            EqBandParams {
                frequency,
                gain_db,
                q: 1.0,
                filter_type,
                enabled: true,
            },
        );
    }
    eq.set_enabled(true);
    eq.snap_gain_stages();
    eq
}

/// Run one second of audio through the EQ so every biquad's coefficient ramp
/// completes and the vectorised settled path is the one being measured.
fn settle(eq: &mut ParametricEq, sample_rate: f32) {
    let (mut l, mut r) = common::stereo_sine(48_000, 997.0, 1009.0, sample_rate);
    eq.process_block(&mut l, &mut r);
}

fn bench_dsp_eq(c: &mut Criterion) {
    let sample_rate = common::SAMPLE_RATE;

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    let mut group = c.benchmark_group("dsp_eq/process_block_f32");
    for bands in [10usize, 31, 64] {
        let mut eq = enabled_eq(bands, sample_rate);
        settle(&mut eq, sample_rate);
        let (pristine_l, pristine_r) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l, mut r) = (pristine_l.clone(), pristine_r.clone());
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bands), &bands, |b, _| {
            b.iter(|| {
                // Re-seed the stimulus every iteration (the EQ works in place,
                // so the buffer would otherwise decay to silence) without
                // paying for signal generation inside the measured region.
                l.copy_from_slice(&pristine_l);
                r.copy_from_slice(&pristine_r);
                eq.process_block(black_box(&mut l), black_box(&mut r));
                black_box(&l);
            });
        });
    }
    group.finish();

    let mut group = c.benchmark_group("dsp_eq/process_block_f64");
    for bands in [10usize, 31, 64] {
        let mut eq = enabled_eq(bands, sample_rate);
        settle(&mut eq, sample_rate);
        let (pristine_l, pristine_r) = common::stereo_sine(BLOCK, 997.0, 1009.0, sample_rate);
        let (mut l64, mut r64): (Vec<f64>, Vec<f64>) = (
            pristine_l.iter().map(|&s| s as f64).collect(),
            pristine_r.iter().map(|&s| s as f64).collect(),
        );
        group.throughput(Throughput::Elements(BLOCK as u64));
        group.bench_with_input(BenchmarkId::from_parameter(bands), &bands, |b, _| {
            b.iter(|| {
                for (dst, src) in l64.iter_mut().zip(pristine_l.iter()) {
                    *dst = *src as f64;
                }
                for (dst, src) in r64.iter_mut().zip(pristine_r.iter()) {
                    *dst = *src as f64;
                }
                eq.process_block_f64(black_box(&mut l64), black_box(&mut r64));
                black_box(&l64);
            });
        });
    }
    group.finish();

    // Control path: building an EQ from a serialised preset. This is what a
    // host pays when the user selects an AutoEQ curve, and it is *not* on the
    // audio thread — so it is measured separately from the throughput numbers
    // above.
    let preset = EqPreset {
        name: "bench-10-band".to_string(),
        output_device_pattern: None,
        preamp_db: -3.0,
        bands: (0..10)
            .map(|i| EqBandConfig {
                enabled: true,
                filter_type: FilterType::Peaking,
                frequency: 31.0 * 2.0f32.powi(i),
                gain_db: 1.5,
                q: 1.1,
            })
            .collect(),
    };
    let mut group = c.benchmark_group("dsp_eq/from_preset");
    group.throughput(Throughput::Elements(preset.bands.len() as u64));
    group.bench_function("10_bands", |b| {
        b.iter(|| black_box(ParametricEq::from_preset(sample_rate, black_box(&preset))));
    });
    group.finish();

    // Control path: the auto-headroom estimate. This runs once per band edit
    // (a slider drag issues dozens a second) and is the reason the estimator
    // uses the φ closed form instead of a per-band complex magnitude: the
    // numbers here are the difference between a drag that keeps up and one
    // that stalls the command thread.
    //
    // `legacy_estimate` is the estimator this replaced, kept alongside it so
    // the comparison is *measured* in the same run rather than asserted from
    // first principles: a 4096-point log sweep that evaluated the complex
    // magnitude (four trigonometric calls) of every band at every point, plus
    // the same golden-section refinement.
    let mut group = c.benchmark_group("dsp_eq/headroom_estimate");
    for bands in [10_usize, 31, 64] {
        let params: Vec<EqBandParams> = (0..bands)
            .map(|i| {
                let t = i as f32 / (bands - 1) as f32;
                EqBandParams::peaking(20.0 * 1000.0f32.powf(t), 4.0, 2.0)
            })
            .collect();
        let mut eq = ParametricEq::new(bands, sample_rate);
        eq.set_enabled(true);
        eq.set_band_layout(&params);
        let coeffs: Vec<BiquadCoeffs<f64>> = params
            .iter()
            .map(|p| {
                p.filter_type.to_filter_type().compute_coeffs::<f64>(
                    sample_rate,
                    p.frequency,
                    p.gain_db,
                    p.q,
                )
            })
            .collect();

        group.throughput(Throughput::Elements(bands as u64));
        group.bench_with_input(BenchmarkId::new("closed_form", bands), &bands, |b, _| {
            b.iter(|| black_box(eq.combined_max_gain_db(black_box(sample_rate))));
        });
        group.bench_with_input(BenchmarkId::new("legacy_complex", bands), &bands, |b, _| {
            b.iter(|| black_box(legacy_estimate(&coeffs, sample_rate)));
        });
    }
    group.finish();
}

/// The headroom estimator as it was written before the `φ` closed form: a
/// 4096-point log sweep of the complex response, then golden-section
/// refinement to 1e-3 Hz. Behaviourally identical output; this exists only to
/// be the measured "before" in the comparison above.
fn legacy_estimate(coeffs: &[BiquadCoeffs<f64>], sample_rate: f32) -> f32 {
    const SWEEP_POINTS: usize = 4096;
    const INV_PHI: f64 = 0.618_033_988_749_894_9;

    let min_f = 20.0_f64;
    let max_f = (sample_rate as f64 * 0.49).min(20_000.0);
    let eval = |freq: f64| -> f64 {
        let mut total = 1.0_f64;
        for c in coeffs {
            total *= c.evaluate_magnitude(freq as f32, sample_rate);
        }
        total
    };

    let mut max_mag = 1.0_f64;
    let log_min = min_f.ln();
    let log_max = max_f.ln();
    let log_step = (log_max - log_min) / SWEEP_POINTS as f64;
    let mut best_index = 0usize;
    for i in 0..=SWEEP_POINTS {
        let mag = eval((log_min + i as f64 * log_step).exp());
        if mag > max_mag {
            max_mag = mag;
            best_index = i;
        }
    }
    let center = log_min + best_index as f64 * log_step;
    let mut a = (center - log_step).exp();
    let mut b = (center + log_step).exp();
    let mut c = b - INV_PHI * (b - a);
    let mut d = a + INV_PHI * (b - a);
    let mut fc = eval(c);
    let mut fd = eval(d);
    max_mag = max_mag.max(fc).max(fd);
    while (b - a).abs() > 1e-3 {
        if fc < fd {
            a = c;
            c = d;
            fc = fd;
            d = a + INV_PHI * (b - a);
            fd = eval(d);
        } else {
            b = d;
            d = c;
            fd = fc;
            c = b - INV_PHI * (b - a);
            fc = eval(c);
        }
        max_mag = max_mag.max(fc).max(fd);
    }
    if max_mag > 1.0 {
        (20.0 * max_mag.log10()) as f32
    } else {
        0.0
    }
}

criterion_group!(benches, bench_dsp_eq);
criterion_main!(benches);

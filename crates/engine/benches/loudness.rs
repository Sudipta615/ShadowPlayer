//! # Loudness analysis: EBU R128 / ITU-R BS.1770-5 and ReplayGain
//!
//! Measures the engine's loudness measurement layer — the analyser the
//! `replaygain-scanner` binary drives, and the streaming meter the playback
//! pipeline uses for normalisation.
//!
//! What the numbers mean:
//!
//! - Each iteration analyses **10 seconds of stereo audio at 48 kHz**.
//!   `Throughput::Elements` is set to 10, so Criterion reports
//!   **seconds of audio analysed per second of CPU**: a reported 8 elements/s
//!   means the analyser processes 8 seconds of audio for every second of CPU
//!   it is given, i.e. it is 8× faster than real time.
//! - The stimulus is a deterministic sweep + noise programme rather than a
//!   pure sine, so the K-weighting filters, the 400 ms momentary blocks, the
//!   3 s short-term window and the gating logic all see real, varying
//!   material. A constant tone would exercise the filters but not the
//!   block/gating statistics.
//!
//! What is actually being measured:
//!
//! - `analysis/bs1770` — [`LoudnessAnalyzer`] with
//!   `LoudnessStandard::ItuBs1770_5` and the EBU R128 compliance profile:
//!   the full offline analysis the `replaygain-scanner` runs (integrated
//!   loudness, true peak, LRA, the loudness/peak timeline, the gating census
//!   and the compliance verdict).
//! - `analysis/replaygain` — the same analyser with
//!   `LoudnessStandard::ReplayGain` and the ReplayGain profile. ReplayGain 2.0
//!   reuses the BS.1770 measurement and only changes the reference point, so
//!   the two rows should be close; the row exists because the *reference*
//!   and the compliance evaluation are part of what the scanner produces.
//! - `analysis_stream_only` — the streaming pass without the tail.
//!   `finish()` consumes the analyser, so it cannot be timed on its own; the
//!   delta between this row and `analysis/bs1770` **is** the cost of
//!   `finish()` (the gating census, the channel-power contributions and the
//!   compliance evaluation).
//! - `meter/process_interleaved` — the streaming [`LoudnessMeter`] alone:
//!   the K-weighting filter and the block statistics, with no timeline and no
//!   compliance evaluation. This is the path a *playback* session pays if it
//!   measures loudness while playing.
//! - `meter/snapshot` — the cost of asking the meter for its integrated
//!   (gated) value, which recomputes the gated mean over the accumulated
//!   400 ms blocks.
//! - `meter/compute_lra` — the loudness-range computation over the
//!   accumulated short-term blocks.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use engine::dsp::loudness::{
    AnalysisMode, LoudnessAnalyzer, LoudnessComplianceProfile, LoudnessMeter, LoudnessStandard,
};

mod common;

/// Seconds of programme audio analysed per iteration.
const SECONDS: usize = 10;
/// Frames per second of programme audio.
const RATE: usize = 48_000;
/// Stereo frames in the analysed programme.
const FRAMES: usize = SECONDS * RATE;

/// A deterministic 10-second stereo programme: a logarithmic sweep with a
/// noise floor, so the loudness statistics see varying material.
fn programme() -> Vec<f32> {
    let sweep = common::sweep(FRAMES, 40.0, 16_000.0, common::SAMPLE_RATE);
    let noise = common::noise(FRAMES, 0xBEEF);
    let mut interleaved = Vec::with_capacity(FRAMES * 2);
    for i in 0..FRAMES {
        let s = sweep[i] * 0.6 + noise[i] * 0.2;
        interleaved.push(s);
        interleaved.push(s * 0.8);
    }
    interleaved
}

fn bench_loudness(c: &mut Criterion) {
    let audio = programme();

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    // ── Full offline analysis ───────────────────────────────────────────────
    let mut group = c.benchmark_group("loudness/analysis");
    group.throughput(Throughput::Elements(SECONDS as u64));
    {
        group.bench_function("bs1770_ebu_r128", |b| {
            b.iter(|| {
                let mut analyzer = LoudnessAnalyzer::new(
                    common::SAMPLE_RATE,
                    2,
                    LoudnessStandard::ItuBs1770_5,
                    AnalysisMode::Programme,
                    LoudnessComplianceProfile::EbuR128,
                );
                analyzer.process_interleaved(black_box(&audio), 2);
                black_box(analyzer.finish());
            });
        });
    }
    {
        group.bench_function("replaygain_2_0", |b| {
            b.iter(|| {
                let mut analyzer = LoudnessAnalyzer::new(
                    common::SAMPLE_RATE,
                    2,
                    LoudnessStandard::ReplayGain,
                    AnalysisMode::Programme,
                    LoudnessComplianceProfile::ReplayGain,
                );
                analyzer.process_interleaved(black_box(&audio), 2);
                black_box(analyzer.finish());
            });
        });
    }
    group.finish();

    // ── The streaming pass without the tail ─────────────────────────────────
    // `finish()` consumes the analyser, so the tail cannot be timed on its
    // own. Instead the streaming pass is measured on its own, and the delta
    // between this row and `analysis/bs1770` above *is* the cost of `finish()`
    // (the gating census, the channel-power contributions and the compliance
    // evaluation).
    {
        let mut group = c.benchmark_group("loudness/analysis_stream_only");
        group.throughput(Throughput::Elements(SECONDS as u64));
        group.bench_function("bs1770_no_finish", |b| {
            b.iter(|| {
                let mut analyzer = LoudnessAnalyzer::new(
                    common::SAMPLE_RATE,
                    2,
                    LoudnessStandard::ItuBs1770_5,
                    AnalysisMode::Programme,
                    LoudnessComplianceProfile::EbuR128,
                );
                analyzer.process_interleaved(black_box(&audio), 2);
                black_box(&analyzer);
            });
        });
        group.finish();
    }

    // ── The streaming meter ──────────────────────────────────────────────────
    let mut group = c.benchmark_group("loudness/meter");
    group.throughput(Throughput::Elements(SECONDS as u64));
    {
        let mut meter = LoudnessMeter::new(common::SAMPLE_RATE, 2);
        group.bench_function("process_interleaved", |b| {
            b.iter(|| {
                meter.process_interleaved(black_box(&audio), 2);
                black_box(meter.snapshot());
            });
        });
    }
    {
        let mut meter = LoudnessMeter::new(common::SAMPLE_RATE, 2);
        meter.process_interleaved(&audio, 2);
        group.bench_function("snapshot", |b| {
            b.iter(|| black_box(meter.snapshot()));
        });
    }
    {
        let mut meter = LoudnessMeter::new(common::SAMPLE_RATE, 2);
        meter.process_interleaved(&audio, 2);
        group.bench_function("compute_lra", |b| {
            b.iter(|| black_box(meter.compute_lra()));
        });
    }
    group.finish();

    // ── Construction cost (a scanner opens one analyser per file) ───────────
    let mut group = c.benchmark_group("loudness/analyzer_construction");
    group.throughput(Throughput::Elements(1));
    group.bench_function("bs1770", |b| {
        b.iter(|| {
            black_box(LoudnessAnalyzer::new(
                common::SAMPLE_RATE,
                2,
                LoudnessStandard::ItuBs1770_5,
                AnalysisMode::Programme,
                LoudnessComplianceProfile::EbuR128,
            ))
        });
    });
    group.finish();

    // Report the measured integrated loudness once so a reader can confirm
    // the analyser really ran on the programme (and did not return silence).
    let mut analyzer = LoudnessAnalyzer::new(
        common::SAMPLE_RATE,
        2,
        LoudnessStandard::ItuBs1770_5,
        AnalysisMode::Programme,
        LoudnessComplianceProfile::EbuR128,
    );
    analyzer.process_interleaved(&audio, 2);
    let result = analyzer.finish();
    eprintln!(
        "[loudness] 10 s programme measured: integrated {:.2} LUFS, true peak {:.2} dBTP, \
         LRA {:.1} LU",
        result.integrated_lufs, result.max_true_peak_dbtp, result.lra_lu
    );
}

criterion_group!(benches, bench_loudness);
criterion_main!(benches);

//! # Codec decode throughput
//!
//! Measures the engine's **decode path** — the `open` cost and the
//! `decode_next` throughput — for the codecs this workspace can produce a
//! deterministic, self-contained fixture for.
//!
//! What the numbers mean:
//!
//! - `decode/open/<codec>` is the cost of opening the file: the container
//!   probe, the format-index build, and (for the codecs whose third-party
//!   crate needs a concrete `File`) the bridge the engine writes through. It is
//!   the latency a user sees when they double-click a track.
//! - `decode/throughput/<codec>` is the cost of the decode loop only. The
//!   decoder is opened *outside* the timed region (via
//!   [`Bencher::iter_custom`]), so the number is pure decode work.
//!   `Throughput::Elements` is the number of decoded frames per iteration, so
//!   Criterion reports **decoded frames per second**; divide the sample rate
//!   into that to get the real-time factor.
//!
//! Which codecs are here, and why:
//!
//! - **WAV (PCM 16-bit)** — the fixture is a hand-written RIFF/WAVE header
//!   plus the samples. Nothing is committed to the repository; the bytes are
//!   generated in memory and written to a temporary file at bench setup,
//!   because the engine's open path is a *file* path.
//! - **WavPack** — the fixture is encoded in memory by `wavicle`'s encoder
//!   (already a dev-dependency of this crate), so the bytes are a real,
//!   spec-conformant `.wv` stream and the decode is the engine's own pure-Rust
//!   WavPack path.
//! - **DSD (DSF, DSD64)** — the fixture is a hand-written DSF container
//!   (`DSD ` / `fmt ` / `data` chunks) carrying a deterministic 1-bit
//!   payload. It exercises the engine's native DSD reader *and* its
//!   five-stage 2:1 FIR decimation cascade (DSD64 → 88.2 kHz PCM).
//!
//! And which codecs are **not** here, and why:
//!
//! - **FLAC, ALAC, APE, TTA, MP3, AAC, Vorbis** — the engine can decode all of
//!   them (`decode::codecs` lists them as `Available`), but this workspace
//!   contains **no encoder** for any of them: Symphonia is a decoder-only
//!   crate, and there is no pure-Rust FLAC / ALAC / APE / TTA / MP3 / AAC /
//!   Vorbis encoder in the dependency tree. Producing a fixture for them would
//!   therefore mean committing a binary asset, which this bench refuses to do.
//!   Rather than benchmark a stub, the codecs are skipped and named here —
//!   the honest statement is "not benchmarked", not "not supported".
//! - **Musepack** — declared-but-undecodable in the engine itself
//!   (`CodecStatus::DeclaredUnavailable`): there is no pure-Rust Musepack
//!   decoder to benchmark.
//!
//! Every fixture is generated at run time into `std::env::temp_dir()` and
//! removed when the bench finishes.

use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::decode::Decoder;

mod common;

/// Sample rate of the PCM fixtures.
const RATE: u32 = 48_000;
/// Seconds of programme in each PCM fixture.
const SECONDS: usize = 2;
/// Stereo frames in each PCM fixture.
const FRAMES: usize = SECONDS * RATE as usize;

/// A stereo programme in memory, as interleaved f32.
fn programme() -> Vec<f32> {
    let (l, r) = common::stereo_sine(FRAMES, 997.0, 1009.0, RATE as f32);
    common::interleave(&l, &r)
}

/// A 16-bit PCM RIFF/WAVE file, written by hand.
fn wav_pcm16(interleaved: &[f32], sample_rate: u32) -> Vec<u8> {
    const CHANNELS: usize = 2;
    const BITS: usize = 16;
    let data_len = interleaved.len() * (BITS / 8);
    let block_align = (CHANNELS * BITS / 8) as u16;
    let byte_rate = sample_rate * block_align as u32;
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&(CHANNELS as u16).to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&(BITS as u16).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for &s in interleaved {
        let v = (s.clamp(-1.0, 1.0) * 32_767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// A DSF container carrying a deterministic 1-bit DSD64 payload.
///
/// `blocks` blocks of `block_bytes` bytes per channel; the DSF payload is
/// block-interleaved per channel (the layout the engine's reader expects).
fn dsf_dsd64(channels: usize, blocks: usize, block_bytes: usize) -> Vec<u8> {
    const DSD64_HZ: u32 = 2_822_400;
    let bytes_per_channel = blocks * block_bytes;
    let sample_count = (bytes_per_channel as u64) * 8;
    let audio_bytes = bytes_per_channel * channels;

    let mut out = Vec::with_capacity(28 + 52 + 12 + audio_bytes);
    // DSD chunk header: chunk id, chunk size, total file size, metadata
    // pointer.
    out.extend_from_slice(b"DSD ");
    out.extend_from_slice(&28u64.to_le_bytes());
    let total_size_pos = out.len();
    out.extend_from_slice(&0u64.to_le_bytes()); // patched below
    out.extend_from_slice(&0u64.to_le_bytes()); // no ID3v2 tag
                                                // fmt chunk.
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&52u64.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // format version
    out.extend_from_slice(&0u32.to_le_bytes()); // format id: DSD raw
    out.extend_from_slice(&(channels as u32).to_le_bytes()); // channel type
    out.extend_from_slice(&(channels as u32).to_le_bytes()); // channel count
    out.extend_from_slice(&DSD64_HZ.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // bits per sample
    out.extend_from_slice(&sample_count.to_le_bytes());
    out.extend_from_slice(&(block_bytes as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // reserved
                                                // data chunk.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((12 + audio_bytes) as u64).to_le_bytes());
    for b in 0..blocks {
        for ch in 0..channels {
            for i in 0..block_bytes {
                let byte = (b as u8)
                    .wrapping_mul(31)
                    .wrapping_add((ch as u8).wrapping_mul(17))
                    .wrapping_add(i as u8);
                out.push(byte);
            }
        }
    }
    // Patch the total file size field.
    let total = (out.len() as u64).to_le_bytes();
    out[total_size_pos..total_size_pos + 8].copy_from_slice(&total);
    out
}

/// Write a fixture to a uniquely-named temporary file and return its path.
fn write_fixture(name: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("shadowplayer-bench-{}-{name}", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("the temporary fixture can be created");
    file.write_all(bytes).expect("the fixture can be written");
    file.flush().expect("the fixture can be flushed");
    path
}

/// Remove a fixture written by [`write_fixture`].
fn remove_fixture(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Decode a whole file and return the number of decoded frames.
fn decode_whole(path: &Path) -> usize {
    let mut decoder = Decoder::open(path).expect("the fixture opens");
    let mut frames = 0usize;
    while let Ok(chunk) = decoder.decode_next(4096) {
        frames += chunk.frame_count;
        black_box(&chunk.samples);
    }
    frames
}

fn bench_decode(c: &mut Criterion) {
    let audio = programme();

    // The engine's realtime thread runs with flush-to-zero / denormals-are-zero
    // enabled (`dsp_utils::enable_flush_zero_denormals_on_current_thread`).
    // Without it a stage whose internal state decays towards zero pays
    // denormal penalties the engine never pays in production, so the bench
    // enables it too.
    let _ftz = engine::dsp_utils::enable_flush_zero_denormals_on_current_thread();

    // ── Fixtures ────────────────────────────────────────────────────────────
    let wav_bytes = wav_pcm16(&audio, RATE);
    let wav_path = write_fixture("wav.wav", &wav_bytes);

    let wv_bytes =
        wavicle::encode_float(2, RATE, &audio).expect("the WavPack encoder accepts the PCM");
    let wv_path = write_fixture("wavpack.wv", &wv_bytes);

    // 32 blocks × 4096 bytes per channel = 131 072 bytes per channel =
    // 1 048 576 DSD frames per channel = 0.37 s of DSD64, which decimates to
    // 32 768 PCM frames at 88.2 kHz.
    let dsf_bytes = dsf_dsd64(2, 32, 4096);
    let dsf_path = write_fixture("dsd.dsf", &dsf_bytes);

    let fixtures: [(&str, &Path); 3] = [
        ("wav_pcm16", wav_path.as_path()),
        ("wavpack", wv_path.as_path()),
        ("dsd_dsd64", dsf_path.as_path()),
    ];

    // ── Open cost ───────────────────────────────────────────────────────────
    let mut group = c.benchmark_group("decode/open");
    for (name, path) in fixtures {
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter(|| black_box(Decoder::open(black_box(path)).expect("the fixture opens")));
        });
    }
    group.finish();

    // ── Decode throughput (open excluded from the timed region) ─────────────
    let mut group = c.benchmark_group("decode/throughput");
    for (name, path) in fixtures {
        // One decode up front, to learn how many frames the fixture yields.
        let frames = decode_whole(path);
        group.throughput(Throughput::Elements(frames as u64));
        group.bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    let mut decoder = Decoder::open(black_box(path)).expect("the fixture opens");
                    let start = Instant::now();
                    let mut decoded = 0usize;
                    while let Ok(chunk) = decoder.decode_next(4096) {
                        decoded += chunk.frame_count;
                        black_box(&chunk.samples);
                    }
                    total += start.elapsed();
                    black_box(decoded);
                }
                total
            });
        });
    }
    group.finish();

    // Report what each fixture actually decoded, so the frame counts in the
    // throughput rows can be sanity-checked.
    for (name, path) in fixtures {
        let frames = decode_whole(path);
        let decoder = Decoder::open(path).expect("the fixture opens");
        let info = decoder.info();
        eprintln!(
            "[decode] {name}: {frames} frames decoded, {} Hz, {} channels, codec {:?}",
            info.sample_rate, info.channels, info.codec
        );
    }

    remove_fixture(&wav_path);
    remove_fixture(&wv_path);
    remove_fixture(&dsf_path);
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);

//! Shared fixtures for the engine's integration suites.
//!
//! Every generator here is **deterministic**: the same arguments always
//! produce byte-identical samples, so a suite that fails can be reproduced
//! exactly from its arguments alone. There are no binary assets in the
//! repository — the WAV fixtures are synthesised at run time through the
//! engine's own writer ([`WavFileWriter`]), which doubles as a check that the
//! writer and the decoder agree on the container.
//!
//! Each suite is its own crate, so this module is pulled in per file with
//! `mod common;`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use engine::output::wav_writer::WavFileWriter;

/// Sample rate the fixtures are generated at unless a suite says otherwise.
pub const SAMPLE_RATE: u32 = 48_000;

/// Channel count of the stereo fixtures.
pub const CHANNELS: u16 = 2;

/// Monotonic counter that makes temporary file names unique inside one
/// process (several suites run in parallel inside the same test binary).
static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// A stereo sine wave, interleaved (`L, R, L, R, …`).
///
/// `amplitude` is the peak amplitude, so the signal's RMS is
/// `amplitude / sqrt(2)`.
pub fn sine(freq_hz: f32, amplitude: f32, frames: usize, sample_rate: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    let step = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;
    for i in 0..frames {
        let s = (i as f32 * step).sin() * amplitude;
        out.push(s);
        out.push(s);
    }
    out
}

/// A mono sine wave (one sample per frame).
pub fn sine_mono(freq_hz: f32, amplitude: f32, frames: usize, sample_rate: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames);
    let step = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;
    for i in 0..frames {
        out.push((i as f32 * step).sin() * amplitude);
    }
    out
}

/// A stereo sine wave with **different content per channel**: the left
/// channel carries the sine, the right channel is silent.
///
/// This is the classic BS.1770 calibration geometry (a programme in one
/// channel of a stereo pair), and it is the only way to tell a
/// channel-sum meter from a channel-average one.
pub fn sine_left_only(freq_hz: f32, amplitude: f32, frames: usize, sample_rate: u32) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    let step = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;
    for i in 0..frames {
        out.push((i as f32 * step).sin() * amplitude);
        out.push(0.0);
    }
    out
}

/// Deterministic pseudo-random noise from a 64-bit LCG.
///
/// A hand-rolled generator rather than `rand` so the fixtures stay
/// reproducible across machines and the crate keeps its dependency list
/// unchanged.
pub fn noise_burst(frames: usize, seed: u64) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    let mut state = seed | 1;
    for _ in 0..frames {
        // xorshift64*
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let r = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
        // Map the top 24 bits into [-1, 1).
        let v = ((r >> 40) as f32 / 8_388_608.0) - 1.0;
        out.push(v);
        out.push(v);
    }
    out
}

/// A unit impulse at the first frame, silence afterwards.
pub fn impulse(frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    if !out.is_empty() {
        out[0] = 1.0;
        out[1] = 1.0;
    }
    out
}

/// A linear ramp from `-amplitude` to `+amplitude`, stereo.
pub fn ramp(frames: usize, amplitude: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames * 2);
    if frames == 0 {
        return out;
    }
    let denom = (frames - 1).max(1) as f32;
    for i in 0..frames {
        let v = -amplitude + 2.0 * amplitude * (i as f32 / denom);
        out.push(v);
        out.push(v);
    }
    out
}

/// A temporary file that removes itself when it goes out of scope.
///
/// The engine's suites write real files (a WAV the decoder has to open), and
/// a suite that leaves them behind turns the temp directory into a landfill.
pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// The path of the temporary file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Write interleaved `f32` samples to a temporary 32-bit float WAV file.
///
/// Uses the engine's own [`WavFileWriter`], so the fixture is produced by the
/// same code path the capture subsystem uses.
pub fn write_wav(name: &str, samples: &[f32], channels: u16, sample_rate: u32) -> TempFile {
    let path = unique_path(name, "wav");
    write_wav_to(&path, samples, channels, sample_rate);
    TempFile { path }
}

/// Write interleaved `f32` samples to a float32 WAV at `path`.
pub fn write_wav_to(path: &Path, samples: &[f32], channels: u16, sample_rate: u32) {
    let mut writer = WavFileWriter::create(path, sample_rate, channels)
        .expect("the engine's WAV writer must be able to create the file");
    writer
        .write_frames(samples)
        .expect("writing fixture samples must succeed");
    writer.finalize().expect("finalizing a WAV must succeed");
}

/// A temporary directory that removes itself when it goes out of scope.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// The path of the temporary directory.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A unique temporary directory for this process.
pub fn temp_dir(name: &str) -> TempDir {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "engine_it_dir_{name}_{pid}_{nanos}_{seq}",
        pid = std::process::id()
    ));
    std::fs::create_dir_all(&path).expect("a temporary directory must be creatable");
    TempDir { path }
}

/// Write raw bytes to a temporary file.
///
/// Used by the suites that need a container the engine does not write (a
/// DSF or a DFF), so the fixture is still generated at run time rather than
/// committed as a binary asset.
pub fn write_bytes(name: &str, bytes: &[u8], extension: &str) -> TempFile {
    let path = unique_path(name, extension);
    std::fs::write(&path, bytes).expect("writing the temp fixture must succeed");
    TempFile { path }
}

/// A unique path in the system temp directory for this process.
pub fn unique_path(name: &str, extension: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = UNIQUE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "engine_it_{name}_{pid}_{nanos}_{seq}.{extension}",
        pid = std::process::id()
    ))
}

/// The RMS of an interleaved buffer's samples (all channels pooled).
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// The peak absolute sample of an interleaved buffer.
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()))
}

/// Magnitude of `samples` at `freq_hz`, measured with a Goertzel filter.
///
/// Used instead of an FFT so the measurement needs no scratch allocation and
/// no dependency: the Goertzel accumulator is exact for a single bin and its
/// magnitude is `2 * |z| / N` for a real sinusoid.
pub fn goertzel_magnitude(samples: &[f32], freq_hz: f32, sample_rate: u32) -> f32 {
    let n = samples.len() as f32;
    if n == 0.0 {
        return 0.0;
    }
    let k = 2.0 * std::f32::consts::PI * freq_hz / sample_rate as f32;
    let coeff = 2.0 * k.cos();
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    for &x in samples {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let real = s1 - s2 * k.cos();
    let imag = s2 * k.sin();
    (2.0 * (real * real + imag * imag).sqrt()) / n
}

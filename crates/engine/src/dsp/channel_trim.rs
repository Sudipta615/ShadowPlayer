//! Per-channel trim, routing matrix, and LFE gain — the multichannel
//! management stage (§5 incremental).
//!
//! This stage runs in the **multichannel passthrough path only** (`>2`
//! channels), on every channel, before the pre-mix chain. Stereo playback
//! keeps the pipeline's balance control; these entries are intentionally not
//! applied to the front L/R pair of a downmixed stereo stream.
//!
//! Per-frame order within the stage (documented signal path):
//!
//! ```text
//! in[ch] → routing matrix (Σ src·matrix[src][dst]) → gain → polarity
//!        → fractional delay → LFE gain → out[ch]
//! ```
//!
//! Every sub-stage is individually bypassable; when nothing is enabled the
//! stage is a pure passthrough and `process_planes` returns immediately.

use std::cell::UnsafeCell;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use config::{
    BassManagementConfig, ChannelEqConfig, ChannelRoutingConfig, ChannelTrimConfig, EqBandConfig,
    FilterType as ConfigFilterType, LfeConfig, MatrixFirRouteConfig,
};

use crate::buffer::{MAX_AUDIO_BLOCK_FRAMES, MAX_CHANNELS};
use crate::dsp::biquad::{BiquadCoeffsF32, BiquadStateF32, FilterType as BiquadFilterType};
use crate::dsp::convolution::ConvolutionEngine;

/// Maximum cascade length for one multichannel EQ channel. The value matches
/// the public stereo EQ limit while keeping the per-channel state bounded.
pub const MAX_CHANNEL_EQ_BANDS: usize = 64;

/// Maximum number of sparse matrix FIR routes. Each route owns a partitioned
/// convolution engine (two FFT planners and its preallocated buffers), so this
/// is a memory/compute guard, not an arbitrary channel limit.
pub const MAX_MATRIX_FIR_ROUTES: usize = 16;

/// Maximum IR length accepted for a matrix route. At 48 kHz this is ~5.5 s;
/// the convolver still reports exactly how much of the file it used.
pub const MAX_MATRIX_IR_SAMPLES: usize = 262_144;

/// Maximum pre-convolution delay for one matrix route (ms). The delay is
/// allocated per route so a profile pays only for the delay it uses.
pub const MAX_MATRIX_FIR_DELAY_MS: f32 = 1_000.0;

/// Maximum per-channel delay (ms). Enough for driver timing alignment while
/// bounding memory: at 192 kHz this is 19 200 samples (a 32 KiB plane per
/// channel), at 384 kHz 38 400 (64 KiB).
pub const MAX_CHANNEL_DELAY_MS: f32 = 100.0;

/// dB → linear gain.
fn db_to_linear(db: f32) -> f32 {
    if db.is_finite() {
        10.0f32.powf(db / 20.0)
    } else {
        1.0
    }
}

fn channel_eq_coeffs(sample_rate: f32, band: &EqBandConfig) -> BiquadCoeffsF32 {
    let filter_type = match band.filter_type {
        ConfigFilterType::Peaking => BiquadFilterType::Peaking,
        ConfigFilterType::LowShelf => BiquadFilterType::Lowshelf,
        ConfigFilterType::HighShelf => BiquadFilterType::Highshelf,
        ConfigFilterType::LowPass => BiquadFilterType::Lowpass,
        ConfigFilterType::HighPass => BiquadFilterType::Highpass,
        ConfigFilterType::Bandpass => BiquadFilterType::Bandpass,
        ConfigFilterType::Notch => BiquadFilterType::Notch,
        ConfigFilterType::AllPass => BiquadFilterType::Allpass,
    };
    filter_type.compute_coeffs::<f32>(sample_rate, band.frequency, band.gain_db, band.q)
}

/// Next power of two ≥ `n` (delay-line capacity must be a power of two so
/// the modulo is a mask).
fn next_pow2(n: usize) -> usize {
    let mut p = 1usize;
    while p < n {
        p <<= 1;
    }
    p
}

/// One loaded matrix FIR route. The source/destination pair is fixed for the
/// life of the configuration; delays and convolution state are per-route so
/// two routes can process the same source with different timing.
struct MatrixFirRoute {
    /// Stable position in the configured route list. Parallel workers write
    /// their route's convolved samples into the slot with this index; the
    /// audio thread then sums the slots in configuration order, which keeps
    /// the result bit-identical to the single-threaded path.
    index: usize,
    source: usize,
    destination: usize,
    gain: f32,
    delay_samples: usize,
    delay_cap: usize,
    delay_mask: usize,
    delay_pos: usize,
    delay_buf: Vec<f32>,
    engine: ConvolutionEngine,
}

/// Minimum route count before a worker pool is considered. Below this the
/// wake/sum overhead is larger than the measured convolution cost.
const MATRIX_FIR_PARALLEL_MIN_ROUTES: usize = 4;

/// One process/reset job published by the audio thread.
///
/// The pointers are valid for the duration of the job: the audio thread does
/// not mutate the source planes or the output slots until every worker has
/// acknowledged the job via `done`.
#[derive(Clone, Copy)]
struct MatrixFirJob {
    planes: *const Vec<f32>,
    channels: usize,
    frames: usize,
    /// Output slot for each route, indexed by `MatrixFirRoute::index`.
    outputs: *const *mut f32,
    /// First frame in the caller's planes to process.
    frame_offset: usize,
    reset: bool,
}

impl MatrixFirJob {
    fn idle() -> Self {
        Self {
            planes: std::ptr::null(),
            channels: 0,
            frames: 0,
            outputs: std::ptr::null(),
            frame_offset: 0,
            reset: false,
        }
    }
}

/// Atomics shared by the audio thread and the matrix workers. `job` is only
/// written before `seq` is released and only read after `seq` is acquired;
/// workers never touch it while the audio thread is mutating it.
struct MatrixFirShared {
    seq: AtomicU64,
    done: AtomicUsize,
    stop: AtomicBool,
    failed: AtomicBool,
    job: UnsafeCell<MatrixFirJob>,
}

// SAFETY: every non-atomic access to `job` is ordered by the `seq` release /
// acquire pair above, and `done`/`failed` use release/acquire as well. The raw
// plane and output pointers inside a job are valid until `done` reaches the
// worker count (or until `reset` completes).
unsafe impl Sync for MatrixFirShared {}
// SAFETY: the shared atomics and ordered job slot are the only cross-thread
// state; `unsafe impl Sync` above covers the pointers inside `job`.
unsafe impl Send for MatrixFirShared {}

struct MatrixFirWorker {
    handle: JoinHandle<()>,
}

/// Preallocated, allocation-free route workers. The audio thread processes
/// its own share of the routes as well, so a two-core host uses both cores
/// instead of leaving one idle while it waits.
struct MatrixFirPool {
    shared: Arc<MatrixFirShared>,
    workers: Vec<MatrixFirWorker>,
    /// One output slot per configured route, in route-index order.
    outputs: Vec<Vec<f32>>,
    /// Ownership anchor for the raw pointers stored in `MatrixFirShared::job`.
    /// The buffer must not move after the workers are spawned.
    output_ptrs: Vec<*mut f32>,
    destinations: Vec<usize>,
    latency_samples: usize,
    heap_bytes: usize,
}

// SAFETY: the pool is moved as a whole to the audio thread. The raw output
// pointers are never dereferenced by the thread that owns this struct; only
// the joined worker threads read them, under the job/ack protocol.
unsafe impl Send for MatrixFirPool {}

impl MatrixFirPool {
    /// Split `routes` across the audio thread and `worker_count` helpers.
    ///
    /// Thread creation is the only fallible part. If it fails, the caller
    /// drops this partial pool and rebuilds the routes in single-threaded
    /// mode, so a host with a restrictive thread limit still gets correct
    /// audio rather than a silent route.
    fn new(
        routes: Vec<MatrixFirRoute>,
        worker_count: usize,
    ) -> Result<(Vec<MatrixFirRoute>, Self), std::io::Error> {
        debug_assert!(routes.len() >= 2);
        debug_assert!(worker_count >= 1);
        debug_assert!(worker_count < routes.len());

        let route_count = routes.len();
        let destinations = routes.iter().map(|route| route.destination).collect();
        let latency_samples = routes
            .iter()
            .map(MatrixFirRoute::latency_samples)
            .max()
            .unwrap_or(0);
        let outputs = (0..route_count)
            .map(|_| vec![0.0f32; MAX_AUDIO_BLOCK_FRAMES])
            .collect::<Vec<_>>();
        let output_ptrs = outputs
            .iter()
            .map(|slot| slot.as_ptr() as *mut f32)
            .collect::<Vec<_>>();

        // Participant 0 is the audio thread; helpers are 1..=worker_count.
        let participants = worker_count + 1;
        let mut groups = (0..participants)
            .map(|_| Vec::new())
            .collect::<Vec<Vec<MatrixFirRoute>>>();
        for (position, route) in routes.into_iter().enumerate() {
            groups[position % participants].push(route);
        }
        let mut groups = groups.into_iter();
        let main_routes = groups.next().unwrap_or_default();
        let worker_routes = groups.collect::<Vec<_>>();
        let worker_heap = worker_routes
            .iter()
            .flatten()
            .map(MatrixFirRoute::heap_bytes)
            .sum::<usize>();

        let shared = Arc::new(MatrixFirShared {
            seq: AtomicU64::new(0),
            done: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            job: UnsafeCell::new(MatrixFirJob {
                outputs: output_ptrs.as_ptr(),
                ..MatrixFirJob::idle()
            }),
        });
        let mut workers = Vec::with_capacity(worker_count);
        for (worker_index, routes) in worker_routes.into_iter().enumerate() {
            if routes.is_empty() {
                continue;
            }
            let shared_ref = Arc::clone(&shared);
            let spawn = std::thread::Builder::new()
                .name(format!("matrix-fir-{worker_index}"))
                .stack_size(256 * 1024)
                .spawn(move || matrix_fir_worker_loop(shared_ref, routes));
            match spawn {
                Ok(handle) => workers.push(MatrixFirWorker { handle }),
                Err(error) => {
                    // Stop the helpers that did start. Their routes die with
                    // them; the caller rebuilds from the saved configuration.
                    shared.stop.store(true, Ordering::Release);
                    shared.seq.fetch_add(1, Ordering::Release);
                    for worker in &workers {
                        worker.handle.thread().unpark();
                    }
                    for worker in workers {
                        let _ = worker.handle.join();
                    }
                    return Err(error);
                }
            }
        }

        let pool_heap = worker_heap
            + outputs
                .iter()
                .map(|slot| slot.capacity() * std::mem::size_of::<f32>())
                .sum::<usize>()
            + output_ptrs.capacity() * std::mem::size_of::<*mut f32>()
            + std::mem::size_of::<Self>();
        Ok((
            main_routes,
            Self {
                shared,
                workers,
                outputs,
                output_ptrs,
                destinations,
                latency_samples,
                heap_bytes: pool_heap,
            },
        ))
    }

    fn worker_count(&self) -> usize {
        self.workers.len()
    }

    /// Publish a block to the helpers and wake them. The audio thread then
    /// processes its own routes before calling [`Self::wait`].
    fn start(&self, planes: *const Vec<f32>, channels: usize, frames: usize, frame_offset: usize) {
        self.shared.done.store(0, Ordering::Release);
        // SAFETY: workers are parked on `seq`; `done` was reset before the
        // new sequence is published, and no worker reads `job` until it
        // observes the release below.
        unsafe {
            *self.shared.job.get() = MatrixFirJob {
                planes,
                channels,
                frames,
                outputs: self.output_ptrs.as_ptr(),
                frame_offset,
                reset: false,
            };
        }
        self.shared.seq.fetch_add(1, Ordering::Release);
        for worker in &self.workers {
            worker.handle.thread().unpark();
        }
    }

    /// Wait for every helper to acknowledge the current job. Returns `true`
    /// if a helper panicked; the caller then falls back to scalar routing for
    /// the rest of the stream rather than hanging or mixing partial output.
    fn wait(&self) -> bool {
        let worker_count = self.workers.len();
        while self.shared.done.load(Ordering::Acquire) < worker_count {
            std::hint::spin_loop();
        }
        self.shared.failed.load(Ordering::Acquire)
    }

    /// Reset every route, including the helper-owned ones, on the control
    /// path. Uses the same job/ack protocol as a block, so it allocates
    /// nothing and cannot race a running block (`&self` excludes one).
    fn reset(&self) {
        if self.workers.is_empty() {
            return;
        }
        self.shared.done.store(0, Ordering::Release);
        // SAFETY: same protocol as `start`; the job carries no plane or
        // output pointers that a reset reads.
        unsafe {
            *self.shared.job.get() = MatrixFirJob {
                outputs: self.output_ptrs.as_ptr(),
                reset: true,
                ..MatrixFirJob::idle()
            };
        }
        self.shared.seq.fetch_add(1, Ordering::Release);
        for worker in &self.workers {
            worker.handle.thread().unpark();
        }
        while self.shared.done.load(Ordering::Acquire) < self.workers.len() {
            std::hint::spin_loop();
        }
    }
}

impl Drop for MatrixFirPool {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.seq.fetch_add(1, Ordering::Release);
        for worker in &self.workers {
            worker.handle.thread().unpark();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.handle.join();
        }
    }
}

fn matrix_fir_worker_loop(shared: Arc<MatrixFirShared>, mut routes: Vec<MatrixFirRoute>) {
    let _ = crate::output::rt_thread::promote_current_thread_to_realtime_silent();
    let mut last_seq = 0u64;
    loop {
        let seq = shared.seq.load(Ordering::Acquire);
        if seq == last_seq {
            if shared.stop.load(Ordering::Acquire) {
                return;
            }
            std::thread::park();
            continue;
        }
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        last_seq = seq;
        // SAFETY: `seq` was acquired after the audio thread released it, and
        // the job remains immutable until `done` is incremented below.
        let job = unsafe { *shared.job.get() };
        let finished = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if job.reset {
                for route in &mut routes {
                    route.reset();
                }
            } else {
                for route in &mut routes {
                    route.process_into_job(job);
                }
            }
        }));
        if finished.is_err() {
            shared.failed.store(true, Ordering::Release);
        }
        shared.done.fetch_add(1, Ordering::Release);
    }
}

pub struct ChannelTrimmer {
    enabled: bool,
    sample_rate: f32,
    /// Linear gain per channel (1.0 = unity).
    gains: Vec<f32>,
    /// Fractional delay in samples per channel (0.0 = none).
    delay_samples: Vec<f32>,
    /// Polarity inversion per channel.
    invert: Vec<bool>,
    /// Per-channel delay lines (circular, power-of-two capacity).
    delay_cap: usize,
    delay_mask: usize,
    delay_bufs: Vec<Vec<f32>>,
    delay_pos: Vec<usize>,
    /// Routing matrix `[src][dst]`; empty when bypassed.
    routing_enabled: bool,
    routing_ok: bool,
    routing: Vec<Vec<f32>>,
    /// LFE gain applied to channels whose role is LFE.
    lfe_enabled: bool,
    lfe_gain: f32,
    lfe_channels: Vec<usize>,
    /// LFE low-pass (bass management): a second-order minimum-phase LP at
    /// `crossover_hz`, applied to LFE-role channels after the LFE gain.
    lfe_lp_enabled: bool,
    lfe_lp_coeffs: BiquadCoeffsF32,
    lfe_lp_states: Vec<BiquadStateF32>,
    /// Fixed flags avoid a `Vec::contains` scan in the audio loop.
    lfe_channel_flags: [bool; MAX_CHANNELS],
    /// Main-speaker bass-management high-pass. It is intentionally separate
    /// from the LFE low-pass so either half can be enabled explicitly.
    mains_hp_enabled: bool,
    mains_hp_coeffs: BiquadCoeffsF32,
    mains_hp_states: Vec<BiquadStateF32>,
    /// Per-channel EQ cascades. Configuration may allocate these vectors;
    /// processing only walks already-built state and never allocates.
    channel_eq_enabled: bool,
    channel_eq_coeffs: Vec<Vec<BiquadCoeffsF32>>,
    channel_eq_states: Vec<Vec<BiquadStateF32>>,
    /// Sparse matrix FIR routes. Configurations load files on the control
    /// path; `process_planes` only walks the built routes and their
    /// preallocated convolution/delay state.
    matrix_fir_configs: Vec<MatrixFirRouteConfig>,
    /// Routes processed by the audio thread. With no pool this is every
    /// route; with a pool it is participant 0's share.
    matrix_fir_routes: Vec<MatrixFirRoute>,
    matrix_fir_pool: Option<MatrixFirPool>,
    matrix_fir_sum: [f32; MAX_CHANNELS],
    matrix_fir_enabled: bool,
    /// Control-path switch used by the pool-failure retry and the parallel
    /// equivalence test. It is not a user setting.
    matrix_fir_parallel: bool,
    /// Test/benchmark override for the participant count. `None` uses the
    /// host's available parallelism.
    matrix_fir_participants: Option<usize>,
}

impl MatrixFirRoute {
    fn reset(&mut self) {
        self.delay_buf.fill(0.0);
        self.delay_pos = 0;
        self.engine.reset();
    }

    /// Process one source plane range into a dedicated output slot. The
    /// convolution state advances exactly as it does in the inline path.
    fn process_range(
        &mut self,
        planes: &[Vec<f32>],
        channels: usize,
        frame_offset: usize,
        frames: usize,
        output: &mut [f32],
    ) {
        for (i, slot) in output.iter_mut().enumerate().take(frames) {
            let x = if self.source < channels {
                planes[self.source][frame_offset + i]
            } else {
                0.0
            };
            let delayed = if self.delay_samples == 0 {
                x
            } else {
                let pos = self.delay_pos;
                let read = (pos + self.delay_cap - self.delay_samples) & self.delay_mask;
                let value = self.delay_buf[read];
                self.delay_buf[pos] = x;
                self.delay_pos = (pos + 1) & self.delay_mask;
                value
            };
            *slot = self.engine.process_mono(delayed) * self.gain;
        }
    }

    /// Worker-side entry point. The raw pointers are validated by the pool
    /// protocol; every index is bounded by the route build and job fields.
    fn process_into_job(&mut self, job: MatrixFirJob) {
        if job.frames == 0 || job.planes.is_null() || job.outputs.is_null() {
            return;
        }
        // SAFETY: `planes` points at `channels` live `Vec<f32>` entries and
        // `outputs` at one live output pointer per configured route. The
        // audio thread keeps both alive and unchanged until `done` is
        // incremented, and route indices are < the configured route count.
        let planes = unsafe { std::slice::from_raw_parts(job.planes, job.channels) };
        let output_ptr = unsafe { *job.outputs.add(self.index) };
        if output_ptr.is_null() {
            return;
        }
        let output = unsafe { std::slice::from_raw_parts_mut(output_ptr, job.frames) };
        self.process_range(planes, job.channels, job.frame_offset, job.frames, output);
    }

    fn latency_samples(&self) -> usize {
        self.delay_samples + self.engine.latency_samples()
    }

    fn heap_bytes(&self) -> usize {
        self.delay_buf.capacity() * std::mem::size_of::<f32>() + self.engine.heap_bytes()
    }
}

impl ChannelTrimmer {
    pub fn new(sample_rate: f32) -> Self {
        let mut t = Self {
            enabled: false,
            sample_rate,
            gains: Vec::new(),
            delay_samples: Vec::new(),
            invert: Vec::new(),
            delay_cap: 0,
            delay_mask: 0,
            delay_bufs: Vec::new(),
            delay_pos: Vec::new(),
            routing_enabled: false,
            routing_ok: false,
            routing: Vec::new(),
            lfe_enabled: false,
            lfe_gain: 1.0,
            lfe_channels: Vec::new(),
            lfe_lp_enabled: false,
            lfe_lp_coeffs: BiquadCoeffsF32::identity(),
            lfe_lp_states: (0..MAX_CHANNELS)
                .map(|_| BiquadStateF32::default())
                .collect(),
            lfe_channel_flags: [false; MAX_CHANNELS],
            mains_hp_enabled: false,
            mains_hp_coeffs: BiquadCoeffsF32::identity(),
            mains_hp_states: (0..MAX_CHANNELS)
                .map(|_| BiquadStateF32::default())
                .collect(),
            channel_eq_enabled: false,
            channel_eq_coeffs: (0..MAX_CHANNELS).map(|_| Vec::new()).collect(),
            channel_eq_states: (0..MAX_CHANNELS).map(|_| Vec::new()).collect(),
            matrix_fir_configs: Vec::new(),
            matrix_fir_routes: Vec::new(),
            matrix_fir_pool: None,
            matrix_fir_sum: [0.0; MAX_CHANNELS],
            matrix_fir_enabled: false,
            matrix_fir_parallel: true,
            matrix_fir_participants: None,
        };
        t.rebuild_delay_lines(MAX_CHANNELS);
        t
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if (self.sample_rate - sample_rate).abs() > 0.5 {
            self.sample_rate = sample_rate;
            self.rebuild_delay_lines(self.delay_bufs.len());
            // IR partitions are rate-specific. Rebuild them here (control
            // path) rather than playing a 44.1 kHz response at 48 kHz.
            if !self.matrix_fir_configs.is_empty() {
                self.build_matrix_fir();
            }
        }
    }

    /// Reset all runtime state without discarding the configuration. This is
    /// what a seek/stop calls: delay lines, biquad memories, and each matrix
    /// convolver return to silence, but the loaded IRs stay resident.
    pub fn reset(&mut self) {
        for buffer in &mut self.delay_bufs {
            buffer.fill(0.0);
        }
        self.delay_pos.fill(0);
        for state in &mut self.lfe_lp_states {
            *state = BiquadStateF32::default();
        }
        for state in &mut self.mains_hp_states {
            *state = BiquadStateF32::default();
        }
        for states in &mut self.channel_eq_states {
            for state in states {
                *state = BiquadStateF32::default();
            }
        }
        for route in &mut self.matrix_fir_routes {
            route.reset();
        }
        if let Some(pool) = &self.matrix_fir_pool {
            pool.reset();
        }
    }

    fn rebuild_delay_lines(&mut self, channels: usize) {
        let samples = (MAX_CHANNEL_DELAY_MS * 0.001 * self.sample_rate).ceil() as usize + 2;
        let cap = next_pow2(samples.max(4));
        self.delay_cap = cap;
        self.delay_mask = cap - 1;
        self.delay_bufs = (0..channels.max(1)).map(|_| vec![0.0; cap]).collect();
        self.delay_pos = vec![0; channels.max(1)];
    }

    /// Apply the per-channel trim configuration (gain/delay/polarity).
    pub fn set_config(&mut self, config: &ChannelTrimConfig, sample_rate: f32) {
        self.set_sample_rate(sample_rate);
        self.enabled = config.enabled;
        self.gains.clear();
        self.delay_samples.clear();
        self.invert.clear();
        let max_ch = MAX_CHANNELS;
        for _ in 0..max_ch {
            self.gains.push(1.0);
            self.delay_samples.push(0.0);
            self.invert.push(false);
        }
        for entry in &config.entries {
            if entry.channel >= max_ch {
                log::warn!(
                    "ChannelTrim: entry for channel {} exceeds MAX_CHANNELS ({}); ignored",
                    entry.channel,
                    max_ch
                );
                continue;
            }
            let delay_ms = entry.delay_ms.clamp(0.0, MAX_CHANNEL_DELAY_MS);
            self.gains[entry.channel] = db_to_linear(entry.gain_db);
            self.delay_samples[entry.channel] = delay_ms * 0.001 * self.sample_rate;
            self.invert[entry.channel] = entry.invert;
        }
        if config.enabled {
            self.rebuild_delay_lines(self.delay_bufs.len());
        }
    }

    /// Compile per-channel EQ entries into bounded biquad cascades.
    ///
    /// This is a configuration-time operation. The realtime path only applies
    /// the preallocated coefficients/states, so a 16-channel block does not
    /// allocate or parse filter parameters while audio is flowing.
    pub fn set_channel_eq(&mut self, config: &ChannelEqConfig, sample_rate: f32) {
        self.channel_eq_enabled = config.enabled;
        self.channel_eq_coeffs = (0..MAX_CHANNELS).map(|_| Vec::new()).collect();
        self.channel_eq_states = (0..MAX_CHANNELS).map(|_| Vec::new()).collect();
        if !config.enabled {
            return;
        }

        for entry in &config.entries {
            if entry.channel >= MAX_CHANNELS {
                log::warn!(
                    "ChannelEq: entry for channel {} exceeds MAX_CHANNELS ({}); ignored",
                    entry.channel,
                    MAX_CHANNELS
                );
                continue;
            }
            let coeffs = &mut self.channel_eq_coeffs[entry.channel];
            let states = &mut self.channel_eq_states[entry.channel];
            for band in entry.bands.iter().take(MAX_CHANNEL_EQ_BANDS) {
                if !band.enabled {
                    continue;
                }
                coeffs.push(channel_eq_coeffs(sample_rate, band));
                states.push(BiquadStateF32::default());
            }
        }
    }

    /// Configure the mains high-pass portion of bass management.
    pub fn set_bass_management(&mut self, config: &BassManagementConfig, sample_rate: f32) {
        self.mains_hp_enabled = config.enabled && config.mains_highpass_enabled;
        if self.mains_hp_enabled {
            self.mains_hp_coeffs =
                BiquadCoeffsF32::highpass(sample_rate, config.crossover_hz, config.q);
            for state in &mut self.mains_hp_states {
                *state = BiquadStateF32::default();
            }
        }
    }

    /// Apply the routing matrix. The matrix must be square and its width must
    /// equal the active channel count at process time; any other shape is
    /// rejected here (a warning is logged) and routing is bypassed.
    pub fn set_routing(&mut self, config: &ChannelRoutingConfig) {
        self.routing_enabled = config.enabled;
        self.routing = config.matrix.clone();
        self.routing_ok = if !config.enabled {
            false
        } else if config.matrix.is_empty() {
            true // empty matrix = identity; enabled but no-op
        } else {
            let n = config.matrix.len();
            let square = config.matrix.iter().all(|row| row.len() == n);
            let bounded = n > 0 && n <= MAX_CHANNELS;
            if !(square && bounded) {
                log::warn!(
                    "ChannelRouting: matrix must be square with 1..={} rows, got {}x{}; routing bypassed",
                    MAX_CHANNELS,
                    n,
                    config.matrix.first().map_or(0, |r| r.len())
                );
            }
            square && bounded
        };

        // Matrix FIR routes are independent of the scalar matrix shape. They
        // still require the routing config's master switch; a profile can use
        // an empty (identity) matrix plus FIR routes.
        self.matrix_fir_configs = if config.enabled {
            config.fir_routes.clone()
        } else {
            Vec::new()
        };
        self.build_matrix_fir();
    }

    /// Compile the configured sparse routes into preloaded convolvers and
    /// delay lines. This performs file I/O and FFT planning on the control
    /// path; it is never called from `process_planes`.
    fn build_matrix_fir(&mut self) {
        // Dropping a pool joins its helper threads before the routes are
        // replaced. This is configuration/control-path work.
        self.matrix_fir_pool = None;
        self.matrix_fir_routes.clear();
        self.matrix_fir_enabled = false;
        if self.matrix_fir_configs.is_empty() {
            return;
        }

        let mut loaded = Vec::with_capacity(self.matrix_fir_configs.len());
        let mut skipped = 0usize;
        for config in &self.matrix_fir_configs {
            if loaded.len() >= MAX_MATRIX_FIR_ROUTES {
                skipped += 1;
                continue;
            }
            if config.source >= MAX_CHANNELS || config.destination >= MAX_CHANNELS {
                log::warn!(
                    "ChannelRouting FIR: route {}→{} exceeds MAX_CHANNELS ({}); ignored",
                    config.source,
                    config.destination,
                    MAX_CHANNELS
                );
                skipped += 1;
                continue;
            }
            let Some(path) = config.ir_path.as_deref().filter(|p| !p.is_empty()) else {
                log::warn!(
                    "ChannelRouting FIR: route {}→{} has no ir_path; ignored",
                    config.source,
                    config.destination
                );
                skipped += 1;
                continue;
            };
            let path = PathBuf::from(path);
            if !path.is_file() {
                log::error!(
                    "ChannelRouting FIR: impulse response {} does not exist; route {}→{} ignored",
                    path.display(),
                    config.source,
                    config.destination
                );
                skipped += 1;
                continue;
            }

            let mut engine = ConvolutionEngine::new(self.sample_rate, MAX_MATRIX_IR_SAMPLES);
            engine.set_enabled(true);
            engine.set_wet_mix(1.0);
            if let Err(error) = engine.load_ir_from_file(&path) {
                log::error!(
                    "ChannelRouting FIR: failed to load {}: {error}; route {}→{} ignored",
                    path.display(),
                    config.source,
                    config.destination
                );
                skipped += 1;
                continue;
            }

            let delay_ms = config.delay_ms.clamp(0.0, MAX_MATRIX_FIR_DELAY_MS);
            let delay_samples = (delay_ms * 0.001 * self.sample_rate).round() as usize;
            let delay_cap = if delay_samples == 0 {
                0
            } else {
                next_pow2(delay_samples + 1)
            };
            let index = loaded.len();
            loaded.push(MatrixFirRoute {
                index,
                source: config.source,
                destination: config.destination,
                gain: db_to_linear(config.gain_db),
                delay_samples,
                delay_cap,
                delay_mask: delay_cap.saturating_sub(1),
                delay_pos: 0,
                delay_buf: vec![0.0; delay_cap],
                engine,
            });
        }

        if skipped > 0 {
            log::warn!(
                "ChannelRouting FIR: {} of {} route(s) were not loaded",
                skipped,
                self.matrix_fir_configs.len()
            );
        }
        if loaded.is_empty() {
            return;
        }

        let cores = self.matrix_fir_participants.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1)
        });
        let participants = cores.min(loaded.len());
        let use_parallel = self.matrix_fir_parallel
            && loaded.len() >= MATRIX_FIR_PARALLEL_MIN_ROUTES
            && participants >= 2;

        let route_count = loaded.len();
        if use_parallel {
            match MatrixFirPool::new(loaded, participants - 1) {
                Ok((main_routes, pool)) => {
                    self.matrix_fir_routes = main_routes;
                    self.matrix_fir_pool = Some(pool);
                }
                Err(error) => {
                    log::warn!(
                        "ChannelRouting FIR: could not start the route worker pool ({error}); \
                         rebuilding {} route(s) single-threaded",
                        route_count
                    );
                    self.matrix_fir_parallel = false;
                    self.build_matrix_fir();
                    return;
                }
            }
        } else {
            self.matrix_fir_routes = loaded;
        }

        self.matrix_fir_enabled = true;
        log::info!(
            "ChannelRouting FIR: {} route(s) loaded, {} samples latency, {}",
            route_count,
            self.matrix_fir_latency_samples(),
            match &self.matrix_fir_pool {
                Some(pool) => format!("{} worker(s) + the audio thread", pool.worker_count()),
                None => "single-threaded".to_string(),
            }
        );
    }

    /// Total added latency of the matrix FIR section: the longest route's
    /// pre-convolution delay plus one convolution partition. Zero when no
    /// route is loaded.
    pub fn matrix_fir_latency_samples(&self) -> usize {
        let inline = self
            .matrix_fir_routes
            .iter()
            .map(MatrixFirRoute::latency_samples)
            .max()
            .unwrap_or(0);
        let pooled = self
            .matrix_fir_pool
            .as_ref()
            .map_or(0, |pool| pool.latency_samples);
        inline.max(pooled)
    }

    /// Number of helper threads currently carrying matrix-FIR routes. Zero
    /// means the stage is running on the audio thread alone.
    pub fn matrix_fir_worker_count(&self) -> usize {
        self.matrix_fir_pool
            .as_ref()
            .map_or(0, MatrixFirPool::worker_count)
    }

    /// Total intrinsic latency of the stage: the longest per-channel trim
    /// delay plus the matrix FIR section. The routing node reports this to the
    /// live graph so a FIR profile's latency is visible, not assumed.
    pub fn latency_samples(&self) -> usize {
        let trim = if self.enabled {
            self.delay_samples
                .iter()
                .copied()
                .fold(0.0f32, f32::max)
                .ceil() as usize
        } else {
            0
        };
        trim + self.matrix_fir_latency_samples()
    }

    /// Heap owned by the matrix FIR routes (delay lines plus partitioned
    /// convolution state), for the live graph's persistent-memory report.
    pub fn matrix_fir_heap_bytes(&self) -> usize {
        self.matrix_fir_routes
            .iter()
            .map(MatrixFirRoute::heap_bytes)
            .sum::<usize>()
            + self
                .matrix_fir_pool
                .as_ref()
                .map_or(0, |pool| pool.heap_bytes)
    }

    /// Apply LFE gain config and the optional LFE low-pass crossover.
    pub fn set_lfe(&mut self, config: &LfeConfig) {
        self.lfe_enabled = config.enabled;
        self.lfe_gain = if config.enabled {
            db_to_linear(config.gain_db)
        } else {
            1.0
        };
        // Bass-management LFE low-pass: second-order Butterworth at the
        // crossover frequency (spec §17/§34). `None` = full-band LFE.
        match config.crossover_hz {
            Some(hz) if hz.is_finite() && hz > 0.0 => {
                let q = std::f32::consts::FRAC_1_SQRT_2;
                self.lfe_lp_coeffs = BiquadCoeffsF32::lowpass(self.sample_rate, hz, q);
                self.lfe_lp_enabled = true;
                for state in &mut self.lfe_lp_states {
                    *state = BiquadStateF32::default();
                }
            }
            _ => {
                self.lfe_lp_enabled = false;
            }
        }
    }

    /// Declare which channel indices carry the LFE role (derived from the
    /// active [`crate::decode::ChannelLayout`]).
    pub fn set_lfe_channels(&mut self, channels: Vec<usize>) {
        self.lfe_channel_flags = [false; MAX_CHANNELS];
        for &channel in &channels {
            if channel < MAX_CHANNELS {
                self.lfe_channel_flags[channel] = true;
            }
        }
        self.lfe_channels = channels;
    }

    /// True when any sub-stage may alter the signal for `channels` channels.
    pub fn is_active(&self, channels: usize) -> bool {
        if !self.enabled
            && !self.routing_enabled
            && !self.lfe_enabled
            && !self.mains_hp_enabled
            && !self.channel_eq_enabled
            && !self.matrix_fir_enabled
        {
            return false;
        }
        if !self.enabled {
            // Routing, LFE management, bass management, per-channel EQ, and
            // matrix FIR can all act independently of trim gain/delay/polarity.
            return self.routing_applies(channels)
                || self.lfe_applies()
                || self.mains_hp_enabled
                || self.channel_eq_enabled
                || self.matrix_fir_enabled;
        }
        true
    }

    fn routing_applies(&self, channels: usize) -> bool {
        self.routing_enabled && self.routing_ok && self.routing.len() == channels
    }

    fn lfe_applies(&self) -> bool {
        self.lfe_enabled && self.lfe_channel_flags.iter().any(|active| *active)
    }

    /// Process `frames` of `channels` planar audio in place.
    pub fn process_planes(&mut self, planes: &mut [Vec<f32>], channels: usize, frames: usize) {
        if !self.is_active(channels) {
            return;
        }
        let ch = channels.min(planes.len());
        if ch == 0 || frames == 0 {
            return;
        }
        let routing = self.routing_applies(ch);
        let mut tmp = [0.0f32; MAX_CHANNELS];

        for i in 0..frames {
            // 1. Routing matrix: out[dst] = Σ_src matrix[src][dst] · in[src].
            if routing {
                for (dst, slot) in tmp.iter_mut().enumerate().take(ch) {
                    let mut acc = 0.0f32;
                    for (src, plane) in planes.iter().enumerate().take(ch) {
                        acc += self.routing[src][dst] * plane[i];
                    }
                    *slot = acc;
                }
                for (c, plane) in planes.iter_mut().enumerate().take(ch) {
                    plane[i] = tmp[c];
                }
            }

            // 2–4. Per-channel gain → polarity → delay, then LFE gain, then
            // the optional LFE low-pass (bass management).
            for (c, plane) in planes.iter_mut().enumerate().take(ch) {
                let is_lfe = self.lfe_enabled && self.lfe_channel_flags[c];
                let mut g = if self.enabled { self.gains[c] } else { 1.0 };
                if self.enabled && self.invert[c] {
                    g = -g;
                }
                if is_lfe {
                    g *= self.lfe_gain;
                }
                let d = if self.enabled {
                    self.delay_samples[c]
                } else {
                    0.0
                };
                let x = plane[i];
                if d > 0.0 {
                    let i0 = d.floor() as usize;
                    let frac = d - i0 as f32;
                    let pos = self.delay_pos[c];
                    // y = (1−frac)·x[n−i0] + frac·x[n−i0−1]; for i0 == 0 the
                    // "delayed by 0" sample is the current input itself.
                    let (a_val, b_val) = if i0 == 0 {
                        (
                            x,
                            self.delay_bufs[c][(pos + self.delay_cap - 1) & self.delay_mask],
                        )
                    } else {
                        (
                            self.delay_bufs[c][(pos + self.delay_cap - i0) & self.delay_mask],
                            self.delay_bufs[c][(pos + self.delay_cap - i0 - 1) & self.delay_mask],
                        )
                    };
                    let y = a_val * (1.0 - frac) + b_val * frac;
                    self.delay_bufs[c][pos] = x;
                    self.delay_pos[c] = (pos + 1) & self.delay_mask;
                    plane[i] = y * g;
                } else {
                    plane[i] = x * g;
                }
                // LFE low-pass runs after gain, only on LFE-role channels.
                if self.lfe_lp_enabled && is_lfe {
                    plane[i] = self.lfe_lp_states[c].process(plane[i], &self.lfe_lp_coeffs);
                }
                // Main high-pass is applied to every non-LFE speaker channel;
                // the LFE path has its own explicitly configured low-pass.
                if self.mains_hp_enabled && !is_lfe {
                    plane[i] = self.mains_hp_states[c].process(plane[i], &self.mains_hp_coeffs);
                }
                // Finally apply the channel-specific EQ cascade. EQ state is
                // independent per channel, so center/surround/height content
                // cannot leak into the front-pair stereo filters.
                if self.channel_eq_enabled {
                    for band in 0..self.channel_eq_coeffs[c].len() {
                        plane[i] = self.channel_eq_states[c][band]
                            .process(plane[i], &self.channel_eq_coeffs[c][band]);
                    }
                }
            }
        }

        // 5. Sparse matrix FIR: read the post-trim source, convolve, and add
        //    to the destination. Every route is clocked for every frame even
        //    when its destination is out of range, so partition timing stays
        //    aligned if the channel layout changes. Sources are read before
        //    any FIR sum is added, which keeps the operation independent of
        //    route order.
        if self.matrix_fir_enabled {
            self.process_matrix_fir(planes, ch, frames);
        }
    }

    /// Matrix FIR stage. Without a pool this is the original per-frame loop;
    /// with a pool the audio thread hands chunks to parked helpers, processes
    /// its own share, then sums every route slot in configuration order.
    fn process_matrix_fir(&mut self, planes: &mut [Vec<f32>], ch: usize, frames: usize) {
        let Some(pool) = self.matrix_fir_pool.as_mut() else {
            let sums = &mut self.matrix_fir_sum;
            for i in 0..frames {
                sums[..ch].fill(0.0);
                for route in &mut self.matrix_fir_routes {
                    let x = if route.source < ch {
                        planes[route.source][i]
                    } else {
                        0.0
                    };
                    let delayed = if route.delay_samples == 0 {
                        x
                    } else {
                        let pos = route.delay_pos;
                        let read = (pos + route.delay_cap - route.delay_samples) & route.delay_mask;
                        let value = route.delay_buf[read];
                        route.delay_buf[pos] = x;
                        route.delay_pos = (pos + 1) & route.delay_mask;
                        value
                    };
                    let convolved = route.engine.process_mono(delayed);
                    if route.destination < ch {
                        sums[route.destination] += convolved * route.gain;
                    }
                }
                for (c, plane) in planes.iter_mut().enumerate().take(ch) {
                    plane[i] += sums[c];
                }
            }
            return;
        };

        let mut failed = false;
        let mut offset = 0usize;
        while offset < frames {
            let chunk = (frames - offset).min(MAX_AUDIO_BLOCK_FRAMES);
            pool.start(planes.as_ptr(), ch, chunk, offset);
            for route in &mut self.matrix_fir_routes {
                let slot = route.index;
                route.process_range(planes, ch, offset, chunk, &mut pool.outputs[slot][..chunk]);
            }
            if pool.wait() {
                failed = true;
                break;
            }
            let sums = &mut self.matrix_fir_sum;
            for i in 0..chunk {
                sums[..ch].fill(0.0);
                for (slot, &destination) in pool.destinations.iter().enumerate() {
                    if destination < ch {
                        sums[destination] += pool.outputs[slot][i];
                    }
                }
                for (channel, plane) in planes.iter_mut().enumerate().take(ch) {
                    plane[offset + i] += sums[channel];
                }
            }
            offset += chunk;
        }
        if failed {
            // A helper panicked. The routes it owned are no longer
            // trustworthy, so stop applying matrix FIR for the rest of this
            // generation instead of hanging or mixing a partial sum.
            self.matrix_fir_enabled = false;
        }
    }
}

impl Default for ChannelTrimmer {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use config::{
        ChannelRoutingConfig, ChannelTrimConfig, ChannelTrimEntry, LfeConfig, MatrixFirRouteConfig,
    };

    fn planes(channels: usize, frames: usize, fill: impl Fn(usize, usize) -> f32) -> Vec<Vec<f32>> {
        (0..channels)
            .map(|c| (0..frames).map(|i| fill(c, i)).collect())
            .collect()
    }

    /// Write a minimal 16-bit mono PCM WAV so the matrix-FIR test exercises
    /// the same file loader production uses.
    fn write_mono_wav(path: &Path, sample_rate: u32, samples: &[f32]) {
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
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);
        std::fs::write(path, bytes).unwrap();
    }

    fn temp_wav(samples: &[f32]) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "shadowplayer_matrix_fir_{}_{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        write_mono_wav(&path, 48_000, samples);
        path
    }

    fn fir_route(source: usize, destination: usize, path: &Path) -> ChannelRoutingConfig {
        ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: vec![MatrixFirRouteConfig {
                source,
                destination,
                ir_path: Some(path.display().to_string()),
                gain_db: 0.0,
                delay_ms: 0.0,
            }],
        }
    }

    fn disabled() -> ChannelTrimConfig {
        ChannelTrimConfig {
            enabled: false,
            entries: Vec::new(),
        }
    }

    #[test]
    fn disabled_is_passthrough() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&ChannelRoutingConfig::default());
        t.set_lfe(&LfeConfig::default());
        let mut p = planes(4, 8, |c, i| (c * 10 + i) as f32);
        let before = p.clone();
        t.process_planes(&mut p, 4, 8);
        assert_eq!(p, before);
    }

    #[test]
    fn gain_and_polarity() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(
            &ChannelTrimConfig {
                enabled: true,
                entries: vec![
                    ChannelTrimEntry {
                        channel: 0,
                        gain_db: -6.0206, // ≈ 0.5×
                        ..Default::default()
                    },
                    ChannelTrimEntry {
                        channel: 1,
                        gain_db: 0.0,
                        invert: true,
                        ..Default::default()
                    },
                ],
            },
            48_000.0,
        );
        let mut p = planes(4, 4, |_c, i| (i + 1) as f32);
        t.process_planes(&mut p, 4, 4);
        for (i, (a, b, c2, c3)) in p[0]
            .iter()
            .zip(p[1].iter())
            .zip(p[2].iter())
            .zip(p[3].iter())
            .map(|(((a, b), c2), c3)| (a, b, c2, c3))
            .enumerate()
        {
            let v = (i + 1) as f32;
            assert!((a - v * 0.5).abs() < 1e-3, "ch0 {a:.4} != {:.4}", v * 0.5);
            assert!((b + v).abs() < 1e-5, "ch1 {b:.4} != {:.4}", -v);
            assert!((c2 - v).abs() < 1e-5, "ch2 should be untouched");
            assert!((c3 - v).abs() < 1e-5, "ch3 should be untouched");
        }
    }

    #[test]
    fn integer_delay() {
        let mut t = ChannelTrimmer::new(1000.0); // 1 sample per ms
        t.set_config(
            &ChannelTrimConfig {
                enabled: true,
                entries: vec![ChannelTrimEntry {
                    channel: 0,
                    delay_ms: 3.0,
                    ..Default::default()
                }],
            },
            1000.0,
        );
        let mut p = planes(2, 10, |_c, i| (i + 1) as f32);
        t.process_planes(&mut p, 2, 10);
        // ch0 delayed by 3 samples: first 3 outputs are 0, then 1,2,3,...
        assert_eq!(p[0][0], 0.0);
        assert_eq!(p[0][1], 0.0);
        assert_eq!(p[0][2], 0.0);
        for (i, &v) in p[0].iter().enumerate().skip(3) {
            assert!((v - (i + 1 - 3) as f32).abs() < 1e-5);
        }
        // ch1 untouched.
        for (i, &v) in p[1].iter().enumerate().take(10) {
            assert!((v - (i + 1) as f32).abs() < 1e-5);
        }
    }

    #[test]
    fn fractional_delay_interpolates() {
        let mut t = ChannelTrimmer::new(1000.0);
        t.set_config(
            &ChannelTrimConfig {
                enabled: true,
                entries: vec![ChannelTrimEntry {
                    channel: 0,
                    delay_ms: 0.5,
                    ..Default::default()
                }],
            },
            1000.0,
        );
        // Ramp input: y[n] = 0.5·x[n] + 0.5·x[n−1] = n + 0.5.
        let mut p = planes(1, 16, |_c, i| (i + 1) as f32);
        t.process_planes(&mut p, 1, 16);
        for (i, &v) in p[0].iter().enumerate() {
            let expected = i as f32 + 0.5;
            assert!((v - expected).abs() < 1e-5, "y[{i}] = {v} != {expected}");
        }
    }

    #[test]
    fn fractional_delay_dc_converges() {
        let mut t = ChannelTrimmer::new(1000.0);
        t.set_config(
            &ChannelTrimConfig {
                enabled: true,
                entries: vec![ChannelTrimEntry {
                    channel: 0,
                    delay_ms: 0.3,
                    ..Default::default()
                }],
            },
            1000.0,
        );
        // Steady DC input: output must converge to the same DC value.
        let mut p = planes(1, 64, |_c, _i| 0.75);
        t.process_planes(&mut p, 1, 64);
        assert!((p[0][63] - 0.75).abs() < 1e-5);
    }

    #[test]
    fn routing_matrix_swaps_channels() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        // Swap: dst0 = src1, dst1 = src0, dst2 = src2.
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: vec![
                vec![0.0, 1.0, 0.0],
                vec![1.0, 0.0, 0.0],
                vec![0.0, 0.0, 1.0],
            ],
            fir_routes: Vec::new(),
        });
        let mut p = planes(3, 4, |c, i| (c * 10 + i) as f32);
        let before = p.clone();
        t.process_planes(&mut p, 3, 4);
        for i in 0..4 {
            assert!((p[0][i] - before[1][i]).abs() < 1e-5);
            assert!((p[1][i] - before[0][i]).abs() < 1e-5);
            assert!((p[2][i] - before[2][i]).abs() < 1e-5);
        }
    }

    #[test]
    fn routing_mismatched_width_is_bypassed() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: vec![vec![1.0, 0.0], vec![0.0, 1.0]], // 2×2, but 4 active channels
            fir_routes: Vec::new(),
        });
        let mut p = planes(4, 4, |c, i| (c * 10 + i) as f32);
        let before = p.clone();
        t.process_planes(&mut p, 4, 4);
        assert_eq!(p, before, "non-matching matrix must be bypassed");
    }

    #[test]
    fn non_square_routing_is_rejected() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: vec![vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]],
            fir_routes: Vec::new(),
        });
        assert!(!t.routing_applies(2));
        assert!(!t.routing_applies(3));
    }

    #[test]
    fn lfe_gain_only_on_lfe_channels() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_lfe(&LfeConfig {
            enabled: true,
            gain_db: 6.0, // ≈ 1.995×
            crossover_hz: None,
        });
        t.set_lfe_channels(vec![3]); // 5.1 layout LFE slot
        let mut p = planes(6, 4, |_c, _i| 0.5);
        t.process_planes(&mut p, 6, 4);
        for (i, &lfe) in p[3].iter().enumerate() {
            assert!((lfe - 0.5 * 1.995).abs() < 1e-3, "LFE ch must be boosted");
            for &v in [&p[0][i], &p[1][i], &p[2][i], &p[4][i], &p[5][i]] {
                assert!((v - 0.5).abs() < 1e-5, "non-LFE ch must be untouched");
            }
        }
    }

    #[test]
    fn lfe_low_pass_dc_unity_and_hf_rolloff() {
        // Bass-management LFE low-pass: DC passes at unity, a 10 kHz tone is
        // strongly attenuated (crossover at 120 Hz), and non-LFE channels are
        // untouched.
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_lfe(&LfeConfig {
            enabled: true,
            gain_db: 0.0,
            crossover_hz: Some(120.0),
        });
        t.set_lfe_channels(vec![3]);

        // DC: after the filter settles, the LFE channel converges to 1.0.
        let mut p = planes(6, 4096, |c, _i| if c == 3 { 1.0 } else { 0.25 });
        t.process_planes(&mut p, 6, 4096);
        let lfe_tail = p[3][1024..].iter().copied().fold(0.0f32, f32::max);
        assert!(
            (lfe_tail - 1.0).abs() < 0.05,
            "LFE DC gain must be unity, got {lfe_tail}"
        );
        for c in [0, 1, 2, 4, 5] {
            assert!(
                (p[c][4095] - 0.25).abs() < 1e-5,
                "non-LFE ch must be untouched"
            );
        }

        // 10 kHz: far above the 120 Hz crossover → heavily attenuated.
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_lfe(&LfeConfig {
            enabled: true,
            gain_db: 0.0,
            crossover_hz: Some(120.0),
        });
        t.set_lfe_channels(vec![3]);
        let mut p = planes(6, 4096, |c, i| {
            if c == 3 {
                (2.0 * std::f32::consts::PI * 10_000.0 * i as f32 / 48_000.0).sin()
            } else {
                0.0
            }
        });
        t.process_planes(&mut p, 6, 4096);
        let in_rms = 1.0 / 2.0f32.sqrt(); // sine RMS = 1/√2
        let out_rms =
            (p[3][1024..].iter().map(|s| s * s).sum::<f32>() / (4096 - 1024) as f32).sqrt();
        assert!(
            out_rms < in_rms * 0.1,
            "10 kHz tone must be rolled off by the 120 Hz LFE low-pass (in {in_rms:.3}, out {out_rms:.4})"
        );
    }
    #[test]
    fn matrix_fir_convolves_the_source_into_the_destination() {
        let path = temp_wav(&[1.0]);
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&fir_route(0, 2, &path));
        assert!(t.is_active(3));
        let frames = 2_048;
        let mut p = planes(
            3,
            frames,
            |c, i| if c == 0 { (i as f32) / 1000.0 } else { 0.0 },
        );
        let source = p[0].clone();
        t.process_planes(&mut p, 3, frames);

        let first = p[2]
            .iter()
            .position(|sample| sample.abs() > 1e-6)
            .expect("FIR route produced no output");
        let reported = t.matrix_fir_latency_samples();
        assert!(
            first <= reported && first + 1 >= reported,
            "FIR first output at {first}, reported latency {reported}"
        );
        for i in first..frames {
            let expected = source[i - first];
            assert!(
                (p[2][i] - expected).abs() < 1e-3,
                "FIR output[{i}] {:.5} != source[{}] {:.5}",
                p[2][i],
                i - first,
                expected
            );
        }
        // The source plane itself is not replaced by the FIR route.
        assert_eq!(p[0], source);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn matrix_fir_applies_route_gain_and_pre_delay() {
        let path = temp_wav(&[1.0]);
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: vec![MatrixFirRouteConfig {
                source: 0,
                destination: 1,
                ir_path: Some(path.display().to_string()),
                gain_db: -6.0206, // ≈ 0.5×
                delay_ms: 1.0,    // 48 samples at 48 kHz
            }],
        });
        let frames = 3_072;
        let mut p = planes(2, frames, |c, i| if c == 0 && i == 0 { 1.0 } else { 0.0 });
        t.process_planes(&mut p, 2, frames);

        let first = p[1]
            .iter()
            .position(|sample| sample.abs() > 1e-6)
            .expect("delayed FIR produced no output");
        let reported = t.matrix_fir_latency_samples();
        assert!(
            (first as isize - reported as isize).abs() <= 1,
            "delayed FIR first output at {first}, reported latency {reported}"
        );
        assert!(
            (p[1][first] - 0.5).abs() < 1e-3,
            "gain not applied: {}",
            p[1][first]
        );
        for (i, sample) in p[1].iter().enumerate() {
            if i != first {
                assert!(
                    sample.abs() < 1e-3,
                    "delayed FIR invented output at {i}: {sample}"
                );
            }
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn matrix_fir_route_without_a_file_is_inactive_not_silent_passthrough() {
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: vec![MatrixFirRouteConfig {
                source: 0,
                destination: 1,
                ir_path: Some("/definitely/missing/response.wav".to_string()),
                gain_db: 0.0,
                delay_ms: 0.0,
            }],
        });
        assert!(!t.is_active(2), "a failed route must not claim activity");
        let mut p = planes(2, 16, |c, i| (c * 10 + i) as f32);
        let before = p.clone();
        t.process_planes(&mut p, 2, 16);
        assert_eq!(p, before);
    }

    #[test]
    fn matrix_fir_parallel_path_matches_single_threaded_bit_for_bit() {
        let path = temp_wav(&[1.0, 0.5, -0.25]);
        let routing = ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: (0..4)
                .map(|route| MatrixFirRouteConfig {
                    source: route % 2,
                    destination: 2 + route % 2,
                    ir_path: Some(path.display().to_string()),
                    gain_db: -(route as f32) * 2.0,
                    delay_ms: (route as f32) * 0.25,
                })
                .collect(),
        };
        let frames = 5_000; // > MAX_AUDIO_BLOCK_FRAMES: exercises pool chunking.
        let input = planes(4, frames, |channel, frame| {
            if channel < 2 {
                ((frame as f32 * 0.013).sin() + (frame as f32 * 0.037).cos()) * 0.1
            } else {
                0.0
            }
        });

        let mut serial = ChannelTrimmer::new(48_000.0);
        serial.set_config(&disabled(), 48_000.0);
        serial.matrix_fir_parallel = false;
        serial.set_routing(&routing);
        let mut serial_out = input.clone();
        serial.process_planes(&mut serial_out, 4, frames);

        let mut parallel = ChannelTrimmer::new(48_000.0);
        parallel.set_config(&disabled(), 48_000.0);
        // Deterministically exercise the pool even on a one-core test host.
        parallel.matrix_fir_participants = Some(2);
        parallel.set_routing(&routing);
        assert_eq!(parallel.matrix_fir_worker_count(), 1);
        let mut parallel_out = input;
        parallel.process_planes(&mut parallel_out, 4, frames);

        assert_eq!(
            serial_out, parallel_out,
            "parallel matrix FIR changed the summed samples"
        );

        parallel.reset();
        let mut silence = planes(4, 2_048, |_, _| 0.0);
        parallel.process_planes(&mut silence, 4, 2_048);
        assert!(
            silence[2]
                .iter()
                .chain(silence[3].iter())
                .all(|s| s.abs() < 1e-6),
            "parallel reset left a worker route's delay/convolution state behind"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn matrix_fir_reset_clears_delay_and_convolution_state() {
        let path = temp_wav(&[1.0]);
        let mut t = ChannelTrimmer::new(48_000.0);
        t.set_config(&disabled(), 48_000.0);
        t.set_routing(&ChannelRoutingConfig {
            enabled: true,
            matrix: Vec::new(),
            fir_routes: vec![MatrixFirRouteConfig {
                source: 0,
                destination: 1,
                ir_path: Some(path.display().to_string()),
                gain_db: 0.0,
                delay_ms: 1.0,
            }],
        });
        let mut p = planes(2, 2_048, |c, _i| if c == 0 { 1.0 } else { 0.0 });
        t.process_planes(&mut p, 2, 2_048);
        assert!(p[1].iter().any(|sample| sample.abs() > 1e-6));
        t.reset();
        let mut silence = planes(2, 1_024, |_c, _i| 0.0);
        t.process_planes(&mut silence, 2, 1_024);
        assert!(
            silence[1].iter().all(|sample| sample.abs() < 1e-6),
            "reset left convolution/delay state behind"
        );
        let _ = std::fs::remove_file(path);
    }
}

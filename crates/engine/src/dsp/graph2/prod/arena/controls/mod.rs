//! Symmetric control surface for [`DspGraph`] — now queued.
//!
//! Control methods enqueue a plain-data [`NodeCmd`] into a per-node
//! SPSC queue ([`ControlBus`]) instead of mutating nodes directly. The audio
//! thread drains every queue at the block boundary
//! ([`DspGraph::control_tick`]), so a command applies deterministically at
//! the start of the next processed block. Heap-bearing operations (IR loads,
//! full config application) travel via the generation-swap path
//! ([`GraphControlHandle::publish_generation`]) instead — `NodeCmd` is
//! strictly `Copy` and allocation-free by construction.
//!
//! The [`ControlBus`] is the only part of the graph shared across threads
//! (SPSC queues + the publish/swap/retire atomics). A
//! [`GraphControlHandle`] clones it out for use from the engine's control
//! thread while the audio thread owns the [`DspGraph`]; in single-threaded
//! use the `DspGraph` control methods (below) delegate to the same handle.

use super::swap::NodeId;
use super::*;
use crate::buffer::{PcmRingBuffer, MAX_CHANNELS};
use crate::dsp::{
    correction::CorrectionIrSet,
    crossfade::MixerState,
    equalizer::EqBandParams,
    limiter::LimiterMode,
    loudness::{LoudnessMetadata, LoudnessMode},
};
use std::sync::atomic::{
    AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::{Arc, Mutex};

/// Depth of each per-node control queue. Bounded, so the block-boundary
/// drain is O(QUEUE_CAPACITY) per queue — deterministic and budgetable.
const CONTROL_QUEUE_CAPACITY: usize = 64;

/// Depth of the retire-hand-back overflow stack. Four reconfigurations inside
/// one crossfade window is already pathological; eight slots is ample headroom
/// before anything is dropped.
const RETIRE_OVERFLOW_SLOTS: usize = 8;

/// One parameter / transport command carried by the control queues.
///
/// Plain data only (`Copy`, no heap, no `Vec`/`String`): heap-bearing
/// operations belong to the generation-swap path, never to a queue.
/// Variants mirror the symmetric control surface one-to-one.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum NodeCmd {
    // ── Shell (transport / top-level) ─────────────────────────────────────
    SetBitPerfect(bool),
    SetDoPBypass(bool),
    SetSpeed(f32),
    SetNonFinitePolicy(crate::dsp::safety::NonFinitePolicy),

    // ── Volume / balance ──────────────────────────────────────────────────
    SetVolumeTarget(f32),
    SetBalance(f32),

    // ── Seek / transition fades ───────────────────────────────────────────
    BeginSeekFadeout,
    BeginSeekFadein,

    // ── Mix bus (per-input chains + transitions) ──────────────────────────
    MixInput {
        input: u8,
        cmd: MixInputCmd,
    },
    MixTransition(MixTransitionCmd),
    /// Program-gated ducking config; `None` disables.
    SetDuck(Option<DuckState>),
    /// Aux bus config: enabled + return gain.
    SetAux {
        enabled: bool,
        return_gain: f32,
    },
    /// Aux insert: runtime toggle of the global convolution
    /// (enabled / wet only; the IR stays as configured).
    SetAuxInsert {
        enabled: bool,
        wet_mix: f32,
    },
    /// Room/headphone correction: live enabled toggle (the loaded IR stays).
    SetCorrectionEnabled(bool),
    /// Room/headphone correction: live wet/dry depth in [0, 1].
    SetCorrectionDepth(f32),
    /// Spatial master: live enable toggle.
    SetSpatialEnabled(bool),
    /// Spatial master: virtual-screen geometry
    /// `(center_azimuth_deg, half_width_deg, elevation_deg, gain)`.
    SetSpatialScreen {
        center_azimuth_deg: f32,
        half_width_deg: f32,
        elevation_deg: f32,
        gain: f32,
    },
    /// Spatial master: room
    /// `(enabled, width, depth, height, absorption, reflection_order,
    /// rt60_ms, late_mix, late_distance, wet)`.
    SetSpatialRoom {
        enabled: bool,
        width: f32,
        depth: f32,
        height: f32,
        absorption: f32,
        reflection_order: u8,
        rt60_ms: f32,
        late_mix: f32,
        late_distance: bool,
        wet: f32,
    },
    /// Acoustic agreement: the scene-wide air-absorption model
    /// applied to live room reflections (the model object carries
    /// `enabled`, `per_meter`, `base_cutoff_hz`, `rolloff_model`).
    SetSpatialAir(crate::spatial::level::AirAbsorption),
    /// Spatial master: listener orientation
    /// `(yaw_deg, pitch_deg, roll_deg)`.
    SetSpatialListener {
        yaw_deg: f32,
        pitch_deg: f32,
        roll_deg: f32,
    },
    /// Listener motion: target listener pose (world-space
    /// orientation + position) the node glides toward per block.
    SetSpatialListenerPose {
        /// Quaternion bit pattern `(x, y, z, w)` — `Quat` is plain data
        /// but not `Copy`-friendly for the queue, so it rides as bits.
        qx: f32,
        qy: f32,
        qz: f32,
        qw: f32,
        px: f32,
        py: f32,
        pz: f32,
    },
    /// Listener motion: smoothing / rate-limit policy for the
    /// listener glide.
    SetSpatialListenerTracking {
        smoothing_ms: f32,
        max_angular_rate_deg_s: f32,
    },
    /// Scene animation (v4.4.0): fire the cue at `cue_index`
    /// (resolved from the bank by name on the control side) at the
    /// current cue clock.
    TriggerSpatialCue(usize),
    /// Scene animation: stop the active cue on `target`
    /// (program object 0 = L, 1 = R).
    StopSpatialCue(usize),
    /// Scene animation: stop every active cue.
    StopAllSpatialCues,
    /// Runtime crossfade config: curve / enabled / duration
    /// mirror the pipeline's `TrackMixer` setters the engine calls on
    /// `handle_set_crossfade_config`.
    SetMixCurve(config::CrossfadeCurve),
    SetMixEnabled(bool),
    SetMixDurationFrames(usize),

    // ── Limiter ───────────────────────────────────────────────────────────
    SetLimiterEnabled(bool),
    SetLimiterMode(LimiterMode),
    SetLimiterParams {
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    },
    SetLimiterTruePeak(bool),

    // ── EQ ────────────────────────────────────────────────────────────────
    SetEqEnabled(bool),
    SetEqAutoHeadroom(bool),
    SetEqPreampDb(f32),
    SetEqBassShelf(f32),
    SetEqTrebleShelf(f32),
    SetEqBand {
        index: usize,
        params: EqBandParams,
    },
    SetMidsideEq(bool),
    SetDynamicEqEnabled(bool),
    SetDynamicEqBand {
        index: usize,
        params: crate::dsp::equalizer::DynamicEqBandParams,
    },

    // ── Convolution ───────────────────────────────────────────────────────
    SetConvolutionWetMix(f32),

    // ── Stereo image ──────────────────────────────────────────────────────
    SetStereoWidth(f32),
    SetStereoEnhancerEnabled(bool),

    // ── Crossfeed ─────────────────────────────────────────────────────────
    SetCrossfeedEnabled(bool),
    SetCrossfeedProfile(config::CrossfeedProfile),
    SetCrossfeedCustom(f32, f32, f32),

    // ── Creative effects ──────────────────────────────────────────────────
    /// The whole creative-effects insert: chorus, flanger, phaser, delay and
    /// saturation, in one command so the settings can never disagree.
    SetFxConfig(config::FxConfig),

    // ── Multiband compressor ──────────────────────────────────────────────
    SetCompressorEnabled(bool),
    SetCompressorBand {
        band: usize,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    },
    SetCompressorBandFeatures {
        band: usize,
        knee_db: f32,
        detector: config::CompressorDetector,
        stereo_link: bool,
    },

    // ── Plugin host ────────────────────────────────────────────
    /// Live plugin host enable toggle (all slots).
    SetPluginEnabled(bool),
    /// Live plugin parameter batch (plain data — allocation-free over
    /// the SPSC queue).
    SetPluginParams(plugin_abi::PluginParams),
    /// Live CLAP plugin enable toggle.
    SetClapPluginEnabled(bool),
    /// Live CLAP plugin parameter change (id, value).
    SetClapPluginParam {
        id: u32,
        value: f64,
    },
}

impl Default for NodeCmd {
    /// Placeholder used to fill queue storage; never observed unless a
    /// producer pushes garbage (it cannot).
    fn default() -> Self {
        NodeCmd::SetVolumeTarget(1.0)
    }
}

/// The cross-thread control plane: per-node SPSC queues, the
/// publish/swap/retire atomics, and the sticky user-state snapshot.
///
/// Producer (control) writes queues + `has_pending`; consumer (audio) drains
/// them and, on drain, mirrors user-state (volume / balance / speed) onto the
/// sticky atomics so a later generation build can inherit it.
pub(crate) struct ControlBus {
    /// One SPSC ring per node slot (canonical `NodeId`s), plus the shell slot
    /// ([`NodeId::SHELL`]).
    queues: Vec<PcmRingBuffer<NodeCmd>>,
    /// Set on any enqueue; the audio side swaps it false at the block
    /// boundary so a quiet block costs one relaxed load, not 18.
    has_pending: AtomicBool,
    /// Control → audio: a fully built generation awaiting the next swap.
    pending: AtomicPtr<GraphGeneration>,
    /// Audio → control: the generation swapped out, awaiting reclamation.
    retired: AtomicPtr<GraphGeneration>,
    /// Overflow slots for retire hand-backs that arrive before the control
    /// thread has drained [`Self::retired`].
    ///
    /// The audio thread must never free a generation, so a collision on
    /// `retired` cannot be resolved in place. These slots give it somewhere to
    /// put the displaced pointer until `reclaim_retired` (control side) drains
    /// them. Bounded: a fixed array, indexed by an atomic cursor. Overflow past
    /// the bound is counted and leaked deliberately rather than freed from the
    /// audio thread — that is the one outcome this contract forbids.
    retired_overflow: [AtomicPtr<GraphGeneration>; RETIRE_OVERFLOW_SLOTS],
    /// Cursor into `retired_overflow`.
    retired_overflow_len: AtomicUsize,
    /// How many retire hand-backs could not be parked (i.e. were leaked).
    retired_overflow_dropped: AtomicU64,
    /// Monotonic generation counter (audio-incremented on swap).
    swap_seq: AtomicU64,
    /// Generations reclaimed by the control side.
    reclaimed: AtomicU64,
    /// Commands dropped on a full queue.
    dropped: AtomicU64,
    /// Wall-clock nanoseconds spent building the most recent generation.
    ///
    /// Published control-side after a rebuild, read by a host that wants to
    /// know what a reconfiguration actually costs. A generation build is the
    /// one operation in the engine that allocates megabytes (mix planes, the
    /// node arena, the plan set, scratch), so a host that rebuilds on a slider
    /// drag pays this cost per rebuild — measured at tens of milliseconds on
    /// the reference machine, against a block deadline of ~2.7 ms at 48 kHz /
    /// 512 frames. That gap is the whole reason this counter exists: the spike
    /// is invisible in steady-state CPU telemetry and only shows up here.
    last_build_nanos: AtomicU64,
    /// Cumulative nanoseconds across every generation build.
    total_build_nanos: AtomicU64,
    /// Number of generation builds.
    build_count: AtomicU64,
    /// Sticky user state (audio-written at drain, control-read at build).
    user_volume: AtomicU32,
    user_balance: AtomicU32,
    user_speed: AtomicU32,
    user_fade_ms: AtomicU32,
    /// Per-slot user state: one entry per possible mix-bus slot,
    /// sized to [`MAX_MIX_SLOTS`]. The audio side mirrors each slot's
    /// gain / balance / mute / active at drain; the control side seeds a
    /// fresh generation from them so a reconfig never snaps lane settings.
    user_slot_gain: Vec<AtomicU32>,
    user_slot_balance: Vec<AtomicU32>,
    user_slot_pan: Vec<AtomicU32>,
    user_slot_mute: Vec<AtomicU8>,
    user_slot_active: Vec<AtomicU8>,
    /// Per-slot metering: peak / RMS dBFS, audio-written once
    /// per block, control-read for telemetry.
    user_slot_peak_db: Vec<AtomicU32>,
    user_slot_rms_db: Vec<AtomicU32>,
    /// Per-slot send levels: packed `master << 32 | aux` bit
    /// patterns, mirrored like gain/pan so sends survive a generation swap.
    user_slot_send: Vec<AtomicU64>,
    /// Per-slot per-channel trim gains: one `AtomicU32` (bit
    /// pattern) per channel, plus a packed invert bitmask.
    user_slot_trim_gain: Vec<[AtomicU32; MAX_CHANNELS]>,
    user_slot_trim_invert: Vec<AtomicU32>,
    /// Aux bus user state: enabled flag + return gain,
    /// mirrored like the per-slot state.
    user_aux_enabled: AtomicU8,
    user_aux_return_gain: AtomicU32,
    /// Aux insert: mirrored enabled / wet-mix so a generation swap
    /// preserves a live runtime toggle (same contract as `user_aux_*`).
    user_aux_insert_enabled: AtomicU8,
    user_aux_insert_wet_mix: AtomicU32,
    /// Aux metering: peak / RMS dBFS of the accumulated sends.
    user_aux_peak_db: AtomicU32,
    user_aux_rms_db: AtomicU32,
    /// Per-send aux peaks (dBFS), one per mix slot — independent
    /// metering for the aux bus's per-slot automation.
    user_aux_send_peak_db: [AtomicU32; MAX_MIX_SLOTS],
    /// Room/headphone correction: live correction enabled / depth, mirrored like the aux
    /// state so a runtime toggle survives a generation swap.
    user_correction_enabled: AtomicU8,
    user_correction_depth: AtomicU32,
    /// Room/headphone correction: the rendered correction IR set. Written ONLY on the
    /// control thread (the `LoadCorrectionIr` / `MeasureRoom` handler and
    /// generation seeding); the audio thread never touches it, so the mutex
    /// is never contended on a hot path — it exists only to keep the bus
    /// `Sync` (the audio side touches other fields of the same struct).
    user_correction_ir: Mutex<Option<Arc<CorrectionIrSet>>>,
    /// Spatial master enabled flag, mirrored like the aux state so
    /// a live runtime toggle survives a generation swap.
    user_spatial_enabled: AtomicU8,
    /// The spatial master's cue bank (serde model), mirrored
    /// like `user_correction_ir` — control-thread writes only (the
    /// `SetSpatialCues` handler + generation seeding), never touched by
    /// the audio thread, so the mutex is never contended on a hot path.
    /// `None` = no live bank (the config's `cues` are authoritative).
    user_cue_bank: Mutex<Option<Arc<Vec<config::SpatialCueConfig>>>>,
    /// The per-target active-cue indices (one per program
    /// object slot), mirrored as plain atomics so a live trigger survives
    /// a generation swap. `u32::MAX` = idle.
    user_active_cues: [AtomicU32; crate::spatial::cue::MAX_ACTIVE_CUES],
    /// Plugin host enabled flag.
    user_plugin_enabled: AtomicU8,
    /// The last live plugin parameter batch, mirrored across the thread
    /// boundary lock-free. See [`PluginParamsSlot`].
    user_plugin_params: PluginParamsSlot,
    /// The creative-effects settings, mirrored so a generation rebuild
    /// inherits them. The control path only: the audio thread reads the
    /// node's own copy.
    user_fx_config: Mutex<config::FxConfig>,
}

/// Lock-free mirror of the live plugin parameter batch.
///
/// The batch is written by the AUDIO THREAD, at the block-boundary
/// `NodeCmd::SetPluginParams` drain in `drain_control`, and read by the
/// control thread when it takes a `snapshot()` for a generation build. It
/// therefore cannot be a `Mutex`: locking on a hot path is exactly what this
/// crate's realtime contract forbids, and `.expect()` on a poisoned mutex
/// would turn one control-thread panic into a panic on every subsequent
/// block. (The field doc previously claimed the write was control-side only;
/// that was wrong and the mutex was genuinely on the audio path.)
///
/// ── Why this is not a seqlock over plain data ────────────────────────────
///
/// It was, and it was undefined behaviour. The payload used to live in an
/// `UnsafeCell<PluginParams>` and both sides copied it non-atomically (a
/// ~516-byte `memcpy` racing a `memcpy`). A non-atomic read racing a
/// non-atomic write *is* the data race — discarding the torn result afterwards
/// does not undo it, and LLVM may hoist or widen the copy out of the `seq`
/// window entirely, which is exactly what the version check existed to
/// prevent. The `# Safety` block claimed "no reader can be copying at the
/// same time"; that was a non-sequentially-consistent assumption, not a
/// property the code had. Real seqlocks are only sound over atomic or
/// immutable payloads.
///
/// So the payload is atomic *at the word level*. `PluginParams` is a
/// fixed-capacity array of `{index: u32, value: f32}` plus a `len`, so it
/// round-trips exactly through `[AtomicU32; …]` — `value.to_bits()` and back
/// are bit-preserving, including NaN payloads and signed zero. Two buffers are
/// published alternately so that a reader is never reading the buffer being
/// written; the `seq` re-check still discards a torn *snapshot*, but now a
/// torn snapshot is merely stale data rather than undefined behaviour.
///
/// Allocation-free on both sides, which is the property the seqlock was
/// reaching for.
struct PluginParamsSlot {
    /// Even = quiescent, odd = write in progress. Monotonic; never wraps in
    /// practice and correctness does not depend on it not wrapping.
    seq: AtomicU32,
    /// Which of the two buffers a reader must use.
    published: AtomicU32,
    /// `[index, value.to_bits()]` per slot, plus the trailing length word.
    buf: [[AtomicU32; PAYLOAD_WORDS]; 2],
}

/// `MAX_PLUGIN_PARAMS` slots × 2 words each, plus one word for `len`.
const PAYLOAD_WORDS: usize = plugin_abi::MAX_PLUGIN_PARAMS * 2 + 1;
/// Byte offset of the `len` word inside a payload buffer.
const LEN_WORD: usize = PAYLOAD_WORDS - 1;

impl PluginParamsSlot {
    fn new() -> Self {
        Self {
            seq: AtomicU32::new(0),
            published: AtomicU32::new(0),
            // `AtomicU32` is not `Copy`, so this cannot be an array-repeat expression.
            buf: std::array::from_fn(|_| std::array::from_fn(|_| AtomicU32::new(0))),
        }
    }

    /// Publish a new batch. Audio thread only (single writer).
    fn store(&self, batch: plugin_abi::PluginParams) {
        use std::sync::atomic::Ordering;
        // Write the buffer that is NOT currently published, so a reader
        // copying the published one never races these stores.
        let slot = 1 - (self.published.load(Ordering::Relaxed) & 1);
        let dst = &self.buf[slot as usize];
        // Odd while in flight: a reader that observes this retries rather than
        // believing it read a settled buffer.
        self.seq.fetch_add(1, Ordering::AcqRel);
        for (i, v) in batch.iter().enumerate().take(plugin_abi::MAX_PLUGIN_PARAMS) {
            dst[i * 2].store(v.index, Ordering::Relaxed);
            dst[i * 2 + 1].store(v.value.to_bits(), Ordering::Relaxed);
        }
        dst[LEN_WORD].store(batch.len() as u32, Ordering::Release);
        // Release: the payload stores above must be visible before the buffer
        // is advertised as the one to read.
        self.published.store(slot, Ordering::Release);
        self.seq.fetch_add(1, Ordering::Release);
    }

    /// Read the last published batch. Control side.
    fn load(&self) -> plugin_abi::PluginParams {
        use std::sync::atomic::Ordering;
        let mut out = plugin_abi::PluginParams::empty();
        // Bounded retries: a writer preempted mid-publish can spin this, but the
        // window is a few dozen word stores, so it is microseconds at worst.
        // The fallback is safe rather than merely stale: every read below is
        // atomic, so the worst case is a mixed-generation batch.
        for attempt in 0..8 {
            let before = self.seq.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let slot = (self.published.load(Ordering::Acquire) & 1) as usize;
            let src = &self.buf[slot];
            let mut scratch = [0.0f32; plugin_abi::MAX_PLUGIN_PARAMS];
            let mut indices = [0u32; plugin_abi::MAX_PLUGIN_PARAMS];
            for i in 0..plugin_abi::MAX_PLUGIN_PARAMS {
                indices[i] = src[i * 2].load(Ordering::Relaxed);
                scratch[i] = f32::from_bits(src[i * 2 + 1].load(Ordering::Relaxed));
            }
            let len =
                (src[LEN_WORD].load(Ordering::Acquire) as usize).min(plugin_abi::MAX_PLUGIN_PARAMS);
            if self.seq.load(Ordering::Acquire) == before {
                for i in 0..len {
                    out.push(indices[i], scratch[i]);
                }
                return out;
            }
            if attempt == 7 {
                // Contended: return what we read rather than spinning forever
                // on a control path. `push` re-validates finiteness and
                // capacity, so this cannot panic.
                for i in 0..len {
                    out.push(indices[i], scratch[i]);
                }
            }
        }
        out
    }
}

impl ControlBus {
    pub(super) fn new(volume_fade_ms: f32) -> Self {
        Self {
            queues: (0..NodeId::SLOTS)
                .map(|_| PcmRingBuffer::<NodeCmd>::new(CONTROL_QUEUE_CAPACITY))
                .collect(),
            has_pending: AtomicBool::new(false),
            pending: AtomicPtr::new(std::ptr::null_mut()),
            retired: AtomicPtr::new(std::ptr::null_mut()),
            retired_overflow: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            retired_overflow_len: AtomicUsize::new(0),
            retired_overflow_dropped: AtomicU64::new(0),
            swap_seq: AtomicU64::new(0),
            reclaimed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            last_build_nanos: AtomicU64::new(0),
            total_build_nanos: AtomicU64::new(0),
            build_count: AtomicU64::new(0),
            user_volume: AtomicU32::new(1.0f32.to_bits()),
            user_balance: AtomicU32::new(0.0f32.to_bits()),
            user_speed: AtomicU32::new(1.0f32.to_bits()),
            user_fade_ms: AtomicU32::new(volume_fade_ms.to_bits()),
            user_slot_gain: (0..MAX_MIX_SLOTS)
                .map(|_| AtomicU32::new(1.0f32.to_bits()))
                .collect(),
            user_slot_balance: (0..MAX_MIX_SLOTS)
                .map(|_| AtomicU32::new(0.0f32.to_bits()))
                .collect(),
            user_slot_pan: (0..MAX_MIX_SLOTS)
                .map(|_| AtomicU32::new(0.0f32.to_bits()))
                .collect(),
            user_slot_mute: (0..MAX_MIX_SLOTS).map(|_| AtomicU8::new(0)).collect(),
            user_slot_active: (0..MAX_MIX_SLOTS).map(|_| AtomicU8::new(1)).collect(),
            user_slot_peak_db: (0..MAX_MIX_SLOTS)
                .map(|_| AtomicU32::new((-96.0f32).to_bits()))
                .collect(),
            user_slot_rms_db: (0..MAX_MIX_SLOTS)
                .map(|_| AtomicU32::new((-96.0f32).to_bits()))
                .collect(),
            user_slot_send: (0..MAX_MIX_SLOTS)
                .map(|_| {
                    AtomicU64::new(((1.0f32.to_bits() as u64) << 32) | 0.0f32.to_bits() as u64)
                })
                .collect(),
            user_slot_trim_gain: (0..MAX_MIX_SLOTS)
                .map(|_| std::array::from_fn(|_| AtomicU32::new(1.0f32.to_bits())))
                .collect(),
            user_slot_trim_invert: (0..MAX_MIX_SLOTS).map(|_| AtomicU32::new(0)).collect(),
            user_aux_enabled: AtomicU8::new(0),
            user_aux_return_gain: AtomicU32::new(1.0f32.to_bits()),
            user_aux_insert_enabled: AtomicU8::new(0),
            user_aux_insert_wet_mix: AtomicU32::new(0.5f32.to_bits()),
            user_aux_peak_db: AtomicU32::new((-96.0f32).to_bits()),
            user_aux_rms_db: AtomicU32::new((-96.0f32).to_bits()),
            user_aux_send_peak_db: [const { AtomicU32::new((-96.0f32).to_bits()) }; MAX_MIX_SLOTS],
            user_correction_enabled: AtomicU8::new(0),
            user_correction_depth: AtomicU32::new(1.0f32.to_bits()),
            user_correction_ir: Mutex::new(None),
            user_spatial_enabled: AtomicU8::new(0),
            user_cue_bank: Mutex::new(None),
            user_active_cues: {
                // `AtomicU32::new` is not `const`-array-repeat-friendly for
                // MSRV; spell the fixed slots out (MAX_ACTIVE_CUES = 8).
                let idle = || AtomicU32::new(u32::MAX);
                let _ = idle;
                [
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                    AtomicU32::new(u32::MAX),
                ]
            },
            user_plugin_enabled: AtomicU8::new(1),
            user_plugin_params: PluginParamsSlot::new(),
            user_fx_config: Mutex::new(config::FxConfig::default()),
        }
    }

    /// Enqueue a command into one queue slot (control path). Drops (and
    /// counts) on a full queue, mirroring the engine's fire-and-forget
    /// `send_command` posture.
    #[inline]
    fn enqueue(&self, slot: usize, cmd: NodeCmd) {
        if self.queues[slot].free_slots() == 0 {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.queues[slot].push_block(std::slice::from_ref(&cmd));
        self.has_pending.store(true, Ordering::Release);
    }

    /// Control-side sticky user-state reads (generation seeding).
    pub(super) fn user_volume(&self) -> f32 {
        f32::from_bits(self.user_volume.load(Ordering::Relaxed))
    }
    pub(super) fn user_balance(&self) -> f32 {
        f32::from_bits(self.user_balance.load(Ordering::Relaxed))
    }
    pub(super) fn user_speed(&self) -> f32 {
        f32::from_bits(self.user_speed.load(Ordering::Relaxed))
    }
    pub(super) fn user_fade_ms(&self) -> f32 {
        f32::from_bits(self.user_fade_ms.load(Ordering::Relaxed))
    }

    /// Update the sticky volume-ramp duration (audio side: called by the
    /// direct `set_volume_fade_ms` setter so future generation builds honor
    /// it).
    pub(super) fn set_user_fade_ms(&self, ms: f32) {
        self.user_fade_ms.store(ms.to_bits(), Ordering::Relaxed);
    }

    /// Publish one slot's metering to the sticky per-slot atomics (audio
    /// side, once per block). Out-of-range slots are ignored.
    pub(super) fn publish_slot_meters(&self, slot: usize, peak_db: f32, rms_db: f32) {
        if let Some(p) = self.user_slot_peak_db.get(slot) {
            p.store(peak_db.to_bits(), Ordering::Relaxed);
        }
        if let Some(r) = self.user_slot_rms_db.get(slot) {
            r.store(rms_db.to_bits(), Ordering::Relaxed);
        }
    }

    /// Control-side read of one slot's metering (peak / RMS dBFS).
    pub(super) fn slot_meters(&self, slot: usize) -> (f32, f32) {
        let peak = self
            .user_slot_peak_db
            .get(slot)
            .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
            .unwrap_or(-96.0);
        let rms = self
            .user_slot_rms_db
            .get(slot)
            .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
            .unwrap_or(-96.0);
        (peak, rms)
    }

    /// Mirror one slot's user state onto the sticky per-slot atomics
    /// (audio side, at drain). The post-apply `MixInput` is the source of
    /// truth (dB/ramped commands land as targets); out-of-range slots are
    /// ignored.
    pub(super) fn set_slot_user_state(&self, slot: usize, input: &super::nodes::MixInput) {
        if let Some(g) = self.user_slot_gain.get(slot) {
            g.store(input.gain.target_gain.to_bits(), Ordering::Relaxed);
        }
        if let Some(b) = self.user_slot_balance.get(slot) {
            b.store(input.balance.to_bits(), Ordering::Relaxed);
        }
        if let Some(p) = self.user_slot_pan.get(slot) {
            p.store(input.pan.to_bits(), Ordering::Relaxed);
        }
        if let Some(m) = self.user_slot_mute.get(slot) {
            m.store(input.mute as u8, Ordering::Relaxed);
        }
        if let Some(a) = self.user_slot_active.get(slot) {
            a.store(input.active as u8, Ordering::Relaxed);
        }
        if let Some(s) = self.user_slot_send.get(slot) {
            let packed = ((input.send.master_gain.clamp(0.0, 1.0).to_bits() as u64) << 32)
                | input.send.aux_gain.clamp(0.0, 1.0).to_bits() as u64;
            s.store(packed, Ordering::Relaxed);
        }
        if let Some(gains) = self.user_slot_trim_gain.get(slot) {
            let mut invert_mask = 0u32;
            for (c, g) in gains.iter().enumerate().take(MAX_CHANNELS) {
                let gain = input.trim.gains.get(c).copied().unwrap_or(1.0);
                g.store(gain.to_bits(), Ordering::Relaxed);
                if input.trim.invert.get(c).copied().unwrap_or(false) {
                    invert_mask |= 1 << c;
                }
            }
            if let Some(iv) = self.user_slot_trim_invert.get(slot) {
                iv.store(invert_mask, Ordering::Relaxed);
            }
        }
    }

    /// Mirror the aux bus state, audio side at drain.
    pub(super) fn set_aux_user_state(&self, enabled: bool, return_gain: f32) {
        self.user_aux_enabled
            .store(enabled as u8, Ordering::Relaxed);
        self.user_aux_return_gain
            .store(return_gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Mirror the Aux insert state, audio side at drain.
    pub(super) fn set_aux_insert_user_state(&self, enabled: bool, wet_mix: f32) {
        self.user_aux_insert_enabled
            .store(enabled as u8, Ordering::Relaxed);
        self.user_aux_insert_wet_mix
            .store(wet_mix.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Control-side read of the mirrored aux insert state.
    pub(super) fn user_aux_insert(&self) -> (bool, f32) {
        (
            self.user_aux_insert_enabled.load(Ordering::Relaxed) != 0,
            f32::from_bits(self.user_aux_insert_wet_mix.load(Ordering::Relaxed)),
        )
    }

    /// Control-side read of the mirrored aux state.
    pub(super) fn user_aux(&self) -> (bool, f32) {
        (
            self.user_aux_enabled.load(Ordering::Relaxed) != 0,
            f32::from_bits(self.user_aux_return_gain.load(Ordering::Relaxed)),
        )
    }

    /// Mirror the Room/headphone correction toggle / depth, audio side at drain.
    pub(super) fn set_correction_user_state(&self, enabled: bool, depth: f32) {
        self.user_correction_enabled
            .store(enabled as u8, Ordering::Relaxed);
        self.user_correction_depth
            .store(depth.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Control-side read of the mirrored correction toggle / depth.
    pub(super) fn user_correction(&self) -> (bool, f32) {
        (
            self.user_correction_enabled.load(Ordering::Relaxed) != 0,
            f32::from_bits(self.user_correction_depth.load(Ordering::Relaxed)),
        )
    }

    /// Mirror the Spatial master enable flag, audio side at drain.
    pub(super) fn set_spatial_user_state(&self, enabled: bool) {
        self.user_spatial_enabled
            .store(enabled as u8, Ordering::Relaxed);
    }

    /// Control-side read of the mirrored spatial master enable flag.
    pub(super) fn user_spatial(&self) -> bool {
        self.user_spatial_enabled.load(Ordering::Relaxed) != 0
    }

    /// Mirror the cue bank (control side: the `SetSpatialCues`
    /// handler and `set_cue_bank` write; generation seeding reads back).
    /// The audio thread never touches this — see the field's docs.
    pub(super) fn set_cue_bank_user_state(&self, cues: Option<Arc<Vec<config::SpatialCueConfig>>>) {
        let mut guard = self.user_cue_bank.lock().expect("cue bank mutex poisoned");
        *guard = cues;
    }

    /// Control-side read of the mirrored cue bank (if any).
    pub(super) fn user_cue_bank(&self) -> Option<Arc<Vec<config::SpatialCueConfig>>> {
        self.user_cue_bank
            .lock()
            .expect("cue bank mutex poisoned")
            .clone()
    }

    /// Mirror the per-target active-cue indices (audio side at drain; the
    /// values are the last-applied trigger per target).
    pub(super) fn set_active_cue_user_state(&self, target: usize, cue_index: Option<usize>) {
        if target >= crate::spatial::cue::MAX_ACTIVE_CUES {
            return;
        }
        let v = cue_index.map(|i| i as u32).unwrap_or(u32::MAX);
        self.user_active_cues[target].store(v, Ordering::Relaxed);
    }

    /// Control-side read of the mirrored active-cue index per target.
    pub(super) fn user_active_cue(&self, target: usize) -> Option<usize> {
        if target >= crate::spatial::cue::MAX_ACTIVE_CUES {
            return None;
        }
        let v = self.user_active_cues[target].load(Ordering::Relaxed);
        (v != u32::MAX).then_some(v as usize)
    }

    /// Mirror the Plugin host enable flag, audio side at drain.
    pub(super) fn set_plugin_user_state(&self, enabled: bool) {
        self.user_plugin_enabled
            .store(enabled as u8, Ordering::Relaxed);
    }

    /// Control-side read of the mirrored plugin host enable flag.
    pub(super) fn user_plugin(&self) -> bool {
        self.user_plugin_enabled.load(Ordering::Relaxed) != 0
    }

    /// Mirror the live plugin parameter batch, audio side at
    /// drain (a plain-data copy — allocation-free, and now also lock-free).
    pub(super) fn set_plugin_params_user_state(&self, batch: plugin_abi::PluginParams) {
        self.user_plugin_params.store(batch);
    }

    /// Control-side read of the mirrored live parameter batch.
    pub(super) fn user_plugin_params(&self) -> Option<plugin_abi::PluginParams> {
        let batch = self.user_plugin_params.load();
        if batch.is_empty() {
            None
        } else {
            Some(batch)
        }
    }

    /// Store the rendered correction IR set (control side: `LoadCorrectionIr`
    /// / `MeasureRoom` land, and generation seeding reads it back). The
    /// audio thread never reads this — see the field's docs.
    pub(super) fn set_correction_ir(&self, set: Arc<CorrectionIrSet>) {
        let mut guard = self
            .user_correction_ir
            .lock()
            .expect("correction IR mutex poisoned");
        *guard = Some(set);
    }

    /// Control-side read of the stored correction IR set (if any).
    pub(super) fn user_correction_ir(&self) -> Option<Arc<CorrectionIrSet>> {
        self.user_correction_ir
            .lock()
            .expect("correction IR mutex poisoned")
            .clone()
    }

    /// Publish the aux meters, audio side once per block.
    pub(super) fn publish_aux_meters(&self, peak_db: f32, rms_db: f32) {
        self.user_aux_peak_db
            .store(peak_db.to_bits(), Ordering::Relaxed);
        self.user_aux_rms_db
            .store(rms_db.to_bits(), Ordering::Relaxed);
    }

    /// Control-side read of the aux meters.
    pub(super) fn aux_meters(&self) -> (f32, f32) {
        (
            f32::from_bits(self.user_aux_peak_db.load(Ordering::Relaxed)),
            f32::from_bits(self.user_aux_rms_db.load(Ordering::Relaxed)),
        )
    }

    /// Publish the per-send aux peaks, audio side once per block.
    pub(super) fn publish_aux_send_peaks(&self, peaks: &[f32; MAX_MIX_SLOTS]) {
        for (slot, cell) in self.user_aux_send_peak_db.iter().enumerate() {
            cell.store(peaks[slot].to_bits(), Ordering::Relaxed);
        }
    }

    /// Control-side read of one slot's aux-send peak (dBFS).
    pub(super) fn aux_send_peak(&self, slot: usize) -> f32 {
        f32::from_bits(
            self.user_aux_send_peak_db[slot.min(MAX_MIX_SLOTS - 1)].load(Ordering::Relaxed),
        )
    }

    /// Atomic snapshot of the current user state, for seeding a generation
    /// build on the control side ([`GraphGeneration::build_with_state`]).
    pub(super) fn snapshot(&self) -> super::swap::UserState {
        use super::swap::SlotState;
        super::swap::UserState {
            volume: self.user_volume(),
            balance: self.user_balance(),
            speed: self.user_speed(),
            volume_fade_ms: self.user_fade_ms(),
            has_live_bus_state: true,
            aux_enabled: self.user_aux_enabled.load(Ordering::Relaxed) != 0,
            aux_return_gain: f32::from_bits(self.user_aux_return_gain.load(Ordering::Relaxed)),
            aux_insert_enabled: self.user_aux_insert_enabled.load(Ordering::Relaxed) != 0,
            aux_insert_wet_mix: f32::from_bits(
                self.user_aux_insert_wet_mix.load(Ordering::Relaxed),
            ),
            correction_enabled: self.user_correction_enabled.load(Ordering::Relaxed) != 0,
            correction_depth: f32::from_bits(self.user_correction_depth.load(Ordering::Relaxed)),
            correction_ir: self.user_correction_ir(),
            spatial_enabled: self.user_spatial_enabled.load(Ordering::Relaxed) != 0,
            spatial_cues: self.user_cue_bank(),
            spatial_active_cues: (0..crate::spatial::cue::MAX_ACTIVE_CUES)
                .map(|t| self.user_active_cue(t))
                .collect::<Vec<_>>()
                .try_into()
                .expect("MAX_ACTIVE_CUES entries"),
            plugin_enabled: self.user_plugin_enabled.load(Ordering::Relaxed) != 0,
            plugin_params: self.user_plugin_params(),
            fx: *self
                .user_fx_config
                .lock()
                .expect("fx config mutex poisoned"),
            // Ducking + automation are NOT mirrored onto the bus atomics:
            // `DspGraph::reconfigure` reads them from the live generation
            // instead (single-threaded control access), so snapshots keep
            // them unset.
            duck: None,
            slot_automation: Vec::new(),
            slots: (0..MAX_MIX_SLOTS)
                .map(|i| {
                    let packed = self.user_slot_send[i].load(Ordering::Relaxed);
                    let invert_mask = self.user_slot_trim_invert[i].load(Ordering::Relaxed);
                    let mut trim_gains = [1.0f32; MAX_CHANNELS];
                    let mut trim_invert = [false; MAX_CHANNELS];
                    for (c, g) in self.user_slot_trim_gain[i].iter().enumerate() {
                        trim_gains[c] = f32::from_bits(g.load(Ordering::Relaxed));
                        trim_invert[c] = invert_mask & (1 << c) != 0;
                    }
                    SlotState {
                        gain: f32::from_bits(self.user_slot_gain[i].load(Ordering::Relaxed)),
                        balance: f32::from_bits(self.user_slot_balance[i].load(Ordering::Relaxed)),
                        pan: f32::from_bits(self.user_slot_pan[i].load(Ordering::Relaxed)),
                        mute: self.user_slot_mute[i].load(Ordering::Relaxed) != 0,
                        active: self.user_slot_active[i].load(Ordering::Relaxed) != 0,
                        send_master_gain: f32::from_bits((packed >> 32) as u32),
                        send_aux_gain: f32::from_bits((packed & 0xffff_ffff) as u32),
                        trim_gains,
                        trim_invert,
                    }
                })
                .collect(),
        }
    }
}

/// The control side of the graph: enqueue commands and publish generations
/// from any thread that holds this handle (the engine's tick thread, or a
/// test's control thread). Clone is cheap (shares the `ControlBus`).
#[derive(Clone)]
pub struct GraphControlHandle {
    bus: Arc<ControlBus>,
}

// `controls` is one queue plus two ends of it. `handle.rs` is the
// control-thread read surface, `handle_controls.rs` the queued mutators,
// `drain.rs` the audio-thread drain, `apply.rs` the per-command application
// and `graph_controls.rs` the direct (non-queued) `DspGraph` surface.
mod apply;
mod drain;
mod graph_controls;
mod handle;
mod handle_controls;

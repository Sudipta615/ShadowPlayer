//! C FFI (Foreign Function Interface) for the independent audio engine.
//!
//! Exposes a stable C ABI for host applications written in C, C++, or any
//! language that can call C functions (Python ctypes, C#, Node.js FFI, etc.).
//!
//! # Design
//!
//! - **Opaque handles**: `EngineHandleFFI` is a `Box<EngineHandle>` behind an
//!   opaque pointer. Callers never see the internal layout.
//! - **No panics across FFI**: Every function returns a status code (0 = Ok)
//!   and writes the result through an output pointer.
//! - **Thread-safe**: Handles are `Clone + Send`; commands are sent through
//!   the same lock-free channel as the Rust API.
//!
//! # Lifecycle
//!
//! ```c
//! EngineHandleFFI* handle = engine_create(ENGINE_BACKEND_DEFAULT);
//! engine_open_file(handle, "/path/to/audio.flac");
//! engine_play(handle);
//! // ...
//! engine_destroy(handle);
//! ```
//!
//! # Type mapping
//!
//! | C type          | Rust         |
//! |-----------------|--------------|
//! | `float`         | `f32`        |
//! | `uint32_t`      | `u32`        |
//! | `int32_t`       | `i32`         |
//! | `const char*`   | `&CStr → &str` |
//! | `EngineHandleFFI*` | `Box<EngineHandle>` |

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};

use crate::decode::codecs::{self as codec_registry, CodecStatus, CodecSupportLevel};
use crate::{AudioEngine, EngineHandle};
use config::EngineConfig;

/// The set of handles this process currently owns, so [`engine_destroy`] can
/// be genuinely idempotent.
///
/// A raw pointer carries no way to tell "already freed" from "valid", so a
/// second `engine_destroy(h)` would be a double free. Registration is a
/// lifecycle-only operation (create/destroy), never a per-block or per-call
/// path, so the mutex here is off every audio path.
///
/// Using raw pointers as set keys is sound because a freed allocation's
/// address can be reused by a *later* `engine_create`, which registers the
/// new handle before the old address can be handed out again — and by then
/// the stale entry has already been removed by the first `engine_destroy`.
fn live_handles() -> &'static Mutex<std::collections::HashSet<usize>> {
    static LIVE: OnceLock<Mutex<std::collections::HashSet<usize>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Borrow the engine behind a raw handle, but only if this process still owns
/// it.
///
/// The entry points used to do `handle.as_ref()` directly. That caught a null
/// handle, but a handle already passed to [`engine_destroy`] — or one that
/// never came from [`engine_create`] — was dereferenced as if it were live,
/// which is undefined behaviour in the host's own process. Checking the
/// registry first turns that into the documented [`EngineStatus::InvalidHandle`]
/// (or the value-returning entry point's sentinel).
///
/// The lookup is a lifecycle-scale check: one uncontended mutex per host call,
/// never on the audio path. [`engine_destroy`] removes the entry *before*
/// freeing the box, so a handle that passes this check cannot be freed by the
/// library while the caller holds the borrow.
///
/// Two things stay the host's contract. A raw pointer carries no information
/// to check them with, so they are documented rather than enforced:
///
/// - not using a handle from another thread while it is being destroyed, and
/// - not reusing a stale pointer after its address was recycled by a later
///   [`engine_create`] (the address is live again, but for a different
///   engine — a raw handle carries no generation).
fn live_handle<'a>(handle: *const EngineHandleFFI) -> Option<&'a EngineHandleFFI> {
    if handle.is_null() {
        return None;
    }
    let live = live_handles().lock().ok()?;
    if !live.contains(&(handle as usize)) {
        return None;
    }
    // SAFETY: the address is registered as a live `Box<EngineHandleFFI>`, and
    // the entry is only removed by `engine_destroy`, which does so before
    // dropping the box — see the contract above for the two cases that remain
    // with the caller.
    Some(unsafe { &*handle })
}

/// Opaque handle to an audio engine instance.
///
/// The engine itself is moved onto a dedicated background tick thread (see
/// `EngineHandleFFI::spawn_tick_thread`); the handle only retains the
/// client-side [`EngineHandle`] plus the stop flag and thread join handle
/// needed to tear it down cleanly in [`engine_destroy`].
pub struct EngineHandleFFI {
    handle: EngineHandle,
    stop: Arc<AtomicBool>,
    tick_thread: Option<std::thread::JoinHandle<()>>,
}

impl EngineHandleFFI {
    /// Spawn the background thread that drives [`AudioEngine::tick_blocking`]
    /// for the lifetime of this handle. The engine is moved into the thread;
    /// dropping it there runs `AudioEngine::stop()` after the stop flag is
    /// set, so resources are released deterministically in `engine_destroy`.
    fn spawn_tick_thread(
        engine: AudioEngine,
    ) -> (Arc<AtomicBool>, Option<std::thread::JoinHandle<()>>) {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::Builder::new()
            .name("ffi-engine-tick".into())
            .spawn(move || {
                let mut engine = engine;
                while !thread_stop.load(Ordering::Relaxed) {
                    // Wake instantly on commands; otherwise tick at most
                    // every 5 ms so playback stays smooth while idle CPU
                    // stays near zero.
                    engine.tick_blocking(std::time::Duration::from_millis(5));
                }
                engine.stop();
            })
            .ok();
        (stop, handle)
    }
}

/// Status codes returned by FFI functions.
#[repr(i32)]
pub enum EngineStatus {
    Ok = 0,
    Error = -1,
    InvalidHandle = -2,
    InvalidArgument = -3,
    EngineNotRunning = -4,
    /// A well-formed query has no answer for this value — e.g. an extension
    /// the codec registry does not carry. Distinct from `Error` so a host can
    /// tell "no such codec" from "the query itself failed".
    NotFound = -5,
}

/// Run an FFI body so a Rust panic cannot unwind into the host.
///
/// A panic that escapes an `extern "C"` function is undefined behaviour by the
/// ABI contract, and since Rust 1.71 it aborts the process. That is a poor
/// failure mode for a *library*: one malformed host argument, one poisoned
/// mutex, or one arithmetic precondition would take the entire host
/// application down with a message aimed at nobody.
///
/// Every export in this module therefore keeps its real body in a private
/// `*_impl` function and is exposed as a thin wrapper that catches unwinding
/// and converts it into this module's ordinary error convention —
/// [`EngineStatus::Error`] for a status-returning entry point, a sentinel for
/// a value-returning one, and `NULL` for a handle factory. No export has a
/// half-applied side effect a panic could leave behind, because every one of
/// them either forwards a command to the engine or reads an atomically
/// published snapshot; both are atomic with respect to the caller.
///
/// The default panic hook still runs, so a caught panic is still reported on
/// stderr. That is deliberate: swallowing the message entirely would turn a
/// host bug into silence.
fn ffi_boundary<R>(body: impl FnOnce() -> R + std::panic::UnwindSafe, fallback: R) -> R {
    match std::panic::catch_unwind(body) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

/// ABI version of this FFI surface, as `(major << 16) | minor`.
///
/// A C host should call this before anything else and refuse to run against a
/// surface whose major version it was not built for. The major component
/// changes when an existing entry point changes meaning, a struct layout
/// changes, or an entry point is removed; the minor component changes only
/// when entry points are *added*, which is backward compatible.
///
/// `major` is 1: the surface introduced in 0.9.0. Before 0.9.0 there was no
/// way to ask, so a 0.8-or-earlier host talking to a 0.9 library had no way to
/// detect the additions — which is exactly the problem this entry point exists
/// to stop recurring.
pub const ENGINE_ABI_MAJOR: u32 = 1;
/// ABI minor version: bumped when entry points are *added*.
pub const ENGINE_ABI_MINOR: u32 = 0;
/// Packed as `(major << 16) | minor`.
pub const ENGINE_ABI_VERSION: u32 = (ENGINE_ABI_MAJOR << 16) | ENGINE_ABI_MINOR;

/// Report [`ENGINE_ABI_VERSION`].
#[no_mangle]
pub extern "C" fn engine_abi_version() -> u32 {
    ENGINE_ABI_VERSION
}

/// Report whether a host built against `host_version` can use this library.
///
/// Returns 1 when the major versions match and the library's minor version is
/// at least the host's, 0 otherwise. A host should treat 0 as "refuse to
/// start" rather than "best effort".
#[no_mangle]
pub extern "C" fn engine_abi_compatible(host_version: u32) -> i32 {
    let host_major = host_version >> 16;
    let host_minor = host_version & 0xFFFF;
    let our_major = ENGINE_ABI_VERSION >> 16;
    let our_minor = ENGINE_ABI_VERSION & 0xFFFF;
    i32::from(host_major == our_major && our_minor >= host_minor)
}

// ── Codec capability registry ──────────────────────────────────────────────

/// Status values for [`EngineCodecCapabilityFFI::status`], matching
/// [`CodecStatus`]. Plain `u32` constants for the same reason as
/// [`backend_id`]: a `#[repr(u32)]` enum parameter in an `extern "C"`
/// signature lets the C caller produce an out-of-range discriminant, which is
/// undefined behaviour before any validation can run.
pub mod codec_status_id {
    /// Fully decodable in this build.
    pub const AVAILABLE: u32 = 0;
    /// Tags are parsed; the audio stream is not decodable.
    pub const METADATA_ONLY: u32 = 1;
    /// Declared for future parity; opening such a file fails explicitly.
    pub const DECLARED_UNAVAILABLE: u32 = 2;
}

/// Support-level values for [`EngineCodecCapabilityFFI::support_level`],
/// matching [`CodecSupportLevel`].
pub mod codec_support_id {
    pub const FULLY_SUPPORTED: u32 = 0;
    pub const DECODE_ONLY: u32 = 1;
    pub const METADATA_ONLY: u32 = 2;
    pub const EXPERIMENTAL: u32 = 3;
    pub const UNAVAILABLE: u32 = 4;
}

/// One row of the codec capability registry, filled by
/// [`engine_codec_capability`].
///
/// Boolean axes are `0`/`1`. `native_precision_bits` is the native integer
/// source precision, or `-1` when there is none to report (1-bit DSD, or a
/// lossy codec that decodes to float). `name` points at a static
/// NUL-terminated string owned by the library and must not be freed by the
/// host.
///
/// The Rust-side `multichannel` alias of `multichannel_decode` is deliberately
/// not carried: it is an alias, not an independent axis.
#[repr(C)]
pub struct EngineCodecCapabilityFFI {
    /// Registry index — the value [`engine_codec_capability`] was called with.
    pub index: u32,
    /// See [`codec_status_id`].
    pub status: u32,
    /// See [`codec_support_id`].
    pub support_level: u32,
    /// Audio frames can be decoded in this build.
    pub decode: u32,
    /// Sample-accurate seeking is supported.
    pub seek: u32,
    /// Encoder delay / end padding is applied (gapless).
    pub gapless: u32,
    /// Metadata (tags) is extracted.
    pub metadata: u32,
    /// ReplayGain tags are read.
    pub replaygain: u32,
    /// Container-native chapters / embedded cue sheets are read.
    pub chapters: u32,
    /// EBU R128 tags are read.
    pub ebu_r128: u32,
    /// Progressive decode from a packet stream is possible.
    pub streaming: u32,
    /// The decoder can expose more than two source channels.
    pub multichannel_decode: u32,
    /// The codec is lossless.
    pub is_lossless: u32,
    /// Native source precision in bits, or `-1` when not applicable.
    pub native_precision_bits: i32,
    /// Static NUL-terminated canonical name (e.g. `"flac"`); never freed.
    pub name: *const c_char,
}

/// Backend selection constants for `engine_create`.
///
/// These are plain `u32` constants, NOT a Rust enum. A `#[repr(u32)]` enum
/// parameter in an `extern "C"` signature is unsound: the C caller fully
/// controls the discriminant, and `match` on an out-of-range value is
/// undefined behaviour that fires *before* any validation can run. The
/// function therefore takes a `u32` and maps it explicitly, returning
/// `NULL` for anything it does not recognise.
///
/// The numbering is shared with [`engine_upsert_endpoint`] so the two entry
/// points cannot disagree about what `1` means.
pub mod backend_id {
    /// Let the platform choose its default shared output.
    pub const AUTO: u32 = 0;
    /// Native WASAPI exclusive mode.
    pub const EXCLUSIVE_WASAPI: u32 = 1;
    /// Direct ALSA `hw:`/`plughw:` access.
    pub const EXCLUSIVE_ALSA: u32 = 2;
    /// Native CoreAudio hog mode.
    pub const EXCLUSIVE_CORE_AUDIO_HOG: u32 = 3;
    /// ASIO direct output.
    pub const EXCLUSIVE_ASIO: u32 = 4;
    /// Linux PipeWire pro-audio output. Declared for the settings format and
    /// this ABI; the engine has no implementation and the selection is
    /// refused under `FallbackPolicy::Strict` (see `config::AudioBackend`).
    pub const PIPEWIRE: u32 = 5;
    /// Linux JACK pro-audio output. Declared, not implemented — see
    /// [`PIPEWIRE`].
    pub const JACK: u32 = 6;
    /// Hardware-free sink: drains and discards, opening no OS device.
    pub const NULL: u32 = 7;
}

/// Map a `u32` backend id onto [`config::AudioBackend`], or `None` when the
/// id is not one this build knows about.
fn backend_from_id(id: u32) -> Option<config::AudioBackend> {
    use backend_id as b;
    Some(match id {
        b::AUTO => config::AudioBackend::Auto,
        b::EXCLUSIVE_WASAPI => config::AudioBackend::ExclusiveWasapi,
        b::EXCLUSIVE_ALSA => config::AudioBackend::ExclusiveAlsa,
        b::EXCLUSIVE_CORE_AUDIO_HOG => config::AudioBackend::ExclusiveCoreAudioHog,
        b::EXCLUSIVE_ASIO => config::AudioBackend::ExclusiveAsio,
        b::PIPEWIRE => config::AudioBackend::PipeWire,
        b::JACK => config::AudioBackend::Jack,
        b::NULL => config::AudioBackend::Null,
        _ => return None,
    })
}

// ── Module layout ──────────────────────────────────────────────────────────
//
// The C ABI is one namespace, but it is not one concept. Each submodule owns
// one area and re-exports its entry points here, so the public path
// `engine::ffi::engine_*` stays stable while the implementation files stay
// reviewable.

mod codec;
mod correction;
mod diagnostics;
mod playback;
mod spatial;

pub use codec::*;
pub use correction::*;
pub use diagnostics::*;
pub use playback::*;
pub use spatial::*;

// The diagnostics code tables are crate-internal helpers, not part of the ABI.
#[cfg(test)]
use diagnostics::{bit_perfect_cause_code, diagnostic_kind_code};

#[cfg(test)]
mod tests;

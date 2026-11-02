use super::*;

// ── Additional output endpoints (routing matrix) ───────────────────────────

/// Add or replace one additional output endpoint. `id` is the stable
/// identifier used by later `engine_remove_endpoint` / `engine_upsert_endpoint`
/// calls; `device` may be NULL to select the backend default; `backend` uses
/// the same constants as `EngineBackend`; `gain` in [0, 4]; `enabled` != 0
/// activates the endpoint immediately.
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_upsert_endpoint(
    handle: *mut EngineHandleFFI,
    id: *const c_char,
    device: *const c_char,
    backend: u32,
    gain: f32,
    enabled: i32,
    drift_correction: i32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_upsert_endpoint_impl(
                handle,
                id,
                device,
                backend,
                gain,
                enabled,
                drift_correction,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_upsert_endpoint_impl(
    handle: *mut EngineHandleFFI,
    id: *const c_char,
    device: *const c_char,
    backend: u32,
    gain: f32,
    enabled: i32,
    drift_correction: i32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if id.is_null() || !gain.is_finite() {
        return EngineStatus::InvalidArgument as i32;
    }
    let id_str = match unsafe { CStr::from_ptr(id) }.to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return EngineStatus::InvalidArgument as i32,
    };
    let device_str = if device.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(device) }.to_str() {
            Ok(s) if !s.is_empty() => Some(s.to_string()),
            _ => return EngineStatus::InvalidArgument as i32,
        }
    };
    let backend = match backend_from_id(backend) {
        Some(b) => b,
        None => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.set_endpoint(config::EndpointConfig {
        id: id_str,
        backend,
        device: device_str,
        gain: gain.clamp(0.0, 4.0),
        enabled: enabled != 0,
        drift_correction: drift_correction != 0,
    });
    EngineStatus::Ok as i32
}

/// Remove a configured additional output endpoint by its stable identifier.
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_remove_endpoint(handle: *mut EngineHandleFFI, id: *const c_char) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_remove_endpoint_impl(handle, id)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_remove_endpoint_impl(handle: *mut EngineHandleFFI, id: *const c_char) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if id.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let id_str = match unsafe { CStr::from_ptr(id) }.to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.remove_endpoint(id_str);
    EngineStatus::Ok as i32
}

/// Remove all configured additional output endpoints.
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_clear_endpoints(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_clear_endpoints_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_clear_endpoints_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.clear_endpoints();
    EngineStatus::Ok as i32
}

/// Number of currently configured additional output endpoints (from the
/// last telemetry refresh; -1 on invalid handle).
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_endpoint_count(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_endpoint_count_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_endpoint_count_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return -1;
    };
    h.handle.playback_info().endpoints.len() as i32
}

/// Copy the identifier of the configured endpoint at `index` into `buf`
/// (at most `buf_len` bytes, NUL-terminated). Returns the required length
/// (excluding the NUL) or a negative status code.
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_endpoint_id(
    handle: *mut EngineHandleFFI,
    index: i32,
    buf: *mut c_char,
    buf_len: usize,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_endpoint_id_impl(handle, index, buf, buf_len)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_endpoint_id_impl(
    handle: *mut EngineHandleFFI,
    index: i32,
    buf: *mut c_char,
    buf_len: usize,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if index < 0 || buf.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let endpoints = &h.handle.playback_info().endpoints;
    let id = match endpoints.get(index as usize) {
        Some(e) => e.id.as_bytes(),
        None => return EngineStatus::InvalidArgument as i32,
    };
    // The return value is the length the id WOULD need (excluding the NUL), so
    // the host can size its buffer and retry. With no room for even the
    // terminator there is nothing to write — writing `*buf.add(0)` here would
    // be a one-byte overflow of a zero-length buffer.
    if buf_len == 0 {
        return id.len() as i32;
    }
    let n = id.len().min(buf_len - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(id.as_ptr() as *const c_char, buf, n);
        *buf.add(n) = 0;
    }
    id.len() as i32
}

/// Read the telemetry for the configured endpoint at `index`: `enabled`,
/// `gain`, `written_frames`, `dropped_frames`, `available_frames`, and
/// `transport_error_count` are written when the matching out-pointer is
/// non-NULL. Returns `EngineStatus::Ok`, `InvalidHandle`, or
/// `InvalidArgument` (index out of range).
#[cfg(feature = "audio-output")]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_endpoint_info(
    handle: *mut EngineHandleFFI,
    index: i32,
    enabled: *mut i32,
    gain: *mut f32,
    written_frames: *mut u64,
    dropped_frames: *mut u64,
    available_frames: *mut usize,
    transport_error_count: *mut u64,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_endpoint_info_impl(
                handle,
                index,
                enabled,
                gain,
                written_frames,
                dropped_frames,
                available_frames,
                transport_error_count,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_endpoint_info_impl(
    handle: *mut EngineHandleFFI,
    index: i32,
    enabled: *mut i32,
    gain: *mut f32,
    written_frames: *mut u64,
    dropped_frames: *mut u64,
    available_frames: *mut usize,
    transport_error_count: *mut u64,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if index < 0 {
        return EngineStatus::InvalidArgument as i32;
    }
    let info = h.handle.playback_info();
    let ep = match info.endpoints.get(index as usize) {
        Some(e) => e,
        None => return EngineStatus::InvalidArgument as i32,
    };
    unsafe {
        if !enabled.is_null() {
            *enabled = ep.enabled as i32;
        }
        if !gain.is_null() {
            *gain = ep.gain;
        }
        if !written_frames.is_null() {
            *written_frames = ep.written_frames;
        }
        if !dropped_frames.is_null() {
            *dropped_frames = ep.dropped_frames;
        }
        if !available_frames.is_null() {
            *available_frames = ep.available_frames;
        }
        if !transport_error_count.is_null() {
            *transport_error_count = ep.transport_error_count;
        }
    }
    EngineStatus::Ok as i32
}

// ── Structured diagnostics (enum-ified categories) ──────────────────────────

/// Stable integer code for a [`crate::DiagnosticKind`] across the FFI boundary.
pub(super) fn diagnostic_kind_code(k: crate::DiagnosticKind) -> i32 {
    match k {
        crate::DiagnosticKind::Internal => 0,
        crate::DiagnosticKind::Decoder => 1,
        crate::DiagnosticKind::Output => 2,
        crate::DiagnosticKind::Resampler => 3,
        crate::DiagnosticKind::Stream => 4,
        crate::DiagnosticKind::Endpoint => 5,
        crate::DiagnosticKind::BitPerfect => 6,
        crate::DiagnosticKind::Configuration => 7,
        crate::DiagnosticKind::Spatial => 8,
        crate::DiagnosticKind::Loudness => 9,
        crate::DiagnosticKind::Dsp => 10,
        crate::DiagnosticKind::Clock => 11,
        crate::DiagnosticKind::Plugin => 12,
        crate::DiagnosticKind::Graph => 13,
        crate::DiagnosticKind::Security => 14,
    }
}

/// Stable integer code for a [`crate::BitPerfectCause`] across the FFI boundary.
pub(super) fn bit_perfect_cause_code(c: crate::BitPerfectCause) -> i32 {
    match c {
        crate::BitPerfectCause::None => 0,
        crate::BitPerfectCause::VolumeNotUnity => 1,
        crate::BitPerfectCause::EqActive => 2,
        crate::BitPerfectCause::DynamicsActive => 3,
        crate::BitPerfectCause::SpeedOrResampleActive => 4,
        crate::BitPerfectCause::SampleRateMismatch => 5,
        crate::BitPerfectCause::UnknownPrecision => 6,
        crate::BitPerfectCause::BitDepthTruncation => 7,
        crate::BitPerfectCause::FormatConversionLossy => 8,
        crate::BitPerfectCause::OutputNotDirectExclusive => 9,
        crate::BitPerfectCause::DitherActive => 10,
        crate::BitPerfectCause::CrossfadeActive => 11,
        crate::BitPerfectCause::SdmConversion => 12,
    }
}

/// Read the engine's structured diagnostics from telemetry.
///
/// Out-params (each optional — pass NULL to skip):
/// - `engine_error_kind` — [`crate::DiagnosticKind`] code for the latest
///   engine error, or `-1` when there is none.
/// - `engine_error_message` / `engine_error_message_len` — a C-string buffer
///   (at least `engine_error_message_len` bytes) that receives the error's
///   human message, NUL-terminated; empty when there is no error.
/// - `bit_perfect_cause` — [`crate::BitPerfectCause`] code (0 = none) for
///   the bit-perfect verdict's first failure.
/// - `diagnostic_count` — total number of typed diagnostics recorded.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_diagnostics_info(
    handle: *mut EngineHandleFFI,
    engine_error_kind: *mut i32,
    engine_error_message: *mut c_char,
    engine_error_message_len: usize,
    bit_perfect_cause: *mut i32,
    diagnostic_count: *mut i32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_diagnostics_info_impl(
                handle,
                engine_error_kind,
                engine_error_message,
                engine_error_message_len,
                bit_perfect_cause,
                diagnostic_count,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_diagnostics_info_impl(
    handle: *mut EngineHandleFFI,
    engine_error_kind: *mut i32,
    engine_error_message: *mut c_char,
    engine_error_message_len: usize,
    bit_perfect_cause: *mut i32,
    diagnostic_count: *mut i32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    let info = h.handle.playback_info();

    // Latest engine diagnostic (typed). `engine_error` text is preserved.
    let latest = info.engine_diagnostics.last();
    unsafe {
        if !engine_error_kind.is_null() {
            *engine_error_kind = match latest {
                Some(diag) => diagnostic_kind_code(diag.kind()),
                None => -1,
            };
        }
        if !engine_error_message.is_null() && engine_error_message_len > 0 {
            let text = info.engine_error.as_deref().unwrap_or("");
            let bytes = text.as_bytes();
            let n = bytes.len().min(engine_error_message_len.saturating_sub(1));
            std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, engine_error_message, n);
            *engine_error_message.add(n) = 0;
        }
        if !bit_perfect_cause.is_null() {
            *bit_perfect_cause = info
                .engine_stats
                .as_ref()
                .and_then(|s| s.bit_perfect_cause)
                .map(bit_perfect_cause_code)
                .unwrap_or(0);
        }
        if !diagnostic_count.is_null() {
            *diagnostic_count = info.engine_diagnostics.len() as i32;
        }
    }
    EngineStatus::Ok as i32
}

use super::*;

// ── Aux insert ───────────────────────────────────────────────────

/// Toggle the Aux insert (the global convolution on the aux bus):
/// `enabled` != 0 turns it on, `wet_mix` in [0, 1] sets the wet/dry balance.
/// The impulse response stays as configured; this is a no-op when no IR
/// engine exists yet.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_aux_insert(
    handle: *mut EngineHandleFFI,
    enabled: i32,
    wet_mix: f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_aux_insert_impl(handle, enabled, wet_mix)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_aux_insert_impl(handle: *mut EngineHandleFFI, enabled: i32, wet_mix: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if !wet_mix.is_finite() {
        return EngineStatus::InvalidArgument as i32;
    }
    h.handle.set_aux_insert(enabled != 0, wet_mix);
    EngineStatus::Ok as i32
}

/// Read the live Aux insert state. On success writes `enabled`
/// (0/1) and `wet_mix` and returns `EngineStatus::Ok`; returns
/// `EngineStatus::InvalidArgument` if either out-pointer is NULL.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_aux_insert_state(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    wet_mix: *mut f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_aux_insert_state_impl(handle, enabled, wet_mix)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_aux_insert_state_impl(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    wet_mix: *mut f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if enabled.is_null() || wet_mix.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let info = h.handle.playback_info();
    unsafe {
        *enabled = info.aux_insert_enabled as i32;
        *wet_mix = info.aux_insert_wet_mix;
    }
    EngineStatus::Ok as i32
}

// ── Room & headphone correction ───────────────────────────────

/// Live toggle of the correction stage (`enabled` != 0 turns it on; the
/// loaded IR stays). Disabled = the plan step is skipped, bit-exact.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_correction_enabled(handle: *mut EngineHandleFFI, enabled: i32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_correction_enabled_impl(handle, enabled)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_correction_enabled_impl(handle: *mut EngineHandleFFI, enabled: i32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.set_correction_enabled(enabled != 0);
    EngineStatus::Ok as i32
}

/// Set the correction wet/dry depth in [0, 1] (1.0 = fully corrected).
/// Non-finite values are rejected.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_correction_depth(handle: *mut EngineHandleFFI, depth: f32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_correction_depth_impl(handle, depth)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_correction_depth_impl(handle: *mut EngineHandleFFI, depth: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if !depth.is_finite() {
        return EngineStatus::InvalidArgument as i32;
    }
    h.handle.set_correction_depth(depth);
    EngineStatus::Ok as i32
}

/// Load a measured IR WAV and derive the correction from it (IR conditioning to derivation with
/// the configured target / boost clamp / smoothing / phase mode), then
/// enable it. `path` must be a non-empty C string; a missing or unreadable
/// file keeps the previous correction (or none) — never a failure state.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_load_correction_ir(
    handle: *mut EngineHandleFFI,
    path: *const c_char,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_load_correction_ir_impl(handle, path)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_load_correction_ir_impl(handle: *mut EngineHandleFFI, path: *const c_char) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if path.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.load_correction_ir(path_str);
    EngineStatus::Ok as i32
}

/// Read the live Room/headphone correction state. On success writes
/// `enabled` (0/1), `depth`, `ir_len_samples`, `latency_ms`,
/// `max_gain_db`, and `phase_mode` (0 = none, 1 = minimum, 2 = linear,
/// 3 = hybrid) and returns `EngineStatus::Ok`; returns
/// `EngineStatus::InvalidArgument` if any out-pointer is NULL.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_correction_info(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    depth: *mut f32,
    ir_len_samples: *mut i64,
    latency_ms: *mut f32,
    max_gain_db: *mut f32,
    phase_mode: *mut i32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_correction_info_impl(
                handle,
                enabled,
                depth,
                ir_len_samples,
                latency_ms,
                max_gain_db,
                phase_mode,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_correction_info_impl(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    depth: *mut f32,
    ir_len_samples: *mut i64,
    latency_ms: *mut f32,
    max_gain_db: *mut f32,
    phase_mode: *mut i32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if enabled.is_null()
        || depth.is_null()
        || ir_len_samples.is_null()
        || latency_ms.is_null()
        || max_gain_db.is_null()
        || phase_mode.is_null()
    {
        return EngineStatus::InvalidArgument as i32;
    }
    let info = h.handle.playback_info().correction;
    unsafe {
        *enabled = info.enabled as i32;
        *depth = info.depth;
        *ir_len_samples = info.ir_len_samples as i64;
        *latency_ms = info.latency_ms;
        *max_gain_db = info.max_gain_db;
        *phase_mode = match info.phase_mode.as_deref() {
            None => 0,
            Some("minimum") => 1,
            Some("linear") => 2,
            Some("hybrid") => 3,
            _ => 0,
        };
    }
    EngineStatus::Ok as i32
}

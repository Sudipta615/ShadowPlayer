use super::*;

/// Read the spatial master output telemetry: the binaural
/// output's left/right-ear peak & RMS (dBFS) and the live voice-budget
/// admission counts (spec §76). Mirrored from the lock-free `PlaybackInfo`
/// snapshot on the telemetry cadence.
///
/// Out-params: `enabled`, `voice_active`, `voice_full_voices`,
/// `voice_degraded_voices`, `voice_dropped_voices` (i32), and `peak_db_l`,
/// `peak_db_r`, `rms_db_l`, `rms_db_r` (f32).
///
/// Prefer [`engine_spatial_listener_pose`] for the live
/// listener pose (yaw/pitch/roll + position) — this function keeps its
/// Signature unchanged.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_spatial_info(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    voice_active: *mut i32,
    voice_full_voices: *mut i32,
    voice_degraded_voices: *mut i32,
    voice_dropped_voices: *mut i32,
    peak_db_l: *mut f32,
    peak_db_r: *mut f32,
    rms_db_l: *mut f32,
    rms_db_r: *mut f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_spatial_info_impl(
                handle,
                enabled,
                voice_active,
                voice_full_voices,
                voice_degraded_voices,
                voice_dropped_voices,
                peak_db_l,
                peak_db_r,
                rms_db_l,
                rms_db_r,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_spatial_info_impl(
    handle: *mut EngineHandleFFI,
    enabled: *mut i32,
    voice_active: *mut i32,
    voice_full_voices: *mut i32,
    voice_degraded_voices: *mut i32,
    voice_dropped_voices: *mut i32,
    peak_db_l: *mut f32,
    peak_db_r: *mut f32,
    rms_db_l: *mut f32,
    rms_db_r: *mut f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if enabled.is_null()
        || voice_active.is_null()
        || voice_full_voices.is_null()
        || voice_degraded_voices.is_null()
        || voice_dropped_voices.is_null()
        || peak_db_l.is_null()
        || peak_db_r.is_null()
        || rms_db_l.is_null()
        || rms_db_r.is_null()
    {
        return EngineStatus::InvalidArgument as i32;
    }
    let info = h.handle.playback_info().spatial;
    // A not-yet-published stage reports a coherent inactive set.
    unsafe {
        *enabled = info.as_ref().map(|i| i.enabled as i32).unwrap_or(0);
        *voice_active = info.as_ref().map(|i| i.voice_active as i32).unwrap_or(0);
        *voice_full_voices = info
            .as_ref()
            .map(|i| i.voice_full_voices as i32)
            .unwrap_or(0);
        *voice_degraded_voices = info
            .as_ref()
            .map(|i| i.voice_degraded_voices as i32)
            .unwrap_or(0);
        *voice_dropped_voices = info
            .as_ref()
            .map(|i| i.voice_dropped_voices as i32)
            .unwrap_or(0);
        *peak_db_l = info.as_ref().map(|i| i.peak_db_l).unwrap_or(-96.0);
        *peak_db_r = info.as_ref().map(|i| i.peak_db_r).unwrap_or(-96.0);
        *rms_db_l = info.as_ref().map(|i| i.rms_db_l).unwrap_or(-96.0);
        *rms_db_r = info.as_ref().map(|i| i.rms_db_r).unwrap_or(-96.0);
    }
    EngineStatus::Ok as i32
}

/// Read the live listener pose from the spatial master telemetry
/// (listener motion, v4.3.0): yaw/pitch/roll in degrees and the
/// world-space position (metres) — the post-glide state the renderers
/// read, mirrored on the telemetry cadence.
///
/// Out-params are optional (pass NULL to skip any).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_spatial_listener_pose(
    handle: *mut EngineHandleFFI,
    yaw_deg: *mut f32,
    pitch_deg: *mut f32,
    roll_deg: *mut f32,
    pos_x: *mut f32,
    pos_y: *mut f32,
    pos_z: *mut f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_spatial_listener_pose_impl(
                handle, yaw_deg, pitch_deg, roll_deg, pos_x, pos_y, pos_z,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_spatial_listener_pose_impl(
    handle: *mut EngineHandleFFI,
    yaw_deg: *mut f32,
    pitch_deg: *mut f32,
    roll_deg: *mut f32,
    pos_x: *mut f32,
    pos_y: *mut f32,
    pos_z: *mut f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    let info = h.handle.playback_info().spatial;
    let pose = info.unwrap_or_default();
    unsafe {
        if let Some(p) = yaw_deg.as_mut() {
            *p = pose.listener_yaw_deg;
        }
        if let Some(p) = pitch_deg.as_mut() {
            *p = pose.listener_pitch_deg;
        }
        if let Some(p) = roll_deg.as_mut() {
            *p = pose.listener_roll_deg;
        }
        if let Some(p) = pos_x.as_mut() {
            *p = pose.listener_position.x;
        }
        if let Some(p) = pos_y.as_mut() {
            *p = pose.listener_position.y;
        }
        if let Some(p) = pos_z.as_mut() {
            *p = pose.listener_position.z;
        }
    }
    EngineStatus::Ok as i32
}

/// Set the target listener pose (listener motion): world-space
/// orientation as a quaternion `(x, y, z, w)` + position `(x, y, z)`
/// metres. The spatial master glides toward it every processed block
/// (nlerp on orientation, one-pole on position) per its tracking policy.
/// Non-finite inputs are rejected.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_listener_pose(
    handle: *mut EngineHandleFFI,
    qx: f32,
    qy: f32,
    qz: f32,
    qw: f32,
    px: f32,
    py: f32,
    pz: f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_set_spatial_listener_pose_impl(handle, qx, qy, qz, qw, px, py, pz)
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_listener_pose_impl(
    handle: *mut EngineHandleFFI,
    qx: f32,
    qy: f32,
    qz: f32,
    qw: f32,
    px: f32,
    py: f32,
    pz: f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    use crate::spatial::math::{Quat, Vec3};
    let q = Quat::new(qx, qy, qz, qw);
    let p = Vec3::new(px, py, pz);
    if !(q.x.is_finite() && q.y.is_finite() && q.z.is_finite() && q.w.is_finite()) {
        return EngineStatus::InvalidArgument as i32;
    }
    if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
        return EngineStatus::InvalidArgument as i32;
    }
    // Normalize a non-unit quaternion so the glide math stays exact.
    let len = q.length();
    let q = if len > f32::EPSILON {
        Quat::new(q.x / len, q.y / len, q.z / len, q.w / len)
    } else {
        Quat::IDENTITY
    };
    h.handle.set_spatial_listener_pose(q, p);
    EngineStatus::Ok as i32
}

/// Set the listener-motion glide policy: one-pole smoothing
/// time constant in ms (`0` snaps exactly) and an optional angular rate
/// limit in deg/s (`0` unlimited). Negative values are rejected.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_listener_tracking(
    handle: *mut EngineHandleFFI,
    smoothing_ms: f32,
    max_angular_rate_deg_s: f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_set_spatial_listener_tracking_impl(handle, smoothing_ms, max_angular_rate_deg_s)
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_listener_tracking_impl(
    handle: *mut EngineHandleFFI,
    smoothing_ms: f32,
    max_angular_rate_deg_s: f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if !smoothing_ms.is_finite() || smoothing_ms < 0.0 {
        return EngineStatus::InvalidArgument as i32;
    }
    if !max_angular_rate_deg_s.is_finite() || max_angular_rate_deg_s < 0.0 {
        return EngineStatus::InvalidArgument as i32;
    }
    h.handle
        .set_spatial_listener_tracking(smoothing_ms, max_angular_rate_deg_s);
    EngineStatus::Ok as i32
}

/// Spatial-health diagnostics (spec §103 extension): explainable per-source
/// status derived from the spatial meters + scene on the telemetry cadence.
/// Each level out-param uses `EngineHealthLevel` (0 = Inactive, 1 = Good,
/// 2 = Moderate, 3 = Poor). `correlation` is the measured inter-channel
/// correlation `[-1, 1]` (0 = unmeasured), `direct_reflected_ratio_db` is
/// the scene-wide direct-vs-reflected ratio (`+∞` = no reflections).
/// Out-params are optional — pass NULL to skip.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_spatial_health(
    handle: *mut EngineHandleFFI,
    status: *mut i32,
    localization: *mut i32,
    reflection_dominance: *mut i32,
    occlusion: *mut i32,
    phase_risk: *mut i32,
    voice_pressure: *mut i32,
    correlation: *mut f32,
    direct_reflected_ratio_db: *mut f32,
    active_sources: *mut i32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_spatial_health_impl(
                handle,
                status,
                localization,
                reflection_dominance,
                occlusion,
                phase_risk,
                voice_pressure,
                correlation,
                direct_reflected_ratio_db,
                active_sources,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_spatial_health_impl(
    handle: *mut EngineHandleFFI,
    status: *mut i32,
    localization: *mut i32,
    reflection_dominance: *mut i32,
    occlusion: *mut i32,
    phase_risk: *mut i32,
    voice_pressure: *mut i32,
    correlation: *mut f32,
    direct_reflected_ratio_db: *mut f32,
    active_sources: *mut i32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    let health = h.handle.playback_info().spatial_health;
    let level_code = |l: crate::spatial::health::HealthLevel| -> i32 {
        match l {
            crate::spatial::health::HealthLevel::Inactive => 0,
            crate::spatial::health::HealthLevel::Good => 1,
            crate::spatial::health::HealthLevel::Moderate => 2,
            crate::spatial::health::HealthLevel::Poor => 3,
        }
    };
    // Every out-param is optional, exactly as documented above: a NULL
    // out-param is skipped rather than treated as an error, and a
    // not-yet-published report reads as a coherent inactive set. The sibling
    // query entry points (`engine_correction_info`, `engine_aux_insert_state`)
    // honour the same contract.
    macro_rules! put {
        ($out:expr, $value:expr) => {
            if !$out.is_null() {
                unsafe { *$out = $value };
            }
        };
    }
    put!(
        status,
        health.as_ref().map(|h| level_code(h.status)).unwrap_or(0)
    );
    put!(
        localization,
        health
            .as_ref()
            .map(|h| level_code(h.localization.level))
            .unwrap_or(0)
    );
    put!(
        reflection_dominance,
        health
            .as_ref()
            .map(|h| level_code(h.reflection_dominance.level))
            .unwrap_or(0)
    );
    put!(
        occlusion,
        health
            .as_ref()
            .map(|h| level_code(h.occlusion.level))
            .unwrap_or(0)
    );
    put!(
        phase_risk,
        health
            .as_ref()
            .map(|h| level_code(h.phase_risk.level))
            .unwrap_or(0)
    );
    put!(
        voice_pressure,
        health
            .as_ref()
            .map(|h| level_code(h.voice_pressure.level))
            .unwrap_or(0)
    );
    put!(
        correlation,
        health.as_ref().map(|h| h.stereo_correlation).unwrap_or(0.0)
    );
    put!(
        direct_reflected_ratio_db,
        health
            .as_ref()
            .map(|h| h.direct_reflected_ratio_db)
            .unwrap_or(f32::INFINITY)
    );
    put!(
        active_sources,
        health
            .as_ref()
            .map(|h| h.active_sources as i32)
            .unwrap_or(0)
    );
    EngineStatus::Ok as i32
}

/// Set the spatial master renderer quality tier (spec §86).
/// `quality` uses `EngineSpatialQuality` (0 = Low, 1 = Medium, 2 = High,
/// 3 = Ultra).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_quality(handle: *mut EngineHandleFFI, quality: i32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_spatial_quality_impl(handle, quality)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_quality_impl(handle: *mut EngineHandleFFI, quality: i32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    let q = match quality {
        0 => config::SpatialQuality::Low,
        1 => config::SpatialQuality::Medium,
        2 => config::SpatialQuality::High,
        3 => config::SpatialQuality::Ultra,
        _ => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.set_spatial_quality(q);
    EngineStatus::Ok as i32
}

/// Set the spatial master's voice budget (spec §76):
/// `enabled != 0` applies `capacity` (concurrent voices) with
/// `full_quality_capacity` full-quality voices, ranked by `policy`
/// (`EngineVoicePriority`: 0 = Fixed, 1 = DistanceWeighted,
/// 2 = GainWeighted, 3 = UserDefined). `enabled == 0` clears the budget
/// (full admission).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_voice(
    handle: *mut EngineHandleFFI,
    enabled: i32,
    capacity: i32,
    full_quality_capacity: i32,
    policy: i32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_set_spatial_voice_impl(handle, enabled, capacity, full_quality_capacity, policy)
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_voice_impl(
    handle: *mut EngineHandleFFI,
    enabled: i32,
    capacity: i32,
    full_quality_capacity: i32,
    policy: i32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if capacity < 0 || full_quality_capacity < 0 {
        return EngineStatus::InvalidArgument as i32;
    }
    let policy = match policy {
        0 => config::VoicePriority::Fixed,
        1 => config::VoicePriority::DistanceWeighted,
        2 => config::VoicePriority::GainWeighted,
        3 => config::VoicePriority::UserDefined,
        _ => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.set_spatial_voice(
        enabled != 0,
        capacity as usize,
        full_quality_capacity as usize,
        policy,
    );
    EngineStatus::Ok as i32
}

/// Set a scalar automation curve on a spatial program object (spec §47). `object` 0 = Left, 1 = Right; `kind` 0 = gain, 1 = spread.
/// `points_count` keyframes are given as parallel `times` (seconds) and
/// `values` (linear) arrays (clamped to a bounded count); the scene
/// automation clock is set to `time_secs`. Pass `points_count == 0` to clear
/// the curve. The curve is built on the control (caller) thread and applied
/// allocation-free at block rate.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_automation(
    handle: *mut EngineHandleFFI,
    object: i32,
    kind: i32,
    times: *const f32,
    values: *const f32,
    points_count: i32,
    time_secs: f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_set_spatial_automation_impl(
                handle,
                object,
                kind,
                times,
                values,
                points_count,
                time_secs,
            )
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_automation_impl(
    handle: *mut EngineHandleFFI,
    object: i32,
    kind: i32,
    times: *const f32,
    values: *const f32,
    points_count: i32,
    time_secs: f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if object != 0 && object != 1 {
        return EngineStatus::InvalidArgument as i32;
    }
    if kind != 0 && kind != 1 {
        return EngineStatus::InvalidArgument as i32;
    }
    if points_count < 0 {
        return EngineStatus::InvalidArgument as i32;
    }
    if points_count > 0 && (times.is_null() || values.is_null()) {
        return EngineStatus::InvalidArgument as i32;
    }
    if points_count == 0 {
        h.handle
            .set_spatial_automation(object as u8, kind as u8, None, time_secs);
        return EngineStatus::Ok as i32;
    }
    // Reject rather than clamp. Clamping would read `64` elements from arrays the
    // caller sized for `points_count` — a silent over-read of up to 54
    // floats. The capacity is a documented API limit, so exceeding it is a
    // caller error we can report instead.
    const MAX_AUTOMATION_POINTS: usize = 64;
    if points_count as usize > MAX_AUTOMATION_POINTS {
        return EngineStatus::InvalidArgument as i32;
    }
    let count = points_count as usize;
    // SAFETY: `count == points_count`, the caller contract for these arrays,
    // and both pointers were checked non-null above.
    let times_slice = unsafe { std::slice::from_raw_parts(times, count) };
    let values_slice = unsafe { std::slice::from_raw_parts(values, count) };
    if times_slice.iter().any(|t| !t.is_finite()) || values_slice.iter().any(|v| !v.is_finite()) {
        return EngineStatus::InvalidArgument as i32;
    }
    let points: Vec<(f32, f32)> = times_slice
        .iter()
        .cloned()
        .zip(values_slice.iter().cloned())
        .collect();
    let Some(curve) = crate::spatial::CurveScalar::from_points(&points) else {
        return EngineStatus::InvalidArgument as i32;
    };
    h.handle.set_spatial_automation(
        object as u8,
        kind as u8,
        Some(std::sync::Arc::new(curve)),
        time_secs,
    );
    EngineStatus::Ok as i32
}

/// Drive the spatial master's program-object automation at `seconds`
/// (spec §47).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_spatial_automation_time(
    handle: *mut EngineHandleFFI,
    seconds: f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_spatial_automation_time_impl(handle, seconds)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_spatial_automation_time_impl(handle: *mut EngineHandleFFI, seconds: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.set_spatial_automation_time(seconds);
    EngineStatus::Ok as i32
}

/// Scene animation (v4.4.0): fire the named cue on the spatial
/// master. The cue's parameter curves apply relative to the firing
/// instant, evaluated at the block boundary.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_trigger_spatial_cue(
    handle: *mut EngineHandleFFI,
    name: *const std::os::raw::c_char,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_trigger_spatial_cue_impl(handle, name)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_trigger_spatial_cue_impl(
    handle: *mut EngineHandleFFI,
    name: *const std::os::raw::c_char,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if name.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let name = match unsafe { std::ffi::CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.trigger_spatial_cue(&name);
    EngineStatus::Ok as i32
}

/// Scene animation: stop the active cue on `target` (0 = L,
/// 1 = R).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_stop_spatial_cue(handle: *mut EngineHandleFFI, target: usize) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_stop_spatial_cue_impl(handle, target)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_stop_spatial_cue_impl(handle: *mut EngineHandleFFI, target: usize) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.stop_spatial_cue(target);
    EngineStatus::Ok as i32
}

/// Spatial diagnostics (v4.5.0): the spatial stage's modeled
/// render cost and budget state, mirrored from telemetry. Out-params:
/// `cost_units` (cost units per block; 0.0 when disabled),
/// `utilization` (fraction of the block budget; `> 1.0` = over budget),
/// `tail_blocks` (blocks of cost headroom remaining; `+∞` when the stage
/// is idle). All optional — pass NULL to skip.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_spatial_render_cost(
    handle: *mut EngineHandleFFI,
    cost_units: *mut f32,
    utilization: *mut f32,
    tail_blocks: *mut f32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| {
            engine_spatial_render_cost_impl(handle, cost_units, utilization, tail_blocks)
        }),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_spatial_render_cost_impl(
    handle: *mut EngineHandleFFI,
    cost_units: *mut f32,
    utilization: *mut f32,
    tail_blocks: *mut f32,
) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    let info = h.handle.playback_info();
    let t = info.spatial.as_ref();
    unsafe {
        if let Some(p) = cost_units.as_mut() {
            *p = t.map(|t| t.render_cost_units).unwrap_or(0.0);
        }
        if let Some(p) = utilization.as_mut() {
            *p = t.map(|t| t.cost_utilization).unwrap_or(0.0);
        }
        if let Some(p) = tail_blocks.as_mut() {
            *p = t.map(|t| t.tail_blocks_remaining).unwrap_or(f32::INFINITY);
        }
    }
    EngineStatus::Ok as i32
}

/// Scene animation: stop every active cue.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_stop_all_spatial_cues(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_stop_all_spatial_cues_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_stop_all_spatial_cues_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.stop_all_spatial_cues();
    EngineStatus::Ok as i32
}

use super::*;
use crate::EngineCommand;
use std::sync::atomic::Ordering;

/// Write a 16-bit stereo WAV whose left channel is a single impulse
/// (the IR the aux-insert tests need; `engine::tests` helpers are
/// private to that module).
fn write_impulse_wav(path: &std::path::Path, sample_rate: u32, n_frames: usize) {
    let mut data = Vec::with_capacity(n_frames * 4);
    for i in 0..n_frames {
        let v = if i == 0 { 32767i16 } else { 0i16 };
        data.extend_from_slice(&v.to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(path, &wav).unwrap();
}

/// Build an [`EngineHandleFFI`] around a real engine with its own tick
/// thread (no audio output is opened — `start()` is never called, so no
/// device is touched). Returns the box plus the raw pointer the extern
/// functions consume.
fn ffi_with_engine(config: config::EngineConfig) -> (Box<EngineHandleFFI>, *mut EngineHandleFFI) {
    let engine = AudioEngine::new(config).expect("engine must construct");
    let handle = engine.handle();
    let (stop, tick_thread) = EngineHandleFFI::spawn_tick_thread(engine);
    let mut ffi = Box::new(EngineHandleFFI {
        handle,
        stop,
        tick_thread,
    });
    let ptr = &mut *ffi as *mut EngineHandleFFI;
    // Register exactly as `engine_create` does: every entry point consults
    // the registry before it dereferences the pointer.
    live_handles()
        .lock()
        .expect("registry")
        .insert(ptr as usize);
    (ffi, ptr)
}

/// Stop the tick thread deterministically and drop the registry entry the
/// way `engine_destroy` would, before the caller's box drops.
fn shutdown(ffi: &mut EngineHandleFFI) {
    if let Ok(mut live) = live_handles().lock() {
        live.remove(&(ffi as *const EngineHandleFFI as usize));
    }
    ffi.stop.store(true, Ordering::Relaxed);
    if let Some(thread) = ffi.tick_thread.take() {
        let _ = thread.join();
    }
}

/// Every entry point checks the registry before touching the pointer, so a
/// handle that has been destroyed is refused with `InvalidHandle` (or the
/// value-returning sentinel) instead of dereferencing freed memory.
#[test]
fn a_destroyed_handle_is_rejected_before_it_is_dereferenced() {
    let handle = engine_create(backend_id::NULL);
    assert!(!handle.is_null(), "null backend creates an engine");

    // Live: the calls go through.
    assert_eq!(engine_play(handle), EngineStatus::Ok as i32);
    assert_eq!(engine_pause(handle), EngineStatus::Ok as i32);
    assert!(engine_position_secs(handle) >= 0.0);
    assert!(engine_playlist_len(handle) >= 0);

    engine_destroy(handle);

    // Destroyed, but the address is still the one the test holds: every
    // entry point must refuse it before dereferencing anything.
    assert_eq!(engine_play(handle), EngineStatus::InvalidHandle as i32);
    assert_eq!(engine_pause(handle), EngineStatus::InvalidHandle as i32);
    assert_eq!(
        engine_playback_state(handle),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(engine_position_secs(handle), -1.0);
    assert_eq!(engine_playlist_len(handle), -1);
    assert_eq!(engine_endpoint_count(handle), -1);

    // And destroying it again stays the documented no-op.
    engine_destroy(handle);
}

#[test]
fn ffi_null_handles_and_invalid_args_rejected() {
    assert_eq!(
        engine_set_aux_insert(std::ptr::null_mut(), 1, 0.5),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_clear_endpoints(std::ptr::null_mut()),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_remove_endpoint(std::ptr::null_mut(), c"ep".as_ptr().cast::<c_char>(),),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(engine_endpoint_count(std::ptr::null_mut()), -1);
    assert_eq!(
        engine_aux_insert_state(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::InvalidHandle as i32
    );

    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());
    // Null out-pointers on the aux read are rejected.
    assert_eq!(
        engine_aux_insert_state(ptr, std::ptr::null_mut(), std::ptr::null_mut()),
        EngineStatus::InvalidArgument as i32
    );
    let mut enabled = 0i32;
    assert_eq!(
        engine_aux_insert_state(ptr, &mut enabled, std::ptr::null_mut()),
        EngineStatus::InvalidArgument as i32
    );
    // Endpoint validation: null/empty id, unknown backend.
    assert_eq!(
        engine_upsert_endpoint(ptr, std::ptr::null(), std::ptr::null(), 0, 1.0, 1, 1),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_upsert_endpoint(ptr, c"".as_ptr().cast(), std::ptr::null(), 0, 1.0, 1, 1),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_upsert_endpoint(ptr, c"ep".as_ptr().cast(), std::ptr::null(), 99, 1.0, 1, 1),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_upsert_endpoint(
            ptr,
            c"ep".as_ptr().cast(),
            std::ptr::null(),
            0,
            f32::NAN,
            1,
            1
        ),
        EngineStatus::InvalidArgument as i32
    );
    shutdown(&mut ffi);
}

#[test]
fn ffi_aux_insert_toggle_reads_back_through_telemetry() {
    // An impulse IR so the insert engine exists and the toggle is
    // observable end to end (command → graph → mirror → telemetry).
    let ir_path = std::env::temp_dir().join(format!(
        "ffi_aux_ir_{}_{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_impulse_wav(&ir_path, 48_000, 2048);

    let mut config = EngineConfig::default();
    config.aux.enabled = true;
    config.aux.return_gain = 0.5;
    config.aux.insert_enabled = false;
    config.aux.insert_wet_mix = 1.0;
    config.aux.insert_ir_path = Some(ir_path.display().to_string());
    let (mut ffi, ptr) = ffi_with_engine(config);

    // Send the SAME command through the handle's channel directly as a
    // control: this isolates the extern wrapper from the engine path.
    let cmd = EngineCommand::SetAuxInsert {
        enabled: true,
        wet_mix: 0.25,
    };
    assert!(
        ffi.handle.send_command(cmd).is_ok(),
        "handle accepts command"
    );
    assert_eq!(engine_set_aux_insert(ptr, 1, 0.25), EngineStatus::Ok as i32);

    // Poll the telemetry mirror (2 s cadence) for the toggled state.
    let mut enabled = 0i32;
    let mut wet = -1.0f32;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut ok = false;
    while std::time::Instant::now() < deadline {
        assert_eq!(
            engine_aux_insert_state(ptr, &mut enabled, &mut wet),
            EngineStatus::Ok as i32
        );
        if enabled == 1 && (wet - 0.25).abs() < 1e-6 {
            ok = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(
        ok,
        "aux insert state read back (enabled={enabled}, wet={wet})"
    );
    shutdown(&mut ffi);
    let _ = std::fs::remove_file(&ir_path);
}

#[test]
fn ffi_spatial_configuration_rejects_bad_args() {
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());

    // Invalid quality / policy enum values are rejected, not mapped.
    assert_eq!(
        engine_set_spatial_quality(ptr, 42),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_voice(ptr, 1, 4, 2, 77),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_voice(ptr, 1, -1, 2, 0),
        EngineStatus::InvalidArgument as i32
    );

    // Automation: null arrays with a nonzero point count are rejected.
    assert_eq!(
        engine_set_spatial_automation(ptr, 0, 0, std::ptr::null(), std::ptr::null(), 3, 0.0,),
        EngineStatus::InvalidArgument as i32
    );
    // A bad object / kind index is rejected.
    let times = [0.0f32, 1.0, 2.0];
    assert_eq!(
        engine_set_spatial_automation(ptr, 7, 0, times.as_ptr(), times.as_ptr(), 3, 0.0,),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_automation(ptr, 0, 9, times.as_ptr(), times.as_ptr(), 3, 0.0,),
        EngineStatus::InvalidArgument as i32
    );
    // Non-finite keyframe values are rejected.
    let bad_vals = [0.0f32, f32::NAN, 1.0];
    assert_eq!(
        engine_set_spatial_automation(ptr, 0, 0, times.as_ptr(), bad_vals.as_ptr(), 3, 0.0,),
        EngineStatus::InvalidArgument as i32
    );

    // Null handle is rejected across the whole surface.
    assert_eq!(
        engine_set_spatial_quality(std::ptr::null_mut(), 1),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_set_spatial_voice(std::ptr::null_mut(), 1, 4, 2, 0),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_set_spatial_automation_time(std::ptr::null_mut(), 0.5),
        EngineStatus::InvalidHandle as i32
    );

    // Listener motion: non-finite / negative inputs are
    // rejected; the pose read tolerates NULL out-params.
    assert_eq!(
        engine_set_spatial_listener_pose(ptr, f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_listener_pose(ptr, 0.0, 0.0, 0.0, 1.0, f32::INFINITY, 0.0, 0.0),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_listener_tracking(ptr, -1.0, 0.0),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_listener_tracking(ptr, 0.0, -5.0),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_set_spatial_listener_pose(std::ptr::null_mut(), 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_spatial_listener_pose(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::InvalidHandle as i32
    );

    shutdown(&mut ffi);
}

#[test]
fn ffi_spatial_configuration_round_trips_into_node() {
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());

    // Valid quality + voice budget dispatches through the handle channel
    // into the graph's SpatialNode once the tick thread drains it.
    assert_eq!(
        engine_set_spatial_quality(ptr, 2), // High
        EngineStatus::Ok as i32
    );
    assert_eq!(
        engine_set_spatial_voice(ptr, 1, 3, 1, 2), // GainWeighted
        EngineStatus::Ok as i32
    );
    // A gain automation curve on the left object feeds the scene clock.
    let times = [0.0f32, 1.0, 2.0];
    let vals = [1.0f32, 0.5, 0.25];
    assert_eq!(
        engine_set_spatial_automation(ptr, 0, 0, times.as_ptr(), vals.as_ptr(), 3, 1.5,),
        EngineStatus::Ok as i32
    );

    // Reading SpatialTelemetry returns a coherent (inactive) set — the
    // stage exists but no playback has produced levels yet.
    let mut enabled = 99i32;
    let mut active = 99i32;
    let mut full = 99i32;
    let mut degraded = 99i32;
    let mut dropped = 99i32;
    let mut pl = -99.0f32;
    let mut pr = -99.0f32;
    let mut rl = -99.0f32;
    let mut rr = -99.0f32;
    assert_eq!(
        engine_spatial_info(
            ptr,
            &mut enabled,
            &mut active,
            &mut full,
            &mut degraded,
            &mut dropped,
            &mut pl,
            &mut pr,
            &mut rl,
            &mut rr,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(enabled, 0);
    assert_eq!(active, 0);
    assert_eq!(full, 0);
    assert_eq!(degraded, 0);
    assert_eq!(dropped, 0);

    // Null out-pointers are rejected.
    assert_eq!(
        engine_spatial_info(
            ptr,
            &mut enabled,
            std::ptr::null_mut(),
            &mut full,
            &mut degraded,
            &mut dropped,
            &mut pl,
            &mut pr,
            &mut rl,
            &mut rr,
        ),
        EngineStatus::InvalidArgument as i32
    );

    shutdown(&mut ffi);
}

#[test]
fn ffi_spatial_listener_pose_round_trips() {
    // The pose target lands on the engine handle channel and
    // the telemetry read mirrors the live pose (the FFI tick thread
    // drains commands and publishes telemetry on its 5 ms cadence).
    use crate::spatial::math::Quat;
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());

    // A 90° yaw target at (1, 2, 0.5): normalized quaternion + position.
    let q = Quat::from_euler_rad(90f32.to_radians(), 0.0, 0.0);
    assert_eq!(
        engine_set_spatial_listener_tracking(ptr, 0.0, 0.0),
        EngineStatus::Ok as i32
    );
    assert_eq!(
        engine_set_spatial_listener_pose(ptr, q.x, q.y, q.z, q.w, 1.0, 2.0, 0.5),
        EngineStatus::Ok as i32
    );
    // A non-unit quaternion is normalized, not rejected (len 2·identity
    // at the origin supersedes the first target).
    assert_eq!(
        engine_set_spatial_listener_pose(ptr, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0),
        EngineStatus::Ok as i32
    );

    // Let the tick thread drain the commands and publish telemetry.
    std::thread::sleep(std::time::Duration::from_millis(120));

    let mut yaw = -999.0f32;
    let mut pitch = -999.0f32;
    let mut roll = -999.0f32;
    let mut x = -999.0f32;
    let mut y = -999.0f32;
    let mut z = -999.0f32;
    assert_eq!(
        engine_spatial_listener_pose(ptr, &mut yaw, &mut pitch, &mut roll, &mut x, &mut y, &mut z,),
        EngineStatus::Ok as i32
    );
    // The normalized-identity-at-origin target was drained: the glide
    // (0 ms policy) reports the snapped pose, not the sentinels.
    assert!(yaw.abs() < 1.0, "yaw {yaw}");
    assert!(pitch.abs() < 1.0, "pitch {pitch}");
    assert!(roll.abs() < 1.0, "roll {roll}");
    assert!(x.abs() < 1e-6, "x {x}");
    assert!(y.abs() < 1e-6, "y {y}");
    assert!(z.abs() < 1e-6, "z {z}");

    // NULL out-params are skipped, not rejected.
    assert_eq!(
        engine_spatial_listener_pose(
            ptr,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::Ok as i32
    );

    shutdown(&mut ffi);
}

#[test]
fn ffi_diagnostics_report_empty_state_and_stable_codes() {
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());

    // No error: kind -1, empty message, no bit-perfect cause, count 0.
    let mut kind = 999i32;
    let mut cause = 999i32;
    let mut count = 999i32;
    let mut buf = [0i8; 64];
    assert_eq!(
        engine_diagnostics_info(
            ptr,
            &mut kind,
            buf.as_mut_ptr(),
            buf.len(),
            &mut cause,
            &mut count,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(kind, -1);
    assert_eq!(count, 0);
    assert_eq!(cause, 0);
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
            .to_str()
            .unwrap(),
        ""
    );

    // Null handle is rejected; null out-pointers are tolerated.
    assert_eq!(
        engine_diagnostics_info(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::InvalidHandle as i32
    );
    assert_eq!(
        engine_diagnostics_info(
            ptr,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut count,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(count, 0);

    // Stable FFI integer mappings.
    assert_eq!(diagnostic_kind_code(crate::DiagnosticKind::Resampler), 3);
    assert_eq!(diagnostic_kind_code(crate::DiagnosticKind::Endpoint), 5);
    assert_eq!(
        diagnostic_kind_code(crate::DiagnosticKind::Configuration),
        7
    );
    assert_eq!(bit_perfect_cause_code(crate::BitPerfectCause::None), 0);
    assert_eq!(bit_perfect_cause_code(crate::BitPerfectCause::EqActive), 2);
    assert_eq!(
        bit_perfect_cause_code(crate::BitPerfectCause::OutputNotDirectExclusive),
        9
    );
    assert_eq!(
        bit_perfect_cause_code(crate::BitPerfectCause::DitherActive),
        10
    );

    shutdown(&mut ffi);
}

#[test]
fn ffi_abi_version_is_self_describing() {
    // A host must be able to ask what it is talking to before it calls
    // anything else, and must be able to refuse a surface it was not
    // built for.
    assert_eq!(engine_abi_version(), ENGINE_ABI_VERSION);
    assert_eq!(
        ENGINE_ABI_VERSION >> 16,
        1,
        "0.9.0 introduces major ABI version 1"
    );

    // Exact match is compatible.
    assert_eq!(engine_abi_compatible(ENGINE_ABI_VERSION), 1);
    // An older host on the same major is compatible: 0.9 only added.
    assert_eq!(engine_abi_compatible(1 << 16), 1);
    // A newer host on the same major is NOT — we cannot know what it
    // expects, so we must not pretend to.
    assert_eq!(engine_abi_compatible((1 << 16) | 99), 0);
    // A different major is never compatible, in either direction.
    assert_eq!(engine_abi_compatible(0), 0);
    assert_eq!(engine_abi_compatible(2 << 16), 0);
}

#[test]
fn ffi_boundary_converts_panic_to_status() {
    // The wrapper must turn a panic into this module's ordinary error
    // convention rather than letting it abort the host process.
    assert_eq!(
        ffi_boundary(
            std::panic::AssertUnwindSafe(|| panic!("deliberate FFI boundary test")),
            EngineStatus::Error as i32
        ),
        EngineStatus::Error as i32
    );
    // A non-panicking body passes its value straight through.
    assert_eq!(ffi_boundary(|| 7i32, -1), 7);
    assert_eq!(ffi_boundary(|| -1.0f32, -2.0), -1.0);
    // A pointer-returning entry point: the fallback is a real sentinel,
    // and a panicking body must yield it rather than propagating.
    let sentinel = std::ptr::without_provenance_mut::<u8>(1);
    assert_eq!(ffi_boundary(|| sentinel, std::ptr::null_mut()), sentinel);
    assert_eq!(
        ffi_boundary(
            || -> *mut u8 { panic!("deliberate pointer boundary test") },
            sentinel
        ),
        sentinel
    );
    assert_eq!(
        ffi_boundary(
            || -> *mut EngineHandleFFI { panic!("handle factory boundary test") },
            std::ptr::null_mut()
        ),
        std::ptr::null_mut()
    );
}

#[test]
fn ffi_spatial_health_accepts_null_out_params() {
    // The doc comment promises "Out-params are optional — pass NULL to
    // skip". This is the call a C host makes when it only wants, say, the
    // status code, and it used to dereference all nine pointers
    // unconditionally — a host-process segfault, not an error return.
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());

    // Every out-param NULL: still Ok, nothing written.
    assert_eq!(
        engine_spatial_health(
            ptr,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::Ok as i32
    );

    // Mixed: the requested out-param is written, the NULL ones skipped.
    let mut status = 99i32;
    assert_eq!(
        engine_spatial_health(
            ptr,
            &mut status,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(status, 0, "pre-publish report reads as Inactive");

    // A NULL *handle* still outranks the out-params.
    assert_eq!(
        engine_spatial_health(
            std::ptr::null_mut(),
            &mut status,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::InvalidHandle as i32
    );
    shutdown(&mut ffi);
}

#[test]
fn ffi_spatial_health_reports_inactive_before_publish() {
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());
    let mut status = 99i32;
    let mut loc = 99i32;
    let mut refl = 99i32;
    let mut occ = 99i32;
    let mut phase = 99i32;
    let mut voice = 99i32;
    let mut corr = 99.0f32;
    let mut ratio = 99.0f32;
    let mut sources = 99i32;
    // Before the first telemetry publish the report reads as a coherent
    // inactive set (status 0 = Inactive, ratio +∞).
    assert_eq!(
        engine_spatial_health(
            ptr,
            &mut status,
            &mut loc,
            &mut refl,
            &mut occ,
            &mut phase,
            &mut voice,
            &mut corr,
            &mut ratio,
            &mut sources,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(status, 0);
    assert_eq!(loc, 0);
    assert_eq!(refl, 0);
    assert_eq!(occ, 0);
    assert_eq!(phase, 0);
    assert_eq!(voice, 0);
    assert_eq!(corr, 0.0);
    assert!(ratio.is_infinite());
    assert_eq!(sources, 0);

    // Null handle rejected.
    assert_eq!(
        engine_spatial_health(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ),
        EngineStatus::InvalidHandle as i32
    );
    shutdown(&mut ffi);
}

#[test]
fn ffi_endpoint_surface_accepts_config_sequence() {
    let (mut ffi, ptr) = ffi_with_engine(EngineConfig::default());
    assert_eq!(engine_endpoint_count(ptr), 0, "no endpoints configured");

    // Upsert (id + device + backend + gain + enabled), then remove,
    // then re-add and clear — every call routes through the C-string
    // parser into `endpoint_configs` (no device is opened: the engine
    // was never started).
    assert_eq!(
        engine_upsert_endpoint(
            ptr,
            c"ffi-ep".as_ptr().cast(),
            c"Fake Device".as_ptr().cast(),
            0,
            0.8,
            1,
            1,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(
        engine_remove_endpoint(ptr, c"ffi-ep".as_ptr().cast()),
        EngineStatus::Ok as i32
    );
    assert_eq!(
        engine_upsert_endpoint(
            ptr,
            c"ffi-ep".as_ptr().cast(),
            std::ptr::null(),
            0,
            0.8,
            1,
            0,
        ),
        EngineStatus::Ok as i32
    );
    assert_eq!(engine_clear_endpoints(ptr), EngineStatus::Ok as i32);
    shutdown(&mut ffi);
}

#[test]
fn ffi_codec_registry_enumerates_and_resolves_extensions() {
    let count = engine_codec_count();
    let rows = codec_registry::all_codecs();
    assert_eq!(count as usize, rows.len());
    assert!(!rows.is_empty(), "the registry must never be empty");

    let mut flac_index = None;
    for i in 0..count {
        let mut row = std::mem::MaybeUninit::<EngineCodecCapabilityFFI>::uninit();
        assert_eq!(
            engine_codec_capability(i, row.as_mut_ptr()),
            EngineStatus::Ok as i32,
            "row {i}"
        );
        let row = unsafe { row.assume_init() };
        assert_eq!(row.index, i);
        assert!(
            row.status <= codec_status_id::DECLARED_UNAVAILABLE,
            "status {} out of range",
            row.status
        );
        assert!(
            row.support_level <= codec_support_id::UNAVAILABLE,
            "support level {} out of range",
            row.support_level
        );
        for (axis, value) in [
            ("decode", row.decode),
            ("seek", row.seek),
            ("gapless", row.gapless),
            ("metadata", row.metadata),
            ("replaygain", row.replaygain),
            ("chapters", row.chapters),
            ("ebu_r128", row.ebu_r128),
            ("streaming", row.streaming),
            ("multichannel_decode", row.multichannel_decode),
            ("is_lossless", row.is_lossless),
        ] {
            assert!(value <= 1, "{axis} = {value} is not a boolean");
        }
        assert!(
            row.native_precision_bits == -1 || (1..=64).contains(&row.native_precision_bits),
            "precision {} out of range",
            row.native_precision_bits
        );
        assert!(!row.name.is_null(), "row {i} name is NULL");
        let name = unsafe { CStr::from_ptr(row.name) };
        assert_eq!(
            name,
            codec_registry::codec_name(rows[i as usize]),
            "row {i} name must be the registry's own static name"
        );
        if name.to_bytes() == b"flac" {
            flac_index = Some(i);
        }
    }
    let flac_index = flac_index.expect("flac must be a registry row");

    // The row's axes agree with its status: FLAC is available, decodable
    // and lossless in every build this registry describes.
    let mut flac_row = std::mem::MaybeUninit::<EngineCodecCapabilityFFI>::uninit();
    assert_eq!(
        engine_codec_capability(flac_index, flac_row.as_mut_ptr()),
        EngineStatus::Ok as i32
    );
    let flac = unsafe { flac_row.assume_init() };
    assert_eq!(flac.status, codec_status_id::AVAILABLE);
    assert_eq!(flac.decode, 1);
    assert_eq!(flac.is_lossless, 1);

    // Out-of-range index and NULL out-pointers are argument errors.
    let mut row = std::mem::MaybeUninit::<EngineCodecCapabilityFFI>::uninit();
    assert_eq!(
        engine_codec_capability(count, row.as_mut_ptr()),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_codec_capability(0, std::ptr::null_mut()),
        EngineStatus::InvalidArgument as i32
    );

    // Extension lookup agrees with the row enumeration; a miss and a
    // malformed call are distinct statuses.
    let mut index = u32::MAX;
    assert_eq!(
        engine_codec_index_for_extension(c"flac".as_ptr(), &mut index),
        EngineStatus::Ok as i32
    );
    assert_eq!(index, flac_index);

    let mut index = u32::MAX;
    assert_eq!(
        engine_codec_index_for_extension(c"XYZ".as_ptr(), &mut index),
        EngineStatus::NotFound as i32
    );
    assert_eq!(index, u32::MAX, "a failed lookup must not write the index");
    assert_eq!(
        engine_codec_index_for_extension(std::ptr::null(), &mut index),
        EngineStatus::InvalidArgument as i32
    );
    assert_eq!(
        engine_codec_index_for_extension(c"flac".as_ptr(), std::ptr::null_mut()),
        EngineStatus::InvalidArgument as i32
    );

    // Codec-string lookup (the diagnostics spelling) reaches the same row.
    let mut index = u32::MAX;
    assert_eq!(
        engine_codec_index_for_codec_string(c"flac".as_ptr(), &mut index),
        EngineStatus::Ok as i32
    );
    assert_eq!(index, flac_index);
    assert_eq!(
        engine_codec_index_for_codec_string(c"no-such-codec".as_ptr(), &mut index),
        EngineStatus::NotFound as i32
    );
}

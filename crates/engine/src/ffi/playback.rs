use super::*;

// ── Lifecycle ──────────────────────────────────────────────────────────────

/// Create and start an audio engine with the default configuration.
///
/// `backend` is one of the [`backend_id`] constants. Returns `NULL` on
/// failure (including an unrecognised `backend` value) — check logs. The
/// caller must call `engine_destroy` to release resources.
#[no_mangle]
pub extern "C" fn engine_create(backend: u32) -> *mut EngineHandleFFI {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_create_impl(backend)),
        std::ptr::null_mut(),
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_create_impl(backend: u32) -> *mut EngineHandleFFI {
    let output_backend = match backend_from_id(backend) {
        Some(b) => b,
        None => {
            log::error!("engine_create: unknown backend id {backend}");
            return std::ptr::null_mut();
        }
    };
    let config = EngineConfig {
        output_backend,
        ..EngineConfig::default()
    };

    let engine = match AudioEngine::new(config) {
        Ok(mut e) => {
            let handle = e.handle();
            if let Err(err) = e.start() {
                log::error!("engine_create: failed to start engine: {}", err);
                return std::ptr::null_mut();
            }
            (handle, e)
        }
        Err(e) => {
            log::error!("engine_create: failed to create engine: {}", e);
            return std::ptr::null_mut();
        }
    };

    let (stop, tick_thread) = EngineHandleFFI::spawn_tick_thread(engine.1);
    let raw = Box::into_raw(Box::new(EngineHandleFFI {
        handle: engine.0,
        stop,
        tick_thread,
    }));
    if let Ok(mut live) = live_handles().lock() {
        live.insert(raw as usize);
    }
    raw
}

/// Destroy an engine and release all resources. Safe to call with `NULL`.
///
/// Signals the background tick thread to stop, joins it (the engine's
/// `Drop` impl stops the audio output there), and shuts down the command
/// channel.
///
/// **Idempotent.** The handle is de-registered before it is freed, so calling
/// this twice with the same pointer is a no-op the second time rather than a
/// double free. Calling it with a pointer this process never returned is also
/// a no-op.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_destroy(engine: *mut EngineHandleFFI) {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_destroy_impl(engine)),
        (),
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_destroy_impl(engine: *mut EngineHandleFFI) {
    if engine.is_null() {
        return;
    }
    // De-register first. If this pointer was never ours, or was already
    // destroyed, leave it alone — the alternative is freeing memory that no
    // longer belongs to us.
    match live_handles().lock() {
        Ok(mut live) => {
            if !live.remove(&(engine as usize)) {
                log::warn!("engine_destroy: handle {:p} is not live; ignoring", engine);
                return;
            }
        }
        Err(_) => {
            // A poisoned registry means some other lifecycle call panicked.
            // Refusing to free is the safe choice: we cannot prove the handle
            // is ours exactly once, and a leak is recoverable where a double
            // free is not.
            log::error!("engine_destroy: handle registry poisoned; refusing to free");
            return;
        }
    }
    unsafe {
        let mut boxed = Box::from_raw(engine);
        boxed.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = boxed.tick_thread.take() {
            let _ = thread.join();
        }
        boxed.handle.shutdown();
    }
}

// ── Playback control ───────────────────────────────────────────────────────

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_play(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_play_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_play_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.play();
    EngineStatus::Ok as i32
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_pause(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_pause_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_pause_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.pause();
    EngineStatus::Ok as i32
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_stop(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_stop_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_stop_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.stop();
    EngineStatus::Ok as i32
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_seek(handle: *mut EngineHandleFFI, position_secs: f32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_seek_impl(handle, position_secs)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_seek_impl(handle: *mut EngineHandleFFI, position_secs: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.seek(position_secs);
    EngineStatus::Ok as i32
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_volume(handle: *mut EngineHandleFFI, volume: f32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_volume_impl(handle, volume)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_volume_impl(handle: *mut EngineHandleFFI, volume: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.set_volume(volume);
    EngineStatus::Ok as i32
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_speed(handle: *mut EngineHandleFFI, speed: f32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_speed_impl(handle, speed)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_speed_impl(handle: *mut EngineHandleFFI, speed: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.set_speed(speed);
    EngineStatus::Ok as i32
}

/// Set volume directly in dB (see `engine_set_volume_db`).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_set_volume_db(handle: *mut EngineHandleFFI, db: f32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_set_volume_db_impl(handle, db)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_set_volume_db_impl(handle: *mut EngineHandleFFI, db: f32) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.set_volume_db(db);
    EngineStatus::Ok as i32
}

// ── Source management ──────────────────────────────────────────────────────

/// Open a file for playback. Returns Ok on success, Error on failure.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_open_file(handle: *mut EngineHandleFFI, path: *const c_char) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_open_file_impl(handle, path)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_open_file_impl(handle: *mut EngineHandleFFI, path: *const c_char) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if path.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.open_file(Path::new(path_str).to_path_buf());
    EngineStatus::Ok as i32
}

/// Open a URI for playback (`file://`, `http://`, `https://`).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_open_uri(handle: *mut EngineHandleFFI, uri: *const c_char) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_open_uri_impl(handle, uri)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_open_uri_impl(handle: *mut EngineHandleFFI, uri: *const c_char) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if uri.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let c_str = unsafe { CStr::from_ptr(uri) };
    let uri_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.open_uri(uri_str.to_string());
    EngineStatus::Ok as i32
}

/// Add a file to the end of the playback queue (without interrupting playback).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_enqueue_file(handle: *mut EngineHandleFFI, path: *const c_char) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_enqueue_file_impl(handle, path)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_enqueue_file_impl(handle: *mut EngineHandleFFI, path: *const c_char) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    if path.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    h.handle.enqueue_file(Path::new(path_str).to_path_buf());
    EngineStatus::Ok as i32
}

/// Skip to the next playlist entry.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_next(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_next_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_next_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.next();
    EngineStatus::Ok as i32
}

/// Skip to the previous playlist entry.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_previous(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_previous_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_previous_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.previous();
    EngineStatus::Ok as i32
}

/// Clear the playback queue. Safe to call with `NULL`.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_clear_playlist(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_clear_playlist_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_clear_playlist_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    h.handle.clear_playlist();
    EngineStatus::Ok as i32
}

// ── Query ──────────────────────────────────────────────────────────────────

/// Get the current playback position in seconds. Returns -1.0 on error.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_position_secs(handle: *mut EngineHandleFFI) -> f32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_position_secs_impl(handle)),
        -1.0,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_position_secs_impl(handle: *mut EngineHandleFFI) -> f32 {
    let Some(h) = live_handle(handle) else {
        return -1.0;
    };
    h.handle.playback_info().position_secs
}

/// Get the track duration in seconds. Returns -1.0 on error.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_duration_secs(handle: *mut EngineHandleFFI) -> f32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_duration_secs_impl(handle)),
        -1.0,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_duration_secs_impl(handle: *mut EngineHandleFFI) -> f32 {
    let Some(h) = live_handle(handle) else {
        return -1.0;
    };
    h.handle.playback_info().duration_secs
}

/// Get the current playback state: 0=Stopped, 1=Playing, 2=Paused.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_playback_state(handle: *mut EngineHandleFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_playback_state_impl(handle)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_playback_state_impl(handle: *mut EngineHandleFFI) -> i32 {
    let Some(h) = live_handle(handle) else {
        return EngineStatus::InvalidHandle as i32;
    };
    match h.handle.state() {
        crate::playback_info::PlaybackState::Stopped => 0,
        crate::playback_info::PlaybackState::Playing => 1,
        crate::playback_info::PlaybackState::Paused => 2,
        crate::playback_info::PlaybackState::Buffering => 3,
    }
}

/// Number of entries in the playback queue (0 when empty).
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_playlist_len(handle: *mut EngineHandleFFI) -> i64 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_playlist_len_impl(handle)),
        -1,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::too_many_arguments)]
fn engine_playlist_len_impl(handle: *mut EngineHandleFFI) -> i64 {
    let Some(h) = live_handle(handle) else {
        return -1;
    };
    h.handle.playlist_len() as i64
}

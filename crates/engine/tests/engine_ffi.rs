//! The C ABI surface.
//!
//! The engine's C ABI is a *stable contract* with hosts written in other
//! languages: a host that cannot trust the boundary cannot use the library.
//! This suite checks the three things a host relies on:
//!
//! * **the ABI version** — a host asks before it does anything else, and the
//!   answer must be the version the library was built with;
//! * **the status codes** — every failure is a typed integer, never a
//!   sentinel buried in a return value and never a crash; and
//! * **panic containment** — a Rust panic must never unwind across the
//!   boundary. Since Rust 1.71 an unwind out of an `extern "C"` function
//!   aborts the process, so this is not a theoretical concern: a host
//!   application must not be taken down by a bug in the library.
//!
//! The whole file is behind the `c-ffi` feature, because the surface does not
//! exist without it.

#![cfg(feature = "c-ffi")]

mod common;

use std::ffi::{c_char, CStr, CString};

use engine::ffi::{
    backend_id, codec_status_id, codec_support_id, engine_abi_compatible, engine_abi_version,
    engine_codec_capability, engine_codec_count, engine_codec_index_for_codec_string,
    engine_codec_index_for_extension, engine_create, engine_destroy, engine_open_file,
    engine_pause, engine_play, engine_playback_state, engine_playlist_len, engine_position_secs,
    engine_seek, engine_set_volume, EngineCodecCapabilityFFI, EngineHandleFFI, EngineStatus,
    ENGINE_ABI_MAJOR, ENGINE_ABI_MINOR, ENGINE_ABI_VERSION,
};

/// A hostile FFI call: what it is called, and the call itself.
type HostileCall<'a> = (&'a str, Box<dyn Fn() -> i32 + 'a>);

/// The code for `status`, as a host would compare it.
fn code(status: EngineStatus) -> i32 {
    status as i32
}

#[test]
fn the_abi_version_is_the_one_the_library_was_built_with() {
    assert_eq!(engine_abi_version(), ENGINE_ABI_VERSION);
    assert_eq!(
        ENGINE_ABI_VERSION,
        (ENGINE_ABI_MAJOR << 16) | ENGINE_ABI_MINOR,
        "the packed version must be the major and minor components"
    );
    assert_eq!(ENGINE_ABI_MAJOR, 1, "the ABI major version is 1");
}

#[test]
fn a_host_built_against_this_abi_is_told_it_may_proceed() {
    assert_eq!(
        engine_abi_compatible(ENGINE_ABI_VERSION),
        1,
        "a host on exactly this version must be accepted"
    );
    // A host on an older minor of the same major is accepted: minor versions
    // only ever add entry points, so an older host cannot be missing one.
    let older_minor = (ENGINE_ABI_MAJOR << 16) | (ENGINE_ABI_MINOR.saturating_sub(1));
    assert_eq!(engine_abi_compatible(older_minor), 1);
    // A host on a newer minor is refused: the library is behind the host and
    // cannot promise the entry points the host was built against.
    let newer_minor = (ENGINE_ABI_MAJOR << 16) | (ENGINE_ABI_MINOR + 1);
    assert_eq!(
        engine_abi_compatible(newer_minor),
        0,
        "a host on a newer minor must be refused"
    );
    // A different major is refused.
    let other_major = ((ENGINE_ABI_MAJOR + 1) << 16) | ENGINE_ABI_MINOR;
    assert_eq!(
        engine_abi_compatible(other_major),
        0,
        "a different major version must be refused"
    );
}

#[test]
fn the_codec_registry_is_exposed_over_the_abi() {
    let count = engine_codec_count();
    assert_eq!(
        count as usize,
        engine::decode::codecs::all_codecs().len(),
        "the FFI must expose every registry row"
    );

    for index in 0..count {
        let mut row = EngineCodecCapabilityFFI {
            index: 0,
            status: 0,
            support_level: 0,
            decode: 0,
            seek: 0,
            gapless: 0,
            metadata: 0,
            replaygain: 0,
            chapters: 0,
            ebu_r128: 0,
            streaming: 0,
            multichannel_decode: 0,
            is_lossless: 0,
            native_precision_bits: 0,
            name: std::ptr::null(),
        };
        assert_eq!(
            engine_codec_capability(index, &mut row),
            code(EngineStatus::Ok),
            "row {index} must be readable"
        );
        assert_eq!(row.index, index, "the row must report its own index");

        let codec = engine::decode::codecs::all_codecs()[index as usize];
        let capability = engine::decode::codecs::capability(codec);
        assert_eq!(
            row.decode,
            u32::from(capability.decode),
            "row {index} must report the same decode capability as the registry"
        );
        assert_eq!(
            row.seek,
            u32::from(capability.seek),
            "row {index} must report the same seek capability as the registry"
        );
        assert_eq!(
            row.status,
            match capability.status {
                engine::decode::codecs::CodecStatus::Available => codec_status_id::AVAILABLE,
                engine::decode::codecs::CodecStatus::MetadataOnly => codec_status_id::METADATA_ONLY,
                engine::decode::codecs::CodecStatus::DeclaredUnavailable => {
                    codec_status_id::DECLARED_UNAVAILABLE
                }
            },
            "row {index} must report the same status as the registry"
        );

        // The name pointer is owned by the library and stays valid for the
        // life of the process.
        assert!(!row.name.is_null(), "row {index} must carry a name");
        let name = unsafe { CStr::from_ptr(row.name) };
        let name = name
            .to_str()
            .unwrap_or_else(|e| panic!("row {index} has a non-UTF-8 name: {e}"));
        assert_eq!(
            name,
            engine::decode::codecs::codec_name(codec)
                .to_str()
                .expect("a codec name must be valid UTF-8"),
            "row {index} must carry the registry's own name"
        );
    }
}

#[test]
fn an_out_of_range_codec_row_is_refused() {
    let mut row = EngineCodecCapabilityFFI {
        index: 0,
        status: 0,
        support_level: 0,
        decode: 0,
        seek: 0,
        gapless: 0,
        metadata: 0,
        replaygain: 0,
        chapters: 0,
        ebu_r128: 0,
        streaming: 0,
        multichannel_decode: 0,
        is_lossless: 0,
        native_precision_bits: 0,
        name: std::ptr::null(),
    };
    assert_eq!(
        engine_codec_capability(engine_codec_count() + 10, &mut row),
        code(EngineStatus::InvalidArgument),
        "a row past the end of the registry must be refused"
    );
    assert_eq!(
        engine_codec_capability(0, std::ptr::null_mut()),
        code(EngineStatus::InvalidArgument),
        "a null output pointer must be refused"
    );
}

#[test]
fn an_unknown_extension_is_reported_as_not_found() {
    let mut index = 0u32;
    let ext = CString::new("xyz").expect("the extension must be a C string");
    assert_eq!(
        engine_codec_index_for_extension(ext.as_ptr(), &mut index),
        code(EngineStatus::NotFound),
        "an extension the registry does not carry must be NotFound, not an error"
    );

    let ext = CString::new("flac").expect("the extension must be a C string");
    assert_eq!(
        engine_codec_index_for_extension(ext.as_ptr(), &mut index),
        code(EngineStatus::Ok)
    );
    assert_eq!(
        index as usize,
        engine::decode::codecs::all_codecs()
            .iter()
            .position(|c| *c == engine::decode::codecs::Codec::Flac)
            .expect("FLAC must be a registry row")
    );

    assert_eq!(
        engine_codec_index_for_extension(std::ptr::null(), &mut index),
        code(EngineStatus::InvalidArgument),
        "a null extension must be InvalidArgument"
    );
    assert_eq!(
        engine_codec_index_for_extension(ext.as_ptr(), std::ptr::null_mut()),
        code(EngineStatus::InvalidArgument),
        "a null output pointer must be InvalidArgument"
    );
}

#[test]
fn a_codec_string_resolves_to_its_registry_row() {
    let mut index = 0u32;
    let name = CString::new("flac").expect("the name must be a C string");
    assert_eq!(
        engine_codec_index_for_codec_string(name.as_ptr(), &mut index),
        code(EngineStatus::Ok)
    );

    let name = CString::new("not a codec").expect("the name must be a C string");
    assert_eq!(
        engine_codec_index_for_codec_string(name.as_ptr(), &mut index),
        code(EngineStatus::NotFound)
    );
}

#[test]
fn a_null_handle_is_refused_rather_than_dereferenced() {
    let null = std::ptr::null_mut::<EngineHandleFFI>();
    assert_eq!(
        engine_play(null),
        code(EngineStatus::InvalidHandle),
        "playing a null handle must be refused"
    );
    assert_eq!(
        engine_pause(null),
        code(EngineStatus::InvalidHandle),
        "pausing a null handle must be refused"
    );
    assert_eq!(
        engine_seek(null, 1.0),
        code(EngineStatus::InvalidHandle),
        "seeking a null handle must be refused"
    );
    assert_eq!(
        engine_set_volume(null, 0.5),
        code(EngineStatus::InvalidHandle),
        "setting the volume of a null handle must be refused"
    );
    assert_eq!(
        engine_position_secs(null),
        -1.0,
        "the position of a null handle must be the sentinel"
    );
    assert_eq!(
        engine_playlist_len(null),
        -1,
        "the queue length of a null handle must be the sentinel"
    );
    assert_eq!(
        engine_playback_state(null),
        code(EngineStatus::InvalidHandle),
        "the state query must report the typed status for a null handle"
    );
}

#[test]
fn a_null_path_is_refused_rather_than_dereferenced() {
    let handle = engine_create(backend_id::NULL);
    assert!(!handle.is_null(), "the null backend must create an engine");
    assert_eq!(
        engine_open_file(handle, std::ptr::null()),
        code(EngineStatus::InvalidArgument),
        "a null path must be InvalidArgument"
    );
    engine_destroy(handle);
}

#[test]
fn an_engine_can_be_created_and_destroyed_over_the_abi() {
    let handle = engine_create(backend_id::NULL);
    assert!(!handle.is_null(), "the null backend must create an engine");
    assert_eq!(engine_play(handle), code(EngineStatus::Ok));
    assert_eq!(engine_pause(handle), code(EngineStatus::Ok));
    assert!(engine_position_secs(handle) >= 0.0);
    assert!(engine_playlist_len(handle) >= 0);
    engine_destroy(handle);
    // Destroying twice is a no-op rather than a double free.
    engine_destroy(handle);
    engine_destroy(std::ptr::null_mut());
}

#[test]
fn an_unknown_backend_id_creates_no_engine() {
    assert!(
        engine_create(9999).is_null(),
        "an unrecognised backend id must create nothing"
    );
}

#[test]
fn a_failure_inside_the_engine_never_unwinds_across_the_boundary() {
    // Every one of these calls is a path where the library could plausibly
    // fail in a way that used to be a panic. The FFI converts each into a
    // status code. If one of them panicked *out* of the boundary the process
    // would abort (Rust 1.71 and later abort on an unwind out of an
    // `extern "C"` function), so this sweep failing catastrophically *is* the
    // containment check — a panic would take the test binary with it.
    let handle = engine_create(backend_id::NULL);
    assert!(!handle.is_null());

    let calls: Vec<HostileCall<'_>> = vec![
        (
            "play a null handle",
            Box::new(|| engine_play(std::ptr::null_mut())),
        ),
        (
            "open a null path",
            Box::new(|| engine_open_file(handle, std::ptr::null())),
        ),
        (
            "open a path that is not valid UTF-8",
            Box::new(|| {
                let bytes = [0x66u8, 0x80, 0x6fu8, 0x00];
                engine_open_file(handle, bytes.as_ptr() as *const c_char)
            }),
        ),
        (
            "seek a null handle to a NaN position",
            Box::new(|| engine_seek(std::ptr::null_mut(), f32::NAN)),
        ),
        (
            "set the volume of a null handle to NaN",
            Box::new(|| engine_set_volume(std::ptr::null_mut(), f32::NAN)),
        ),
        (
            "read a codec row past the end of the registry",
            Box::new(|| {
                let mut row = EngineCodecCapabilityFFI {
                    index: 0,
                    status: 0,
                    support_level: 0,
                    decode: 0,
                    seek: 0,
                    gapless: 0,
                    metadata: 0,
                    replaygain: 0,
                    chapters: 0,
                    ebu_r128: 0,
                    streaming: 0,
                    multichannel_decode: 0,
                    is_lossless: 0,
                    native_precision_bits: 0,
                    name: std::ptr::null(),
                };
                engine_codec_capability(u32::MAX, &mut row)
            }),
        ),
    ];

    for (what, call) in calls {
        // `catch_unwind` is belt and braces: the FFI already catches panics
        // internally, so nothing should reach this layer either.
        let status = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
            .unwrap_or_else(|_| panic!("{what} unwound out of the FFI boundary"));
        assert_ne!(
            status,
            code(EngineStatus::Error),
            "{what} must not report a generic error; it must report a typed one"
        );
    }

    // And the handle is still usable: the boundary did not corrupt anything.
    assert_eq!(engine_play(handle), code(EngineStatus::Ok));
    engine_destroy(handle);
}

#[test]
fn the_status_codes_are_stable() {
    // A host compares these integers, so their values are part of the ABI.
    assert_eq!(code(EngineStatus::Ok), 0);
    assert_eq!(code(EngineStatus::Error), -1);
    assert_eq!(code(EngineStatus::InvalidHandle), -2);
    assert_eq!(code(EngineStatus::InvalidArgument), -3);
    assert_eq!(code(EngineStatus::EngineNotRunning), -4);
    assert_eq!(code(EngineStatus::NotFound), -5);
}

#[test]
fn the_support_levels_are_stable() {
    assert_eq!(codec_support_id::FULLY_SUPPORTED, 0);
    assert_eq!(codec_support_id::DECODE_ONLY, 1);
    assert_eq!(codec_support_id::METADATA_ONLY, 2);
    assert_eq!(codec_support_id::EXPERIMENTAL, 3);
    assert_eq!(codec_support_id::UNAVAILABLE, 4);
    assert_eq!(codec_status_id::AVAILABLE, 0);
    assert_eq!(codec_status_id::METADATA_ONLY, 1);
    assert_eq!(codec_status_id::DECLARED_UNAVAILABLE, 2);
}

#[test]
fn the_fixture_helper_is_usable_from_a_c_abi_suite() {
    // The common module is shared with the other suites; this keeps the
    // `mod common;` declaration honest.
    let samples = common::sine(1_000.0, 0.5, 256, common::SAMPLE_RATE);
    assert_eq!(samples.len(), 512);
}

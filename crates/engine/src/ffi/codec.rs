use super::*;

/// Number of rows in the codec capability registry.
///
/// The registry is fixed at build time in a stable order, so a host can
/// enumerate it once with
/// `for i in 0..engine_codec_count() { engine_codec_capability(i, &row); }`.
/// Never `0` — the registry always has rows.
#[no_mangle]
pub extern "C" fn engine_codec_count() -> u32 {
    codec_registry::all_codecs().len() as u32
}

/// Fill `out` with registry row `index`.
///
/// Returns `Ok`, or `InvalidArgument` when `out` is NULL or `index` is not a
/// row of the registry (see [`engine_codec_count`]). The written struct is
/// self-contained: the `name` pointer stays valid for the life of the
/// process.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_codec_capability(index: u32, out: *mut EngineCodecCapabilityFFI) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_codec_capability_impl(index, out)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
fn engine_codec_capability_impl(index: u32, out: *mut EngineCodecCapabilityFFI) -> i32 {
    if out.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let Some(&codec) = codec_registry::all_codecs().get(index as usize) else {
        return EngineStatus::InvalidArgument as i32;
    };
    let cap = codec_registry::capability(codec);
    let row = EngineCodecCapabilityFFI {
        index,
        status: match cap.status {
            CodecStatus::Available => codec_status_id::AVAILABLE,
            CodecStatus::MetadataOnly => codec_status_id::METADATA_ONLY,
            CodecStatus::DeclaredUnavailable => codec_status_id::DECLARED_UNAVAILABLE,
        },
        support_level: match cap.support_level {
            CodecSupportLevel::FullySupported => codec_support_id::FULLY_SUPPORTED,
            CodecSupportLevel::DecodeOnly => codec_support_id::DECODE_ONLY,
            CodecSupportLevel::MetadataOnly => codec_support_id::METADATA_ONLY,
            CodecSupportLevel::Experimental => codec_support_id::EXPERIMENTAL,
            CodecSupportLevel::Unavailable => codec_support_id::UNAVAILABLE,
        },
        decode: u32::from(cap.decode),
        seek: u32::from(cap.seek),
        gapless: u32::from(cap.gapless),
        metadata: u32::from(cap.metadata),
        replaygain: u32::from(cap.replaygain),
        chapters: u32::from(cap.chapters),
        ebu_r128: u32::from(cap.ebu_r128),
        streaming: u32::from(cap.streaming),
        multichannel_decode: u32::from(cap.multichannel_decode),
        is_lossless: u32::from(cap.is_lossless),
        native_precision_bits: cap.native_precision_bits.map_or(-1, |bits| bits as i32),
        name: codec_registry::codec_name(codec).as_ptr(),
    };
    unsafe { out.write(row) };
    EngineStatus::Ok as i32
}

/// Resolve a file extension (case-insensitive, without the dot) to a registry
/// index.
///
/// Writes the index and returns `Ok`; returns `NotFound` for an extension the
/// registry does not carry and `InvalidArgument` for NULL pointers or
/// non-UTF-8 input. A failed lookup leaves `*out_index` untouched.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_codec_index_for_extension(ext: *const c_char, out_index: *mut u32) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_codec_index_for_extension_impl(ext, out_index)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
fn engine_codec_index_for_extension_impl(ext: *const c_char, out_index: *mut u32) -> i32 {
    if ext.is_null() || out_index.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let ext = match unsafe { CStr::from_ptr(ext) }.to_str() {
        Ok(s) => s,
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    // Non-null, checked above.
    let out_index = unsafe { &mut *out_index };
    resolve_codec_index(codec_registry::for_extension(ext), out_index)
}

/// Resolve a codec identifier string (as reported by decoder diagnostics,
/// e.g. `"flac"`, `"vorbis"`, `"s16"`) to a registry index.
///
/// Matching is the registry's own `for_codec_string` rule (case-insensitive
/// substring), so it accepts what diagnostics actually print. Returns `Ok`,
/// `NotFound`, or `InvalidArgument` for NULL/invalid pointers, exactly like
/// [`engine_codec_index_for_extension`].
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[no_mangle]
pub extern "C" fn engine_codec_index_for_codec_string(
    name: *const c_char,
    out_index: *mut u32,
) -> i32 {
    ffi_boundary(
        std::panic::AssertUnwindSafe(|| engine_codec_index_for_codec_string_impl(name, out_index)),
        EngineStatus::Error as i32,
    )
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
fn engine_codec_index_for_codec_string_impl(name: *const c_char, out_index: *mut u32) -> i32 {
    if name.is_null() || out_index.is_null() {
        return EngineStatus::InvalidArgument as i32;
    }
    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return EngineStatus::InvalidArgument as i32,
    };
    // Non-null, checked above.
    let out_index = unsafe { &mut *out_index };
    resolve_codec_index(codec_registry::for_codec_string(name), out_index)
}

/// Write the registry index of `codec` (or the right status) into
/// `out_index`.
fn resolve_codec_index(codec: Option<codec_registry::Codec>, out_index: &mut u32) -> i32 {
    let index = match codec {
        Some(codec) => match codec_registry::all_codecs()
            .iter()
            .position(|&c| c == codec)
        {
            Some(index) => index,
            // Reachable only if `for_extension`/`for_codec_string` learn a
            // codec that `all_codecs` never lists: an internal
            // inconsistency, not a bad query.
            None => return EngineStatus::Error as i32,
        },
        None => return EngineStatus::NotFound as i32,
    };
    *out_index = index as u32;
    EngineStatus::Ok as i32
}

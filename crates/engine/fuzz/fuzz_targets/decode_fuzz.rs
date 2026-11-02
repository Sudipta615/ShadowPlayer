#![no_main]

//! Fuzz the decoder dispatch and the first frames of every backend.
//!
//! The first input byte selects an extension, so one corpus exercises WAV,
//! FLAC, MP3, Ogg/Vorbis, Opus, DSD, APE, TTA, WavPack and the ISO-MP4 reader
//! instead of multiplying every input by the number of formats. The target
//! stops after a few chunks: it is looking for panics and arithmetic errors in
//! header/first-frame parsing, not for a full-album decode per input.

use engine::decode::Decoder;
use libfuzzer_sys::fuzz_target;

const EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "ogg", "oga", "opus", "dsf", "dff", "ape", "mac", "tta", "wv", "m4a",
    "aac", "aiff", "mkv",
];

fuzz_target!(|data: &[u8]| {
    let Some((&selector, payload)) = data.split_first() else {
        return;
    };
    let extension = EXTENSIONS[selector as usize % EXTENSIONS.len()];

    let Ok(mut decoder) = Decoder::open_memory(payload.to_vec(), Some(extension)) else {
        return;
    };

    // These read the same headers through different accessors; a parser that
    // survives `open` can still panic when asked for its duration or format.
    let _ = decoder.info();
    let _ = decoder.format_info();
    let _ = decoder.duration_secs();

    for _ in 0..8 {
        if decoder.decode_next(4096).is_err() {
            break;
        }
    }
});

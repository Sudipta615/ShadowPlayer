#![no_main]

//! Fuzz the LRC parser and the plain-text fallback.
//!
//! Both entry points accept arbitrary UTF-8, including malformed timestamps,
//! metadata tags, multiple timestamps per line and an `[offset:]` that shifts
//! every line. The parser returns data rather than errors, so the target only
//! needs to prove it never panics.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    let _ = lyrics::parse_lrc(text);
    let _ = lyrics::interpret_lyrics_text(text);
});

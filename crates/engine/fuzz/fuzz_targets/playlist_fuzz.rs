#![no_main]

//! Fuzz the three playlist parsers.
//!
//! A playlist is untrusted input and the parsers deliberately do no I/O, so
//! this target is fast enough to run for ten minutes. Both the relative-path
//! (`base_dir = Some`) and network-string (`base_dir = None`) call shapes are
//! exercised because path resolution is the format's main correctness
//! property.

use engine::playlist::io::PlaylistFormat;
use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };

    for format in [
        PlaylistFormat::M3u,
        PlaylistFormat::Pls,
        PlaylistFormat::Xspf,
    ] {
        let _ = format.parse(text, Some(Path::new("/music/library")));
        let _ = format.parse(text, None);
    }
});

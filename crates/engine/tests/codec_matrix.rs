//! The codec registry against the real decode dispatch.
//!
//! `decode::codecs` is the engine's declared statement of what it can
//! decode, seek and measure. The registry is only worth anything if it agrees
//! with the dispatch that actually opens files, so this suite checks both
//! directions:
//!
//! * a codec the registry says it can decode must not be refused by the
//!   dispatch; and
//! * a codec the registry says it *cannot* decode must be refused with a
//!   typed error whose message **names the format** — never a silent
//!   "unknown format", which would leave the user with no idea what to
//!   install.
//!
//! The deliberately-unsupported formats (Musepack and TAK) are checked
//! explicitly: they exist in the registry precisely so they can be *named*
//! rather than ignored.

use std::io::Write;
use std::path::{Path, PathBuf};

mod common;

use engine::decode::codecs::{
    all_codecs, capability, codec_name, for_codec_string, for_extension, Codec, CodecStatus,
};

/// The phrase every refusal the *dispatch itself* writes carries.
///
/// A refusal that contains it is the engine saying "this build cannot open
/// this format"; a refusal that does not contain it came from somewhere
/// downstream (a demuxer probe, a missing track), which is a different
/// diagnosis entirely.
const DISPATCH_REFUSAL: &str = "in this build";

/// One representative extension per codec the registry knows.
fn extension_for(codec: Codec) -> Option<&'static str> {
    Some(match codec {
        Codec::Mp3 => "mp3",
        Codec::Flac => "flac",
        Codec::OggVorbis => "ogg",
        Codec::Opus => "opus",
        Codec::Wav => "wav",
        Codec::Aac => "m4a",
        Codec::Alac => "alac",
        Codec::Ape => "ape",
        Codec::Dsd => "dsf",
        Codec::Pcm => "pcm",
        Codec::Aiff => "aiff",
        Codec::Mka => "mka",
        Codec::WavPack => "wv",
        Codec::Musepack => "mpc",
        Codec::Tak => "tak",
        Codec::Tta => "tta",
        Codec::Unknown => return None,
    })
}

/// A file with the given extension, holding bytes that are deliberately not
/// a real audio stream.
fn junk_file(extension: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "engine_codec_matrix_{}_{}.{extension}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut file = std::fs::File::create(&path).expect("a temp file must be creatable");
    file.write_all(b"not a real audio stream")
        .expect("the temp file must be writable");
    path
}

/// Open `path` and describe how the dispatch answered.
fn dispatch(path: &Path) -> Result<(), engine::decode::DecodeError> {
    engine::decode::Decoder::open(path).map(|_| ())
}

#[test]
fn every_codec_in_the_registry_is_reachable_by_extension() {
    for codec in all_codecs() {
        let ext = extension_for(*codec)
            .unwrap_or_else(|| panic!("{codec:?} has no representative extension in this suite"));
        assert_eq!(
            for_extension(ext),
            Some(*codec),
            ".{ext} must map back to {codec:?}"
        );
    }
}

#[test]
fn the_registry_never_claims_more_than_the_build_delivers() {
    for codec in all_codecs() {
        let cap = capability(*codec);
        assert_eq!(cap.codec, *codec);
        match cap.status {
            CodecStatus::Available => assert!(
                cap.decode,
                "{codec:?} is Available, so it must claim decode = true"
            ),
            CodecStatus::MetadataOnly | CodecStatus::DeclaredUnavailable => assert!(
                !cap.decode,
                "{codec:?} is {:?}, so it must not claim decode = true",
                cap.status
            ),
        }
    }
}

#[test]
fn a_codec_the_registry_can_decode_is_not_refused_by_the_dispatch() {
    for codec in all_codecs() {
        let cap = capability(*codec);
        if cap.status != CodecStatus::Available || !cap.decode {
            continue;
        }
        let ext = extension_for(*codec)
            .unwrap_or_else(|| panic!("{codec:?} has no representative extension in this suite"));
        let path = junk_file(ext);
        let result = dispatch(&path);
        let _ = std::fs::remove_file(&path);

        match result {
            Ok(()) => {}
            Err(engine::decode::DecodeError::UnsupportedFormat(message)) => assert!(
                !message.contains(DISPATCH_REFUSAL),
                "{codec:?} is Available + decode in the registry, so the \
                 dispatch must not refuse it; it said: {message}"
            ),
            Err(other) => {
                // A downstream failure (the bytes really are junk) is the
                // expected outcome for a file that is not a real stream.
                assert!(
                    !other.to_string().contains(DISPATCH_REFUSAL),
                    "{codec:?} must not be refused by the dispatch, it said: {other}"
                );
            }
        }
    }
}

#[test]
fn a_codec_the_registry_cannot_decode_is_refused_by_name() {
    for codec in all_codecs() {
        let cap = capability(*codec);
        if cap.status == CodecStatus::Available && cap.decode {
            continue;
        }
        let ext = extension_for(*codec)
            .unwrap_or_else(|| panic!("{codec:?} has no representative extension in this suite"));
        let path = junk_file(ext);
        let result = dispatch(&path);
        let _ = std::fs::remove_file(&path);

        let message = match result {
            Ok(()) => panic!(
                "{codec:?} is {:?} + decode = false in the registry, so the \
                 dispatch must refuse it; it opened the file",
                cap.status
            ),
            Err(engine::decode::DecodeError::UnsupportedFormat(message)) => message,
            Err(other) => panic!(
                "{codec:?} is {:?} + decode = false in the registry, so it must \
                 be refused as an unsupported format; it failed elsewhere: {other}",
                cap.status
            ),
        };
        assert!(
            message.contains(DISPATCH_REFUSAL),
            "{codec:?}: the refusal must explain that the format is not in this \
             build, it said: {message}"
        );
        assert!(
            message.len() > DISPATCH_REFUSAL.len(),
            "{codec:?}: the refusal must explain itself, not be a bare refusal: \
             {message}"
        );
    }
}

#[test]
fn the_musepack_refusal_names_musepack() {
    let path = junk_file("mpc");
    let result = dispatch(&path);
    let _ = std::fs::remove_file(&path);

    let message = match result {
        Ok(()) => panic!("a .mpc file must not open: this build has no Musepack decoder"),
        Err(engine::decode::DecodeError::UnsupportedFormat(message)) => message,
        Err(other) => panic!("a .mpc file must be refused as an unsupported format: {other}"),
    };
    assert!(
        message.contains("Musepack"),
        "the Musepack refusal must name the format, it said: {message}"
    );
    assert!(
        message.contains(".mpc"),
        "the Musepack refusal must name the file extension, it said: {message}"
    );
    assert!(
        message.contains(DISPATCH_REFUSAL),
        "the Musepack refusal must say the format is not in this build, it \
         said: {message}"
    );
    assert_eq!(
        capability(Codec::Musepack).status,
        CodecStatus::DeclaredUnavailable,
        "Musepack must stay declared-but-unavailable"
    );
}

#[test]
fn the_tak_refusal_names_tak() {
    let path = junk_file("tak");
    let result = dispatch(&path);
    let _ = std::fs::remove_file(&path);

    let message = match result {
        Ok(()) => panic!("a .tak file must not open: this build has no TAK decoder"),
        Err(engine::decode::DecodeError::UnsupportedFormat(message)) => message,
        Err(other) => panic!("a .tak file must be refused as an unsupported format: {other}"),
    };
    assert!(
        message.contains("TAK"),
        "the TAK refusal must name the format, it said: {message}"
    );
    assert!(
        message.contains(".tak"),
        "the TAK refusal must name the file extension, it said: {message}"
    );
    assert!(
        message.contains(DISPATCH_REFUSAL),
        "the TAK refusal must say the format is not in this build, it said: {message}"
    );
    assert_eq!(
        capability(Codec::Tak).status,
        CodecStatus::DeclaredUnavailable,
        "TAK must stay declared-but-unavailable"
    );
}

#[test]
fn the_registry_names_are_unique_and_round_trip() {
    let mut names: Vec<String> = Vec::new();
    for codec in all_codecs() {
        let name = codec_name(*codec);
        let text = name
            .to_str()
            .expect("a codec name must be valid UTF-8")
            .to_string();
        assert!(!text.is_empty(), "{codec:?} must have a non-empty name");
        assert!(
            !names.contains(&text),
            "the codec name {text} is used twice"
        );
        names.push(text);
    }

    // The name is also the identity the FFI exposes, and it must map back
    // through the codec-string lookup.
    //
    // Two rows are exempt, and the exemption is the registry's own
    // distinction: their name is a *container* name, not a codec name. The
    // WAV container carries PCM (the codec string a demuxer reports is
    // "pcm" / "s16" / …), and the Matroska container carries an inner codec
    // that is only known after the container is opened. Their names are
    // identity names and are not expected to come back through the
    // codec-string lookup.
    const CONTAINER_ROWS: [Codec; 2] = [Codec::Wav, Codec::Mka];
    for codec in all_codecs() {
        if CONTAINER_ROWS.contains(codec) {
            continue;
        }
        let name = codec_name(*codec)
            .to_str()
            .expect("a codec name must be valid UTF-8");
        assert_eq!(
            for_codec_string(name),
            Some(*codec),
            "the name {name} must map back to {codec:?}"
        );
    }
}

#[test]
fn an_unrecognised_extension_is_not_in_the_registry() {
    assert_eq!(for_extension("xyz"), None);
    assert_eq!(for_extension(""), None);
    assert_eq!(for_codec_string("not a codec"), None);
}

#[test]
fn a_real_wav_file_decodes_to_the_samples_that_were_written() {
    // The registry claims WAV is decodable; here it actually is.
    let frames = 4096;
    let samples = crate::common::sine(1_000.0, 0.5, frames, crate::common::SAMPLE_RATE);
    let file = crate::common::write_wav(
        "codec_matrix",
        &samples,
        crate::common::CHANNELS,
        crate::common::SAMPLE_RATE,
    );

    let mut decoder = engine::decode::Decoder::open(file.path()).expect("a WAV must open");
    assert_eq!(decoder.info().sample_rate, crate::common::SAMPLE_RATE);
    assert_eq!(decoder.info().channels, crate::common::CHANNELS as usize);

    let mut decoded = Vec::new();
    loop {
        match decoder.decode_next(1024) {
            Ok(chunk) => decoded.extend_from_slice(&chunk.samples),
            Err(engine::decode::DecodeError::EndOfStream) => break,
            Err(other) => panic!("decoding the WAV failed: {other}"),
        }
    }
    assert_eq!(decoded.len(), samples.len());
    for (i, (a, b)) in samples.iter().zip(decoded.iter()).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "decoded sample {i} differs: {a} -> {b}"
        );
    }
}

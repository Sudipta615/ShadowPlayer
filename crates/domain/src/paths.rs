//! A reversible text form for filesystem paths.
//!
//! A path is not text. On Unix it is a byte string that need not be valid
//! UTF-8, and `to_string_lossy()` replaces every byte it cannot decode with
//! U+FFFD. Two different files whose names differ only in such bytes then
//! produced the same string — and because the string is both the database key
//! and the seed of the track id, the second file overwrote the first row
//! instead of being indexed next to it.
//!
//! [`encode_path_key`] escapes exactly the bytes that are not valid UTF-8 as
//! `%XX`, so nothing is lost; [`decode_path_key`] turns the key back into the
//! real path. Ordinary text passes through unchanged, which matters twice
//! over: keys for everyday paths stay as they always were (an existing row is
//! found at the same key after an upgrade), and the database stays readable.
//!
//! The one piece of bookkeeping is `%` itself. A `%` is escaped to `%25` only
//! when what follows it would otherwise be read as one of our escapes — that
//! is, when it is `25` or a byte value `80`..`FF`. `100% real.flac` and
//! `song%20name.flac` keep their keys; `a%8A` (text) becomes `a%258A` so that
//! it cannot be confused with a file whose name really contains the byte
//! `0x8A`.
//!
//! On Windows a file name is UTF-16 and Win32 rejects unpaired surrogates, so
//! the lossy string is already faithful there and both functions are the plain
//! text form.

use std::path::{Path, PathBuf};

/// The text form of `path`, used as the database key and as the seed for a
/// track id. Reversible: see [`decode_path_key`].
#[cfg(unix)]
pub fn encode_path_key(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;

    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                push_text(text, &mut out);
                break;
            }
            Err(error) => {
                let valid = &rest[..error.valid_up_to()];
                // `valid_up_to` guarantees this is UTF-8; the fallback is only
                // there to keep the function total.
                if let Ok(text) = std::str::from_utf8(valid) {
                    push_text(text, &mut out);
                }
                // `error_len()` is `None` for a sequence that is truncated by
                // the end of the input; escape those bytes too.
                let bad_len = error
                    .error_len()
                    .unwrap_or(rest.len() - error.valid_up_to());
                for byte in &rest[error.valid_up_to()..error.valid_up_to() + bad_len] {
                    out.push_str(&format!("%{byte:02X}"));
                }
                rest = &rest[error.valid_up_to() + bad_len..];
            }
        }
    }

    out
}

/// Rebuild the path that [`encode_path_key`] was given.
///
/// Keys written before this encoding existed — plain `to_string_lossy` text —
/// decode to themselves: the escapes are recognised only where the encoder
/// could have produced them.
#[cfg(unix)]
pub fn decode_path_key(key: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;

    let raw = key.as_bytes();
    let mut bytes = Vec::with_capacity(raw.len());
    let mut index = 0;

    while index < raw.len() {
        if raw[index] == b'%' {
            if let [hi, lo, ..] = &raw[index + 1..] {
                if let Some(byte) = decoded_escape(*hi, *lo) {
                    bytes.push(byte);
                    index += 3;
                    continue;
                }
            }
        }
        bytes.push(raw[index]);
        index += 1;
    }

    PathBuf::from(std::ffi::OsString::from_vec(bytes))
}

/// A `%` is only escaped when the text after it would be read as an escape.
#[cfg(unix)]
fn push_text(text: &str, out: &mut String) {
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let mut lookahead = chars.clone();
        let first = lookahead.next();
        let second = lookahead.next();
        match (first, second) {
            // `%25` is the escape for a literal percent sign.
            (Some('2'), Some('5')) => out.push_str("%25"),
            // `%XX` with `XX` >= 80 is the escape for a byte that could not be
            // decoded as UTF-8.
            (Some(first), Some(second))
                if second.is_ascii_hexdigit()
                    && matches!(first, '8' | '9' | 'A' | 'B' | 'C' | 'D' | 'E' | 'F') =>
            {
                out.push_str("%25");
            }
            // Anything else is literal text and stays exactly as it is.
            _ => out.push('%'),
        }
    }
}

/// The byte an escape stands for, if it is one of ours.
#[cfg(unix)]
fn decoded_escape(hi: u8, lo: u8) -> Option<u8> {
    let (hi, lo) = (hex_digit(hi)?, hex_digit(lo)?);
    let byte = (hi << 4) | lo;
    // Only `%25` (the escaped percent sign) and `%80`..`%FF` (bytes that were
    // not valid UTF-8) are escapes. An ASCII escape cannot come from the
    // encoder, so `song%20name` keeps its `%20` when decoded.
    (byte == b'%' || byte >= 0x80).then_some(byte)
}

#[cfg(unix)]
fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// A Win32 file name is valid UTF-16, so the plain text form is already
/// faithful. See the module documentation.
#[cfg(not(unix))]
pub fn encode_path_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// See [`encode_path_key`].
#[cfg(not(unix))]
pub fn decode_path_key(key: &str) -> PathBuf {
    PathBuf::from(key)
}

#[cfg(all(test, unix))]
mod tests {
    use super::{decode_path_key, encode_path_key};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    fn path_from_bytes(bytes: &[u8]) -> PathBuf {
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }

    #[test]
    fn ordinary_paths_are_their_own_key() {
        for text in [
            "/music/Artist/Album/01 Song.flac",
            "/music/caf\u{e9}/\u{65e5}\u{672c}\u{8a9e}.opus",
            "/music/song%20name.flac",
            "/music/100% real.flac",
            "/music/a%9",
        ] {
            let path = Path::new(text);
            assert_eq!(encode_path_key(path), text, "key must not change");
            assert_eq!(decode_path_key(text), path, "and must decode to itself");
        }
    }

    #[test]
    fn bytes_that_are_not_utf8_survive_the_round_trip() {
        // 0x8A alone, a broken sequence in the middle, and one truncated by
        // the end of the name.
        let path = path_from_bytes(b"/music/\x8a/br\xc3ken/\xfe\xff/fi\xe2");
        let key = encode_path_key(&path);
        assert!(key.contains("%8A"), "invalid byte escaped: {key}");
        assert_eq!(decode_path_key(&key), path);
    }

    #[test]
    fn two_paths_that_differ_only_in_an_invalid_byte_stay_distinct() {
        let a = path_from_bytes(b"/music/song-\x8a.flac");
        let b = path_from_bytes(b"/music/song-\x8b.flac");
        assert_ne!(encode_path_key(&a), encode_path_key(&b));
        assert_ne!(
            encode_path_key(&a),
            encode_path_key(&Path::new("/music/song-\u{fffd}.flac")),
            "and neither collides with the lossy text"
        );
    }

    /// Text that merely looks like an escape is escaped in turn, so the
    /// encoding stays injective.
    #[test]
    fn text_that_looks_like_an_escape_is_not_one() {
        let text = Path::new("/music/a%8A.mp3");
        let key = encode_path_key(text);
        assert_eq!(key, "/music/a%258A.mp3");
        assert_eq!(decode_path_key(&key), text);

        let raw = path_from_bytes(b"/music/a\x8a.mp3");
        assert_ne!(key, encode_path_key(&raw), "raw byte and lookalike differ");
        assert_eq!(decode_path_key(&encode_path_key(&raw)), raw);
    }

    #[test]
    fn stray_percent_signs_are_literal() {
        assert_eq!(decode_path_key("/music/100% sure.flac"), PathBuf::from("/music/100% sure.flac"));
        assert_eq!(decode_path_key("/a%2"), PathBuf::from("/a%2"));
        assert_eq!(decode_path_key("%"), PathBuf::from("%"));
    }
}

//! Byte-to-text decoding for the sidecar files a music library is full of.
//!
//! Lyrics, `.cue` sheets and `.m3u` playlists are text files written by other
//! programs — Winamp, foobar2000, a phone, a Japanese tag editor — and plenty of
//! them are not UTF-8. Reading them with `fs::read_to_string` fails on any
//! non-UTF-8 byte and the file simply vanishes from the library with no
//! explanation. This decodes the encodings that actually turn up, in order of
//! confidence, and never fails.

/// Decode bytes that are known to be text.
///
/// Tried in order: a UTF-8 BOM, a UTF-16 BOM, strict UTF-8, then Windows-1252
/// (which is a superset of Latin-1 in the printable range and the encoding
/// Windows tools write). The return value is `(text, guessed)` where `guessed`
/// is `true` when the bytes were not valid UTF-8 and the fallback was used, so
/// a caller can log it.
pub fn decode_text_bytes(bytes: &[u8]) -> (String, bool) {
    // UTF-8 with BOM.
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(rest).into_owned(), false);
    }

    // UTF-16, which is what Windows tools write when they "save as Unicode".
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return (decode_utf16(rest, true), true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return (decode_utf16(rest, false), true);
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (decode_cp1252(bytes), true),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Decode as Windows-1252: Latin-1, except that 0x80..=0x9F carry the
/// typographic punctuation Windows editors put in lyrics (curly quotes, em
/// dashes, ellipses) instead of control codes.
fn decode_cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];

    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_passes_through_untouched() {
        let (text, guessed) = decode_text_bytes("café — naïve".as_bytes());
        assert_eq!(text, "café — naïve");
        assert!(!guessed);
    }

    #[test]
    fn a_utf8_bom_is_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("hello".as_bytes());
        assert_eq!(decode_text_bytes(&bytes), ("hello".to_string(), false));
    }

    #[test]
    fn utf16_with_a_bom_is_decoded() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "スピッツ".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let (text, guessed) = decode_text_bytes(&bytes);
        assert_eq!(text, "スピッツ");
        assert!(guessed);
    }

    /// The bytes Windows editors write for curly quotes and em dashes.
    #[test]
    fn latin1_cp1252_lyrics_still_read() {
        // "Don't Stop – 0x92 (right single quote), 0x96 (en dash)"
        let bytes = [b'D', b'o', b'n', 0x92, b't', b' ', 0x96, b' ', b'x'];
        let (text, guessed) = decode_text_bytes(&bytes);
        assert_eq!(text, "Don\u{2019}t \u{2013} x");
        assert!(guessed);
    }

    #[test]
    fn plain_latin1_accents_decode() {
        let (text, guessed) = decode_text_bytes(&[b'c', b'a', b'f', 0xE9]);
        assert_eq!(text, "café");
        assert!(guessed);
    }
}

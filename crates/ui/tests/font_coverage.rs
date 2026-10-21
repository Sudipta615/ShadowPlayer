//! Guards the bundled fonts against the class of bug where a UI label asks
//! for a glyph the font does not have.
//!
//! The full-screen transport used to draw Unicode media symbols (`⏮`, `⏸`,
//! `⏭`) from Inter, which has no glyphs for them, so the buttons rendered as
//! `.notdef` boxes. The icons are now drawn from SVG, and this test makes the
//! whole class impossible to reintroduce:
//!
//! 1. parse the cmap table of every bundled font to learn its codepoints;
//! 2. walk `crates/ui/src/**/*.rs` and extract every string literal outside
//!    the test modules;
//! 3. require every non-ASCII codepoint in those strings to be present in the
//!    font, unless it is in [`VECTOR_DRAWN`] (characters painted as paths
//!    instead of text).
//!
//! It also checks that every SVG referenced by `icons.rs` exists, so a rename
//! cannot silently leave a button without an icon.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Codepoints that are deliberately painted as vector paths rather than text,
/// so the font does not need them. Keep this list tiny and justified: a glyph
/// belongs here only if there is code that draws it, not merely because the
/// font lacks it.
const VECTOR_DRAWN: &[char] = &[];

#[test]
fn every_user_facing_character_has_a_glyph() {
    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fonts = [
        "assets/fonts/Inter-Regular.ttf",
        "assets/fonts/Inter-Medium.ttf",
    ];

    let mut all_codepoints: Option<BTreeSet<u32>> = None;
    for font in fonts {
        let data = fs::read(ui_dir.join(font)).unwrap_or_else(|e| {
            panic!("could not read bundled font {font}: {e}");
        });
        let codepoints = cmap_codepoints(&data).unwrap_or_else(|e| {
            panic!("could not parse the cmap table of {font}: {e}");
        });
        assert!(
            codepoints.contains(&u32::from('A')),
            "{font} does not cover basic Latin"
        );
        assert!(
            codepoints.contains(&u32::from('…')),
            "{font} does not cover the ellipsis the UI relies on"
        );
        all_codepoints = Some(match all_codepoints {
            Some(mut previous) => {
                previous.retain(|cp| codepoints.contains(cp));
                previous
            }
            None => codepoints,
        });
    }
    let supported = all_codepoints.expect("at least one font");
    let vector_drawn: BTreeSet<u32> = VECTOR_DRAWN.iter().copied().map(u32::from).collect();

    let mut failures = Vec::new();
    for path in rust_sources(&ui_dir.join("src")) {
        let source = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("could not read {}: {e}", path.display());
        });
        // Every test module in this crate is at the end of its file; user
        // facing literals in tests (unicode fixtures, for example) are not
        // rendered by the app and must not be checked.
        let source = match source.find("#[cfg(test)]") {
            Some(index) => &source[..index],
            None => &source[..],
        };

        for literal in string_literals(source) {
            for (offset, ch) in literal.text.char_indices() {
                let cp = u32::from(ch);
                if cp < 0x80 || supported.contains(&cp) || vector_drawn.contains(&cp) {
                    continue;
                }
                let column = literal.text[..offset].chars().count() + 1;
                failures.push(format!(
                    "{}:{}:{}: U+{cp:04X} {ch:?} has no glyph in the bundled fonts (in {:?})",
                    path.display(),
                    literal.line,
                    column,
                    literal.text,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "user-facing text uses characters the bundled fonts cannot draw:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_icon_referenced_by_the_icon_module_exists() {
    let ui_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let icons_rs = ui_dir.join("src/components/icons.rs");
    let source = fs::read_to_string(&icons_rs).expect("icons.rs is part of the crate");

    let mut referenced = BTreeSet::new();
    for line in source.lines() {
        let Some(start) = line.find("assets/icons/") else {
            continue;
        };
        let rest = &line[start..];
        let Some(end) = rest.find(".svg") else {
            continue;
        };
        referenced.insert(rest[..end + 4].to_string());
    }

    assert!(
        !referenced.is_empty(),
        "the scan of icons.rs found no SVG references; the test is not looking at the right file"
    );

    let mut missing = Vec::new();
    for reference in referenced {
        let path = ui_dir.join(&reference);
        if !path.is_file() {
            missing.push(path.display().to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "icons.rs references SVG files that do not exist:\n{}",
        missing.join("\n")
    );
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        let entries = fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("could not read directory {}: {e}", path.display()));
        for entry in entries {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

// ── String literal extraction ───────────────────────────────────────────────

struct Literal {
    line: usize,
    text: String,
}

/// Extract every ordinary and raw string literal, skipping comments and char
/// literals. Lifetimes (`'a`) are not char literals and are skipped as a
/// single character.
fn string_literals(source: &str) -> Vec<Literal> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;

    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'\n' {
                        line += 1;
                        i += 1;
                    } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                let (text, next, newlines) = read_quoted(bytes, i + 1);
                out.push(Literal { line, text });
                line += newlines;
                i = next;
            }
            b'r' | b'b'
                if bytes.get(i + 1) == Some(&b'"')
                    || (bytes.get(i + 1) == Some(&b'#')
                        && raw_string_start(bytes, i).is_some()) =>
            {
                match raw_string_start(bytes, i) {
                    Some((content_start, hashes)) => {
                        let (text, next, newlines) = read_raw(bytes, content_start, hashes);
                        out.push(Literal { line, text });
                        line += newlines;
                        i = next;
                    }
                    None => i += 1,
                }
            }
            b'\'' => {
                i = skip_char_literal_or_lifetime(bytes, i);
            }
            _ => i += 1,
        }
    }

    out
}

/// If `i` starts a raw string (`r"`, `r#"`, `br"`, `br#"`), return the byte
/// index just after the opening quote and the number of `#` delimiters.
fn raw_string_start(bytes: &[u8], mut i: usize) -> Option<(usize, usize)> {
    if bytes.get(i) == Some(&b'b') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'r') {
        return None;
    }
    i += 1;
    let mut hashes = 0;
    while bytes.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    if bytes.get(i) == Some(&b'"') {
        Some((i + 1, hashes))
    } else {
        None
    }
}

fn read_raw(bytes: &[u8], start: usize, hashes: usize) -> (String, usize, usize) {
    let mut i = start;
    let mut line = 1;
    loop {
        if bytes.get(i) == Some(&b'"') {
            let mut end = i + 1;
            let mut seen = 0;
            while seen < hashes && bytes.get(end) == Some(&b'#') {
                seen += 1;
                end += 1;
            }
            if seen == hashes {
                let text = String::from_utf8_lossy(&bytes[start..i]).into_owned();
                return (text, end, line);
            }
        }
        if i >= bytes.len() {
            // Unterminated: rustc will report it. Return what we have so the
            // test does not panic with a less useful message.
            let text = String::from_utf8_lossy(&bytes[start..]).into_owned();
            return (text, bytes.len(), line);
        }
        if bytes[i] == b'\n' {
            line += 1;
        }
        i += 1;
    }
}

/// Read an ordinary quoted string starting after the opening `"`. Returns the
/// decoded text, the byte index after the closing quote, and newline count.
fn read_quoted(bytes: &[u8], start: usize) -> (String, usize, usize) {
    let mut text = String::new();
    let mut i = start;
    let mut newlines = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (text, i + 1, newlines),
            b'\n' => {
                newlines += 1;
                text.push('\n');
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&escape) = bytes.get(i) else {
                    break;
                };
                match escape {
                    b'n' => {
                        text.push('\n');
                        i += 1;
                    }
                    b'r' => {
                        text.push('\r');
                        i += 1;
                    }
                    b't' => {
                        text.push('\t');
                        i += 1;
                    }
                    b'0' => {
                        text.push('\0');
                        i += 1;
                    }
                    b'\\' => {
                        text.push('\\');
                        i += 1;
                    }
                    b'"' => {
                        text.push('"');
                        i += 1;
                    }
                    b'\'' => {
                        text.push('\'');
                        i += 1;
                    }
                    b'\n' => {
                        newlines += 1;
                        i += 1;
                    }
                    b'x' => {
                        if let Some(byte) = take_hex(bytes, i + 1, 2) {
                            text.push(char::from_u32(byte).unwrap_or('\u{FFFD}'));
                            i += 3;
                        } else {
                            i += 1;
                        }
                    }
                    b'u' => {
                        if bytes.get(i + 1) == Some(&b'{') {
                            if let Some(close) = bytes[i + 2..].iter().position(|&b| b == b'}') {
                                let digits = std::str::from_utf8(&bytes[i + 2..i + 2 + close]).ok();
                                if let Some(cp) =
                                    digits.and_then(|d| u32::from_str_radix(d, 16).ok())
                                {
                                    text.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                                }
                                i += 2 + close + 1;
                            } else {
                                i += 2;
                            }
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        // Unknown escape: keep the escaped character.
                        let ch = source_char(bytes, i);
                        text.push(ch);
                        i += ch.len_utf8();
                    }
                }
            }
            _ => {
                let ch = source_char(bytes, i);
                text.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    (text, i, newlines)
}

fn source_char(bytes: &[u8], i: usize) -> char {
    let rest = &bytes[i..];
    let text = String::from_utf8_lossy(&rest[..rest.len().min(4)]);
    text.chars().next().unwrap_or('\u{FFFD}')
}

fn take_hex(bytes: &[u8], start: usize, count: usize) -> Option<u32> {
    let end = start + count;
    let digits = std::str::from_utf8(bytes.get(start..end)?).ok()?;
    u32::from_str_radix(digits, 16).ok()
}

/// Skip `'x'`, `'\n'`, or a lifetime such as `'a`.
fn skip_char_literal_or_lifetime(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        // Escaped char literal: skip the escape and find the closing quote.
        i += 1;
        if bytes.get(i) == Some(&b'u') && bytes.get(i + 1) == Some(&b'{') {
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            i += 1;
        } else {
            i += 1;
        }
        if bytes.get(i) == Some(&b'\'') {
            return i + 1;
        }
        return i;
    }

    let Some(&first) = bytes.get(i) else {
        return i;
    };
    if first < 0x80 {
        if bytes.get(i + 1) == Some(&b'\'') {
            return i + 2;
        }
        return i;
    }

    // A non-ASCII scalar followed by `'` is a char literal.
    let ch = source_char(bytes, i);
    i += ch.len_utf8();
    if bytes.get(i) == Some(&b'\'') {
        i + 1
    } else {
        i
    }
}

// ── TrueType cmap parsing ───────────────────────────────────────────────────

fn cmap_codepoints(data: &[u8]) -> Result<BTreeSet<u32>, String> {
    let num_tables = be_u16(data, 4).ok_or("truncated sfnt header")?;
    let mut cmap_offset = None;
    for index in 0..usize::from(num_tables) {
        let record = 12 + index * 16;
        if data.get(record..record + 4) == Some(b"cmap") {
            cmap_offset = be_u32(data, record + 8);
            break;
        }
    }
    let cmap = cmap_offset.ok_or("no cmap table")? as usize;

    let num_subtables = be_u16(data, cmap + 2).ok_or("truncated cmap header")?;
    let mut codepoints = BTreeSet::new();
    for index in 0..usize::from(num_subtables) {
        let record = cmap + 4 + index * 8;
        // Encoding offsets are relative to the start of the cmap table.
        let subtable =
            cmap + be_u32(data, record + 4).ok_or("truncated cmap encoding record")? as usize;
        let format = be_u16(data, subtable).ok_or("truncated cmap subtable")?;
        match format {
            4 => parse_format_4(data, subtable, &mut codepoints)?,
            12 => parse_format_12(data, subtable, &mut codepoints)?,
            // Other formats exist but the bundled Inter files use 4 and 12.
            _ => {}
        }
    }

    if codepoints.is_empty() {
        return Err("the cmap table yielded no codepoints".into());
    }
    Ok(codepoints)
}

fn parse_format_4(
    data: &[u8],
    offset: usize,
    codepoints: &mut BTreeSet<u32>,
) -> Result<(), String> {
    let seg_count_x2 = be_u16(data, offset + 6).ok_or("truncated format 4 header")? as usize;
    let seg_count = seg_count_x2 / 2;
    let end_codes = offset + 14;
    let start_codes = end_codes + seg_count_x2 + 2; // +2 for reservedPad
    let id_deltas = start_codes + seg_count_x2;
    let id_range_offsets = id_deltas + seg_count_x2;

    for segment in 0..seg_count {
        let end = be_u16(data, end_codes + segment * 2).ok_or("truncated endCode")?;
        let start = be_u16(data, start_codes + segment * 2).ok_or("truncated startCode")?;
        let delta = be_i16(data, id_deltas + segment * 2).ok_or("truncated idDelta")?;
        let range_offset =
            be_u16(data, id_range_offsets + segment * 2).ok_or("truncated idRangeOffset")?;
        if start == 0xFFFF && end == 0xFFFF {
            continue;
        }

        for codepoint in start..=end {
            let glyph = if range_offset == 0 {
                codepoint.wrapping_add(delta as u16)
            } else {
                let address = id_range_offsets
                    + segment * 2
                    + usize::from(range_offset)
                    + usize::from(codepoint - start) * 2;
                be_u16(data, address)
                    .map(|glyph| glyph.wrapping_add(delta as u16))
                    .unwrap_or(0)
            };
            if glyph != 0 {
                codepoints.insert(u32::from(codepoint));
            }
        }
    }
    Ok(())
}

fn parse_format_12(
    data: &[u8],
    offset: usize,
    codepoints: &mut BTreeSet<u32>,
) -> Result<(), String> {
    let num_groups = be_u32(data, offset + 12).ok_or("truncated format 12 header")?;
    for group in 0..num_groups as usize {
        let record = offset + 16 + group * 12;
        let start = be_u32(data, record).ok_or("truncated format 12 group")?;
        let end = be_u32(data, record + 4).ok_or("truncated format 12 group")?;
        if end < start {
            continue;
        }
        // Inter's groups are small; this bound keeps a corrupt font from
        // exhausting memory while still covering every Unicode scalar.
        if end - start > 0x11_0000 {
            return Err(format!("implausible format 12 group {start:#X}..={end:#X}"));
        }
        for codepoint in start..=end {
            codepoints.insert(codepoint);
        }
    }
    Ok(())
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn be_i16(data: &[u8], offset: usize) -> Option<i16> {
    be_u16(data, offset).map(|value| value as i16)
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

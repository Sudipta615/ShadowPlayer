//! Shared helpers for the repository modules.
//!
//! Everything here is crate-private: the helpers encode invariants (id parsing,
//! FTS tokenisation, `LIKE` escaping) that every repository must follow but no
//! caller outside this module should need.

use domain::models::Track;
use domain::paths::{decode_path_key, encode_path_key};
use rusqlite::{Result, Row};
use std::path::Path;
use std::str::FromStr;

/// Decode a UUID column.
///
/// A malformed value is a hard error rather than a fabricated id: the old
/// `unwrap_or_default()` produced a *fresh random* UUID for corrupt rows, so
/// the resulting track could never be matched back to the database and any
/// later update or delete silently did nothing.
pub(super) fn parse_id<T: FromStr>(text: &str, column: &str) -> Result<T> {
    text.parse::<T>().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("column `{column}` holds a malformed id: {text:?}"),
            )),
        )
    })
}

/// Build an FTS5 query that can match what the table's `porter unicode61`
/// tokenizer actually indexed.
///
/// The old builder stripped every non-alphanumeric character and appended a
/// single `*`, so `AC/DC` became `ACDC*` — a token that is nowhere in the index,
/// because the tokenizer had stored `ac` and `dc` — and `don't` became `dont*`,
/// which never matched `don't Stop`. Nothing the index could contain was
/// reachable through punctuation.
///
/// Here the input is split exactly the way the tokenizer splits it (whitespace
/// gives terms, alphanumeric runs give tokens), and each term becomes either
/// `"token"*` or, for a term that spans several tokens, the phrase
/// `"first last"*` so `AC/DC` still finds `AC/DC` (and `AC DC`) while `ACDC`
/// stays a single unrelated token. Terms are `AND`ed. Everything is quoted, so
/// FTS5 operators a user happens to type (`AND`, `NOT`, `^`) are searched for
/// literally instead of being interpreted — and the result is always a valid
/// query, never a syntax error.
///
/// `None` means the input holds no searchable token at all.
pub(super) fn fts_query(input: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();

    for term in input.split_whitespace() {
        // A term's own alphanumeric runs, in order — the tokenizer's output.
        let tokens: Vec<String> = term
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .map(|t| t.to_lowercase())
            .collect();

        match tokens.len() {
            0 => continue,
            1 => terms.push(format!("\"{}\"*", tokens[0])),
            _ => terms.push(format!("\"{}\"*", tokens.join(" "))),
        }
    }

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

/// Escape a path for use as a `LIKE` prefix under `ESCAPE '\'`.
///
/// Two things the naive `format!("{}%", path)` got wrong: a prefix with no
/// trailing separator also matched sibling directories that merely start with
/// the same text (`/music/rock` matched `/music/rockabilly`), and any `%`, `_`
/// or `\` inside the path acted as a wildcard instead of a literal.
pub(super) fn like_prefix(prefix: &Path) -> String {
    // `LIKE` runs against the stored path keys, so the prefix has to be
    // encoded the same way the paths were.
    let mut literal = encode_path_key(prefix);
    if literal.is_empty() {
        return "%".to_string();
    }
    if !literal.ends_with(std::path::MAIN_SEPARATOR) {
        literal.push(std::path::MAIN_SEPARATOR);
    }
    let escaped = literal
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}%")
}

/// `LIKE` pattern that finds `needle` anywhere in a value.
///
/// The text comes straight from the search box, so its `%`, `_` and `\` have to
/// be escaped: typing `100%` must search for the percent sign, not for "one
/// followed by anything".
pub(super) fn like_substring(needle: &str) -> String {
    let mut out = String::with_capacity(needle.len() + 2);
    out.push('%');
    for ch in needle.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

pub(super) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(super) fn map_track(row: &Row<'_>) -> Result<Track> {
    let id_str: String = row.get("id")?;
    // The stored key is the lossless form; the real path is what the app
    // opens, plays and displays.
    let path_str: String = row.get("path")?;
    let is_fav_int: i32 = row.get("is_favorite")?;
    let is_lossless_int: i32 = row.get("is_lossless")?;

    let title: String = row.get("title")?;
    let artist: String = row.get("artist")?;
    let album: String = row.get("album")?;
    let sort_artist_folded = Some(domain::text::fold_collation_key(&artist));
    let sort_title_folded = Some(domain::text::fold_collation_key(&title));
    let sort_album_folded = Some(domain::text::fold_collation_key(&album));

    Ok(Track {
        id: parse_id(&id_str, "tracks.id")?,
        path: decode_path_key(&path_str),
        title,
        artist,
        album,
        genre: row.get("genre")?,
        year: row.get("year")?,
        track_number: row.get("track_number")?,
        disc_number: row.get("disc_number")?,
        duration_secs: row.get("duration_secs")?,
        bpm: row
            .get::<_, Option<i64>>("bpm")?
            .and_then(|b| u16::try_from(b).ok())
            .filter(|b| *b > 0),
        sample_rate: row.get("sample_rate")?,
        channels: row.get::<_, i64>("channels")? as usize,
        format: row.get("format")?,
        bitrate_kbps: row.get("bitrate_kbps")?,
        bit_depth: row.get("bit_depth")?,
        is_lossless: is_lossless_int != 0,
        replaygain_track_db: row.get("replaygain_track_db")?,
        replaygain_track_peak: row.get("replaygain_track_peak")?,
        replaygain_album_db: row.get("replaygain_album_db")?,
        replaygain_album_peak: row.get("replaygain_album_peak")?,
        skip_count: row.get::<_, i64>("skip_count")? as u32,
        r128_loudness_lufs: row.get("r128_loudness_lufs")?,
        file_size_bytes: row.get::<_, i64>("file_size_bytes")? as u64,
        mtime_unix_secs: row.get("mtime_unix_secs")?,
        mtime_nanos: row.get::<_, Option<i64>>("mtime_nanos")?.unwrap_or(0) as u32,
        content_hash: row.get("content_hash")?,
        file_id: row.get("file_id")?,
        is_favorite: is_fav_int != 0,
        rating: row.get::<_, i64>("rating")?.clamp(0, 5) as u8,
        has_lyrics: row.get::<_, i64>("has_lyrics")? != 0,
        play_count: row.get("play_count")?,
        last_played_at: row.get("last_played_at")?,
        added_at: row.get("added_at")?,
        sort_artist_folded,
        sort_title_folded,
        sort_album_folded,
    })
}

//! Streak and export helpers for the listening history.
use std::path::PathBuf;

use database::ScrobbleRepo;
use domain::models::Scrobble;

use super::ExportFormat;

/// Current and longest streak from a sorted list of distinct day numbers.
///
/// A streak is still "current" when its last day is today or yesterday: a
/// listener who has not played anything yet today has not broken it.
pub fn streaks(days: &[i64], today: i64) -> (usize, usize) {
    let Some(&last) = days.last() else {
        return (0, 0);
    };

    let mut longest = 1usize;
    let mut run = 1usize;
    for pair in days.windows(2) {
        if pair[1] == pair[0] + 1 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }

    let current = if last == today || last == today - 1 {
        let mut n = 1usize;
        let mut i = days.len() - 1;
        while i > 0 && days[i - 1] == days[i] - 1 {
            n += 1;
            i -= 1;
        }
        n
    } else {
        0
    };

    (current, longest)
}

/// The CSV form of a history.
///
/// RFC 4180 quoting: a field with a comma, a quote or a newline is wrapped in
/// quotes and its own quotes are doubled.
pub fn to_csv(scrobbles: &[Scrobble]) -> String {
    let mut out = String::from("title,artist,album,played_at,played_at_utc,listened_secs\n");
    for s in scrobbles {
        out.push_str(&csv_field(&s.title));
        out.push(',');
        out.push_str(&csv_field(&s.artist));
        out.push(',');
        out.push_str(&csv_field(&s.album));
        out.push(',');
        out.push_str(&s.played_at.to_string());
        out.push(',');
        out.push_str(&csv_field(&format_utc(s.played_at)));
        out.push(',');
        out.push_str(&format!("{:.1}", s.listened_secs));
        out.push('\n');
    }
    out
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// The JSON form of a history.
pub fn to_json(scrobbles: &[Scrobble]) -> String {
    let mut rows = Vec::with_capacity(scrobbles.len());
    for s in scrobbles {
        rows.push(serde_json::json!({
            "artist": s.artist,
            "title": s.title,
            "album": s.album,
            "played_at": s.played_at,
            "played_at_utc": format_utc(s.played_at),
            "duration_secs": s.duration_secs,
            "listened_secs": s.listened_secs,
        }));
    }
    serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".to_string())
}

/// `artist|title|album|timestamp`, the shape other scrobblers import.
///
/// A pipe inside a field would break the row, so it is replaced rather than
/// escaped: the format has no escape convention.
pub fn to_lastfm_rows(scrobbles: &[Scrobble]) -> String {
    let mut out = String::new();
    for s in scrobbles {
        out.push_str(&pipe_field(&s.artist));
        out.push('|');
        out.push_str(&pipe_field(&s.title));
        out.push('|');
        out.push_str(&pipe_field(&s.album));
        out.push('|');
        out.push_str(&s.played_at.to_string());
        out.push('\n');
    }
    out
}

fn pipe_field(value: &str) -> String {
    value.replace(['|', '\n', '\r'], " ")
}

/// Write a history to the path the user picked.
pub fn write_export(
    path: &PathBuf,
    format: ExportFormat,
    scrobbles: &[Scrobble],
) -> Result<(), String> {
    let body = match format {
        ExportFormat::Csv => to_csv(scrobbles),
        ExportFormat::Json => to_json(scrobbles),
        ExportFormat::LastFm => to_lastfm_rows(scrobbles),
    };
    std::fs::write(path, body).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// Ask for a destination and write the whole history there.
///
/// `Ok(None)` means the user dismissed the picker.
pub fn export_with_dialog(
    repo: &ScrobbleRepo,
    format: ExportFormat,
) -> Result<Option<PathBuf>, String> {
    let scrobbles = repo
        .list_all()
        .map_err(|e| format!("Could not read the listening history: {e}"))?;
    let Some(path) = rfd::FileDialog::new()
        .set_title("Export Listening History")
        .set_file_name(format!("shadowplayer-listening.{}", format.extension()))
        .add_filter(format.label(), &[format.extension()])
        .save_file()
    else {
        return Ok(None);
    };
    write_export(&path, format, &scrobbles)?;
    Ok(Some(path))
}

/// `2026-10-21 18:04:33 UTC`.
pub fn format_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

/// A short date for the header: `21 Oct 2026`.
pub(super) fn short_date(unix_secs: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (year, month, day) = civil_from_days(unix_secs.div_euclid(86_400));
    format!(
        "{day} {} {year}",
        MONTHS[(month.saturating_sub(1) as usize).min(11)]
    )
}

/// Howard Hinnant's `civil_from_days`: the date for a day number.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// "3 h 12 min", "41 min", "38 s".
pub fn format_span(secs: u64) -> String {
    let hours = secs / 3_600;
    let minutes = (secs % 3_600) / 60;
    if hours > 0 {
        format!("{hours} h {minutes} min")
    } else if minutes > 0 {
        format!("{minutes} min")
    } else {
        format!("{secs} s")
    }
}

//! The listening history: a local, offline answer to a scrobbling service.
//!
//! The scrobble log is the rows; this is what the rows add up to — top
//! artists, albums and tracks for a period, scrobbles per day and per hour,
//! streaks, and what has been on repeat. Everything is computed from the
//! local `scrobbles` table: no account, no network, no telemetry.

use crate::theme::Theme;
use database::ScrobbleRepo;
use domain::models::{Scrobble, ScrobbleCount};
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The period the charts and lists cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListeningPeriod {
    Week,
    #[default]
    Month,
    Year,
    All,
}

impl ListeningPeriod {
    pub const ALL: [ListeningPeriod; 4] = [
        ListeningPeriod::Week,
        ListeningPeriod::Month,
        ListeningPeriod::Year,
        ListeningPeriod::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Week => "Last 7 days",
            Self::Month => "Last 30 days",
            Self::Year => "Last year",
            Self::All => "All time",
        }
    }

    /// The cutoff timestamp, or `None` for all time.
    pub fn since(self, now: i64) -> Option<i64> {
        match self {
            Self::Week => Some(now - 7 * 86_400),
            Self::Month => Some(now - 30 * 86_400),
            Self::Year => Some(now - 365 * 86_400),
            Self::All => None,
        }
    }
}

/// Everything the listening view shows, computed once per period.
#[derive(Debug, Clone, Default)]
pub struct ListeningReport {
    pub total: usize,
    pub listened_secs: f64,
    pub first_played_at: Option<i64>,
    pub top_artists: Vec<ScrobbleCount>,
    pub top_albums: Vec<ScrobbleCount>,
    pub top_tracks: Vec<ScrobbleCount>,
    /// Tracks played at least three times in the last week.
    pub on_repeat: Vec<ScrobbleCount>,
    /// The last 30 days, including the days with no scrobbles.
    pub per_day: Vec<(i64, usize)>,
    pub per_hour: [usize; 24],
    pub current_streak: usize,
    pub longest_streak: usize,
    pub days_listened: usize,
    pub today: i64,
}

impl ListeningReport {
    /// Read the aggregate view out of the repository.
    ///
    /// A failed query returns an empty report rather than a half-populated
    /// one: a chart built from half a history is worse than an honest zero.
    pub fn load(repo: &ScrobbleRepo, period: ListeningPeriod) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let since = period.since(now);

        let today = repo.today_day_number().unwrap_or(0);
        let days = repo.distinct_days().unwrap_or_default();
        let (current_streak, longest_streak) = streaks(&days, today);

        let mut per_day = Vec::with_capacity(30);
        // `per_day` filters on the Unix timestamp, not the day number, so the
        // day-number cutoff has to be turned back into one.
        let by_day: BTreeMap<i64, usize> = repo
            .per_day(Some(today.saturating_sub(29) * 86_400))
            .unwrap_or_default()
            .into_iter()
            .collect();
        for day in (today.saturating_sub(29))..=today {
            per_day.push((day, by_day.get(&day).copied().unwrap_or(0)));
        }

        let week = Some(now - 7 * 86_400);
        Self {
            total: repo.count_since(since).unwrap_or(0),
            listened_secs: repo.total_listened_secs_since(since).unwrap_or(0.0),
            first_played_at: repo.first_played_at().unwrap_or(None),
            top_artists: repo.top_artists(since, 5).unwrap_or_default(),
            top_albums: repo.top_albums(since, 5).unwrap_or_default(),
            top_tracks: repo.top_tracks(since, 5).unwrap_or_default(),
            on_repeat: repo
                .top_tracks(week, 20)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c.count >= 3)
                .take(5)
                .collect(),
            per_day,
            per_hour: repo.per_hour(since).unwrap_or([0; 24]),
            current_streak,
            longest_streak,
            days_listened: days.len(),
            today,
        }
    }

    /// The busiest hour of the day, if there is one.
    pub fn peak_hour(&self) -> Option<(usize, usize)> {
        self.per_hour
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .filter(|(_, count)| *count > 0)
    }
}

/// The modal's own state.
#[derive(Default)]
pub struct ListeningModalState {
    pub is_open: bool,
    pub period: ListeningPeriod,
    pub report: ListeningReport,
    /// The period the report was computed for. `None` forces a reload, which
    /// is what opening the view does.
    loaded_period: Option<ListeningPeriod>,
}

impl ListeningModalState {
    /// Open the view and recompute it on the next frame.
    pub fn open(&mut self) {
        self.is_open = true;
        self.loaded_period = None;
    }
}

/// What the view asked the app to do.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListeningActions {
    pub close: bool,
    pub export: Option<ExportFormat>,
}

/// The export formats the history can be written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// A spreadsheet-friendly table.
    Csv,
    /// The full records, for another program.
    Json,
    /// `artist|title|album|timestamp`, the shape other scrobblers accept.
    LastFm,
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
            Self::LastFm => "Last.fm rows",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::LastFm => "txt",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Csv => "one row per scrobble, with an ISO timestamp",
            Self::Json => "the full records, for another program",
            Self::LastFm => "artist|title|album|timestamp, one per line",
        }
    }
}

/// Show the listening history and return what the user asked for.
pub fn show_listening_modal(
    ctx: &egui::Context,
    state: &mut ListeningModalState,
    repo: &ScrobbleRepo,
    theme: &Theme,
) -> ListeningActions {
    let mut actions = ListeningActions::default();
    if !state.is_open {
        return actions;
    }

    if state.loaded_period != Some(state.period) {
        state.report = ListeningReport::load(repo, state.period);
        state.loaded_period = Some(state.period);
    }
    let report = state.report.clone();

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "listening_history",
        "Listening History",
        760.0,
        theme,
        |ui| {
            actions.export = render_report_body(ui, &report, &mut state.period, theme);
        },
    );

    if closed {
        state.is_open = false;
        actions.close = true;
    }
    actions
}

/// Draw the listening report itself.
///
/// Split from [`show_listening_modal`] so the layout can be rendered at fixed
/// widths in a test without a database. The body owns no state except the
/// period buttons, which write back through `period`.
pub fn render_report_body(
    ui: &mut egui::Ui,
    report: &ListeningReport,
    period: &mut ListeningPeriod,
    theme: &Theme,
) -> Option<ExportFormat> {
    // The body's inner width. Charts and header rows lay themselves out
    // against this, not against whatever a nested layout happens to report:
    // that mismatch was why the date and the right edge of both charts were
    // clipped by the modal frame.
    let content_w = ui.available_width();

    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

    // ── The headline numbers ───────────────────────────────────────
    let tiles: [(&str, String); 4] = [
        ("Scrobbles", report.total.to_string()),
        ("Listened", format_span(report.listened_secs as u64)),
        ("Days listened", report.days_listened.to_string()),
        (
            "Streak",
            format!(
                "{} day{}",
                report.current_streak,
                if report.current_streak == 1 { "" } else { "s" }
            ),
        ),
    ];
    // Four tiles on one row at the modal's design width. The fixed 168 px
    // tiles plus spacing total 690 px, which is wider than the 640 px window
    // the app supports; shrinking the tile to the row's share keeps the row
    // from widening (and then clipping) the whole body.
    let tile_w = ((content_w - theme.space_sm * 3.0) / 4.0).clamp(120.0, 168.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
        for (label, value) in tiles {
            stat_tile(ui, label, &value, theme, tile_w);
        }
    });

    if report.total == 0 && report.first_played_at.is_none() {
        ui.add_space(theme.space_md);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new(tr!("Nothing has been scrobbled yet"))
                    .font(crate::theme::font(13.0))
                    .color(theme.text_muted),
            );
            ui.label(
                egui::RichText::new(
                    tr!("A play is scrobbled once it has been heard for thirty \n                     seconds, or half the track, whichever comes first."),
                )
                .font(crate::theme::font(11.5))
                .color(theme.text_dim),
            );
        });
        return None;
    }

    // ── The period and the first-scrobble date ─────────────────────
    //
    // The buttons and the date are siblings with explicit widths. Nested
    // right-to-left layouts inside a horizontal row get the row's remaining
    // width, which is not the frame's inner width; the date then overflowed
    // the frame and was clipped. Reserving its measured width up front keeps
    // both siblings inside `content_w`.
    let date_text = report
        .first_played_at
        .map(|first| format!("First scrobble {}", short_date(first)));
    let date_w = date_text
        .as_ref()
        .map(|text| {
            ui.painter()
                .layout_no_wrap(text.clone(), crate::theme::font(10.5), theme.text_dim)
                .size()
                .x
                + theme.space_sm
        })
        .unwrap_or(0.0);
    let buttons_w = (content_w - date_w - theme.space_sm).max(140.0);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
        ui.allocate_ui_with_layout(
            Vec2::new(buttons_w, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
                for choice in ListeningPeriod::ALL {
                    let selected = choice == *period;
                    let button = egui::Button::new(
                        egui::RichText::new(choice.label())
                            .font(crate::theme::font(11.5))
                            .color(if selected {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            }),
                    )
                    .fill(if selected {
                        theme.bg_card_hover
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(if selected {
                        Stroke::new(1.0, theme.accent)
                    } else {
                        Stroke::NONE
                    })
                    .corner_radius(CornerRadius::same(theme.radius_sm));
                    if ui.add(button).clicked() {
                        *period = choice;
                    }
                }
            },
        );
        if let Some(date) = date_text {
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width().max(date_w), 24.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(
                        egui::RichText::new(date)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_dim),
                    );
                },
            );
        }
    });

    // ── The three top lists ────────────────────────────────────────
    ui.columns(3, |cols| {
        top_list(&mut cols[0], "Top artists", &report.top_artists, theme);
        top_list(&mut cols[1], "Top albums", &report.top_albums, theme);
        top_list(&mut cols[2], "Top tracks", &report.top_tracks, theme);
    });

    // ── On repeat ──────────────────────────────────────────────────
    if !report.on_repeat.is_empty() {
        section_heading(ui, "On repeat this week", theme);
        let card = egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(egui::Margin::symmetric(
                theme.space_xl as i8,
                theme.space_md as i8,
            ));
        card.show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
            for entry in &report.on_repeat {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    // Reserve the count's width before laying out the
                    // name: a truncating label would otherwise eat the
                    // whole row and the count would sit on top of it.
                    let count_w = 96.0;
                    let text_w = (ui.available_width() - count_w).max(80.0);
                    let text = if entry.detail.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}  ·  {}", entry.name, entry.detail)
                    };
                    ui.allocate_ui_with_layout(
                        Vec2::new(text_w, 16.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(text)
                                        .font(crate::theme::font(12.0))
                                        .color(theme.text_primary),
                                )
                                .truncate(),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}× this week", entry.count))
                                .font(crate::theme::mono_font(10.5))
                                .color(theme.accent_warm),
                        );
                    });
                });
            }
        });
    }

    // ── Scrobbles per day ─────────────────────────────────────────
    section_heading(ui, "Scrobbles per day", theme);
    let day_values: Vec<usize> = report.per_day.iter().map(|(_, n)| *n).collect();
    bar_chart(ui, &day_values, theme, 54.0, theme.accent, content_w);
    let busiest = day_values.iter().copied().max().unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(tr!("Last 30 days"))
                .font(crate::theme::font(10.5))
                .color(theme.text_dim),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "peak {busiest} · today {}",
                    day_values.last().copied().unwrap_or(0)
                ))
                .font(crate::theme::mono_font(10.0))
                .color(theme.text_muted),
            );
        });
    });

    // ── By hour of day ────────────────────────────────────────────
    section_heading(ui, "By time of day", theme);
    bar_chart(
        ui,
        &report.per_hour,
        theme,
        44.0,
        theme.accent_highlight,
        content_w,
    );
    if let Some((hour, count)) = report.peak_hour() {
        ui.label(
            egui::RichText::new(format!(
                "You listen most around {:02}:00 — {count} scrobbles",
                hour
            ))
            .font(crate::theme::font(10.5))
            .color(theme.text_dim),
        );
    }

    // ── Streaks ────────────────────────────────────────────────────
    section_heading(ui, "Streaks", theme);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
        for (label, value) in [
            ("Current", report.current_streak),
            ("Longest", report.longest_streak),
            ("Days with a scrobble", report.days_listened),
        ] {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(format!("{value}"))
                        .font(crate::theme::font(15.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
            });
        }
    });

    // ── Export ─────────────────────────────────────────────────────
    section_heading(ui, "Export", theme);
    let mut export = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);
        for format in [ExportFormat::Csv, ExportFormat::Json, ExportFormat::LastFm] {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(format.label())
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    )
                    .fill(theme.bg_pill)
                    .corner_radius(CornerRadius::same(theme.radius_md)),
                )
                .on_hover_text(format.description())
                .clicked()
            {
                export = Some(format);
            }
        }
    });
    ui.label(
        egui::RichText::new(
            tr!("Export writes the whole history, not only the selected period. \n             The file is yours; nothing leaves the machine."),
        )
        .font(crate::theme::font(10.5))
        .color(theme.text_dim),
    );

    export
}

// ─────────────────────────────────────────────────────────────────────────────
// Pure helpers
// ─────────────────────────────────────────────────────────────────────────────

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
fn short_date(unix_secs: i64) -> String {
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

// ─────────────────────────────────────────────────────────────────────────────
// Drawing helpers
// ─────────────────────────────────────────────────────────────────────────────

fn stat_tile(ui: &mut egui::Ui, label: &str, value: &str, theme: &Theme, width: f32) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(egui::Margin::symmetric(
            theme.space_xl as i8,
            theme.space_md as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 24.0);
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(value)
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
            });
        });
}

fn section_heading(ui: &mut egui::Ui, title: &str, theme: &Theme) {
    ui.add_space(theme.space_xs);
    ui.label(
        egui::RichText::new(title)
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_secondary),
    );
    ui.add_space(theme.space_xxs);
}

fn top_list(ui: &mut egui::Ui, title: &str, entries: &[ScrobbleCount], theme: &Theme) {
    section_heading(ui, title, theme);
    if entries.is_empty() {
        ui.label(
            egui::RichText::new(tr!("Nothing yet"))
                .font(crate::theme::font(11.0))
                .color(theme.text_dim),
        );
        return;
    }
    for entry in entries {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
            // Counts ride with the detail line rather than being right-aligned:
            // a column's reported width is wider than its actual clip rect, so
            // a right-to-left child would land in the next column. One
            // truncating label per line keeps three narrow columns honest.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(entry.name.clone())
                        .font(crate::theme::font(12.0))
                        .color(theme.text_primary),
                )
                .truncate(),
            );
            let detail = if entry.detail.is_empty() {
                format!("{}×", entry.count)
            } else {
                format!("{}× · {}", entry.count, entry.detail)
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(detail)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_dim),
                )
                .truncate(),
            );
        });
    }
}

fn bar_chart(
    ui: &mut egui::Ui,
    values: &[usize],
    theme: &Theme,
    height: f32,
    color: egui::Color32,
    width: f32,
) {
    if values.is_empty() {
        return;
    }
    // The width is the body's inner width, measured before any nested layout
    // narrowed the available rect. Sizing against `ui.available_width()` here
    // made the right-hand end of every chart run under the modal's edge.
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f32;
    let gap = 2.0f32;
    let n = values.len() as f32;
    let bar_w = ((rect.width() - gap * (n - 1.0)) / n).max(1.0);
    for (i, value) in values.iter().enumerate() {
        let h = (*value as f32 / max) * (height - 2.0);
        let x = rect.min.x + i as f32 * (bar_w + gap);
        let bar = Rect::from_min_max(
            Pos2::new(x, rect.max.y - h),
            Pos2::new(x + bar_w, rect.max.y),
        );
        // An empty day still gets a hairline, so the chart reads as a
        // timeline rather than a few floating bars.
        if *value == 0 {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, rect.max.y - 1.0),
                    Pos2::new(x + bar_w, rect.max.y),
                ),
                CornerRadius::same(theme.radius_xs),
                theme.border_subtle,
            );
        } else {
            painter.rect_filled(bar, CornerRadius::same(theme.radius_xs), color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrobble(artist: &str, title: &str, album: &str, played_at: i64) -> Scrobble {
        Scrobble {
            track_id: None,
            artist: artist.to_string(),
            title: title.to_string(),
            album: album.to_string(),
            played_at,
            duration_secs: 240.0,
            listened_secs: 200.0,
        }
    }

    #[test]
    fn streaks_count_the_current_and_the_longest_run() {
        let today = 20_000;
        let days = [
            today - 10,
            today - 9,
            today - 8,
            today - 2,
            today - 1,
            today,
        ];
        assert_eq!(streaks(&days, today), (3, 3));

        // Yesterday still counts as current: the day is not over.
        assert_eq!(streaks(&[today - 5, today - 4, today - 1], today), (1, 2));

        // A gap ends the current streak but not the longest.
        assert_eq!(
            streaks(&[today - 8, today - 7, today - 6, today - 3], today),
            (0, 3)
        );

        assert_eq!(streaks(&[], today), (0, 0));
        assert_eq!(streaks(&[today], today), (1, 1));
    }

    #[test]
    fn the_period_cutoffs_are_the_ones_they_say() {
        let now = 1_000_000_000;
        assert_eq!(ListeningPeriod::Week.since(now), Some(now - 7 * 86_400));
        assert_eq!(ListeningPeriod::Month.since(now), Some(now - 30 * 86_400));
        assert_eq!(ListeningPeriod::All.since(now), None);
    }

    #[test]
    fn csv_quotes_only_where_it_has_to() {
        let rows = vec![
            scrobble("Aphex Twin", "Windowlicker", "Windowlicker", 1_700_000_000),
            scrobble("A, B", "Quote \"me\"", "New\nLine", 1_700_000_001),
        ];
        let csv = to_csv(&rows);
        assert!(csv.starts_with("title,artist,album,played_at,played_at_utc,listened_secs\n"));
        assert!(csv.contains("\"A, B\""), "a comma forces quotes: {csv}");
        assert!(
            csv.contains("\"Quote \"\"me\"\"\""),
            "quotes are doubled: {csv}"
        );
        assert!(
            csv.contains("\"New\nLine\""),
            "a newline forces quotes: {csv}"
        );
        assert_eq!(
            csv.lines()
                .filter(|l| l.starts_with("Windowlicker"))
                .count(),
            1
        );
    }

    #[test]
    fn the_lastfm_rows_are_pipe_separated_and_never_break_the_row() {
        let rows = vec![scrobble("A|B", "C\nD", "E", 1_700_000_000)];
        let text = to_lastfm_rows(&rows);
        assert_eq!(text, "A B|C D|E|1700000000\n");
    }

    #[test]
    fn json_round_trips_the_history() {
        let rows = vec![scrobble("One", "A", "X", 1_700_000_000)];
        let json = to_json(&rows);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0]["artist"], "One");
        assert_eq!(parsed[0]["title"], "A");
        assert_eq!(parsed[0]["played_at"], 1_700_000_000);
        assert!(parsed[0]["played_at_utc"]
            .as_str()
            .unwrap()
            .contains("2023"));
    }

    #[test]
    fn utc_formatting_matches_a_known_timestamp() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13:20 UTC");
        assert_eq!(short_date(1_700_000_000), "14 Nov 2023");
    }

    #[test]
    fn durations_read_like_a_person_wrote_them() {
        assert_eq!(format_span(38), "38 s");
        assert_eq!(format_span(41 * 60), "41 min");
        assert_eq!(format_span(3 * 3600 + 12 * 60), "3 h 12 min");
    }
}

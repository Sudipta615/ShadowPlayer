//! The listening history: a local, offline answer to a scrobbling service.
//!
//! The scrobble log is the rows; this is what the rows add up to — top
//! artists, albums and tracks for a period, scrobbles per day and per hour,
//! streaks, and what has been on repeat. Everything is computed from the
//! local `scrobbles` table: no account, no network, no telemetry.

use crate::theme::Theme;
use database::ScrobbleRepo;
use domain::models::ScrobbleCount;
use eframe::egui;
use std::collections::BTreeMap;

mod export;
mod report;

pub use export::*;
pub use report::render_report_body;

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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::models::Scrobble;

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

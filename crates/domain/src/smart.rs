//! Rule-based "smart playlists": a saved query over the library that re-evaluates
//! itself every time it is opened.

use serde::{Deserialize, Serialize};

use crate::ids::PlaylistId;

/// A track attribute a rule can test against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartField {
    Title,
    Artist,
    Album,
    Genre,
    Year,
    Duration,
    PlayCount,
    Favorite,
    Added,
    LastPlayed,
    Format,
    SampleRate,
    Lossless,
    /// How many times the track was skipped before it was heard.
    SkipCount,
    /// The listener's 0–5 star rating.
    Rating,
    /// A sidecar or embedded lyric was found when the track was indexed.
    HasLyrics,
    /// The track has never been played.
    NeverPlayed,
    /// Source PCM precision in bits (16, 24, 32, …).
    BitDepth,
    /// Beats per minute, tagged by the file.
    Bpm,
    /// Audio bitrate in kbps (e.g. 320, 1411).
    Bitrate,
    /// Number of audio channels (1 = mono, 2 = stereo, etc.).
    Channels,
    /// File size in bytes.
    FileSize,
    /// Track number on disc.
    TrackNumber,
    /// Disc number in multi-disc set.
    DiscNumber,
}

impl SmartField {
    /// Every field, in the order the UI should present them.
    pub const ALL: [SmartField; 24] = [
        SmartField::Title,
        SmartField::Artist,
        SmartField::Album,
        SmartField::Genre,
        SmartField::Year,
        SmartField::Duration,
        SmartField::PlayCount,
        SmartField::Favorite,
        SmartField::Added,
        SmartField::LastPlayed,
        SmartField::Format,
        SmartField::SampleRate,
        SmartField::Lossless,
        SmartField::SkipCount,
        SmartField::Rating,
        SmartField::HasLyrics,
        SmartField::NeverPlayed,
        SmartField::BitDepth,
        SmartField::Bpm,
        SmartField::Bitrate,
        SmartField::Channels,
        SmartField::FileSize,
        SmartField::TrackNumber,
        SmartField::DiscNumber,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SmartField::Title => "Title",
            SmartField::Artist => "Artist",
            SmartField::Album => "Album",
            SmartField::Genre => "Genre",
            SmartField::Year => "Year",
            SmartField::Duration => "Duration (s)",
            SmartField::PlayCount => "Play Count",
            SmartField::Favorite => "Favorite",
            SmartField::Added => "Date Added",
            SmartField::LastPlayed => "Last Played",
            SmartField::Format => "Format",
            SmartField::SampleRate => "Sample Rate",
            SmartField::Lossless => "Lossless",
            SmartField::SkipCount => "Skip count",
            SmartField::Rating => "Rating",
            SmartField::HasLyrics => "Has Lyrics",
            SmartField::NeverPlayed => "Never Played",
            SmartField::BitDepth => "Bit Depth",
            SmartField::Bpm => "BPM",
            SmartField::Bitrate => "Bitrate (kbps)",
            SmartField::Channels => "Channels",
            SmartField::FileSize => "File Size (bytes)",
            SmartField::TrackNumber => "Track Number",
            SmartField::DiscNumber => "Disc Number",
        }
    }

    /// The underlying `tracks` column.
    pub fn column(self) -> &'static str {
        match self {
            SmartField::Title => "title",
            SmartField::Artist => "artist",
            SmartField::Album => "album",
            SmartField::Genre => "genre",
            SmartField::Year => "year",
            SmartField::Duration => "duration_secs",
            SmartField::PlayCount => "play_count",
            SmartField::Favorite => "is_favorite",
            SmartField::Added => "added_at",
            SmartField::LastPlayed => "last_played_at",
            SmartField::Format => "format",
            SmartField::SampleRate => "sample_rate",
            SmartField::Lossless => "is_lossless",
            SmartField::SkipCount => "skip_count",
            SmartField::Rating => "rating",
            SmartField::HasLyrics => "has_lyrics",
            // `Never Played` is the inverse of a play count, not a stored
            // flag. The SQL builder special-cases it; the column is named so
            // sorting still has a real key.
            SmartField::NeverPlayed => "play_count",
            SmartField::BitDepth => "bit_depth",
            SmartField::Bpm => "bpm",
            SmartField::Bitrate => "bitrate_kbps",
            SmartField::Channels => "channels",
            SmartField::FileSize => "file_size_bytes",
            SmartField::TrackNumber => "track_number",
            SmartField::DiscNumber => "disc_number",
        }
    }

    /// `true` for fields compared numerically rather than as text.
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            SmartField::Year
                | SmartField::Duration
                | SmartField::PlayCount
                | SmartField::SampleRate
                | SmartField::Favorite
                | SmartField::Lossless
                | SmartField::SkipCount
                | SmartField::Rating
                | SmartField::BitDepth
                | SmartField::Bpm
                | SmartField::Bitrate
                | SmartField::Channels
                | SmartField::FileSize
                | SmartField::TrackNumber
                | SmartField::DiscNumber
        )
    }

    /// `true` for fields that hold a unix timestamp (seconds).
    pub fn is_date(self) -> bool {
        matches!(self, SmartField::Added | SmartField::LastPlayed)
    }

    /// `true` for boolean flags.
    pub fn is_bool(self) -> bool {
        matches!(
            self,
            SmartField::Favorite
                | SmartField::Lossless
                | SmartField::HasLyrics
                | SmartField::NeverPlayed
        )
    }

    /// The operators that make sense for this field.
    pub fn operators(self) -> &'static [SmartOp] {
        if self.is_bool() {
            &[SmartOp::IsTrue, SmartOp::IsFalse]
        } else if self.is_date() {
            &[
                SmartOp::InLastDays,
                SmartOp::NotInLastDays,
                SmartOp::IsEmpty,
                SmartOp::IsNotEmpty,
            ]
        } else if self.is_numeric() {
            &[
                SmartOp::Equals,
                SmartOp::NotEquals,
                SmartOp::GreaterThan,
                SmartOp::LessThan,
            ]
        } else {
            &[
                SmartOp::Contains,
                SmartOp::NotContains,
                SmartOp::Equals,
                SmartOp::NotEquals,
                SmartOp::IsEmpty,
                SmartOp::IsNotEmpty,
            ]
        }
    }
}

/// A comparison applied to a [`SmartField`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartOp {
    Contains,
    NotContains,
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    /// Played/added within the last `value` days.
    InLastDays,
    /// Not played/added within the last `value` days.
    NotInLastDays,
    IsTrue,
    IsFalse,
    IsEmpty,
    IsNotEmpty,
}

impl SmartOp {
    /// Every operator, in presentation order (used to build dropdowns).
    pub const ALL: [SmartOp; 12] = [
        SmartOp::Contains,
        SmartOp::NotContains,
        SmartOp::Equals,
        SmartOp::NotEquals,
        SmartOp::GreaterThan,
        SmartOp::LessThan,
        SmartOp::InLastDays,
        SmartOp::NotInLastDays,
        SmartOp::IsTrue,
        SmartOp::IsFalse,
        SmartOp::IsEmpty,
        SmartOp::IsNotEmpty,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SmartOp::Contains => "contains",
            SmartOp::NotContains => "does not contain",
            SmartOp::Equals => "is",
            SmartOp::NotEquals => "is not",
            SmartOp::GreaterThan => "is greater than",
            SmartOp::LessThan => "is less than",
            SmartOp::InLastDays => "in the last (days)",
            SmartOp::NotInLastDays => "not in the last (days)",
            SmartOp::IsTrue => "is yes",
            SmartOp::IsFalse => "is no",
            SmartOp::IsEmpty => "is empty",
            SmartOp::IsNotEmpty => "is not empty",
        }
    }

    /// Whether the value field should be shown/edited for this operator.
    pub fn needs_value(self) -> bool {
        !matches!(
            self,
            SmartOp::IsTrue | SmartOp::IsFalse | SmartOp::IsEmpty | SmartOp::IsNotEmpty
        )
    }
}

/// One rule in a smart playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartRule {
    pub field: SmartField,
    pub op: SmartOp,
    pub value: String,
}

impl SmartRule {
    pub fn new(field: SmartField) -> Self {
        let op = field
            .operators()
            .first()
            .copied()
            .unwrap_or(SmartOp::Contains);
        Self {
            field,
            op,
            value: String::new(),
        }
    }

    /// Repair an operator the field cannot use, returning a description of the
    /// change when one was made.
    ///
    /// `SmartField::operators` has always said which pairs are meaningful, but
    /// nothing enforced it: a rule stored as `Date Added is 2024` reached SQL as
    /// `added_at = '2024' COLLATE NOCASE` — a comparison between a unix
    /// timestamp and four characters — so the playlist quietly matched nothing
    /// and gave the user no way to see why. Values are never invented: a
    /// non-numeric value on a date or number field is left alone and the SQL
    /// layer ignores the rule.
    pub fn normalized(&self) -> (SmartRule, Option<String>) {
        let mut fixed = self.clone();
        if self.field.operators().contains(&self.op) {
            return (fixed, None);
        }

        let was = self.op;
        if self.field.is_bool() {
            // "Favorite is true/yes/1" — anything else, including an
            // unparseable value, is read as "no".
            fixed.op = if matches!(
                self.value.trim().to_ascii_lowercase().as_str(),
                "true" | "yes" | "1"
            ) {
                SmartOp::IsTrue
            } else {
                SmartOp::IsFalse
            };
            fixed.value = String::new();
        } else if self.field.is_date() {
            fixed.op = match self.op {
                SmartOp::NotEquals | SmartOp::NotContains | SmartOp::NotInLastDays => {
                    SmartOp::NotInLastDays
                }
                // A year was the most likely intent of `Date Added is 2024`
                // (the year field is separate), but the only date value the
                // engine stores is a timestamp, so `in the last N days` is the
                // nearest reading that a human can also see and correct.
                _ => SmartOp::InLastDays,
            };
        } else if self.field.is_numeric() {
            fixed.op = match self.op {
                SmartOp::NotEquals | SmartOp::NotContains => SmartOp::NotEquals,
                SmartOp::InLastDays | SmartOp::NotInLastDays | SmartOp::Contains => SmartOp::Equals,
                _ => SmartOp::Equals,
            };
        } else {
            fixed.op = match self.op {
                SmartOp::GreaterThan | SmartOp::LessThan | SmartOp::InLastDays => SmartOp::Contains,
                SmartOp::NotInLastDays => SmartOp::NotContains,
                SmartOp::IsTrue => SmartOp::IsNotEmpty,
                SmartOp::IsFalse => SmartOp::IsEmpty,
                other => other,
            };
        }

        let note = format!(
            "{} {} {:?} is not a valid test, reading it as {}",
            self.field.label(),
            was.label(),
            self.value,
            fixed.op.label()
        );
        (fixed, Some(note))
    }

    /// Check that this rule can produce a meaningful query.
    ///
    /// `operators()` says which pairs are meaningful; this goes one step
    /// further and checks the value. An incomplete or impossible rule is
    /// rejected with a sentence the editor can show, instead of reaching SQL
    /// as a comparison that silently matches nothing.
    pub fn validate(&self) -> Result<(), String> {
        if !self.field.operators().contains(&self.op) {
            return Err(format!(
                "{} cannot use “{}”.",
                self.field.label(),
                self.op.label()
            ));
        }

        if !self.op.needs_value() || self.field.is_bool() {
            return Ok(());
        }

        let value = self.value.trim();
        if value.is_empty() {
            return Err(format!(
                "{} needs a value for “{}”.",
                self.field.label(),
                self.op.label()
            ));
        }

        if self.field.is_date() {
            let days: i64 = value
                .parse()
                .map_err(|_| format!("{} expects a whole number of days.", self.field.label()))?;
            if days < 0 {
                return Err(format!(
                    "{} cannot look back a negative number of days.",
                    self.field.label()
                ));
            }
            return Ok(());
        }

        if self.field.is_numeric() {
            let number: f64 = value
                .parse()
                .map_err(|_| format!("{} expects a number.", self.field.label()))?;
            if !number.is_finite() {
                return Err(format!("{} expects a finite number.", self.field.label()));
            }
            match self.field {
                SmartField::Rating if !(0.0..=5.0).contains(&number) => {
                    return Err("Rating must be between 0 and 5.".to_string());
                }
                SmartField::SampleRate if number <= 0.0 => {
                    return Err("Sample Rate must be greater than zero.".to_string());
                }
                SmartField::BitDepth if number <= 0.0 => {
                    return Err("Bit Depth must be greater than zero.".to_string());
                }
                SmartField::Year
                | SmartField::PlayCount
                | SmartField::SkipCount
                | SmartField::Bitrate
                | SmartField::Channels
                | SmartField::FileSize
                | SmartField::TrackNumber
                | SmartField::DiscNumber
                    if number < 0.0 =>
                {
                    return Err(format!("{} cannot be negative.", self.field.label()));
                }
                SmartField::Duration if number < 0.0 => {
                    return Err("Duration cannot be negative.".to_string());
                }
                _ => {}
            }
        }

        Ok(())
    }
}

/// When a smart playlist is re-evaluated.
///
/// The rules themselves are a saved query, so the only question is *when* it
/// is run again. A library that changes under a "Recently added" playlist
/// makes it stale; how stale depends on what the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmartRefreshPolicy {
    /// Re-evaluate whenever the library changes.
    ///
    /// The default: a smart playlist is meant to be self-updating, and the
    /// library is the only thing that can change what it matches.
    #[default]
    LibraryChange,
    /// Re-evaluate on a timer, whether or not the library changed.
    ///
    /// For rules that depend on time — "played in the last week" — which a
    /// library change alone would never notice.
    Interval,
    /// Re-evaluate only when the user asks.
    ///
    /// The result is frozen until the user presses Refresh, so a long query
    /// over a huge library is paid for once, not on every visit.
    Manual,
}

impl SmartRefreshPolicy {
    pub const ALL: [SmartRefreshPolicy; 3] = [
        SmartRefreshPolicy::LibraryChange,
        SmartRefreshPolicy::Interval,
        SmartRefreshPolicy::Manual,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::LibraryChange => "When the library changes",
            Self::Interval => "On a timer",
            Self::Manual => "Only when I ask",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::LibraryChange => "Re-evaluate the smart playlists after every scan",
            Self::Interval => "Re-evaluate the smart playlists every few minutes",
            Self::Manual => "Keep the last result until you press Refresh",
        }
    }
}

/// How often the smart playlists are re-evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartRefreshSettings {
    pub policy: SmartRefreshPolicy,
    /// How many minutes between refreshes under [`SmartRefreshPolicy::Interval`].
    pub interval_minutes: u32,
}

impl Default for SmartRefreshSettings {
    fn default() -> Self {
        Self {
            policy: SmartRefreshPolicy::default(),
            interval_minutes: 15,
        }
    }
}

impl SmartRefreshSettings {
    /// The smallest interval that is still useful.
    ///
    /// A refresh every second would re-run every smart playlist on every
    /// frame of a long library; the setting is clamped rather than rejected
    /// so an old payload cannot make the app spin.
    pub const MIN_INTERVAL_MINUTES: u32 = 1;

    pub fn clamped(self) -> Self {
        Self {
            policy: self.policy,
            interval_minutes: self.interval_minutes.max(Self::MIN_INTERVAL_MINUTES),
        }
    }

    /// The interval in seconds.
    pub fn interval_secs(&self) -> i64 {
        i64::from(self.clamped().interval_minutes) * 60
    }
}

/// A saved, self-updating playlist definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartPlaylist {
    pub id: PlaylistId,
    pub name: String,
    /// `true` = every rule must match (AND); `false` = any rule (OR).
    pub match_all: bool,
    pub rules: Vec<SmartRule>,
    pub limit_count: Option<u32>,
    /// Field to sort by, if any.
    pub sort_by: Option<SmartField>,
    pub sort_desc: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// The stored rule list could not be read back at all.
    ///
    /// Not persisted — it describes this load, not the definition. Such a
    /// playlist matches nothing rather than everything: an unreadable blob used
    /// to load as "no rules", and a query with no conditions quietly returned
    /// the entire library under the playlist's name.
    #[serde(skip)]
    pub rules_unreadable: bool,
}

impl SmartPlaylist {
    /// The first rule that cannot produce a meaningful query, with a message
    /// suitable for the editor. `None` means the definition is runnable.
    pub fn validation_error(&self) -> Option<String> {
        if self.rules_unreadable {
            return Some(
                "The saved rules could not be read. Re-save the playlist to replace them."
                    .to_string(),
            );
        }
        self.rules.iter().find_map(|rule| rule.validate().err())
    }

    /// A blank definition with a single starter rule.
    pub fn draft(id: PlaylistId) -> Self {
        Self {
            id,
            name: String::new(),
            match_all: true,
            rules: vec![SmartRule::new(SmartField::Artist)],
            limit_count: None,
            sort_by: None,
            sort_desc: false,
            created_at: 0,
            updated_at: 0,
            rules_unreadable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(field: SmartField, op: SmartOp, value: &str) -> SmartRule {
        SmartRule {
            field,
            op,
            value: value.to_string(),
        }
    }

    #[test]
    fn every_operator_a_field_offers_is_accepted() {
        for field in SmartField::ALL {
            for op in field.operators() {
                let value = if field.is_date() {
                    "7"
                } else if field.is_numeric() && !field.is_bool() {
                    if field == SmartField::Rating {
                        "4"
                    } else {
                        "16"
                    }
                } else {
                    "text"
                };
                let rule = rule(field, *op, value);
                assert!(
                    rule.validate().is_ok(),
                    "{} {} {value} should validate: {:?}",
                    field.label(),
                    op.label(),
                    rule.validate()
                );
            }
        }
    }

    #[test]
    fn impossible_pairs_and_values_are_explained() {
        let invalid = [
            rule(SmartField::Added, SmartOp::Equals, "2024"),
            rule(SmartField::Rating, SmartOp::GreaterThan, "9"),
            rule(SmartField::Rating, SmartOp::LessThan, "-1"),
            rule(SmartField::BitDepth, SmartOp::GreaterThan, "0"),
            rule(SmartField::SampleRate, SmartOp::Equals, "not-a-number"),
            rule(SmartField::Added, SmartOp::InLastDays, "-2"),
            rule(SmartField::Title, SmartOp::Contains, "   "),
            rule(SmartField::NeverPlayed, SmartOp::Equals, ""),
        ];
        for rule in invalid {
            assert!(
                rule.validate().is_err(),
                "{} {} {:?} must be rejected",
                rule.field.label(),
                rule.op.label(),
                rule.value
            );
        }
    }

    #[test]
    fn boolean_and_empty_operators_need_no_value() {
        for field in [
            SmartField::Favorite,
            SmartField::Lossless,
            SmartField::HasLyrics,
            SmartField::NeverPlayed,
        ] {
            for op in [SmartOp::IsTrue, SmartOp::IsFalse] {
                assert!(rule(field, op, "").validate().is_ok());
            }
        }
        assert!(rule(SmartField::Genre, SmartOp::IsEmpty, "")
            .validate()
            .is_ok());
        assert!(rule(SmartField::Genre, SmartOp::IsNotEmpty, "")
            .validate()
            .is_ok());
    }

    #[test]
    fn a_draft_has_a_visible_validation_error_until_it_is_complete() {
        let mut draft = SmartPlaylist::draft(crate::ids::PlaylistId::new());
        assert!(
            draft.validation_error().is_some(),
            "the starter rule has no value yet"
        );
        draft.rules[0].value = "Radiohead".to_string();
        assert_eq!(draft.validation_error(), None);
    }
}

/// A local "radio" station seeded from one track.
///
/// Radio is a smart shuffle, not a stream: it scores the library the user
/// already owns and never makes a network request. The weights encode the
/// three things a listener hears as "similar" — genre, tempo and rating —
/// plus a small preference for tracks that have not been overplayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioQuery {
    pub genre: Option<String>,
    pub artist: String,
    pub bpm: Option<u16>,
    pub rating: u8,
    /// A per-request salt, so pressing "Start Radio" again gives a different
    /// order without making the query itself random.
    pub salt: u64,
}

impl RadioQuery {
    /// How strongly `candidate` belongs on this station. Higher is better.
    pub fn score(&self, candidate: &crate::models::Track) -> i64 {
        let mut score = 0i64;

        if let (Some(seed_genre), Some(candidate_genre)) = (&self.genre, &candidate.genre) {
            if seed_genre.eq_ignore_ascii_case(candidate_genre) {
                score += 40;
            }
        }
        if self.artist.eq_ignore_ascii_case(&candidate.artist) {
            score += 15;
        }
        if let (Some(seed_bpm), Some(candidate_bpm)) = (self.bpm, candidate.bpm) {
            let distance = seed_bpm.abs_diff(candidate_bpm) as i64;
            // Within 5 BPM is a near-match; the reward tapers to zero at 30.
            score += (30 - distance * 2).clamp(0, 30);
        }
        let rating_distance = self.rating.abs_diff(candidate.rating) as i64;
        score += (10 - rating_distance * 2).max(0);
        // Prefer discovery over the same handful of tracks.
        score += 8i64.saturating_sub(candidate.play_count.min(8) as i64);
        // A recently skipped track should not come straight back.
        score -= (candidate.skip_count.min(5) as i64) * 3;

        score
    }

    /// A stable, salted tie-breaker. Two calls with the same salt produce the
    /// same order; a new station uses a new salt and therefore a new shuffle.
    pub fn shuffle_key(&self, candidate: &crate::models::Track) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.salt.hash(&mut hasher);
        candidate.id.hash(&mut hasher);
        hasher.finish()
    }
}

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
}

impl SmartField {
    /// Every field, in the order the UI should present them.
    pub const ALL: [SmartField; 18] = [
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
                SmartField::Year | SmartField::PlayCount | SmartField::SkipCount
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

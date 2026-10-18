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
}

impl SmartField {
    /// Every field, in the order the UI should present them.
    pub const ALL: [SmartField; 13] = [
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
        )
    }

    /// `true` for fields that hold a unix timestamp (seconds).
    pub fn is_date(self) -> bool {
        matches!(self, SmartField::Added | SmartField::LastPlayed)
    }

    /// `true` for boolean flags.
    pub fn is_bool(self) -> bool {
        matches!(self, SmartField::Favorite | SmartField::Lossless)
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
        let op = field.operators().first().copied().unwrap_or(SmartOp::Contains);
        Self {
            field,
            op,
            value: String::new(),
        }
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
}

impl SmartPlaylist {
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
        }
    }
}

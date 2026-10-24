//! Persisted songs-table preferences and the quality/format filter chips.

use serde::{Deserialize, Serialize};

use super::columns::ColumnPreset;

/// Quality/format chips that narrow whatever view is open, so the user can
/// filter the current list without navigating away. Lives here (next to
/// [`SortField`]) because this is where the chips are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuickFilter {
    #[default]
    All,
    Lossless,
    HiRes,
    Favorites,
    Played,
    /// Only tracks the listener has given at least one star.
    Rating,
}

impl QuickFilter {
    pub const ALL: [QuickFilter; 4] = [
        QuickFilter::All,
        QuickFilter::Lossless,
        QuickFilter::HiRes,
        QuickFilter::Rating,
    ];

    pub fn label(self) -> &'static str {
        match self {
            QuickFilter::All => "All",
            QuickFilter::Lossless => "Lossless",
            QuickFilter::HiRes => "Hi-Res",
            QuickFilter::Favorites => "Favorites",
            QuickFilter::Played => "Played",
            QuickFilter::Rating => "Rating",
        }
    }

    /// Whether this chip narrows the list (i.e. is not the pass-through).
    pub fn is_active_filter(self) -> bool {
        self != QuickFilter::All
    }
}

// ── Persisted table preferences ─────────────────────────────────────────────

/// The flexible columns' shares of the table's width.
///
/// Stored as fractions rather than pixels so a wider window gives every column
/// more room without changing their proportions. `title` is deliberately the
/// largest share: it carries the artwork, the title and the playing indicator,
/// and it is the column a user scans first.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColumnWidths {
    pub title: f32,
    pub artist: f32,
    pub album: f32,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        // Expressed as 42 : 26 : 26 and normalised so the shares sum to one.
        Self {
            title: 0.42,
            artist: 0.26,
            album: 0.26,
        }
        .normalized()
    }
}

impl ColumnWidths {
    /// The smallest share a column may be dragged to.
    pub(super) const MIN: f32 = 0.12;

    /// Keep the three shares positive and summing to one.
    ///
    /// The defaults are written as 42 : 26 : 26 and normalised to sum to one.
    /// Normalising also means a settings file written by an older build (or by
    /// hand) can never make the columns overlap or leave a gap.
    pub fn normalized(self) -> Self {
        // Negative shares cannot come from the drag maths, but a hand-edited
        // settings file is not required to be sane.
        let raw = [
            self.title.max(0.0),
            self.artist.max(0.0),
            self.album.max(0.0),
        ];
        let sum: f32 = raw.iter().sum();
        if !sum.is_finite() || sum <= f32::EPSILON {
            return Self::default();
        }

        let mut shares = [raw[0] / sum, raw[1] / sum, raw[2] / sum];
        let deficit: f32 = shares
            .iter()
            .map(|share| (Self::MIN - share).max(0.0))
            .sum();
        if deficit > 0.0 {
            // Lift every undersized share to MIN and take the difference from
            // the columns that have room above MIN, in proportion to that
            // room. Clamping and then re-normalising would push the small
            // columns below MIN again when one column held almost everything.
            let headroom: f32 = shares
                .iter()
                .map(|share| (share - Self::MIN).max(0.0))
                .sum();
            if headroom <= f32::EPSILON {
                return Self::default();
            }
            for share in &mut shares {
                *share = if *share < Self::MIN {
                    Self::MIN
                } else {
                    *share - deficit * (*share - Self::MIN) / headroom
                };
            }
        }

        Self {
            title: shares[0],
            artist: shares[1],
            album: shares[2],
        }
        .rebalance()
    }

    /// Re-normalise after clamping, so the three shares still sum to one.
    fn rebalance(self) -> Self {
        let sum = self.title + self.artist + self.album;
        Self {
            title: self.title / sum,
            artist: self.artist / sum,
            album: self.album / sum,
        }
    }

    /// Drag the boundary after `column` (0 = title, 1 = artist) by `delta`
    /// fractions of the flexible width.
    pub fn drag(self, boundary: usize, delta: f32) -> Self {
        let mut next = self.normalized();
        let (left, right) = match boundary {
            0 => (&mut next.title, &mut next.artist),
            1 => (&mut next.artist, &mut next.album),
            _ => return next,
        };
        let delta = delta.clamp(Self::MIN - *left, *right - Self::MIN);
        *left += delta;
        *right -= delta;
        next.rebalance()
    }
}

/// How tall a song-table row is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RowDensity {
    /// Room for artwork and two readable lines.
    #[default]
    Comfortable,
    /// More rows on screen, at the cost of the artwork.
    Compact,
}

impl RowDensity {
    /// The height of one row's slot, including its 2 px gap.
    pub fn row_height(self) -> f32 {
        match self {
            Self::Comfortable => 44.0,
            Self::Compact => 26.0,
        }
    }

    /// The side of the artwork thumbnail.
    pub fn thumb_size(self) -> f32 {
        match self {
            Self::Comfortable => 34.0,
            Self::Compact => 14.0,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Comfortable => Self::Compact,
            Self::Compact => Self::Comfortable,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "Comfortable rows",
            Self::Compact => "Compact rows",
        }
    }
}

/// Width of the rating column: five 12 px stars and their gaps.
pub(super) const COL_RATING_W: f32 = 68.0;
pub(super) const COL_RATING_W_MIN: f32 = 54.0;
pub(super) const COL_RATING_W_MAX: f32 = 104.0;

fn default_rating_width() -> f32 {
    COL_RATING_W
}

/// Everything the table remembers between launches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongTablePrefs {
    /// Kept for settings written before column presets existed. New layouts
    /// use `presets`; this field is still read so an upgrade does not lose the
    /// user's Title/Artist/Album proportions.
    pub widths: ColumnWidths,
    pub density: RowDensity,
    /// Rating-column width in pixels. Ratings are a fixed-width cell rather
    /// than a share of the flexible columns, but the user can still widen it
    /// and expect the choice to survive a restart.
    #[serde(default = "default_rating_width")]
    pub rating_width: f32,
    /// Named column layouts. Empty in settings written by older builds.
    pub presets: Vec<ColumnPreset>,
    /// Index into `presets`.
    pub active_preset: usize,
}

impl Default for SongTablePrefs {
    fn default() -> Self {
        Self {
            widths: ColumnWidths::default(),
            density: RowDensity::default(),
            rating_width: default_rating_width(),
            presets: ColumnPreset::builtins(),
            active_preset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::songs_table::columns::SongColumn;

    #[test]
    fn density_moves_between_the_two_documented_heights() {
        assert_eq!(RowDensity::Comfortable.row_height(), 44.0);
        assert_eq!(RowDensity::Compact.row_height(), 26.0);
        assert_eq!(RowDensity::Comfortable.toggled(), RowDensity::Compact);
        assert!(RowDensity::Compact.thumb_size() < RowDensity::Comfortable.thumb_size());
    }

    #[test]
    fn preset_layouts_round_trip_through_settings_json() {
        let prefs = SongTablePrefs::default();
        let json = serde_json::to_string(&prefs).unwrap();
        let parsed: SongTablePrefs = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.presets, prefs.presets);
        assert_eq!(parsed.active_preset, 0);
    }

    #[test]
    fn a_settings_file_without_a_rating_width_uses_the_default() {
        let legacy = r#"{
            "widths": { "title": 0.42, "artist": 0.26, "album": 0.26 },
            "density": "Compact"
        }"#;
        let prefs: SongTablePrefs = serde_json::from_str(legacy).unwrap();
        assert_eq!(prefs.rating_width, COL_RATING_W);
        assert_eq!(prefs.density, RowDensity::Compact);
    }

    #[test]
    fn legacy_widths_seed_the_default_preset() {
        let widths = ColumnWidths {
            title: 0.6,
            artist: 0.25,
            album: 0.15,
        };
        let preset = ColumnPreset::from_legacy_widths(widths);
        assert_eq!(preset.name, "Default");
        assert_eq!(
            preset.columns,
            vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album]
        );
        assert!((preset.weights[0] - 0.6).abs() < 1e-5);
        assert!((preset.weights[1] - 0.25).abs() < 1e-5);
        assert!((preset.weights[2] - 0.15).abs() < 1e-5);
        assert!((preset.weights.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }
}

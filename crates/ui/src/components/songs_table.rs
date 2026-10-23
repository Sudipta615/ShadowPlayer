use crate::components::text::truncated_galley;
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, SortField, SortOrder, Track};
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Maximum number of new cover textures decoded per frame. Caps the work done in
/// a single frame so fast scrolling on a slow CPU can't cause a stutter.
const ART_LOADS_PER_FRAME: usize = 8;

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
    pub const ALL: [QuickFilter; 6] = [
        QuickFilter::All,
        QuickFilter::Lossless,
        QuickFilter::HiRes,
        QuickFilter::Favorites,
        QuickFilter::Played,
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
    const MIN: f32 = 0.12;

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

/// A column that can appear in the songs table.
///
/// `Title` carries the artwork and the playing indicator and is always
/// present. The rest are optional, ordered by the active [`ColumnPreset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SongColumn {
    Title,
    Artist,
    Album,
    Year,
    Genre,
    BitDepth,
    SampleRate,
    Codec,
    Bpm,
    PlayCount,
}

impl SongColumn {
    /// Columns the preset editor can add. `Title` is fixed first.
    pub const OPTIONAL: [SongColumn; 9] = [
        SongColumn::Artist,
        SongColumn::Album,
        SongColumn::Year,
        SongColumn::Genre,
        SongColumn::BitDepth,
        SongColumn::SampleRate,
        SongColumn::Codec,
        SongColumn::Bpm,
        SongColumn::PlayCount,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SongColumn::Title => "Title",
            SongColumn::Artist => "Artist",
            SongColumn::Album => "Album",
            SongColumn::Year => "Year",
            SongColumn::Genre => "Genre",
            SongColumn::BitDepth => "Bit Depth",
            SongColumn::SampleRate => "Sample Rate",
            SongColumn::Codec => "Codec",
            SongColumn::Bpm => "BPM",
            SongColumn::PlayCount => "Plays",
        }
    }

    /// The sort key this column maps to, if the table supports sorting by it.
    pub fn sort_field(self) -> Option<SortField> {
        match self {
            SongColumn::Title => Some(SortField::Title),
            SongColumn::Artist => Some(SortField::Artist),
            SongColumn::Album => Some(SortField::Album),
            _ => None,
        }
    }

    /// A column's share of the flexible width before normalisation.
    fn default_weight(self) -> f32 {
        match self {
            SongColumn::Title => 0.42,
            SongColumn::Artist => 0.25,
            SongColumn::Album => 0.25,
            SongColumn::Year => 0.08,
            SongColumn::Genre => 0.14,
            SongColumn::BitDepth => 0.10,
            SongColumn::SampleRate => 0.12,
            SongColumn::Codec => 0.10,
            SongColumn::Bpm => 0.08,
            SongColumn::PlayCount => 0.09,
        }
    }

    /// The cell text for a track. Empty values become an em dash so the row
    /// does not look like it is missing a column.
    pub fn value(self, track: &Track) -> String {
        let value = match self {
            SongColumn::Title => track.title.clone(),
            SongColumn::Artist => track.artist.clone(),
            SongColumn::Album => track.album.clone(),
            SongColumn::Year => track.year.map(|year| year.to_string()).unwrap_or_default(),
            SongColumn::Genre => track.genre.clone().unwrap_or_default(),
            SongColumn::BitDepth => track
                .bit_depth
                .map(|bits| format!("{bits}-bit"))
                .unwrap_or_default(),
            SongColumn::SampleRate => {
                if track.sample_rate == 0 {
                    String::new()
                } else {
                    format!("{:.1} kHz", track.sample_rate as f32 / 1000.0)
                }
            }
            SongColumn::Codec => track.format.clone(),
            SongColumn::Bpm => track.bpm.map(|bpm| bpm.to_string()).unwrap_or_default(),
            SongColumn::PlayCount => track.play_count.to_string(),
        };
        if value.trim().is_empty() {
            "—".to_string()
        } else {
            value
        }
    }
}

/// A named table layout: an ordered list of columns and their width shares.
///
/// Presets are data, not code: the built-in Default / Casual / Audiophile
/// layouts can be edited, copied or replaced, and the user's own presets are
/// stored with the rest of the table preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnPreset {
    pub name: String,
    pub columns: Vec<SongColumn>,
    /// One share per column, parallel to `columns`.
    pub weights: Vec<f32>,
}

impl Default for ColumnPreset {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            columns: vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album],
            weights: vec![0.42, 0.26, 0.26],
        }
        .normalized()
    }
}

impl ColumnPreset {
    /// Build the Default layout while keeping the column proportions an older
    /// build saved in `SongTablePrefs::widths`.
    pub fn from_legacy_widths(widths: ColumnWidths) -> Self {
        let widths = widths.normalized();
        Self {
            name: "Default".to_string(),
            columns: vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album],
            weights: vec![widths.title, widths.artist, widths.album],
        }
        .normalized()
    }

    /// The factory layout with this name, if it is one of the built-ins.
    ///
    /// "Reset this preset" uses this to restore Casual or Audiophile to the
    /// layout they shipped with, rather than collapsing every preset to the
    /// Default columns.
    pub fn factory(name: &str) -> Option<Self> {
        Self::builtins()
            .into_iter()
            .find(|preset| preset.name.eq_ignore_ascii_case(name))
    }

    /// The presets a fresh install starts with.
    pub fn builtins() -> Vec<Self> {
        vec![
            Self::default(),
            Self {
                name: "Casual".to_string(),
                columns: vec![SongColumn::Title, SongColumn::Artist],
                weights: vec![0.68, 0.32],
            }
            .normalized(),
            Self {
                name: "Audiophile".to_string(),
                columns: vec![
                    SongColumn::Title,
                    SongColumn::Artist,
                    SongColumn::Album,
                    SongColumn::Codec,
                    SongColumn::BitDepth,
                    SongColumn::SampleRate,
                    SongColumn::Bpm,
                ],
                weights: vec![0.30, 0.17, 0.17, 0.09, 0.08, 0.10, 0.09],
            }
            .normalized(),
        ]
    }

    /// Title first, no duplicates, one valid weight per column.
    pub fn normalized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.columns.retain(|column| seen.insert(*column));
        if let Some(pos) = self
            .columns
            .iter()
            .position(|column| *column == SongColumn::Title)
        {
            let title = self.columns.remove(pos);
            self.columns.insert(0, title);
        } else {
            self.columns.insert(0, SongColumn::Title);
        }

        let mut weights = Vec::with_capacity(self.columns.len());
        for (idx, column) in self.columns.iter().enumerate() {
            let weight = self
                .weights
                .get(idx)
                .copied()
                .unwrap_or(column.default_weight());
            weights.push(if weight.is_finite() {
                weight.max(0.0)
            } else {
                column.default_weight()
            });
        }
        let sum: f32 = weights.iter().sum();
        if !sum.is_finite() || sum <= f32::EPSILON {
            weights = self
                .columns
                .iter()
                .map(|column| column.default_weight())
                .collect();
        } else {
            for weight in &mut weights {
                *weight /= sum;
            }
        }
        self.weights = Self::enforce_minimums(&weights);
        if self.name.trim().is_empty() {
            self.name = "Custom".to_string();
        }
        self
    }

    fn enforce_minimums(weights: &[f32]) -> Vec<f32> {
        const MIN: f32 = 0.06;
        let mut out = weights.to_vec();
        if out.len() <= 1 {
            return vec![1.0];
        }
        let min = MIN.min(1.0 / out.len() as f32);
        let mut deficit = 0.0;
        for weight in &mut out {
            if *weight < min {
                deficit += min - *weight;
                *weight = min;
            }
        }
        if deficit > 0.0 {
            let headroom: f32 = out.iter().map(|weight| (weight - min).max(0.0)).sum();
            if headroom <= f32::EPSILON {
                return vec![1.0 / out.len() as f32; out.len()];
            }
            for weight in &mut out {
                if *weight > min {
                    *weight -= deficit * (*weight - min) / headroom;
                }
            }
        }
        let sum: f32 = out.iter().sum();
        for weight in &mut out {
            *weight /= sum;
        }
        out
    }

    /// Drag the boundary after column `boundary` by `delta` fractions of the
    /// flexible width.
    pub fn drag(&self, boundary: usize, delta: f32) -> Self {
        let mut next = self.clone().normalized();
        if boundary + 1 >= next.columns.len() {
            return next;
        }
        const MIN: f32 = 0.06;
        let (left, right) = next.weights.split_at_mut(boundary + 1);
        let left = &mut left[boundary];
        let right = &mut right[0];
        let delta = delta.clamp(MIN - *left, *right - MIN);
        *left += delta;
        *right -= delta;
        let sum: f32 = next.weights.iter().sum();
        if sum > f32::EPSILON {
            for weight in &mut next.weights {
                *weight /= sum;
            }
        }
        next
    }

    pub fn move_column(&mut self, from: usize, to: usize) {
        if from < self.columns.len() && to < self.columns.len() {
            let column = self.columns.remove(from);
            let weight = self.weights.remove(from);
            self.columns.insert(to, column);
            self.weights.insert(to, weight);
        }
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
            Self::Comfortable => 36.0,
            Self::Compact => 26.0,
        }
    }

    /// The side of the artwork thumbnail.
    pub fn thumb_size(self) -> f32 {
        (self.row_height() - 12.0).clamp(14.0, 26.0)
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
const COL_RATING_W: f32 = 68.0;
const COL_RATING_W_MIN: f32 = 54.0;
const COL_RATING_W_MAX: f32 = 104.0;

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

// ── Table geometry ──────────────────────────────────────────────────────────

/// Width of the row-number column (checkbox + track number).
const COL_NUM_W: f32 = 54.0;
/// Width of the duration column.
const COL_TIME_W: f32 = 52.0;
/// Width of the favorite-heart column.
const COL_FAV_W: f32 = 32.0;
/// Width of the overflow-menu column.
const COL_MENU_W: f32 = 26.0;
/// Horizontal gap between two columns.
const COL_GAP: f32 = 8.0;
/// The gap between the artwork thumbnail and the title text.
const ART_GAP: f32 = 8.0;

/// The horizontal layout of the songs table.
///
/// The header and the rows share this one model. They used to work out their
/// positions separately and drifted apart: the header spent 8 px on each of
/// its six gaps while the row budgeted 40 px in total, so the columns did not
/// line up and the header overflowed the row by 8 px.
#[derive(Debug, Clone, Copy)]
struct ColumnSpan {
    column: SongColumn,
    x: f32,
    w: f32,
}

struct TableColumns {
    /// Left edge of the row-number column.
    num_x: f32,
    /// Left edge of the title column — where the artwork sits.
    title_x: f32,
    /// Left edge of the title *text*, past the artwork.
    title_text_x: f32,
    /// Width available for the title text.
    title_text_w: f32,
    /// Thumbnail width plus the gap before the title text.
    art_inset: f32,
    /// One span per preset column, in preset order.
    spans: Vec<ColumnSpan>,
    /// Left edge of the duration column.
    duration_x: f32,
    /// Left edge and width of the rating column.
    rating_x: f32,
    rating_w: f32,
    /// Left edge of the favorite-heart column.
    favorite_x: f32,
    /// Left edge of the overflow-menu column.
    menu_x: f32,
    /// The width shared by every preset column.
    flexible_w: f32,
}

impl TableColumns {
    /// Lay the table out for a content area `full_width` wide.
    fn new(full_width: f32, preset: &ColumnPreset, density: RowDensity, rating_w: f32) -> Self {
        let fallback;
        let preset = if preset.columns.is_empty() {
            fallback = ColumnPreset::default();
            &fallback
        } else {
            preset
        };
        let rating_w = if rating_w.is_finite() {
            rating_w.clamp(COL_RATING_W_MIN, COL_RATING_W_MAX)
        } else {
            COL_RATING_W
        };
        let right_block = COL_TIME_W + rating_w + COL_FAV_W + COL_MENU_W;
        let column_count = preset.columns.len().max(1);
        let fixed = COL_NUM_W + right_block + COL_GAP * column_count as f32;
        let flexible = (full_width - fixed).max(180.0);

        // The artwork shrinks in compact mode, and the title text follows it
        // rather than keeping a fixed inset that would waste its space.
        let art_inset = density.thumb_size() + ART_GAP;

        let mut spans = Vec::with_capacity(preset.columns.len());
        let mut x = COL_NUM_W;
        for (idx, column) in preset.columns.iter().enumerate() {
            let weight = preset.weights.get(idx).copied().unwrap_or(0.0);
            let w = flexible * weight;
            spans.push(ColumnSpan {
                column: *column,
                x,
                w,
            });
            x += w + COL_GAP;
        }

        let title = spans
            .iter()
            .find(|span| span.column == SongColumn::Title)
            .copied()
            .unwrap_or(ColumnSpan {
                column: SongColumn::Title,
                x: COL_NUM_W,
                w: flexible,
            });

        Self {
            num_x: 0.0,
            title_x: title.x,
            title_text_x: title.x + art_inset,
            title_text_w: (title.w - art_inset).max(40.0),
            art_inset,
            spans,
            duration_x: full_width - right_block,
            rating_x: full_width - rating_w - COL_FAV_W - COL_MENU_W,
            rating_w,
            favorite_x: full_width - COL_FAV_W - COL_MENU_W,
            menu_x: full_width - COL_MENU_W,
            flexible_w: flexible,
        }
    }
}

/// The call to action offered by the empty state.
enum EmptyStateAction {
    OpenFolders,
    ClearSearch,
}

/// Explain an empty list and offer the next step.
///
/// The table used to render its header and then nothing at all, so an empty
/// library, a search that matched nothing and an empty playlist were all
/// indistinguishable from a broken app.
fn render_empty_state(
    ui: &mut egui::Ui,
    theme: &Theme,
    search_query: &str,
    library_track_count: usize,
    action: &mut SongsTableAction,
) {
    let (headline, body, cta): (&str, String, Option<(&str, EmptyStateAction)>) =
        if library_track_count == 0 {
            (
                "Your library is empty",
                "Add a music folder and ShadowPlayer will scan it, read the tags \
                 and build your library."
                    .to_string(),
                Some(("Add a music folder", EmptyStateAction::OpenFolders)),
            )
        } else if !search_query.trim().is_empty() {
            (
                "Nothing matched your search",
                format!("No tracks, albums, artists or playlists match “{search_query}”."),
                Some(("Clear search", EmptyStateAction::ClearSearch)),
            )
        } else {
            (
                "No tracks here yet",
                "This view is empty. Try another one, or add more music to your library."
                    .to_string(),
                None,
            )
        };

    // In a short window the illustration and the explanation are dropped, so
    // the message and its button still fit. The empty state used to be cut
    // off at the app's own minimum window height.
    let tight = ui.available_height() < 220.0;

    let top_space = if tight {
        2.0
    } else {
        (ui.available_height() * 0.16).clamp(6.0, 48.0)
    };
    ui.add_space(top_space);
    ui.vertical_centered(|ui| {
        if !tight {
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
            crate::components::icons::draw_music_note(ui, icon_rect, theme.text_dim);
            ui.add_space(10.0);
        }
        ui.label(
            egui::RichText::new(headline)
                .font(crate::theme::font(16.0))
                .strong()
                .color(theme.text_primary),
        );
        if !tight {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(body)
                    .font(crate::theme::font(12.5))
                    .color(theme.text_muted),
            );
        }

        if let Some((label, cta_action)) = cta {
            ui.add_space(if tight { 8.0 } else { 14.0 });
            let resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .font(crate::theme::font(13.0))
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .min_size(Vec2::new(158.0, 32.0)),
            );
            resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label)
            });
            if resp.clicked() {
                match cta_action {
                    EmptyStateAction::OpenFolders => action.open_folders = true,
                    EmptyStateAction::ClearSearch => action.clear_search = true,
                }
            }
        }
    });
}

/// The sort fields offered by the sort menu, in the order they are listed.
const SORT_FIELDS: &[(SortField, &str)] = &[
    (SortField::TrackNumber, "Track"),
    (SortField::Title, "Title"),
    (SortField::Artist, "Artist"),
    (SortField::Album, "Album"),
    (SortField::Duration, "Duration"),
    (SortField::DateAdded, "Added"),
    (SortField::Favorite, "Favorites"),
    (SortField::Rating, "Rating"),
];

/// The short name of a sort field, as shown on the sort pill.
fn sort_field_label(field: SortField) -> &'static str {
    SORT_FIELDS
        .iter()
        .find(|(f, _)| *f == field)
        .map(|(_, label)| *label)
        .unwrap_or("Title")
}

/// Draw the sort-direction arrow next to the active column header.
///
/// The active column used to be signalled by colour alone, so the direction
/// of the sort was invisible unless the user happened to find the separate
/// direction button in the toolbar.
fn paint_sort_arrow(
    painter: &egui::Painter,
    center: Pos2,
    is_active: bool,
    sort_order: SortOrder,
    color: egui::Color32,
) {
    if !is_active {
        return;
    }
    let ascending = sort_order == SortOrder::Ascending;
    let stroke = Stroke::new(1.5, color);
    let tip = if ascending {
        center.y - 4.0
    } else {
        center.y + 4.0
    };
    let barbs = if ascending {
        center.y + 1.0
    } else {
        center.y - 1.0
    };
    painter.add(egui::Shape::line(
        vec![Pos2::new(center.x, tip), Pos2::new(center.x, barbs)],
        stroke,
    ));
    painter.add(egui::Shape::line(
        vec![
            Pos2::new(center.x - 3.5, barbs),
            Pos2::new(center.x, tip),
            Pos2::new(center.x + 3.5, barbs),
        ],
        stroke,
    ));
}

/// A one-line explanation of what a quick-filter chip does.
fn quick_filter_tooltip(filter: QuickFilter) -> &'static str {
    match filter {
        QuickFilter::All => "Show every track",
        QuickFilter::Lossless => "Show only lossless tracks (FLAC, ALAC, WAV, DSD…)",
        QuickFilter::HiRes => "Show only Hi-Res tracks (above 48 kHz or 1000 kbps)",
        QuickFilter::Favorites => "Show only tracks you have favorited",
        QuickFilter::Played => "Show only tracks you have played before",
        QuickFilter::Rating => "Show only tracks with at least one star",
    }
}

/// The colour of a track's quality marker.
///
/// The tag used to be painted as a chip inside the title column, where it
/// consumed the same width as several characters of the title. A 3 px marker
/// on the row's leading edge says the same thing without taking any room; its
/// tooltip carries the words.
fn quality_marker_color(label: &str, theme: &Theme) -> Color32 {
    if label == "HI-RES" {
        theme.bit_perfect_green
    } else {
        theme.info
    }
}

pub struct SongsTableAction {
    pub play_track: Option<TrackId>,
    pub queue_next_track: Option<TrackId>,
    pub add_to_queue: Option<TrackId>,
    pub add_to_playlist: Option<(TrackId, PlaylistId)>,
    pub toggle_favorite: Option<TrackId>,
    /// Set one track's rating to 0–5 stars.
    pub set_rating: Option<(TrackId, u8)>,
    pub sort_field_changed: Option<SortField>,
    /// User tapped the ascending/descending toggle.
    pub sort_order_changed: Option<SortOrder>,
    /// User tapped a quality/format chip.
    pub quick_filter_changed: Option<QuickFilter>,
    /// The user dragged a separator to a new set of shares.
    pub column_preset_changed: Option<ColumnPreset>,
    /// A column was added, removed or reordered from the menu. Unlike a drag,
    /// this is a completed edit and should be persisted immediately.
    pub column_preset_edited: bool,
    /// The user chose another saved preset.
    pub select_column_preset: Option<usize>,
    /// Store this layout under the name in `preset_name_draft`.
    pub save_column_preset: Option<ColumnPreset>,
    /// Remove the active preset.
    pub delete_column_preset: bool,
    /// The rating column was dragged to a new pixel width.
    pub rating_width_changed: Option<f32>,
    /// The drag ended; this is the moment to persist the widths.
    pub column_drag_finished: bool,
    /// The density button was tapped.
    pub toggle_density: bool,
    /// "Reset columns" was chosen from the context menu.
    pub reset_columns: bool,
    pub toggle_view_mode: bool,
    pub open_folder: Option<PathBuf>,
    pub show_properties: Option<TrackId>,
    pub remove_track: Option<TrackId>,
    /// User chose "Edit Song Info" from the context menu.
    pub edit_track: Option<TrackId>,
    /// User chose "Per-track DSP" from the context menu.
    pub open_dsp: Option<TrackId>,
    /// User chose "Auto-Tag with MusicBrainz" from the context menu.
    pub autotag_track: Option<TrackId>,
    /// Seed a local radio station from this track.
    pub start_radio: Option<TrackId>,

    // ── Bulk (multi-select) actions ─────────────────────────────────────────
    // The selected ids themselves live in `LibraryState`; these flags only say
    // *what* to do with them.
    pub play_selected: bool,
    pub queue_next_selected: bool,
    pub add_selected_to_queue: bool,
    pub add_selected_to_playlist: Option<PlaylistId>,
    /// Add every selected track to Favorites.
    pub favorite_selected: bool,
    /// Set every selected track to this rating (0 clears).
    pub rate_selected: Option<u8>,
    /// Hand the selected tracks to the batch organizer.
    pub organize_selected: bool,
    /// Bulk auto-tag selected tracks.
    pub autotag_selected: bool,
    /// Bulk artwork fetch for selected tracks.
    pub fetch_artwork_selected: bool,
    pub remove_selected: bool,
    pub clear_selection: bool,
    /// The empty state asked the user to add a music folder.
    pub open_folders: bool,
    /// The empty state asked the user to clear the search query.
    pub clear_search: bool,
}

pub struct SongsTable<'a> {
    pub tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a HashSet<TrackId>,
    pub playlists: &'a [Playlist],
    pub current_sort: SortField,
    /// Whether the current sort runs ascending or descending.
    pub sort_order: SortOrder,
    pub is_grid_view: bool,
    /// Active quality/format chip.
    pub quick_filter: QuickFilter,
    /// The active column layout.
    pub column_preset: &'a ColumnPreset,
    /// Names of every saved preset, in menu order.
    pub column_preset_names: &'a [String],
    /// Index of `column_preset` in the saved list.
    pub active_preset: usize,
    /// Work-in-progress name for "Save as preset".
    pub preset_name_draft: &'a mut String,
    /// Persisted width of the fixed rating column.
    pub rating_width: f32,
    /// Whether rows are comfortable or compact.
    pub density: RowDensity,
    /// Multi-selection, shared with the app so it survives tab changes.
    pub selected_track_ids: &'a mut HashSet<TrackId>,
    /// Anchor for shift-range selection.
    pub last_clicked_index: &'a mut Option<usize>,
    /// Artwork service used to lazily fetch textures for visible rows only.
    pub artwork: &'a mut ArtworkService,
    pub theme: &'a Theme,
    /// Hide the large "Songs" title and toolbar in short windows, leaving the
    /// table header and rows. Sorting stays available from the header cells.
    pub show_header: bool,
    /// The current search query, so the empty state can say what it found
    /// nothing for.
    pub search_query: &'a str,
    /// How many tracks the library holds in total. Zero means the library
    /// itself is empty, which is a different situation from a filtered view
    /// that happens to be empty.
    pub library_track_count: usize,
}

impl<'a> SongsTable<'a> {
    pub fn show(self, ui: &mut Ui) -> SongsTableAction {
        let mut action = SongsTableAction {
            play_track: None,
            queue_next_track: None,
            add_to_queue: None,
            add_to_playlist: None,
            toggle_favorite: None,
            set_rating: None,
            sort_field_changed: None,
            sort_order_changed: None,
            quick_filter_changed: None,
            column_preset_changed: None,
            column_preset_edited: false,
            select_column_preset: None,
            save_column_preset: None,
            delete_column_preset: false,
            rating_width_changed: None,
            column_drag_finished: false,
            toggle_density: false,
            reset_columns: false,
            toggle_view_mode: false,
            open_folder: None,
            show_properties: None,
            remove_track: None,
            edit_track: None,
            open_dsp: None,
            autotag_track: None,
            start_radio: None,
            play_selected: false,
            queue_next_selected: false,
            add_selected_to_queue: false,
            add_selected_to_playlist: None,
            favorite_selected: false,
            rate_selected: None,
            organize_selected: false,
            autotag_selected: false,
            fetch_artwork_selected: false,
            remove_selected: false,
            clear_selection: false,
            open_folders: false,
            clear_search: false,
        };

        // Split the borrows so the scroll-area closures can mutably borrow
        // `artwork`/`action` while reading the other fields.
        let SongsTable {
            tracks,
            active_track_id,
            is_playing,
            favorite_track_ids,
            playlists,
            current_sort,
            sort_order,
            is_grid_view,
            quick_filter,
            column_preset,
            column_preset_names,
            active_preset,
            preset_name_draft,
            rating_width,
            density,
            selected_track_ids,
            last_clicked_index,
            artwork,
            theme,
            show_header,
            search_query,
            library_track_count,
        } = self;

        let ctx = ui.ctx().clone();
        let mut art_budget = ART_LOADS_PER_FRAME;

        // 1. Section Header
        if show_header {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

                ui.label(
                    egui::RichText::new(tr!("Songs"))
                        .font(crate::theme::font(22.0))
                        .strong()
                        .color(theme.text_primary),
                );

                ui.label(
                    egui::RichText::new(if quick_filter.is_active_filter() {
                        format!("{} songs · {} filter", tracks.len(), quick_filter.label())
                    } else {
                        format!("{} songs", tracks.len())
                    })
                    .font(crate::theme::font(13.0))
                    .color(theme.text_muted),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                    if !is_grid_view {
                        render_column_preset_menu(
                            ui,
                            column_preset,
                            column_preset_names,
                            active_preset,
                            preset_name_draft,
                            theme,
                            &mut action,
                        );
                    }

                    // View Mode Toggles (Grid vs List). The active button's
                    // fill fades in rather than snapping, and the animation
                    // ids are fixed, so the state stays bounded.
                    let grid_t = ui.ctx().animate_bool_with_time(
                        ui.make_persistent_id("view_mode_grid"),
                        is_grid_view,
                        0.16,
                    );
                    let list_t = ui.ctx().animate_bool_with_time(
                        ui.make_persistent_id("view_mode_list"),
                        !is_grid_view,
                        0.16,
                    );
                    let grid_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, grid_t);
                    let list_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, list_t);

                    let (grid_rect, grid_resp) =
                        ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                    ui.painter().rect_filled(
                        grid_rect,
                        CornerRadius::same(theme.radius_md),
                        grid_bg,
                    );
                    if !is_grid_view {
                        ui.painter().rect_stroke(
                            grid_rect,
                            CornerRadius::same(theme.radius_md),
                            Stroke::new(1.0_f32, theme.border_subtle),
                            egui::StrokeKind::Inside,
                        );
                    }
                    crate::components::icons::draw_grid_icon(
                        ui,
                        grid_rect.shrink2(Vec2::new(7.0, 7.0)),
                        if is_grid_view {
                            Color32::WHITE
                        } else {
                            theme.text_muted
                        },
                    );
                    grid_resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            is_grid_view,
                            "Show covers as a grid",
                        )
                    });
                    let _ = grid_resp.clone().on_hover_text(tr!(Msg::S315382037EF4E6A9));
                    if grid_resp.clicked() && !is_grid_view {
                        action.toggle_view_mode = true;
                    }

                    let (list_rect, list_resp) =
                        ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                    ui.painter().rect_filled(
                        list_rect,
                        CornerRadius::same(theme.radius_md),
                        list_bg,
                    );
                    crate::components::icons::draw_list_icon(
                        ui,
                        list_rect.shrink2(Vec2::new(7.0, 7.0)),
                        if !is_grid_view {
                            Color32::WHITE
                        } else {
                            theme.text_muted
                        },
                    );
                    list_resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            !is_grid_view,
                            "Show tracks as a list",
                        )
                    });
                    let _ = list_resp.clone().on_hover_text(tr!(Msg::S432E9931F963515D));
                    if list_resp.clicked() && is_grid_view {
                        action.toggle_view_mode = true;
                    }

                    // Row density: three lines that get closer together in
                    // compact mode. The icon is painted rather than typed so
                    // it does not depend on a font glyph.
                    let (density_rect, density_resp) =
                        ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                    let density_bg = if matches!(density, RowDensity::Compact) {
                        theme.accent
                    } else if density_resp.hovered() {
                        theme.bg_card_hover
                    } else {
                        theme.bg_pill
                    };
                    ui.painter().rect_filled(
                        density_rect,
                        CornerRadius::same(theme.radius_md),
                        density_bg,
                    );
                    if !matches!(density, RowDensity::Compact) {
                        ui.painter().rect_stroke(
                            density_rect,
                            CornerRadius::same(theme.radius_md),
                            Stroke::new(1.0_f32, theme.border_subtle),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let line_col = if matches!(density, RowDensity::Compact) {
                        Color32::WHITE
                    } else {
                        theme.text_muted
                    };
                    let line_gap = if matches!(density, RowDensity::Compact) {
                        4.0
                    } else {
                        6.0
                    };
                    for i in -1..=1 {
                        let y = density_rect.center().y + i as f32 * line_gap;
                        ui.painter().line_segment(
                            [
                                Pos2::new(density_rect.center().x - 6.0, y),
                                Pos2::new(density_rect.center().x + 6.0, y),
                            ],
                            Stroke::new(1.5_f32, line_col),
                        );
                    }
                    density_resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            matches!(density, RowDensity::Compact),
                            "Compact rows",
                        )
                    });
                    if density_resp
                        .on_hover_text(format!(
                            "{} — click for {}",
                            density.label(),
                            density.toggled().label().to_lowercase()
                        ))
                        .clicked()
                    {
                        action.toggle_density = true;
                    }

                    ui.add_space(6.0);

                    // Sort Dropdown Pill
                    let sort_label = format!("Sort by: {}", sort_field_label(current_sort));

                    let (sort_rect, sort_resp) =
                        ui.allocate_exact_size(Vec2::new(124.0, 30.0), Sense::click());
                    let sp = ui.painter_at(sort_rect);
                    let sort_bg = if sort_resp.hovered() {
                        theme.bg_card_hover
                    } else {
                        theme.bg_pill
                    };
                    sp.rect_filled(sort_rect, CornerRadius::same(theme.radius_pill), sort_bg);
                    sp.rect_stroke(
                        sort_rect,
                        CornerRadius::same(theme.radius_pill),
                        Stroke::new(1.0_f32, theme.border_subtle),
                        egui::StrokeKind::Inside,
                    );
                    sp.text(
                        Pos2::new(sort_rect.min.x + 14.0, sort_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        &sort_label,
                        crate::theme::font(12.5),
                        theme.text_primary,
                    );
                    let chev_rect = Rect::from_center_size(
                        Pos2::new(sort_rect.max.x - 14.0, sort_rect.center().y),
                        Vec2::new(12.0, 12.0),
                    );
                    crate::components::icons::draw_chevron_down(ui, chev_rect, theme.text_muted);

                    // The pill carries a chevron, so it has to open a menu.
                    // It used to cycle through the fields one click at a
                    // time, which read as a broken dropdown.
                    let sort_menu_id = ui.make_persistent_id("sort_menu");
                    let mut sort_menu_open: bool =
                        ui.ctx().data(|d| d.get_temp(sort_menu_id).unwrap_or(false));
                    if sort_resp
                        .clone()
                        .on_hover_text(tr!(Msg::SA67DAD356DD3A456))
                        .clicked()
                    {
                        sort_menu_open = !sort_menu_open;
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(sort_menu_id, sort_menu_open));
                    }
                    if sort_menu_open {
                        let popup_pos = sort_rect.left_bottom() + Vec2::new(0.0, 4.0);
                        let mut close = false;
                        let mut popup_rect: Option<Rect> = None;
                        egui::Area::new(sort_menu_id.with("popup"))
                            .order(egui::Order::Foreground)
                            .fixed_pos(popup_pos)
                            .show(ui.ctx(), |ui| {
                                egui::Frame::popup(ui.style()).show(ui, |ui| {
                                    ui.set_min_width(124.0);
                                    for (field, name) in SORT_FIELDS {
                                        let selected = current_sort == *field;
                                        if ui
                                            .selectable_label(
                                                selected,
                                                egui::RichText::new(*name)
                                                    .font(crate::theme::font(12.5))
                                                    .color(if selected {
                                                        theme.accent_highlight
                                                    } else {
                                                        theme.text_secondary
                                                    }),
                                            )
                                            .clicked()
                                        {
                                            action.sort_field_changed = Some(*field);
                                            close = true;
                                        }
                                    }
                                });
                                popup_rect = Some(ui.min_rect());
                            });

                        let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
                            && !sort_resp.hovered()
                            && !popup_rect
                                .map(|r| ui.rect_contains_pointer(r))
                                .unwrap_or(false);
                        if close || clicked_elsewhere {
                            ui.ctx().data_mut(|d| d.insert_temp(sort_menu_id, false));
                        }
                    }

                    // Sort direction toggle
                    let (dir_rect, dir_resp) =
                        ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                    let dp = ui.painter_at(dir_rect);
                    let dir_bg = if dir_resp.hovered() {
                        theme.bg_card_hover
                    } else {
                        theme.bg_pill
                    };
                    dp.rect_filled(dir_rect, CornerRadius::same(theme.radius_md), dir_bg);
                    dp.rect_stroke(
                        dir_rect,
                        CornerRadius::same(theme.radius_md),
                        Stroke::new(1.0_f32, theme.border_subtle),
                        egui::StrokeKind::Inside,
                    );
                    let ascending = sort_order == SortOrder::Ascending;
                    let arrow_col = if dir_resp.hovered() {
                        theme.accent_highlight
                    } else {
                        theme.text_secondary
                    };
                    let cy = dir_rect.center().y;
                    let cx = dir_rect.center().x;
                    if ascending {
                        // ↑ : tip at the top, barbs below it
                        dp.add(egui::Shape::line(
                            vec![Pos2::new(cx, cy - 6.0), Pos2::new(cx, cy + 6.0)],
                            Stroke::new(1.8_f32, arrow_col),
                        ));
                        dp.add(egui::Shape::line(
                            vec![
                                Pos2::new(cx - 4.5, cy - 1.5),
                                Pos2::new(cx, cy - 6.0),
                                Pos2::new(cx + 4.5, cy - 1.5),
                            ],
                            Stroke::new(1.8_f32, arrow_col),
                        ));
                    } else {
                        dp.add(egui::Shape::line(
                            vec![Pos2::new(cx, cy - 6.0), Pos2::new(cx, cy + 6.0)],
                            Stroke::new(1.8_f32, arrow_col),
                        ));
                        dp.add(egui::Shape::line(
                            vec![
                                Pos2::new(cx - 4.5, cy + 1.5),
                                Pos2::new(cx, cy + 6.0),
                                Pos2::new(cx + 4.5, cy + 1.5),
                            ],
                            Stroke::new(1.8_f32, arrow_col),
                        ));
                    }
                    if dir_resp
                        .on_hover_text(if ascending {
                            "Ascending — click for descending"
                        } else {
                            "Descending — click for ascending"
                        })
                        .clicked()
                    {
                        action.sort_order_changed = Some(if ascending {
                            SortOrder::Descending
                        } else {
                            SortOrder::Ascending
                        });
                    }
                });
            });

            // Quick-filter chips. They narrow the list without leaving the
            // view, so the user does not have to hunt for a collection view
            // to find the lossless or Hi-Res tracks.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                for filter in QuickFilter::ALL {
                    let is_active = quick_filter == filter;
                    let label = filter.label();
                    // One animation id per chip, so the state stays bounded.
                    let chip_t = ui.ctx().animate_bool_with_time(
                        ui.make_persistent_id(("chip", label)),
                        is_active,
                        0.16,
                    );
                    let resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(label)
                                .font(crate::theme::font(11.5))
                                .color(crate::theme::lerp_color(
                                    theme.text_secondary,
                                    Color32::WHITE,
                                    chip_t,
                                )),
                        )
                        .fill(crate::theme::lerp_color(
                            theme.bg_pill,
                            theme.accent,
                            chip_t,
                        ))
                        .stroke(Stroke::new(
                            1.0,
                            if is_active {
                                theme.accent_highlight
                            } else {
                                theme.border_subtle
                            },
                        ))
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .min_size(Vec2::new(0.0, 24.0)),
                    );
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            ui.is_enabled(),
                            is_active,
                            format!("Filter: {label}"),
                        )
                    });
                    if resp.on_hover_text(quick_filter_tooltip(filter)).clicked() {
                        action.quick_filter_changed = Some(filter);
                    }
                }
            });

            ui.add_space(6.0);
        }

        // Bulk action bar — only shown while a selection exists.
        if !selected_track_ids.is_empty() {
            render_bulk_bar(ui, selected_track_ids.len(), playlists, theme, &mut action);
            ui.add_space(6.0);
        }

        // An empty list gets an explanation and a way out, not a blank area.
        if tracks.is_empty() {
            render_empty_state(ui, theme, search_query, library_track_count, &mut action);
            return action;
        }

        if is_grid_view {
            // ── Grid View (virtualized in horizontal bands) ─────────────────
            let card_w = theme.card_w;
            let card_h = 180.0;
            let gap = theme.card_gutter;
            let avail_w = ui.available_width();
            ui.set_width(avail_w);
            let cols = (((avail_w + gap) / (card_w + gap)).floor()).max(1.0) as usize;
            let total_rows = tracks.len().div_ceil(cols.max(1));
            let band_h = card_h + gap;

            egui::ScrollArea::vertical()
                .id_salt("songs_grid_scroll")
                .auto_shrink([false, false])
                .show_rows(ui, band_h, total_rows, |ui, row_range| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                    for row in row_range {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                            for col in 0..cols {
                                let idx = row * cols + col;
                                if idx >= tracks.len() {
                                    break;
                                }
                                let track = &tracks[idx];
                                render_grid_card(
                                    ui,
                                    track,
                                    idx,
                                    card_w,
                                    card_h,
                                    active_track_id,
                                    favorite_track_ids,
                                    playlists,
                                    selected_track_ids,
                                    last_clicked_index,
                                    artwork,
                                    &ctx,
                                    &mut art_budget,
                                    theme,
                                    &mut action,
                                );
                            }
                        });
                    }
                });
        } else {
            // ── List View (virtualized, renders only visible rows) ──────────
            let row_h = density.row_height();

            // Static column headers, painted on the same column model the rows
            // use, so the two can never drift apart.
            let full_width = ui.available_width();
            let cols = TableColumns::new(full_width, column_preset, density, rating_width);

            let (header_rect, _) =
                ui.allocate_exact_size(Vec2::new(full_width, 20.0), Sense::hover());
            let hp = ui.painter_at(header_rect);
            let header_y = header_rect.center().y;

            let header_cell = |ui: &mut egui::Ui, salt: &str, x: f32, w: f32| -> egui::Response {
                let rect = Rect::from_min_size(
                    Pos2::new(header_rect.min.x + x, header_rect.min.y),
                    Vec2::new(w, 20.0),
                );
                ui.interact(rect, ui.make_persistent_id(salt), Sense::click())
            };

            // Column 1: select-all checkbox + the row-number sort target.
            let num_h_resp = header_cell(ui, "hdr_num", cols.num_x, COL_NUM_W);
            let all_selected =
                !tracks.is_empty() && tracks.iter().all(|t| selected_track_ids.contains(&t.id));
            let any_selected = tracks.iter().any(|t| selected_track_ids.contains(&t.id));
            let cb = Rect::from_center_size(
                Pos2::new(header_rect.min.x + cols.num_x + 16.0, header_y),
                Vec2::new(15.0, 15.0),
            );
            paint_checkbox(&hp, cb, all_selected, any_selected && !all_selected, theme);
            hp.text(
                Pos2::new(header_rect.min.x + cols.num_x + 42.0, header_y),
                egui::Align2::LEFT_CENTER,
                "#",
                crate::theme::font(11.8),
                theme.text_muted,
            );
            if num_h_resp
                .on_hover_text(tr!(Msg::SC5E23A32409C57E6))
                .clicked()
            {
                let pointer = ui.input(|i| i.pointer.interact_pos());
                let over_box = pointer
                    .map(|p| p.x < header_rect.min.x + cols.num_x + 30.0)
                    .unwrap_or(false);
                if over_box {
                    if all_selected {
                        selected_track_ids.clear();
                    } else {
                        for t in tracks.iter() {
                            selected_track_ids.insert(t.id);
                        }
                    }
                    *last_clicked_index = None;
                } else {
                    action.sort_field_changed = Some(SortField::TrackNumber);
                }
            }

            // Column 2: title.
            let title_h_resp = header_cell(
                ui,
                "hdr_title",
                cols.title_x,
                cols.title_text_w + cols.art_inset,
            );
            let title_col = if current_sort == SortField::Title {
                theme.accent_highlight
            } else {
                theme.text_muted
            };
            let title_label =
                hp.layout_no_wrap("TITLE".to_string(), crate::theme::font(11.8), title_col);
            let title_label_w = title_label.size().x;
            hp.galley(
                Pos2::new(
                    header_rect.min.x + cols.title_text_x,
                    header_y - title_label.size().y * 0.5,
                ),
                title_label,
                title_col,
            );
            paint_sort_arrow(
                &hp,
                Pos2::new(
                    header_rect.min.x + cols.title_text_x + title_label_w + 8.0,
                    header_y,
                ),
                current_sort == SortField::Title,
                sort_order,
                title_col,
            );
            if title_h_resp
                .on_hover_text(tr!(Msg::S1D45DD9F445ECA8C))
                .clicked()
            {
                action.sort_field_changed = Some(SortField::Title);
            }

            // Remaining preset columns, in the order the user chose.
            for span in &cols.spans {
                if span.column == SongColumn::Title {
                    continue;
                }
                let sort = span.column.sort_field();
                let is_sorted = sort.is_some_and(|field| field == current_sort);
                let header_col = if is_sorted {
                    theme.accent_highlight
                } else {
                    theme.text_muted
                };
                let salt = format!("hdr_col_{:?}", span.column);
                let resp = header_cell(ui, &salt, span.x, span.w);
                let label = hp.layout_no_wrap(
                    span.column.label().to_uppercase(),
                    crate::theme::font(11.0),
                    header_col,
                );
                let label_w = label.size().x;
                hp.galley(
                    Pos2::new(header_rect.min.x + span.x, header_y - label.size().y * 0.5),
                    label,
                    header_col,
                );
                if let Some(field) = sort {
                    paint_sort_arrow(
                        &hp,
                        Pos2::new(header_rect.min.x + span.x + label_w + 8.0, header_y),
                        is_sorted,
                        sort_order,
                        header_col,
                    );
                    if resp
                        .on_hover_text(tr_args!(
                            "Sort by {column}",
                            "column" => span.column.label()
                        ))
                        .clicked()
                    {
                        action.sort_field_changed = Some(field);
                    }
                }
            }

            // Column 5: duration.
            let clock_resp = header_cell(ui, "hdr_duration", cols.duration_x, COL_TIME_W);
            let clock_col = if current_sort == SortField::Duration {
                theme.accent_highlight
            } else {
                theme.text_muted
            };
            let clock_icon_rect = Rect::from_center_size(
                Pos2::new(
                    header_rect.min.x + cols.duration_x + COL_TIME_W * 0.5,
                    header_y,
                ),
                Vec2::new(14.0, 14.0),
            );
            crate::components::icons::draw_clock(ui, clock_icon_rect, clock_col);
            if clock_resp
                .on_hover_text(tr!(Msg::SDE0B06B549269586))
                .clicked()
            {
                action.sort_field_changed = Some(SortField::Duration);
            }

            // Column 6: rating.
            let rating_h_resp = header_cell(ui, "hdr_rating", cols.rating_x, cols.rating_w);
            let rating_col = if current_sort == SortField::Rating {
                theme.accent_highlight
            } else {
                theme.text_muted
            };
            let rating_label =
                hp.layout_no_wrap("RATING".to_string(), crate::theme::font(11.0), rating_col);
            let rating_label_w = rating_label.size().x;
            hp.galley(
                Pos2::new(
                    header_rect.min.x + cols.rating_x + (cols.rating_w - rating_label_w) * 0.5,
                    header_y - rating_label.size().y * 0.5,
                ),
                rating_label,
                rating_col,
            );
            if rating_h_resp
                .on_hover_text(tr!(Msg::S3E71E2EC5E3B3043))
                .clicked()
            {
                action.sort_field_changed = Some(SortField::Rating);
            }

            // Column 7: favorites.
            let heart_resp = header_cell(ui, "hdr_fav", cols.favorite_x, COL_FAV_W);
            let heart_icon_rect = Rect::from_center_size(
                Pos2::new(
                    header_rect.min.x + cols.favorite_x + COL_FAV_W * 0.5,
                    header_y,
                ),
                Vec2::new(14.0, 14.0),
            );
            crate::components::icons::draw_heart(ui, heart_icon_rect, theme.text_muted, false);
            if heart_resp
                .on_hover_text(tr!(Msg::S5C1B64888710263F))
                .clicked()
            {
                action.sort_field_changed = Some(SortField::Favorite);
            }

            // ── Draggable column separators ─────────────────────────────────
            // The separator is a real widget: it gets an accessible name, a
            // resize cursor and a drag state, rather than being a painted line
            // that happens to respond to the mouse.
            for boundary in 0..cols.spans.len().saturating_sub(1) {
                let span = cols.spans[boundary];
                let x = span.x + span.w + COL_GAP * 0.5;
                let sep_rect = Rect::from_min_size(
                    Pos2::new(header_rect.min.x + x - 4.0, header_rect.min.y),
                    Vec2::new(8.0, 20.0),
                );
                let sep = ui.interact(
                    sep_rect,
                    ui.make_persistent_id(("column_separator", boundary)),
                    Sense::click_and_drag(),
                );
                if sep.hovered() || sep.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    hp.line_segment(
                        [
                            Pos2::new(sep_rect.center().x, header_rect.min.y),
                            Pos2::new(sep_rect.center().x, header_rect.max.y),
                        ],
                        Stroke::new(1.0_f32, theme.accent_highlight),
                    );
                }
                let label = tr_args!(
                    "Resize the {column} column",
                    "column" => span.column.label()
                );
                sep.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                if sep.dragged() {
                    let dx = ui.input(|i| i.pointer.delta().x);
                    let next = column_preset.drag(boundary, dx / cols.flexible_w.max(1.0));
                    action.column_preset_changed = Some(next);
                }
                if sep.drag_stopped() {
                    action.column_drag_finished = true;
                }
            }

            // The rating column's right edge is draggable too. It is fixed
            // width, so this changes the pixel width rather than a share.
            let rating_sep_rect = Rect::from_min_size(
                Pos2::new(header_rect.min.x + cols.favorite_x - 4.0, header_rect.min.y),
                Vec2::new(8.0, 20.0),
            );
            let rating_sep = ui.interact(
                rating_sep_rect,
                ui.make_persistent_id("column_separator_rating"),
                Sense::click_and_drag(),
            );
            if rating_sep.hovered() || rating_sep.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                hp.line_segment(
                    [
                        Pos2::new(rating_sep_rect.center().x, header_rect.min.y),
                        Pos2::new(rating_sep_rect.center().x, header_rect.max.y),
                    ],
                    Stroke::new(1.0_f32, theme.accent_highlight),
                );
            }
            rating_sep.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, "Resize the Rating column")
            });
            if rating_sep.dragged() {
                let dx = ui.input(|i| i.pointer.delta().x);
                action.rating_width_changed =
                    Some((cols.rating_w + dx).clamp(COL_RATING_W_MIN, COL_RATING_W_MAX));
            }
            if rating_sep.drag_stopped() {
                action.column_drag_finished = true;
            }

            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .id_salt("songs_table_scroll")
                .auto_shrink([false, false])
                .show_rows(ui, row_h, tracks.len(), |ui, row_range| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                    for idx in row_range {
                        let track = &tracks[idx];
                        let is_active = active_track_id == Some(track.id);
                        let is_favorite = favorite_track_ids.contains(&track.id);
                        let is_selected = selected_track_ids.contains(&track.id);
                        let full_width = ui.available_width();

                        let (row_rect, row_resp) =
                            ui.allocate_exact_size(Vec2::new(full_width, row_h), Sense::click());
                        row_resp.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::Button,
                                true,
                                is_active,
                                format!("{} by {}", track.title, track.artist),
                            )
                        });
                        // The painted row sits one pixel inside each edge of
                        // its density-sized slot, leaving a 2 px gap.
                        let vis_rect = Rect::from_min_max(
                            Pos2::new(row_rect.min.x, row_rect.min.y + 1.0),
                            Pos2::new(row_rect.max.x, row_rect.max.y - 1.0),
                        );
                        let cy = vis_rect.center().y;

                        let painter = ui.painter_at(vis_rect);
                        // The hover is suppressed during a screenshot run, so
                        // a captured row is not half-highlighted. The check
                        // used to read the environment on every row of every
                        // frame, in a release build too.
                        #[cfg(feature = "eval-tools")]
                        let is_hovered =
                            row_resp.hovered() && std::env::var("SHADOWPLAYER_EVAL").is_err();
                        #[cfg(not(feature = "eval-tools"))]
                        let is_hovered = row_resp.hovered();

                        if is_active {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(theme.radius_md),
                                theme.bg_card_active,
                            );
                            painter.rect_stroke(
                                vis_rect,
                                CornerRadius::same(theme.radius_md),
                                Stroke::new(1.0_f32, theme.accent),
                                egui::StrokeKind::Inside,
                            );
                        } else if is_selected {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(theme.radius_md),
                                theme.bg_card_hover,
                            );
                            painter.rect_stroke(
                                vis_rect,
                                CornerRadius::same(theme.radius_md),
                                Stroke::new(1.0_f32, theme.info),
                                egui::StrokeKind::Inside,
                            );
                        } else if is_hovered {
                            painter.rect_filled(
                                vis_rect,
                                CornerRadius::same(theme.radius_md),
                                theme.bg_card_hover,
                            );
                        }

                        // Interaction zones that must NOT trigger playback.
                        let fav_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + cols.favorite_x + COL_FAV_W * 0.5, cy),
                            Vec2::new(24.0, 24.0),
                        );
                        let menu_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + cols.menu_x + COL_MENU_W * 0.5, cy),
                            Vec2::new(20.0, 20.0),
                        );
                        let rating_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + cols.rating_x + cols.rating_w * 0.5, cy),
                            Vec2::new(cols.rating_w, row_h.min(24.0)),
                        );
                        let rating_resp = ui.interact(
                            rating_rect,
                            ui.scope_id().with(("row_rating", idx)),
                            Sense::click(),
                        );
                        let pointer = ui.input(|i| i.pointer.interact_pos());
                        let hover_star = if rating_resp.hovered() {
                            pointer.and_then(|p| {
                                rating_star_at(cols.rating_x, cols.rating_w, p.x, vis_rect.min.x)
                            })
                        } else {
                            None
                        };
                        // Selection checkbox lives in the first column.
                        let check_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + 16.0, cy),
                            Vec2::new(20.0, 20.0),
                        );
                        let over_widget = pointer
                            .map(|p| {
                                fav_rect.contains(p)
                                    || menu_rect.contains(p)
                                    || rating_rect.contains(p)
                            })
                            .unwrap_or(false);
                        let over_check = pointer.map(|p| check_rect.contains(p)).unwrap_or(false);

                        // Click behaviour: plain click plays (matching the queue),
                        // the checkbox toggles, Ctrl/Cmd toggles, Shift extends.
                        if row_resp.clicked() && !over_widget {
                            let modifiers = ui.input(|i| i.modifiers);
                            if modifiers.shift && last_clicked_index.is_some() {
                                let anchor = last_clicked_index.unwrap_or(idx);
                                let (lo, hi) = if anchor <= idx {
                                    (anchor, idx)
                                } else {
                                    (idx, anchor)
                                };
                                for i in lo..=hi {
                                    if let Some(t) = tracks.get(i) {
                                        selected_track_ids.insert(t.id);
                                    }
                                }
                                *last_clicked_index = Some(idx);
                            } else if modifiers.command || modifiers.ctrl || over_check {
                                if !selected_track_ids.remove(&track.id) {
                                    selected_track_ids.insert(track.id);
                                }
                                *last_clicked_index = Some(idx);
                            } else {
                                action.play_track = Some(track.id);
                                *last_clicked_index = Some(idx);
                            }
                        }

                        if rating_resp.clicked() {
                            if let Some(star) = rating_star_at(
                                cols.rating_x,
                                cols.rating_w,
                                pointer.map(|p| p.x).unwrap_or(0.0),
                                vis_rect.min.x,
                            ) {
                                action.set_rating = Some((track.id, star));
                            }
                        }
                        if rating_resp.secondary_clicked() {
                            action.set_rating = Some((track.id, 0));
                        }
                        rating_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                format!("Rating: {} of 5", track.rating),
                            )
                        });
                        let _ = rating_resp
                            .clone()
                            .on_hover_text(tr!(Msg::SCFA981C8B38EA071));

                        // Column 1: selection box, else hover play button, else index.
                        if is_selected {
                            paint_checkbox(&painter, check_rect.shrink(3.0), true, false, theme);
                        } else if is_hovered {
                            paint_checkbox(&painter, check_rect.shrink(3.0), false, false, theme);
                            let play_icon_rect = Rect::from_center_size(
                                Pos2::new(vis_rect.min.x + 40.0, cy),
                                Vec2::new(14.0, 14.0),
                            );
                            let t1 =
                                Pos2::new(play_icon_rect.min.x + 11.0, play_icon_rect.center().y);
                            let t2 = Pos2::new(
                                play_icon_rect.min.x + 3.0,
                                play_icon_rect.center().y - 5.5,
                            );
                            let t3 = Pos2::new(
                                play_icon_rect.min.x + 3.0,
                                play_icon_rect.center().y + 5.5,
                            );
                            painter.add(egui::Shape::convex_polygon(
                                vec![t1, t2, t3],
                                Color32::WHITE,
                                Stroke::NONE,
                            ));
                        } else {
                            let num_text = format!("{}", idx + 1);
                            let num_col = if is_active {
                                theme.accent_highlight
                            } else {
                                theme.text_muted
                            };
                            painter.text(
                                Pos2::new(vis_rect.min.x + 34.0, cy),
                                egui::Align2::LEFT_CENTER,
                                &num_text,
                                crate::theme::font(12.8),
                                num_col,
                            );
                        }

                        // Column 2: Thumbnail + Title + Equalizer Indicator
                        let thumb = density.thumb_size();
                        let thumb_rect = Rect::from_center_size(
                            Pos2::new(vis_rect.min.x + cols.title_x + thumb * 0.5, cy),
                            Vec2::splat(thumb),
                        );

                        if let Some(tex) = art_for(artwork, &ctx, &mut art_budget, track) {
                            egui::Image::new(&tex)
                                .corner_radius(CornerRadius::same(theme.radius_sm))
                                .fit_to_exact_size(thumb_rect.size())
                                .paint_at(ui, thumb_rect);
                        } else {
                            painter.rect_filled(
                                thumb_rect,
                                CornerRadius::same(theme.radius_sm),
                                theme.bg_card,
                            );
                            let inset = (thumb * 0.22).max(3.0);
                            crate::components::icons::draw_disc(
                                ui,
                                thumb_rect.shrink(inset),
                                theme.accent_warm,
                            );
                        }

                        // The playing track's title is tinted as well as its row
                        // being highlighted: the two branches here used to be
                        // identical, so the highlight was a no-op.
                        let active_text = if is_active {
                            theme.accent
                        } else {
                            theme.text_primary
                        };
                        let title_x = vis_rect.min.x + cols.title_text_x;

                        // The quality tag left the title column. A 3 px marker
                        // on the leading edge carries the same information
                        // without eating a dozen characters of the title.
                        if let Some(label) = track.quality_tag() {
                            let marker = Rect::from_min_max(
                                Pos2::new(vis_rect.min.x + 1.0, vis_rect.min.y + 4.0),
                                Pos2::new(vis_rect.min.x + 4.0, vis_rect.max.y - 4.0),
                            );
                            painter.rect_filled(
                                marker,
                                CornerRadius::same(theme.radius_xs),
                                quality_marker_color(label, theme),
                            );
                            let marker_resp = ui.interact(
                                Rect::from_min_size(marker.min, Vec2::new(10.0, marker.height())),
                                ui.make_persistent_id(("quality_marker", track.id)),
                                Sense::hover(),
                            );
                            marker_resp.on_hover_text(match label {
                                "HI-RES" => "Hi-Res audio — above 48 kHz or 1000 kbps",
                                _ => "Lossless audio — no generation loss",
                            });
                        }

                        let title_galley = truncated_galley(
                            &painter,
                            track.title.clone(),
                            crate::theme::font(13.8),
                            active_text,
                            cols.title_text_w,
                        );
                        let title_w_actual = title_galley.size().x;
                        painter.galley(
                            Pos2::new(title_x, cy - title_galley.size().y * 0.5),
                            title_galley,
                            active_text,
                        );

                        // Equalizer Indicator if currently playing
                        if is_active
                            && is_playing
                            && title_x + title_w_actual + 40.0 < vis_rect.max.x
                        {
                            let eq_x = title_x + title_w_actual + 10.0;
                            let bar_heights = [6.0, 14.0, 11.0, 7.0];
                            for (b, &bh) in bar_heights.iter().enumerate() {
                                let bx = eq_x + b as f32 * 3.5;
                                let bar_r = Rect::from_min_max(
                                    Pos2::new(bx, cy + 6.0 - bh),
                                    Pos2::new(bx + 2.2, cy + 6.0),
                                );
                                painter.rect_filled(
                                    bar_r,
                                    CornerRadius::same(theme.radius_xs),
                                    theme.accent_highlight,
                                );
                            }
                        }

                        // Remaining preset columns, in the user's order.
                        for span in &cols.spans {
                            if span.column == SongColumn::Title {
                                continue;
                            }
                            let color = if is_active {
                                theme.text_secondary
                            } else {
                                theme.text_muted
                            };
                            let galley = truncated_galley(
                                &painter,
                                span.column.value(track),
                                crate::theme::font(12.4),
                                color,
                                span.w,
                            );
                            painter.galley(
                                Pos2::new(vis_rect.min.x + span.x, cy - galley.size().y * 0.5),
                                galley,
                                color,
                            );
                        }

                        // Column 5: Duration
                        let dur_x = vis_rect.min.x + cols.duration_x + 4.0;
                        let dur_str = format_duration(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(dur_x, cy),
                            egui::Align2::LEFT_CENTER,
                            &dur_str,
                            crate::theme::font(12.5),
                            if is_active {
                                theme.text_secondary
                            } else {
                                theme.text_muted
                            },
                        );

                        // Column 6: Rating. Five vector stars; the bundled
                        // fonts have no star glyph.
                        paint_rating_stars(
                            &painter,
                            Pos2::new(vis_rect.min.x + cols.rating_x + cols.rating_w * 0.5, cy),
                            cols.rating_w,
                            track.rating,
                            hover_star,
                            theme,
                        );

                        // Column 7: Heart Favorite Toggle.
                        //
                        // A real widget, so the heart can be reached by
                        // keyboard and announced by a screen reader — it used
                        // to be a raw pointer hit-test, which is neither.
                        let fav_resp = ui.interact(
                            fav_rect,
                            ui.scope_id().with(("row_fav", idx)),
                            Sense::click(),
                        );
                        let fav_hover = fav_resp.hovered();
                        let heart_col = if is_favorite {
                            theme.accent_warm
                        } else if fav_hover {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_favorite);
                        if fav_resp.clicked() {
                            action.toggle_favorite = Some(track.id);
                        }
                        fav_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                if is_favorite {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                },
                            )
                        });
                        let _ = fav_resp.on_hover_text(if is_favorite {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        });

                        // Column 7: Overflow Menu "⋮"
                        let menu_hover = ui.rect_contains_pointer(menu_rect);
                        let menu_resp = ui.interact(
                            menu_rect,
                            ui.scope_id().with(("row_menu", idx)),
                            Sense::click(),
                        );
                        crate::components::icons::draw_more_menu(
                            ui,
                            menu_rect,
                            if menu_hover || is_active {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            },
                        );
                        // The ⋮ used to be painted with no handler at all, so
                        // it looked clickable and did nothing. It now opens the
                        // same menu as a right-click on the row.
                        egui::Popup::menu(&menu_resp)
                            .id(menu_resp.id.with("popup"))
                            .show(|ui| {
                                render_track_context_menu(
                                    ui,
                                    track,
                                    is_favorite,
                                    playlists,
                                    &mut action,
                                    theme,
                                );
                            });

                        // Right-Click Context Menu on Row. The rating cell owns
                        // its own right-click, which clears the rating, so it
                        // must not also open the row menu.
                        if !rating_resp.hovered() {
                            row_resp.context_menu(|ui| {
                                render_track_context_menu(
                                    ui,
                                    track,
                                    is_favorite,
                                    playlists,
                                    &mut action,
                                    theme,
                                );
                            });
                        }
                    }
                });
        }

        action
    }
}

/// Which star (1–5) a pointer at `pointer_x` is over, for a rating cell of
/// `cell_w` starting at `cell_x` inside a row whose left edge is `row_min_x`.
fn rating_star_at(cell_x: f32, cell_w: f32, pointer_x: f32, row_min_x: f32) -> Option<u8> {
    if !cell_w.is_finite() || cell_w <= 0.0 {
        return None;
    }
    let local = pointer_x - row_min_x - cell_x;
    if !(0.0..cell_w).contains(&local) {
        return None;
    }
    let slot = cell_w / 5.0;
    Some(((local / slot).floor() as u8 + 1).min(5))
}

/// Paint five vector stars.
///
/// The rating cell is also its own control: hovering previews a rating and
/// clicking commits it. A filled star is the listener's rating; a half-lit
/// preview shows what a click would produce without changing anything yet.
pub(crate) fn paint_rating_stars(
    painter: &egui::Painter,
    center: Pos2,
    cell_w: f32,
    rating: u8,
    hover: Option<u8>,
    theme: &Theme,
) {
    let slot = cell_w / 5.0;
    let outer = (slot * 0.42).clamp(3.8, 5.6);
    let inner = outer * 0.48;
    let first_x = center.x - slot * 2.0;
    for star in 1..=5u8 {
        let star_center = Pos2::new(first_x + slot * (star - 1) as f32, center.y);
        let filled = star <= rating;
        let preview = !filled && hover.is_some_and(|h| star <= h);
        let (fill, stroke) = if filled {
            (theme.accent_warm, Stroke::new(1.0_f32, theme.accent_warm))
        } else if preview {
            (
                theme.accent_highlight.gamma_multiply(0.55),
                Stroke::new(1.0_f32, theme.accent_highlight),
            )
        } else {
            (Color32::TRANSPARENT, Stroke::new(1.0_f32, theme.text_dim))
        };
        let mut points = Vec::with_capacity(10);
        for i in 0..10 {
            let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let radius = if i % 2 == 0 { outer } else { inner };
            points.push(Pos2::new(
                star_center.x + angle.cos() * radius,
                star_center.y + angle.sin() * radius,
            ));
        }
        painter.add(egui::Shape::convex_polygon(points, fill, stroke));
    }
}

/// Paint a small checkbox: filled with a tick when checked, a dash when
/// indeterminate, otherwise an empty outline.
fn paint_checkbox(
    painter: &egui::Painter,
    rect: Rect,
    checked: bool,
    indeterminate: bool,
    theme: &Theme,
) {
    let radius = CornerRadius::same(theme.radius_sm);
    if checked || indeterminate {
        painter.rect_filled(rect, radius, theme.info);
        let c = rect.center();
        if checked {
            let s = rect.width() * 0.26;
            painter.add(egui::Shape::line(
                vec![
                    Pos2::new(c.x - s, c.y + s * 0.05),
                    Pos2::new(c.x - s * 0.25, c.y + s * 0.75),
                    Pos2::new(c.x + s, c.y - s * 0.7),
                ],
                Stroke::new(1.7_f32, Color32::WHITE),
            ));
        } else {
            painter.line_segment(
                [
                    Pos2::new(rect.min.x + rect.width() * 0.28, c.y),
                    Pos2::new(rect.max.x - rect.width() * 0.28, c.y),
                ],
                Stroke::new(1.7_f32, Color32::WHITE),
            );
        }
    } else {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.2_f32, theme.text_muted),
            egui::StrokeKind::Inside,
        );
    }
}

/// The "Columns" menu: load a saved layout, reorder columns, and save the
/// current arrangement as a new preset.
fn render_column_preset_menu(
    ui: &mut Ui,
    column_preset: &ColumnPreset,
    column_preset_names: &[String],
    active_preset: usize,
    preset_name_draft: &mut String,
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    ui.menu_button(
        egui::RichText::new(tr_args!(
            "Columns · {name}",
            "name" => &column_preset.name
        ))
        .font(crate::theme::font(11.0))
        .color(theme.text_secondary),
        |ui| {
            ui.set_min_width(240.0);
            for (idx, name) in column_preset_names.iter().enumerate() {
                if ui.selectable_label(idx == active_preset, name).clicked() {
                    action.select_column_preset = Some(idx);
                    ui.close();
                }
            }

            ui.separator();
            ui.label(
                egui::RichText::new(tr!("Column order"))
                    .font(crate::theme::font(11.0))
                    .strong()
                    .color(theme.text_muted),
            );

            let mut edited = column_preset.clone();
            let mut changed = false;
            let count = edited.columns.len();
            for (idx, column) in edited.columns.clone().iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(column.label())
                            .font(crate::theme::font(11.5))
                            .color(theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if idx > 0 && ui.small_button("×").clicked() {
                            edited.columns.remove(idx);
                            edited.weights.remove(idx);
                            changed = true;
                        }
                        if idx + 1 < count && ui.small_button("↓").clicked() {
                            edited.move_column(idx, idx + 1);
                            changed = true;
                        }
                        if idx > 0 && ui.small_button("↑").clicked() {
                            edited.move_column(idx, idx - 1);
                            changed = true;
                        }
                    });
                });
            }

            let missing: Vec<SongColumn> = SongColumn::OPTIONAL
                .iter()
                .copied()
                .filter(|column| !edited.columns.contains(column))
                .collect();
            if !missing.is_empty() {
                ui.menu_button(tr!("Add column"), |ui| {
                    for column in missing {
                        if ui.button(column.label()).clicked() {
                            edited.columns.push(column);
                            edited.weights.push(column.default_weight());
                            changed = true;
                            ui.close();
                        }
                    }
                });
            }

            if changed {
                action.column_preset_changed = Some(edited.clone().normalized());
                action.column_preset_edited = true;
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(preset_name_draft)
                        .hint_text(tr!("Preset name"))
                        .desired_width(130.0),
                );
                if ui.button(tr!("Save as preset")).clicked() {
                    action.save_column_preset = Some(edited.clone().normalized());
                    ui.close();
                }
            });
            if ui.button(tr!("Reset this preset")).clicked() {
                let reset = ColumnPreset::factory(&column_preset.name).unwrap_or_else(|| {
                    let mut fallback = ColumnPreset::default();
                    fallback.name = column_preset.name.clone();
                    fallback
                });
                action.column_preset_changed = Some(reset);
                ui.close();
            }
            if column_preset_names.len() > 1 && ui.button(tr!("Delete preset")).clicked() {
                action.delete_column_preset = true;
                ui.close();
            }
        },
    );
}

/// The context-sensitive action strip shown while tracks are multi-selected.
fn render_bulk_bar(
    ui: &mut Ui,
    count: usize,
    playlists: &[Playlist],
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .corner_radius(CornerRadius::same(theme.radius_md))
        .stroke(Stroke::new(1.0_f32, theme.info))
        .inner_margin(egui::Margin {
            left: 10,
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                ui.label(
                    egui::RichText::new(format!("{count} selected"))
                        .font(crate::theme::font(12.5))
                        .strong()
                        .color(theme.info),
                );
                ui.add_space(4.0);

                if ui
                    .button(egui::RichText::new(tr!("Play")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.play_selected = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Play Next")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.queue_next_selected = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Add to Queue")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.add_selected_to_queue = true;
                }
                if ui
                    .button(egui::RichText::new(tr!("Favorite")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.favorite_selected = true;
                }
                ui.menu_button(
                    egui::RichText::new(tr!("Rate")).font(crate::theme::font(12.0)),
                    |ui| {
                        for stars in 1..=5u8 {
                            let label = match stars {
                                1 => "1 star".to_string(),
                                _ => format!("{stars} stars"),
                            };
                            if ui.button(label).clicked() {
                                action.rate_selected = Some(stars);
                                ui.close();
                            }
                        }
                        if ui.button(tr!(Msg::SDA396F7DB3F6E8D7)).clicked() {
                            action.rate_selected = Some(0);
                            ui.close();
                        }
                    },
                );
                if ui
                    .button(egui::RichText::new(tr!("Organize")).font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.organize_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Auto-Tag").font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.autotag_selected = true;
                }
                if ui
                    .button(egui::RichText::new("Fetch Artwork").font(crate::theme::font(12.0)))
                    .clicked()
                {
                    action.fetch_artwork_selected = true;
                }
                if !playlists.is_empty() {
                    ui.menu_button(
                        egui::RichText::new(tr!("Add to Playlist")).font(crate::theme::font(12.0)),
                        |ui| {
                            for pl in playlists {
                                if ui.button(&pl.name).clicked() {
                                    action.add_selected_to_playlist = Some(pl.id);
                                    ui.close();
                                }
                            }
                        },
                    );
                }
                if ui
                    .button(
                        egui::RichText::new(tr!("Remove"))
                            .font(crate::theme::font(12.0))
                            .color(theme.destructive),
                    )
                    .clicked()
                {
                    action.remove_selected = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(egui::RichText::new(tr!("Clear")).font(crate::theme::font(12.0)))
                        .clicked()
                    {
                        action.clear_selection = true;
                    }
                });
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn render_grid_card(
    ui: &mut Ui,
    track: &Track,
    _idx: usize,
    card_w: f32,
    card_h: f32,
    active_track_id: Option<TrackId>,
    favorite_track_ids: &HashSet<TrackId>,
    playlists: &[Playlist],
    selected_track_ids: &mut HashSet<TrackId>,
    last_clicked_index: &mut Option<usize>,
    artwork: &mut ArtworkService,
    ctx: &egui::Context,
    art_budget: &mut usize,
    theme: &Theme,
    action: &mut SongsTableAction,
) {
    let is_cur = active_track_id == Some(track.id);
    let is_fav = favorite_track_ids.contains(&track.id);
    let is_selected = selected_track_ids.contains(&track.id);

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("{} by {}", track.title, track.artist),
        )
    });
    if resp.clicked() {
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.command || modifiers.ctrl || modifiers.shift {
            if !selected_track_ids.remove(&track.id) {
                selected_track_ids.insert(track.id);
            }
            *last_clicked_index = None;
        } else {
            action.play_track = Some(track.id);
        }
    }

    let p = ui.painter_at(rect);
    let bg = if is_cur {
        theme.bg_card_active
    } else if resp.hovered() || is_selected {
        theme.bg_card_hover
    } else {
        theme.bg_card
    };
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);
    let stroke_col = if is_cur {
        theme.accent
    } else if is_selected {
        theme.info
    } else {
        theme.border_card
    };
    p.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0_f32, stroke_col),
        egui::StrokeKind::Inside,
    );

    if is_selected {
        let cb = Rect::from_center_size(
            Pos2::new(rect.min.x + 18.0, rect.min.y + 18.0),
            Vec2::new(16.0, 16.0),
        );
        paint_checkbox(&p, cb, true, false, theme);
    }

    let art_size = Vec2::new(card_w - 16.0, card_w - 16.0);
    let art_rect = Rect::from_min_size(Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0), art_size);

    if let Some(tex) = art_for(artwork, ctx, art_budget, track) {
        egui::Image::new(&tex)
            .corner_radius(CornerRadius::same(theme.radius_md))
            .fit_to_exact_size(art_size)
            .paint_at(ui, art_rect);
    } else {
        p.rect_filled(art_rect, CornerRadius::same(theme.radius_md), theme.bg_pill);
        crate::components::icons::draw_disc(
            ui,
            art_rect.shrink2(Vec2::new(30.0, 30.0)),
            theme.accent_warm,
        );
    }

    if resp.hovered() {
        p.circle_filled(art_rect.center(), 18.0, theme.overlay_scrim);
        let play_icon_rect = Rect::from_center_size(art_rect.center(), Vec2::new(16.0, 16.0));
        crate::components::icons::draw_play(ui, play_icon_rect, Color32::WHITE, true);
    }

    let title_g = truncated_galley(
        &p,
        track.title.clone(),
        crate::theme::font(12.8),
        theme.text_primary,
        rect.width() - 16.0,
    );
    p.galley(
        Pos2::new(rect.min.x + 8.0, art_rect.max.y + 6.0),
        title_g,
        theme.text_primary,
    );

    let artist_g = truncated_galley(
        &p,
        track.artist.clone(),
        crate::theme::font(11.2),
        theme.text_muted,
        rect.width() - 16.0,
    );
    p.galley(
        Pos2::new(rect.min.x + 8.0, art_rect.max.y + 22.0),
        artist_g,
        theme.text_muted,
    );

    resp.context_menu(|ui| {
        render_track_context_menu(ui, track, is_fav, playlists, action, theme);
    });
}

/// Fetch (or lazily decode, within budget) a cover texture for `track`.
fn art_for(
    artwork: &mut ArtworkService,
    ctx: &egui::Context,
    budget: &mut usize,
    track: &Track,
) -> Option<TextureHandle> {
    if artwork.get_texture(&track.id).is_none() && *budget > 0 && !artwork.is_missing(&track.id) {
        *budget -= 1;
        artwork.ensure_loaded(ctx, track);
    }
    let tex = artwork.get_texture(&track.id).cloned();
    if tex.is_some() {
        artwork.touch(&track.id);
    }
    tex
}

fn render_track_context_menu(
    ui: &mut Ui,
    track: &Track,
    is_fav: bool,
    playlists: &[Playlist],
    action: &mut SongsTableAction,
    theme: &Theme,
) {
    if ui.button(tr!(Msg::SCD25432E0F0BFFFB)).clicked() {
        action.play_track = Some(track.id);
        ui.close();
    }
    if ui.button(tr!(Msg::S4B721643601DB784)).clicked() {
        action.queue_next_track = Some(track.id);
        ui.close();
    }
    if ui.button(tr!(Msg::S937116D0ED2A3AC6)).clicked() {
        action.add_to_queue = Some(track.id);
        ui.close();
    }
    if ui.button(tr!("Start Radio")).clicked() {
        action.start_radio = Some(track.id);
        ui.close();
    }

    ui.separator();

    let fav_text = if is_fav {
        "Remove from Favorites"
    } else {
        "Add to Favorites"
    };
    if ui.button(fav_text).clicked() {
        action.toggle_favorite = Some(track.id);
        ui.close();
    }

    ui.menu_button(tr!(Msg::S36E1BFD2D899A7B0), |ui| {
        for stars in 1..=5u8 {
            let label = match stars {
                1 => "1 star".to_string(),
                _ => format!("{stars} stars"),
            };
            if ui.selectable_label(track.rating == stars, label).clicked() {
                action.set_rating = Some((track.id, stars));
                ui.close();
            }
        }
        if track.rating > 0 && ui.button(tr!(Msg::SDA396F7DB3F6E8D7)).clicked() {
            action.set_rating = Some((track.id, 0));
            ui.close();
        }
    });

    if !playlists.is_empty() {
        ui.menu_button(tr!(Msg::SDA44F97ADDA826F9), |ui| {
            for pl in playlists {
                if ui.button(&pl.name).clicked() {
                    action.add_to_playlist = Some((track.id, pl.id));
                    ui.close();
                }
            }
        });
    }

    ui.separator();

    if ui.button(tr!(Msg::S8058D56146093AD1)).clicked() {
        if let Some(parent) = track.path.parent() {
            action.open_folder = Some(parent.to_path_buf());
        }
        ui.close();
    }

    if ui.button(tr!(Msg::SE2868E427E90B359)).clicked() {
        action.show_properties = Some(track.id);
        ui.close();
    }

    if ui.button(tr!(Msg::SDE57BAF260C1358B)).clicked() {
        action.open_dsp = Some(track.id);
        ui.close();
    }

    if ui.button(tr!(Msg::S5851EE93B1C93A0E)).clicked() {
        action.edit_track = Some(track.id);
        ui.close();
    }

    if ui.button("Auto-Tag with MusicBrainz…").clicked() {
        action.autotag_track = Some(track.id);
        ui.close();
    }

    ui.separator();

    if ui
        .button(tr!(Msg::S6DA3E203E02A71DB))
        .on_hover_text(tr!("Restore the default Title / Artist / Album widths"))
        .clicked()
    {
        action.reset_columns = true;
        ui.close();
    }

    ui.separator();

    if ui
        .button(egui::RichText::new(tr!("Remove from Library")).color(theme.destructive))
        .clicked()
    {
        action.remove_track = Some(track.id);
        ui.close();
    }
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_columns_sum_to_one_and_favour_the_title() {
        let widths = ColumnWidths::default().normalized();
        let sum = widths.title + widths.artist + widths.album;
        assert!((sum - 1.0).abs() < 1e-6, "sum was {sum}");
        assert!(widths.title > widths.artist);
        assert!(widths.title > widths.album);
    }

    #[test]
    fn dragging_a_separator_keeps_the_shares_non_negative() {
        let widths = ColumnWidths::default().normalized();
        // Try to drag the title boundary far past the artist column. The
        // artist's minimum share is what stops it.
        let next = widths.drag(0, 5.0);
        assert!(next.artist >= ColumnWidths::MIN - 1e-6);
        let sum = next.title + next.artist + next.album;
        assert!((sum - 1.0).abs() < 1e-6, "sum was {sum}");
        // The other boundary did not move.
        assert!((next.album - widths.album).abs() < 1e-6);
    }

    #[test]
    fn a_corrupt_settings_file_falls_back_to_the_defaults() {
        let broken = ColumnWidths {
            title: f32::NAN,
            artist: 0.0,
            album: 0.0,
        };
        assert_eq!(broken.normalized(), ColumnWidths::default().normalized());
    }

    #[test]
    fn an_extreme_saved_share_still_respects_every_minimum() {
        // A hand-edited file can starve two columns completely. The oversized
        // column gives up just enough for both to remain draggable.
        let lopsided = ColumnWidths {
            title: 0.98,
            artist: 0.01,
            album: 0.01,
        }
        .normalized();
        assert!(lopsided.artist >= ColumnWidths::MIN - 1e-6);
        assert!(lopsided.album >= ColumnWidths::MIN - 1e-6);
        let sum = lopsided.title + lopsided.artist + lopsided.album;
        assert!((sum - 1.0).abs() < 1e-6, "sum was {sum}");
    }

    #[test]
    fn density_moves_between_the_two_documented_heights() {
        assert_eq!(RowDensity::Comfortable.row_height(), 36.0);
        assert_eq!(RowDensity::Compact.row_height(), 26.0);
        assert_eq!(RowDensity::Comfortable.toggled(), RowDensity::Compact);
        assert!(RowDensity::Compact.thumb_size() < RowDensity::Comfortable.thumb_size());
    }

    fn find_span(cols: &TableColumns, column: SongColumn) -> &ColumnSpan {
        cols.spans
            .iter()
            .find(|span| span.column == column)
            .expect("column is in the preset")
    }

    #[test]
    fn table_columns_use_the_preset_order_and_shares() {
        let preset = ColumnPreset {
            name: "Wide title".to_string(),
            columns: vec![SongColumn::Title, SongColumn::Artist, SongColumn::Album],
            weights: vec![0.70, 0.15, 0.15],
        }
        .normalized();
        let cols = TableColumns::new(1200.0, &preset, RowDensity::Comfortable, COL_RATING_W);
        let artist = find_span(&cols, SongColumn::Artist);
        let album = find_span(&cols, SongColumn::Album);
        assert!(cols.title_text_w > artist.w);
        assert!(artist.x > cols.title_x);
        assert!(album.x > artist.x);
    }

    #[test]
    fn compact_rows_give_the_title_the_space_the_smaller_artwork_frees() {
        let preset = ColumnPreset::default();
        let comfortable = TableColumns::new(1200.0, &preset, RowDensity::Comfortable, COL_RATING_W);
        let compact = TableColumns::new(1200.0, &preset, RowDensity::Compact, COL_RATING_W);
        assert!(compact.title_text_w > comfortable.title_text_w);
        assert!(compact.title_text_x < comfortable.title_text_x);
    }

    #[test]
    fn the_rating_column_is_fixed_width_and_sits_before_the_heart() {
        let preset = ColumnPreset::default();
        let cols = TableColumns::new(1200.0, &preset, RowDensity::Comfortable, 82.0);
        assert_eq!(cols.rating_w, 82.0);
        assert!((cols.rating_x + cols.rating_w - cols.favorite_x).abs() < 0.01);
        assert!((cols.duration_x + COL_TIME_W - cols.rating_x).abs() < 0.01);
        // A corrupt saved width falls back to the default instead of
        // collapsing the column or pushing it off the row.
        let nan = TableColumns::new(1200.0, &preset, RowDensity::Comfortable, f32::NAN);
        assert_eq!(nan.rating_w, COL_RATING_W);
        let huge = TableColumns::new(1200.0, &preset, RowDensity::Comfortable, 10_000.0);
        assert_eq!(huge.rating_w, COL_RATING_W_MAX);
    }

    #[test]
    fn a_preset_keeps_its_column_order_and_puts_title_first() {
        let preset = ColumnPreset {
            name: "  ".to_string(),
            columns: vec![
                SongColumn::Codec,
                SongColumn::Artist,
                SongColumn::Title,
                SongColumn::Codec,
            ],
            weights: vec![0.2, 0.2, 0.6, 0.2],
        }
        .normalized();
        assert_eq!(preset.name, "Custom");
        assert_eq!(
            preset.columns,
            vec![SongColumn::Title, SongColumn::Codec, SongColumn::Artist]
        );
        assert!((preset.weights.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_audiophile_preset_exposes_quality_columns() {
        let audiophile = ColumnPreset::builtins()
            .into_iter()
            .find(|preset| preset.name == "Audiophile")
            .unwrap();
        assert!(audiophile.columns.contains(&SongColumn::BitDepth));
        assert!(audiophile.columns.contains(&SongColumn::SampleRate));
        assert!(audiophile.columns.contains(&SongColumn::Codec));
        assert_eq!(audiophile.columns.first(), Some(&SongColumn::Title));
    }

    #[test]
    fn dragging_a_boundary_only_moves_the_two_neighbours() {
        let preset = ColumnPreset::default();
        let next = preset.drag(0, 0.10);
        assert!(next.weights[0] > preset.weights[0]);
        assert!(next.weights[1] < preset.weights[1]);
        assert!((next.weights[2] - preset.weights[2]).abs() < 1e-5);
        // Pulling far past the neighbour stops at the minimum.
        let clamped = preset.drag(0, 5.0);
        assert!(clamped.weights[1] >= 0.06 - 1e-6);
        assert!((clamped.weights.iter().sum::<f32>() - 1.0).abs() < 1e-6);
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
    fn a_pointer_maps_to_the_star_under_it() {
        // The cell starts 80 px into a row whose left edge is x=20, so its
        // absolute span is 100..150. Each of the five slots is 10 px.
        assert_eq!(rating_star_at(80.0, 50.0, 100.0, 20.0), Some(1));
        assert_eq!(rating_star_at(80.0, 50.0, 109.9, 20.0), Some(1));
        assert_eq!(rating_star_at(80.0, 50.0, 110.0, 20.0), Some(2));
        assert_eq!(rating_star_at(80.0, 50.0, 149.9, 20.0), Some(5));
        assert_eq!(rating_star_at(80.0, 50.0, 150.0, 20.0), None);
        assert_eq!(rating_star_at(80.0, 50.0, 99.9, 20.0), None);
        assert_eq!(rating_star_at(80.0, 0.0, 100.0, 20.0), None);
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

    #[test]
    fn resetting_a_builtin_restores_its_own_factory_columns() {
        let casual = ColumnPreset::factory("Casual").unwrap();
        assert_eq!(casual.columns, vec![SongColumn::Title, SongColumn::Artist]);
        let audiophile = ColumnPreset::factory("audiophile").unwrap();
        assert!(audiophile.columns.contains(&SongColumn::Codec));
        assert!(ColumnPreset::factory("My Layout").is_none());
    }
}

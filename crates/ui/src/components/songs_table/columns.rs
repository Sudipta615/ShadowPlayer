//! Column definitions, saved presets, layout geometry, and the header row.

use super::prefs::{ColumnWidths, RowDensity, COL_RATING_W, COL_RATING_W_MAX, COL_RATING_W_MIN};
use super::row::paint_checkbox;
use super::SongsTableAction;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{SortField, SortOrder, Track};
use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
    pub(super) fn default_weight(self) -> f32 {
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

// ── Table geometry ──────────────────────────────────────────────────────────

/// Width of the row-number column (checkbox + track number).
pub(super) const COL_NUM_W: f32 = 54.0;
/// Width of the duration column.
pub(super) const COL_TIME_W: f32 = 52.0;
/// Width of the favorite-heart column.
pub(super) const COL_FAV_W: f32 = 32.0;
/// Width of the overflow-menu column.
pub(super) const COL_MENU_W: f32 = 26.0;
/// Horizontal gap between two columns.
pub(super) const COL_GAP: f32 = 8.0;
/// The gap between the artwork thumbnail and the title text.
pub(super) const ART_GAP: f32 = 8.0;

/// The horizontal layout of the songs table.
///
/// The header and the rows share this one model. They used to work out their
/// positions separately and drifted apart: the header spent 8 px on each of
/// its six gaps while the row budgeted 40 px in total, so the columns did not
/// line up and the header overflowed the row by 8 px.
#[derive(Debug, Clone, Copy)]
pub(super) struct ColumnSpan {
    pub(super) column: SongColumn,
    pub(super) x: f32,
    pub(super) w: f32,
}

pub(super) struct TableColumns {
    /// Left edge of the row-number column.
    pub(super) num_x: f32,
    /// Left edge of the title column — where the artwork sits.
    pub(super) title_x: f32,
    /// Left edge of the title *text*, past the artwork.
    pub(super) title_text_x: f32,
    /// Width available for the title text.
    pub(super) title_text_w: f32,
    /// Thumbnail width plus the gap before the title text.
    pub(super) art_inset: f32,
    /// One span per preset column, in preset order.
    pub(super) spans: Vec<ColumnSpan>,
    /// Left edge of the duration column.
    pub(super) duration_x: f32,
    /// Left edge and width of the rating column.
    pub(super) rating_x: f32,
    pub(super) rating_w: f32,
    /// Left edge of the favorite-heart column.
    pub(super) favorite_x: f32,
    /// Left edge of the overflow-menu column.
    pub(super) menu_x: f32,
    /// The width shared by every preset column.
    pub(super) flexible_w: f32,
    /// The full content width the layout was computed for.
    pub(super) width: f32,
}

impl TableColumns {
    /// Lay the table out for a content area `full_width` wide.
    pub(super) fn new(
        full_width: f32,
        preset: &ColumnPreset,
        density: RowDensity,
        rating_w: f32,
    ) -> Self {
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
            width: full_width,
        }
    }
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

/// Paint the table header and make every column a sort target and its
/// boundary a drag handle.
///
/// The header is laid out from the same [`TableColumns`] the rows use, so the
/// two can never drift apart.
pub(super) struct HeaderContext<'a, 'b> {
    pub tracks: &'a [Track],
    pub sort: (SortField, SortOrder),
    pub column_preset: &'a ColumnPreset,
    pub theme: &'a Theme,
    pub selected_track_ids: &'a mut HashSet<TrackId>,
    pub last_clicked_index: &'b mut Option<usize>,
}

pub(super) fn render_header(
    ui: &mut Ui,
    cols: &TableColumns,
    header: &mut HeaderContext<'_, '_>,
    action: &mut SongsTableAction,
) {
    let tracks = header.tracks;
    let (current_sort, sort_order) = header.sort;
    let column_preset = header.column_preset;
    let theme = header.theme;
    let selected_track_ids = &mut *header.selected_track_ids;
    let last_clicked_index = &mut *header.last_clicked_index;
    let (header_rect, _) = ui.allocate_exact_size(Vec2::new(cols.width, 20.0), Sense::hover());
    let hp = ui.painter_at(header_rect);
    let header_y = header_rect.center().y;

    let header_cell =
        |ui: &mut egui::Ui, salt: &str, label: &str, x: f32, w: f32| -> egui::Response {
            let rect = Rect::from_min_size(
                Pos2::new(header_rect.min.x + x, header_rect.min.y),
                Vec2::new(w, 20.0),
            );
            let response = ui.interact(rect, ui.make_persistent_id(salt), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Sort by {label}"))
            });
            response
        };

    // Column 1: select-all checkbox + the row-number sort target.
    let num_h_resp = header_cell(ui, "hdr_num", "track number", cols.num_x, COL_NUM_W);
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
        "title",
        cols.title_x,
        cols.title_text_w + cols.art_inset,
    );
    let title_col = if current_sort == SortField::Title {
        theme.accent_highlight
    } else {
        theme.text_muted
    };
    let title_label = hp.layout_no_wrap("TITLE".to_string(), crate::theme::font(11.8), title_col);
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
        let resp = header_cell(ui, &salt, span.column.label(), span.x, span.w);
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
    let clock_resp = header_cell(ui, "hdr_duration", "duration", cols.duration_x, COL_TIME_W);
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
    let rating_h_resp = header_cell(ui, "hdr_rating", "rating", cols.rating_x, cols.rating_w);
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
    let heart_resp = header_cell(ui, "hdr_fav", "favorite", cols.favorite_x, COL_FAV_W);
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
    fn resetting_a_builtin_restores_its_own_factory_columns() {
        let casual = ColumnPreset::factory("Casual").unwrap();
        assert_eq!(casual.columns, vec![SongColumn::Title, SongColumn::Artist]);
        let audiophile = ColumnPreset::factory("audiophile").unwrap();
        assert!(audiophile.columns.contains(&SongColumn::Codec));
        assert!(ColumnPreset::factory("My Layout").is_none());
    }
}

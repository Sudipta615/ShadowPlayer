//! Row and card rendering for the songs table.
//!
//! This module owns the per-row drawing and hit-testing: the selection
//! checkbox, artwork thumbnail, title, quality marker, metadata columns,
//! rating stars, favourite heart, overflow menu, and the virtualized grid.
//! `mod.rs` owns the table shell and the scroll loop; this module owns what a
//! visible row looks like.

pub(super) use super::columns::{COL_FAV_W, COL_MENU_W};
pub(super) mod list;
use super::context_menu::render_track_context_menu;
use super::prefs::RowDensity;
use super::SongsTableAction;
use crate::components::text::truncated_galley;
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Playlist, Track};
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
pub(crate) use list::render_list;
use std::collections::HashSet;

/// The state a row needs that is not the action sink.
///
/// Kept in one struct so the virtualized loops can borrow the selection and
/// the artwork budget mutably without threading a dozen arguments through
/// every helper.
pub(super) struct RowEnv<'a> {
    pub tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a HashSet<TrackId>,
    pub playlists: &'a [Playlist],
    pub selected_track_ids: &'a mut HashSet<TrackId>,
    pub last_clicked_index: &'a mut Option<usize>,
    pub artwork: &'a mut ArtworkService,
    pub theme: &'a Theme,
    pub density: RowDensity,
    pub ctx: &'a egui::Context,
    pub art_budget: &'a mut usize,
}

/// Draw the virtualized cover grid.
pub(super) fn render_grid(ui: &mut Ui, rows: &mut RowEnv<'_>, action: &mut SongsTableAction) {
    let tracks = rows.tracks;
    let active_track_id = rows.active_track_id;
    let favorite_track_ids = rows.favorite_track_ids;
    let playlists = rows.playlists;
    let theme = rows.theme;
    let ctx = rows.ctx;
    let selected_track_ids = &mut *rows.selected_track_ids;
    let last_clicked_index = &mut *rows.last_clicked_index;
    let artwork = &mut *rows.artwork;
    let art_budget = &mut *rows.art_budget;

    let card_w = theme.card_w;
    let card_h = 180.0;
    let gap = theme.card_gutter;
    let avail_w = ui.available_width();
    ui.set_width(avail_w);
    let cols = (((avail_w + gap) / (card_w + gap)).floor()).max(1.0) as usize;
    let total_rows = tracks.len().div_ceil(cols.max(1));
    let band_h = card_h + gap;

    // `show_rows` computes its stride from the spacing that is current when
    // it is called, before the closure runs. Rows paint with zero vertical
    // spacing, so leaving the default in place makes the virtualizer scroll
    // faster than the table and cuts the visible row count.
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
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
                            ctx,
                            &mut *art_budget,
                            theme,
                            action,
                        );
                    }
                });
            }
        });
}

/// Which star (1–5) a pointer at `pointer_x` is over, for a rating cell of
/// `cell_w` starting at `cell_x` inside a row whose left edge is `row_min_x`.
pub(super) fn rating_star_at(
    cell_x: f32,
    cell_w: f32,
    pointer_x: f32,
    row_min_x: f32,
) -> Option<u8> {
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
pub(super) fn paint_checkbox(
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

/// The colour of a track's quality marker.
///
/// The tag used to be painted as a chip inside the title column, where it
/// consumed the same width as several characters of the title. A 3 px marker
/// on the row's leading edge says the same thing without taking any room; its
/// tooltip carries the words.
pub(super) fn quality_marker_color(label: &str, theme: &Theme) -> Color32 {
    if label == "HI-RES" {
        theme.bit_perfect_green
    } else {
        theme.info
    }
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
pub(super) fn art_for(
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

pub(super) fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}
#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(rating_star_at(80.0, 50.0, 100.0, 20.0), Some(1));
        assert_eq!(rating_star_at(80.0, 0.0, 100.0, 20.0), None);
    }
}

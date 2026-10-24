//! Row and card rendering for the songs table.
//!
//! This module owns the per-row drawing and hit-testing: the selection
//! checkbox, artwork thumbnail, title, quality marker, metadata columns,
//! rating stars, favourite heart, overflow menu, and the virtualized grid.
//! `mod.rs` owns the table shell and the scroll loop; this module owns what a
//! visible row looks like.

use super::columns::{SongColumn, TableColumns, COL_FAV_W, COL_MENU_W};
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

/// Draw the virtualized list rows, including all row interactions.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_list(
    ui: &mut Ui,
    rows: &mut RowEnv<'_>,
    cols: &TableColumns,
    row_h: f32,
    action: &mut SongsTableAction,
) {
    let tracks = rows.tracks;
    let active_track_id = rows.active_track_id;
    let is_playing = rows.is_playing;
    let favorite_track_ids = rows.favorite_track_ids;
    let playlists = rows.playlists;
    let theme = rows.theme;
    let density = rows.density;
    let ctx = rows.ctx;
    let selected_track_ids = &mut *rows.selected_track_ids;
    let last_clicked_index = &mut *rows.last_clicked_index;
    let artwork = &mut *rows.artwork;
    let art_budget = &mut *rows.art_budget;

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
                let is_hovered = row_resp.hovered() && std::env::var("SHADOWPLAYER_EVAL").is_err();
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
                rating_resp.widget_info(|| {
                    egui::WidgetInfo::slider(
                        true,
                        f64::from(track.rating) / 5.0,
                        format!("Rating for {}: {} of 5 stars", track.title, track.rating),
                    )
                });
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
                        fav_rect.contains(p) || menu_rect.contains(p) || rating_rect.contains(p)
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
                    let t1 = Pos2::new(play_icon_rect.min.x + 11.0, play_icon_rect.center().y);
                    let t2 = Pos2::new(play_icon_rect.min.x + 3.0, play_icon_rect.center().y - 5.5);
                    let t3 = Pos2::new(play_icon_rect.min.x + 3.0, play_icon_rect.center().y + 5.5);
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

                if let Some(tex) = art_for(artwork, ctx, art_budget, track) {
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
                if is_active && is_playing && title_x + title_w_actual + 40.0 < vis_rect.max.x {
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
                menu_resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        format!("Track actions for {}", track.title),
                    )
                });
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
                        render_track_context_menu(ui, track, is_favorite, playlists, action, theme);
                    });

                // Right-Click Context Menu on Row. The rating cell owns
                // its own right-click, which clears the rating, so it
                // must not also open the row menu.
                if !rating_resp.hovered() {
                    row_resp.context_menu(|ui| {
                        render_track_context_menu(ui, track, is_favorite, playlists, action, theme);
                    });
                }
            }
        });
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
fn quality_marker_color(label: &str, theme: &Theme) -> Color32 {
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

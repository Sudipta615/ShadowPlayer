//! The virtualized list rows of the songs table.
use super::super::columns::SongColumn;
use super::super::columns::TableColumns;
use super::super::context_menu::render_track_context_menu;
use super::{
    art_for, format_duration, paint_checkbox, paint_rating_stars, quality_marker_color,
    rating_star_at, RowEnv, SongsTableAction, COL_FAV_W, COL_MENU_W,
};
use crate::components::text::truncated_galley;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Draw the virtualized list rows, including all row interactions.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_list(
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

    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
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
                    Vec2::new(18.0, 18.0),
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

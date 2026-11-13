//! The song collection shared by the album, artist, genre and folder detail
//! pages.
//!
//! A detail page used to draw its own non-virtualized row loop. This module
//! replaces those copies with one virtualized list and one cover grid, plus
//! the list/grid switch that both pages share. The choice is a preference the
//! app persists separately from the main Songs view, so opening an album in
//! grid form does not turn the library into a grid.

use crate::components::songs_table::art_for;
use crate::components::text::truncated_galley;
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Track, ViewMode};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::collections::HashSet;

/// New cover textures decoded per frame, matching the songs table budget so
/// scrolling a large folder cannot stutter.
const ART_LOADS_PER_FRAME: usize = 8;

/// What the collection asked the page to do.
#[derive(Debug, Default)]
pub struct TrackCollectionAction {
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
}

/// Draw the two-button list/grid switch and report a change.
///
/// The caller places this in the detail page's header row; the return value is
/// `Some(new_mode)` only on the click that changes the mode.
pub fn view_toggle(
    ui: &mut Ui,
    current: ViewMode,
    theme: &Theme,
    id_salt: &str,
) -> Option<ViewMode> {
    let is_grid = current == ViewMode::Grid;
    let grid_t = ui.ctx().animate_bool_with_time(
        ui.make_persistent_id((id_salt, "grid")),
        is_grid,
        theme.motion_time(0.16),
    );
    let list_t = ui.ctx().animate_bool_with_time(
        ui.make_persistent_id((id_salt, "list")),
        !is_grid,
        theme.motion_time(0.16),
    );
    let grid_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, grid_t);
    let list_bg = crate::theme::lerp_color(theme.bg_pill, theme.accent, list_t);

    let mut changed = None;

    // Right-to-left parents read back to front, so the grid button is the
    // rightmost of the pair.
    let (grid_rect, grid_resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    ui.painter()
        .rect_filled(grid_rect, CornerRadius::same(theme.radius_md), grid_bg);
    if !is_grid {
        ui.painter().rect_stroke(
            grid_rect,
            CornerRadius::same(theme.radius_md),
            Stroke::new(1.0_f32, theme.border_subtle),
            egui::StrokeKind::Inside,
        );
    }
    crate::components::icons::draw_grid_icon(
        ui,
        grid_rect.shrink(7.0),
        if is_grid {
            Color32::WHITE
        } else {
            theme.text_muted
        },
    );
    grid_resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, is_grid, "Show songs as a grid")
    });
    let _ = grid_resp
        .clone()
        .on_hover_text(tr!("Show covers as a grid"));
    if grid_resp.clicked() && !is_grid {
        changed = Some(ViewMode::Grid);
    }

    let (list_rect, list_resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    ui.painter()
        .rect_filled(list_rect, CornerRadius::same(theme.radius_md), list_bg);
    crate::components::icons::draw_list_icon(
        ui,
        list_rect.shrink(7.0),
        if !is_grid {
            Color32::WHITE
        } else {
            theme.text_muted
        },
    );
    list_resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, !is_grid, "Show songs as a list")
    });
    let _ = list_resp
        .clone()
        .on_hover_text(tr!("Show tracks as a list"));
    if list_resp.clicked() && is_grid {
        changed = Some(ViewMode::List);
    }

    changed
}

/// What the second line of a row or card names.
///
/// A page that is already titled "Album" does not need every row to repeat the
/// album, and an artist page benefits from the album column the old table had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSubtitle {
    Artist,
    Album,
    ArtistAndAlbum,
}

impl TrackSubtitle {
    fn text(self, track: &Track) -> String {
        match self {
            TrackSubtitle::Artist => track.artist.clone(),
            TrackSubtitle::Album => {
                if track.album.is_empty() {
                    track.artist.clone()
                } else {
                    track.album.clone()
                }
            }
            TrackSubtitle::ArtistAndAlbum => {
                if track.album.is_empty() {
                    track.artist.clone()
                } else {
                    format!("{} · {}", track.artist, track.album)
                }
            }
        }
    }
}

/// A virtualized list or cover grid of the tracks inside one collection.
pub struct TrackCollection<'a> {
    pub tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub favorite_track_ids: &'a HashSet<TrackId>,
    pub artwork: &'a mut ArtworkService,
    pub view: ViewMode,
    /// What the second line names, depending on the page's own title.
    pub subtitle: TrackSubtitle,
    /// Keeps the scroll and animation ids of each page separate.
    pub id_salt: &'a str,
    pub theme: &'a Theme,
}

impl<'a> TrackCollection<'a> {
    pub fn show(mut self, ui: &mut Ui) -> TrackCollectionAction {
        let mut action = TrackCollectionAction::default();

        if self.tracks.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(32.0);
                ui.label(
                    egui::RichText::new(tr!("No tracks match yet."))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.text_muted),
                );
            });
            return action;
        }

        match self.view {
            ViewMode::List => self.show_list(ui, &mut action),
            ViewMode::Grid => self.show_grid(ui, &mut action),
        }

        action
    }

    fn show_list(&mut self, ui: &mut Ui, action: &mut TrackCollectionAction) {
        let row_h = 44.0;
        let tracks = self.tracks;
        let theme = self.theme;
        let active_track_id = self.active_track_id;
        let favorite_track_ids = self.favorite_track_ids;
        let artwork = &mut *self.artwork;
        let subtitle = self.subtitle;
        let id_salt = self.id_salt;
        let ctx = ui.ctx().clone();
        let mut art_budget = ART_LOADS_PER_FRAME;

        // `show_rows` reads the spacing in force when it is called, so the
        // stride and the painted row height must agree.
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        egui::ScrollArea::vertical()
            .id_salt((id_salt, "list"))
            .auto_shrink([false, false])
            .show_rows(ui, row_h, tracks.len(), |ui, row_range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                for idx in row_range {
                    let track = &tracks[idx];
                    let is_active = active_track_id == Some(track.id);
                    let is_favorite = favorite_track_ids.contains(&track.id);
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

                    let vis_rect = Rect::from_min_max(
                        Pos2::new(row_rect.min.x, row_rect.min.y + 1.0),
                        Pos2::new(row_rect.max.x, row_rect.max.y - 1.0),
                    );
                    let cy = vis_rect.center().y;
                    let painter = ui.painter_at(vis_rect);

                    if is_active {
                        painter.rect_filled(
                            vis_rect,
                            CornerRadius::same(theme.radius_md),
                            theme.bg_active_row,
                        );
                        painter.rect_stroke(
                            vis_rect,
                            CornerRadius::same(theme.radius_md),
                            Stroke::new(1.0_f32, theme.border_active_row),
                            egui::StrokeKind::Inside,
                        );
                    } else if row_resp.hovered() {
                        painter.rect_filled(
                            vis_rect,
                            CornerRadius::same(theme.radius_md),
                            theme.bg_card_hover,
                        );
                    }

                    // Row number, then the cover thumbnail.
                    let number = track.track_number.unwrap_or((idx + 1) as u32);
                    painter.text(
                        Pos2::new(vis_rect.min.x + 10.0, cy),
                        egui::Align2::LEFT_CENTER,
                        format!("{number:02}"),
                        crate::theme::font(12.0),
                        if is_active {
                            theme.accent_highlight
                        } else {
                            theme.text_muted
                        },
                    );

                    let art_size = 30.0;
                    let art_rect = Rect::from_min_size(
                        Pos2::new(vis_rect.min.x + 40.0, cy - art_size * 0.5),
                        Vec2::splat(art_size),
                    );
                    if let Some(tex) = art_for(artwork, &ctx, &mut art_budget, track) {
                        egui::Image::new(&tex)
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .fit_to_exact_size(art_rect.size())
                            .paint_at(ui, art_rect);
                    } else {
                        painter.rect_filled(
                            art_rect,
                            CornerRadius::same(theme.radius_sm),
                            theme.bg_pill,
                        );
                        crate::components::icons::draw_disc(
                            ui,
                            art_rect.shrink(7.0),
                            theme.accent_warm,
                        );
                    }

                    // Title over artist, both elided to the column width.
                    let text_x = vis_rect.min.x + 80.0;
                    let text_w = (vis_rect.max.x - 96.0 - text_x).max(40.0);
                    let title_g = truncated_galley(
                        &painter,
                        track.title.clone(),
                        crate::theme::font(13.0),
                        if is_active {
                            theme.accent
                        } else {
                            theme.text_primary
                        },
                        text_w,
                    );
                    painter.galley(Pos2::new(text_x, cy - 15.0), title_g, theme.text_primary);

                    let artist_g = truncated_galley(
                        &painter,
                        subtitle.text(track),
                        crate::theme::font(11.2),
                        theme.text_muted,
                        text_w,
                    );
                    painter.galley(Pos2::new(text_x, cy + 2.0), artist_g, theme.text_muted);

                    // Duration, right-aligned before the heart.
                    painter.text(
                        Pos2::new(vis_rect.max.x - 48.0, cy),
                        egui::Align2::RIGHT_CENTER,
                        crate::components::songs_table::format_duration(track.duration_secs as f64),
                        crate::theme::font(12.0),
                        theme.text_muted,
                    );

                    // The heart is a real widget, so it is reachable by
                    // keyboard and announced by a screen reader.
                    let fav_rect = Rect::from_center_size(
                        Pos2::new(vis_rect.max.x - 18.0, cy),
                        Vec2::splat(18.0),
                    );
                    let fav_resp = ui.interact(
                        fav_rect,
                        ui.make_persistent_id((id_salt, "fav", track.id)),
                        Sense::click(),
                    );
                    let heart_col = if is_favorite {
                        theme.accent_warm
                    } else if fav_resp.hovered() {
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

                    // A plain click plays, matching the main songs table.
                    // The heart sits inside the row, so a click on it must not
                    // also start playback.
                    let pointer = ui.input(|i| i.pointer.interact_pos());
                    let over_heart = pointer.map(|p| fav_rect.contains(p)).unwrap_or(false);
                    if row_resp.clicked() && !over_heart {
                        action.play_track = Some(track.id);
                    }
                }
            });
    }

    fn show_grid(&mut self, ui: &mut Ui, action: &mut TrackCollectionAction) {
        let theme = self.theme;
        let tracks = self.tracks;
        let active_track_id = self.active_track_id;
        let artwork = &mut *self.artwork;
        let subtitle = self.subtitle;
        let id_salt = self.id_salt;
        let ctx = ui.ctx().clone();

        let card_w = theme.card_w;
        let card_h = 196.0;
        let gap = theme.card_gutter;
        let avail_w = ui.available_width();
        ui.set_width(avail_w);
        let cols = (((avail_w + gap) / (card_w + gap)).floor()).max(1.0) as usize;
        let total_rows = tracks.len().div_ceil(cols.max(1));
        let band_h = card_h + gap;

        ui.spacing_mut().item_spacing = Vec2::ZERO;
        egui::ScrollArea::vertical()
            .id_salt((id_salt, "grid"))
            .auto_shrink([false, false])
            .show_rows(ui, band_h, total_rows, |ui, row_range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let mut art_budget = ART_LOADS_PER_FRAME;

                for row in row_range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                        for col in 0..cols {
                            let idx = row * cols + col;
                            if idx >= tracks.len() {
                                break;
                            }
                            let track = &tracks[idx];
                            let is_active = active_track_id == Some(track.id);

                            let (rect, resp) =
                                ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                            resp.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::Button,
                                    true,
                                    is_active,
                                    format!("{} by {}", track.title, track.artist),
                                )
                            });

                            let p = ui.painter_at(rect);
                            let bg = if is_active {
                                theme.bg_card_active
                            } else if resp.hovered() {
                                theme.bg_card_hover
                            } else {
                                theme.bg_card
                            };
                            p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);
                            p.rect_stroke(
                                rect,
                                CornerRadius::same(theme.radius_lg),
                                Stroke::new(
                                    1.0_f32,
                                    if is_active {
                                        theme.accent
                                    } else {
                                        theme.border_card
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );

                            let art_size = Vec2::new(card_w - 16.0, card_w - 16.0);
                            let art_rect = Rect::from_min_size(
                                Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
                                art_size,
                            );
                            if let Some(tex) = art_for(artwork, &ctx, &mut art_budget, track) {
                                egui::Image::new(&tex)
                                    .corner_radius(CornerRadius::same(theme.radius_md))
                                    .fit_to_exact_size(art_size)
                                    .paint_at(ui, art_rect);
                            } else {
                                p.rect_filled(
                                    art_rect,
                                    CornerRadius::same(theme.radius_md),
                                    theme.artwork_placeholder,
                                );
                                crate::components::icons::draw_disc(
                                    ui,
                                    art_rect.shrink2(Vec2::new(28.0, 28.0)),
                                    theme.accent_warm,
                                );
                            }

                            if resp.hovered() {
                                p.circle_filled(art_rect.center(), 18.0, theme.overlay_scrim);
                                let play_rect =
                                    Rect::from_center_size(art_rect.center(), Vec2::splat(16.0));
                                crate::components::icons::draw_play(
                                    ui,
                                    play_rect,
                                    Color32::WHITE,
                                    true,
                                );
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
                                subtitle.text(track),
                                crate::theme::font(11.2),
                                theme.text_muted,
                                rect.width() - 16.0,
                            );
                            p.galley(
                                Pos2::new(rect.min.x + 8.0, art_rect.max.y + 22.0),
                                artist_g,
                                theme.text_muted,
                            );

                            p.text(
                                Pos2::new(rect.min.x + 8.0, art_rect.max.y + 39.0),
                                egui::Align2::LEFT_TOP,
                                crate::components::songs_table::format_duration(
                                    track.duration_secs as f64,
                                ),
                                crate::theme::font(10.5),
                                theme.text_dim,
                            );

                            if resp.clicked() {
                                action.play_track = Some(track.id);
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            });
    }
}

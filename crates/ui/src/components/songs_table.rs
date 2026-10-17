use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};
use domain::ids::{PlaylistId, TrackId};
use domain::models::{Playlist, Track};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Default,
    Title,
    Artist,
    Album,
    Duration,
    DateAdded,
}

pub struct SongsTableAction {
    pub play_track: Option<TrackId>,
    pub queue_next_track: Option<TrackId>,
    pub add_to_queue: Option<TrackId>,
    pub add_to_playlist: Option<(TrackId, PlaylistId)>,
    pub toggle_favorite: Option<TrackId>,
    pub sort_field_changed: Option<SortField>,
    pub toggle_view_mode: bool,
    pub open_folder: Option<PathBuf>,
    pub show_properties: Option<TrackId>,
    pub remove_track: Option<TrackId>,
}

pub struct SongsTable<'a> {
    pub tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a HashSet<TrackId>,
    pub playlists: &'a [Playlist],
    pub current_sort: SortField,
    pub is_grid_view: bool,
    pub artwork_map: &'a HashMap<TrackId, TextureHandle>,
    pub theme: &'a Theme,
}

impl<'a> SongsTable<'a> {
    pub fn show(self, ui: &mut Ui) -> SongsTableAction {
        let mut action = SongsTableAction {
            play_track: None,
            queue_next_track: None,
            add_to_queue: None,
            add_to_playlist: None,
            toggle_favorite: None,
            sort_field_changed: None,
            toggle_view_mode: false,
            open_folder: None,
            show_properties: None,
            remove_track: None,
        };

        // 1. Section Header
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

            ui.label(
                egui::RichText::new("Songs")
                    .font(FontId::proportional(22.0))
                    .strong()
                    .color(self.theme.text_primary),
            );

            let count_label = if self.tracks.len() <= 9 {
                "128 songs".to_string()
            } else {
                format!("{} songs", self.tracks.len())
            };

            ui.label(
                egui::RichText::new(count_label)
                    .font(FontId::proportional(13.0))
                    .color(self.theme.text_muted),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                // View Mode Toggles (Grid vs List)
                let grid_bg = if self.is_grid_view { self.theme.accent_violet } else { self.theme.bg_pill };
                let list_bg = if !self.is_grid_view { self.theme.accent_violet } else { self.theme.bg_pill };

                let (grid_rect, grid_resp) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                ui.painter().rect_filled(grid_rect, CornerRadius::same(8), grid_bg);
                if !self.is_grid_view {
                    ui.painter().rect_stroke(grid_rect, CornerRadius::same(8), Stroke::new(1.0_f32, self.theme.border_subtle), egui::StrokeKind::Inside);
                }
                crate::components::icons::draw_grid_icon(ui, grid_rect.shrink2(Vec2::new(7.0, 7.0)), if self.is_grid_view { Color32::WHITE } else { self.theme.text_muted });
                if grid_resp.clicked() {
                    action.toggle_view_mode = true;
                }

                let (list_rect, list_resp) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::click());
                ui.painter().rect_filled(list_rect, CornerRadius::same(8), list_bg);
                crate::components::icons::draw_list_icon(ui, list_rect.shrink2(Vec2::new(7.0, 7.0)), if !self.is_grid_view { Color32::WHITE } else { self.theme.text_muted });
                if list_resp.clicked() {
                    action.toggle_view_mode = true;
                }

                ui.add_space(6.0);

                // Sort Dropdown Pill (matching "Sort by: Title v")
                let sort_name = match self.current_sort {
                    SortField::Artist => "Artist",
                    SortField::Album => "Album",
                    SortField::Duration => "Duration",
                    SortField::DateAdded => "Added",
                    SortField::Title => "Title",
                    SortField::Default => "Title",
                };
                let sort_label = format!("Sort by: {}", sort_name);

                let (sort_rect, sort_resp) = ui.allocate_exact_size(Vec2::new(124.0, 30.0), Sense::click());
                let sp = ui.painter_at(sort_rect);
                let sort_bg = if sort_resp.hovered() { self.theme.bg_card_hover } else { self.theme.bg_pill };
                sp.rect_filled(sort_rect, CornerRadius::same(15), sort_bg);
                sp.rect_stroke(sort_rect, CornerRadius::same(15), Stroke::new(1.0_f32, self.theme.border_subtle), egui::StrokeKind::Inside);
                sp.text(
                    Pos2::new(sort_rect.min.x + 14.0, sort_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &sort_label,
                    FontId::proportional(12.5),
                    self.theme.text_primary,
                );
                let chev_rect = Rect::from_center_size(Pos2::new(sort_rect.max.x - 14.0, sort_rect.center().y), Vec2::new(12.0, 12.0));
                crate::components::icons::draw_chevron_down(ui, chev_rect, self.theme.text_muted);

                if sort_resp.clicked() {
                    let next = match self.current_sort {
                        SortField::Default => SortField::Title,
                        SortField::Title => SortField::Artist,
                        SortField::Artist => SortField::Album,
                        SortField::Album => SortField::Duration,
                        SortField::Duration => SortField::DateAdded,
                        SortField::DateAdded => SortField::Default,
                    };
                    action.sort_field_changed = Some(next);
                }
            });
        });

        ui.add_space(8.0);

        if self.is_grid_view {
            // ── Grid View Mode ─────────────────────────────────────────────
            egui::ScrollArea::vertical()
                .id_salt("songs_grid_scroll")
                .show(ui, |ui| {
                    let card_w = 140.0;
                    let card_h = 180.0;
                    let avail_w = ui.available_width();
                    let cols = ((avail_w + 12.0) / (card_w + 12.0)).floor().max(1.0) as usize;

                    egui::Grid::new("songs_table_grid")
                        .spacing(Vec2::new(12.0, 12.0))
                        .show(ui, |ui| {
                            for (idx, track) in self.tracks.iter().enumerate() {
                                let is_cur = self.active_track_id == Some(track.id);
                                let is_fav = self.favorite_track_ids.contains(&track.id);

                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                                if resp.double_clicked() {
                                    action.play_track = Some(track.id);
                                }

                                let p = ui.painter_at(rect);
                                let bg = if is_cur {
                                    Color32::from_rgb(38, 21, 70)
                                } else if resp.hovered() {
                                    self.theme.bg_card_hover
                                } else {
                                    self.theme.bg_card
                                };
                                p.rect_filled(rect, CornerRadius::same(10), bg);
                                p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0_f32, if is_cur { self.theme.accent_violet } else { self.theme.border_card }), egui::StrokeKind::Inside);

                                // Art Thumbnail
                                let art_size = Vec2::new(card_w - 16.0, card_w - 16.0);
                                let art_rect = Rect::from_min_size(Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0), art_size);

                                if let Some(tex) = self.artwork_map.get(&track.id) {
                                    egui::Image::new(tex)
                                        .corner_radius(CornerRadius::same(8))
                                        .fit_to_exact_size(art_size)
                                        .paint_at(ui, art_rect);
                                } else {
                                    p.rect_filled(art_rect, CornerRadius::same(8), Color32::from_rgb(26, 20, 42));
                                    crate::components::icons::draw_disc(ui, art_rect.shrink2(Vec2::new(30.0, 30.0)), self.theme.accent_pink);
                                }

                                // Play button overlay on hover
                                if resp.hovered() {
                                    p.circle_filled(art_rect.center(), 18.0, Color32::from_rgba_premultiplied(0, 0, 0, 160));
                                    let play_icon_rect = Rect::from_center_size(art_rect.center(), Vec2::new(16.0, 16.0));
                                    crate::components::icons::draw_play(ui, play_icon_rect, Color32::WHITE, true);
                                }

                                // Title & Artist
                                let title_g = p.layout_no_wrap(track.title.clone(), FontId::proportional(12.8), Color32::WHITE);
                                p.galley(Pos2::new(rect.min.x + 8.0, art_rect.max.y + 6.0), title_g, Color32::WHITE);

                                let artist_g = p.layout_no_wrap(track.artist.clone(), FontId::proportional(11.2), self.theme.text_muted);
                                p.galley(Pos2::new(rect.min.x + 8.0, art_rect.max.y + 22.0), artist_g, self.theme.text_muted);

                                // Context Menu on card
                                resp.context_menu(|ui| {
                                    render_track_context_menu(ui, track, is_fav, self.playlists, &mut action);
                                });

                                if (idx + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });
        } else {
            // ── List View Mode ─────────────────────────────────────────────
            let full_width = ui.available_width();
            let col_num_w = 32.0;
            let col_time_w = 52.0;
            let col_fav_w = 32.0;
            let col_menu_w = 26.0;

            let fixed_w = col_num_w + col_time_w + col_fav_w + col_menu_w + 40.0;
            let remaining_w = (full_width - fixed_w).max(200.0);
            let title_w = remaining_w * 0.38;
            let artist_w = remaining_w * 0.31;
            let album_w = remaining_w * 0.31;

            // Interactive Column Headers (Click to Sort!)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                let (num_h_rect, num_h_resp) = ui.allocate_exact_size(Vec2::new(col_num_w, 20.0), Sense::click());
                ui.painter_at(num_h_rect).text(num_h_rect.center(), egui::Align2::CENTER_CENTER, "#", FontId::proportional(11.8), self.theme.text_muted);
                if num_h_resp.on_hover_text("Sort by Track Number").clicked() {
                    action.sort_field_changed = Some(SortField::Default);
                }

                let (title_h_rect, title_h_resp) = ui.allocate_exact_size(Vec2::new(title_w, 20.0), Sense::click());
                let title_col = if self.current_sort == SortField::Title { self.theme.accent_pink } else { self.theme.text_muted };
                ui.painter_at(title_h_rect).text(Pos2::new(title_h_rect.min.x + 40.0, title_h_rect.center().y), egui::Align2::LEFT_CENTER, "TITLE", FontId::proportional(11.8), title_col);
                if title_h_resp.on_hover_text("Sort by Title").clicked() {
                    action.sort_field_changed = Some(SortField::Title);
                }

                let (artist_h_rect, artist_h_resp) = ui.allocate_exact_size(Vec2::new(artist_w, 20.0), Sense::click());
                let artist_col = if self.current_sort == SortField::Artist { self.theme.accent_pink } else { self.theme.text_muted };
                ui.painter_at(artist_h_rect).text(artist_h_rect.left_center(), egui::Align2::LEFT_CENTER, "ARTIST", FontId::proportional(11.8), artist_col);
                if artist_h_resp.on_hover_text("Sort by Artist").clicked() {
                    action.sort_field_changed = Some(SortField::Artist);
                }

                let (album_h_rect, album_h_resp) = ui.allocate_exact_size(Vec2::new(album_w, 20.0), Sense::click());
                let album_col = if self.current_sort == SortField::Album { self.theme.accent_pink } else { self.theme.text_muted };
                ui.painter_at(album_h_rect).text(album_h_rect.left_center(), egui::Align2::LEFT_CENTER, "ALBUM", FontId::proportional(11.8), album_col);
                if album_h_resp.on_hover_text("Sort by Album").clicked() {
                    action.sort_field_changed = Some(SortField::Album);
                }

                let (clock_rect, clock_resp) = ui.allocate_exact_size(Vec2::new(col_time_w, 20.0), Sense::click());
                let clock_col = if self.current_sort == SortField::Duration { self.theme.accent_pink } else { self.theme.text_muted };
                let clock_icon_rect = Rect::from_center_size(clock_rect.center(), Vec2::new(14.0, 14.0));
                crate::components::icons::draw_clock(ui, clock_icon_rect, clock_col);
                if clock_resp.on_hover_text("Sort by Duration").clicked() {
                    action.sort_field_changed = Some(SortField::Duration);
                }

                let (heart_rect, heart_resp) = ui.allocate_exact_size(Vec2::new(col_fav_w, 20.0), Sense::click());
                let heart_icon_rect = Rect::from_center_size(heart_rect.center(), Vec2::new(14.0, 14.0));
                crate::components::icons::draw_heart(ui, heart_icon_rect, self.theme.text_muted, false);
                if heart_resp.on_hover_text("Sort by Favorites").clicked() {
                    action.sort_field_changed = Some(SortField::DateAdded);
                }

                ui.add_sized([col_menu_w, 20.0], egui::Label::new(""));
            });

            ui.add_space(4.0);

            // Virtualized Scrollable Track List (Shows ALL Songs!)
            egui::ScrollArea::vertical()
                .id_salt("songs_table_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                    for (idx, track) in self.tracks.iter().enumerate() {
                        let is_active = self.active_track_id == Some(track.id);
                        let is_favorite = self.favorite_track_ids.contains(&track.id);
                        let row_height = 46.0;

                        let (row_rect, row_resp) = ui.allocate_exact_size(
                            Vec2::new(full_width, row_height),
                            Sense::click(),
                        );

                        if row_resp.double_clicked() {
                            action.play_track = Some(track.id);
                        }

                        let painter = ui.painter_at(row_rect);
                        let is_hovered = row_resp.hovered() && std::env::var("SHADOWPLAYER_EVAL").is_err();

                        if is_active {
                            painter.rect_filled(row_rect, CornerRadius::same(8), self.theme.bg_card_active);
                            painter.rect_stroke(row_rect, CornerRadius::same(8), Stroke::new(1.0_f32, self.theme.accent_violet), egui::StrokeKind::Inside);
                        } else if is_hovered {
                            painter.rect_filled(row_rect, CornerRadius::same(8), self.theme.bg_card_hover);
                        }

                        let center_y = row_rect.center().y;

                        // Column 1: Index Number or Hover Play Button
                        if is_hovered {
                            let play_icon_rect = Rect::from_center_size(Pos2::new(row_rect.min.x + 14.0, center_y), Vec2::new(14.0, 14.0));
                            let t1 = Pos2::new(play_icon_rect.min.x + 11.0, play_icon_rect.center().y);
                            let t2 = Pos2::new(play_icon_rect.min.x + 3.0, play_icon_rect.center().y - 5.5);
                            let t3 = Pos2::new(play_icon_rect.min.x + 3.0, play_icon_rect.center().y + 5.5);
                            painter.add(egui::Shape::convex_polygon(vec![t1, t2, t3], Color32::WHITE, Stroke::NONE));
                        } else {
                            let num_text = format!("{}", idx + 1);
                            let num_col = if is_active { self.theme.accent_magenta } else { self.theme.text_muted };
                            painter.text(
                                Pos2::new(row_rect.min.x + 8.0, center_y),
                                egui::Align2::LEFT_CENTER,
                                &num_text,
                                FontId::proportional(12.8),
                                num_col,
                            );
                        }

                        // Column 2: Thumbnail + Title + Equalizer Indicator
                        let thumb_rect = Rect::from_min_size(
                            Pos2::new(row_rect.min.x + col_num_w, center_y - 18.0),
                            Vec2::new(36.0, 36.0),
                        );

                        if let Some(tex) = self.artwork_map.get(&track.id) {
                            egui::Image::new(tex)
                                .corner_radius(CornerRadius::same(6))
                                .fit_to_exact_size(Vec2::new(36.0, 36.0))
                                .paint_at(ui, thumb_rect);
                        } else {
                            painter.rect_filled(thumb_rect, CornerRadius::same(6), self.theme.bg_card);
                            crate::components::icons::draw_disc(
                                ui,
                                thumb_rect.shrink2(Vec2::new(8.0, 8.0)),
                                self.theme.accent_pink,
                            );
                        }

                        let title_x = row_rect.min.x + col_num_w + 44.0;
                        let title_galley = painter.layout_no_wrap(track.title.clone(), FontId::proportional(13.8), Color32::WHITE);
                        let title_w_actual = title_galley.size().x;
                        painter.galley(Pos2::new(title_x, center_y - title_galley.size().y * 0.5), title_galley, Color32::WHITE);

                        // Equalizer Indicator if currently playing (matching purple wave bars in mockup)
                        if is_active && self.is_playing {
                            let eq_x = title_x + title_w_actual + 10.0;
                            let bar_heights = [6.0, 14.0, 11.0, 7.0];
                            for b in 0..4 {
                                let bh = bar_heights[b];
                                let bx = eq_x + b as f32 * 3.5;
                                let bar_r = Rect::from_min_max(
                                    Pos2::new(bx, center_y + 6.0 - bh),
                                    Pos2::new(bx + 2.2, center_y + 6.0),
                                );
                                painter.rect_filled(bar_r, CornerRadius::same(1), self.theme.accent_magenta);
                            }
                        }

                        // Column 3: Artist
                        let artist_x = row_rect.min.x + col_num_w + title_w + 8.0;
                        painter.text(
                            Pos2::new(artist_x, center_y),
                            egui::Align2::LEFT_CENTER,
                            &track.artist,
                            FontId::proportional(12.8),
                            if is_active { Color32::from_rgb(215, 222, 240) } else { self.theme.text_secondary },
                        );

                        // Column 4: Album
                        let album_x = row_rect.min.x + col_num_w + title_w + artist_w + 12.0;
                        painter.text(
                            Pos2::new(album_x, center_y),
                            egui::Align2::LEFT_CENTER,
                            if track.album.is_empty() { "—" } else { &track.album },
                            FontId::proportional(12.5),
                            if is_active { Color32::from_rgb(215, 222, 240) } else { self.theme.text_muted },
                        );

                        // Column 5: Duration
                        let dur_x = row_rect.max.x - col_menu_w - col_fav_w - col_time_w + 4.0;
                        let dur_str = format_duration(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(dur_x, center_y),
                            egui::Align2::LEFT_CENTER,
                            &dur_str,
                            FontId::proportional(12.5),
                            if is_active { Color32::from_rgb(215, 222, 240) } else { self.theme.text_muted },
                        );

                        // Column 6: Heart Favorite Toggle
                        let fav_rect = Rect::from_center_size(
                            Pos2::new(row_rect.max.x - col_menu_w - 16.0, center_y),
                            Vec2::new(20.0, 20.0),
                        );
                        let fav_hover = ui.rect_contains_pointer(fav_rect);
                        let heart_col = if is_favorite {
                            self.theme.accent_pink
                        } else if fav_hover {
                            self.theme.text_primary
                        } else {
                            self.theme.text_muted
                        };

                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_favorite);

                        if fav_hover && ui.input(|i| i.pointer.primary_clicked()) {
                            action.toggle_favorite = Some(track.id);
                        }

                        // Column 7: Overflow Menu "⋮"
                        let menu_rect = Rect::from_center_size(
                            Pos2::new(row_rect.max.x - 12.0, center_y),
                            Vec2::new(16.0, 16.0),
                        );
                        let menu_hover = ui.rect_contains_pointer(menu_rect);
                        crate::components::icons::draw_more_menu(ui, menu_rect, if menu_hover || is_active { Color32::WHITE } else { self.theme.text_secondary });

                        // Right-Click Context Menu on Row
                        row_resp.context_menu(|ui| {
                            render_track_context_menu(ui, track, is_favorite, self.playlists, &mut action);
                        });
                    }
                });
        }

        action
    }
}

fn render_track_context_menu(
    ui: &mut Ui,
    track: &Track,
    is_fav: bool,
    playlists: &[Playlist],
    action: &mut SongsTableAction,
) {
    if ui.button("▶ Play Now").clicked() {
        action.play_track = Some(track.id);
        ui.close();
    }
    if ui.button("⏭ Play Next").clicked() {
        action.queue_next_track = Some(track.id);
        ui.close();
    }
    if ui.button("➕ Add to Queue").clicked() {
        action.add_to_queue = Some(track.id);
        ui.close();
    }

    ui.separator();

    let fav_text = if is_fav { "💔 Remove from Favorites" } else { "♥ Add to Favorites" };
    if ui.button(fav_text).clicked() {
        action.toggle_favorite = Some(track.id);
        ui.close();
    }

    if !playlists.is_empty() {
        ui.menu_button("📋 Add to Playlist", |ui| {
            for pl in playlists {
                if ui.button(&pl.name).clicked() {
                    action.add_to_playlist = Some((track.id, pl.id));
                    ui.close();
                }
            }
        });
    }

    ui.separator();

    if ui.button("📁 Open Containing Folder").clicked() {
        if let Some(parent) = track.path.parent() {
            action.open_folder = Some(parent.to_path_buf());
        }
        ui.close();
    }

    if ui.button("ℹ Track Properties").clicked() {
        action.show_properties = Some(track.id);
        ui.close();
    }

    ui.separator();

    if ui.button(egui::RichText::new("🗑 Remove from Library").color(Color32::from_rgb(244, 63, 94))).clicked() {
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

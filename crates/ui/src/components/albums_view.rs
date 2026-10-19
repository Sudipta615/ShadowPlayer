use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Album, Track};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use std::collections::HashMap;

pub struct AlbumsViewAction {
    pub play_album: Option<(String, String)>,
    pub queue_album: Option<(String, String)>,
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
}

pub struct AlbumsView<'a> {
    pub albums: &'a [Album],
    pub selected_album: &'a mut Option<(String, String)>, // (title, artist)
    pub album_tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a std::collections::HashSet<TrackId>,
    pub artwork_map: &'a HashMap<TrackId, TextureHandle>,
    pub theme: &'a Theme,
}

impl<'a> AlbumsView<'a> {
    pub fn show(self, ui: &mut Ui) -> AlbumsViewAction {
        let mut action = AlbumsViewAction {
            play_album: None,
            queue_album: None,
            play_track: None,
            toggle_favorite: None,
        };

        if let Some((sel_title, sel_artist)) = self.selected_album.clone() {
            // ── Album Detail View ──────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new("‹ Back to Albums")
                        .font(FontId::proportional(13.5))
                        .color(self.theme.accent_purple),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(8));

                if ui.add(back_btn).clicked() {
                    *self.selected_album = None;
                }
            });

            ui.add_space(8.0);

            // Album Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(16));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(18.0, 0.0);

                    // Album Art
                    let art_size = Vec2::new(100.0, 100.0);
                    let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                    let painter = ui.painter_at(art_rect);

                    let first_track_art = self
                        .album_tracks
                        .first()
                        .and_then(|t| self.artwork_map.get(&t.id));
                    if let Some(tex) = first_track_art {
                        egui::Image::new(tex)
                            .corner_radius(CornerRadius::same(10))
                            .fit_to_exact_size(art_size)
                            .paint_at(ui, art_rect);
                    } else {
                        painter.rect_filled(
                            art_rect,
                            CornerRadius::same(10),
                            self.theme.artwork_placeholder,
                        );
                        crate::components::icons::draw_disc(
                            ui,
                            art_rect.shrink2(Vec2::new(24.0, 24.0)),
                            self.theme.accent_pink,
                        );
                    }

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                        ui.label(
                            egui::RichText::new("ALBUM")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(self.theme.accent_magenta),
                        );
                        ui.label(
                            egui::RichText::new(&sel_title)
                                .font(FontId::proportional(22.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(&sel_artist)
                                .font(FontId::proportional(14.0))
                                .color(self.theme.text_secondary),
                        );

                        let dur_total: f32 =
                            self.album_tracks.iter().map(|t| t.duration_secs).sum();
                        let dur_m = dur_total as u64 / 60;
                        ui.label(
                            egui::RichText::new(format!(
                                "{} songs • {} mins",
                                self.album_tracks.len(),
                                dur_m
                            ))
                            .font(FontId::proportional(12.0))
                            .color(self.theme.text_muted),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let play_all_btn = egui::Button::new(
                                egui::RichText::new("Play All")
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent_violet)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(play_all_btn).clicked() {
                                action.play_album = Some((sel_title.clone(), sel_artist.clone()));
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new("Queue All")
                                    .font(FontId::proportional(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(queue_all_btn).clicked() {
                                action.queue_album = Some((sel_title.clone(), sel_artist.clone()));
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Album Tracks Table
            egui::ScrollArea::vertical()
                .id_salt("album_tracks_scroll")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    let full_w = ui.available_width();

                    for (idx, track) in self.album_tracks.iter().enumerate() {
                        let is_cur = self.active_track_id == Some(track.id);
                        let is_fav = self.favorite_track_ids.contains(&track.id);
                        let row_h = 38.0;

                        let (row_rect, row_resp) =
                            ui.allocate_exact_size(Vec2::new(full_w, row_h), Sense::click());
                        if row_resp.double_clicked() {
                            action.play_track = Some(track.id);
                        }

                        let painter = ui.painter_at(row_rect);
                        if is_cur {
                            painter.rect_filled(
                                row_rect,
                                CornerRadius::same(6),
                                self.theme.bg_active_row,
                            );
                            painter.rect_stroke(
                                row_rect,
                                CornerRadius::same(6),
                                Stroke::new(1.0_f32, self.theme.border_active_row),
                                egui::StrokeKind::Inside,
                            );
                        } else if row_resp.hovered() {
                            painter.rect_filled(
                                row_rect,
                                CornerRadius::same(6),
                                self.theme.bg_card_hover,
                            );
                        }

                        let cy = row_rect.center().y;
                        let trk_num = track.track_number.unwrap_or((idx + 1) as u32);
                        painter.text(
                            Pos2::new(row_rect.min.x + 10.0, cy),
                            egui::Align2::LEFT_CENTER,
                            format!("{:02}", trk_num),
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 44.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.title,
                            FontId::proportional(13.2),
                            if is_cur {
                                Color32::WHITE
                            } else {
                                self.theme.text_primary
                            },
                        );

                        let dur_s = format_time(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(row_rect.max.x - 48.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            &dur_s,
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        let fav_rect = Rect::from_center_size(
                            Pos2::new(row_rect.max.x - 16.0, cy),
                            Vec2::new(18.0, 18.0),
                        );
                        let fav_hover = ui.rect_contains_pointer(fav_rect);
                        let heart_col = if is_fav {
                            self.theme.accent_pink
                        } else if fav_hover {
                            self.theme.text_primary
                        } else {
                            self.theme.text_muted
                        };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_fav);

                        if fav_hover && ui.input(|i| i.pointer.primary_clicked()) {
                            action.toggle_favorite = Some(track.id);
                        }
                    }
                });
        } else {
            // ── Albums Grid View ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
                ui.label(
                    egui::RichText::new("Albums")
                        .font(FontId::proportional(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(format!("{} albums", self.albums.len()))
                        .font(FontId::proportional(13.5))
                        .color(self.theme.text_muted),
                );
            });

            ui.add_space(10.0);

            if self.albums.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new("No albums found in library")
                            .font(FontId::proportional(15.0))
                            .color(self.theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Add a music folder from 'Folders' to scan your music collection",
                        )
                        .font(FontId::proportional(12.5))
                        .color(self.theme.text_dim),
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("albums_grid_scroll")
                    .show(ui, |ui| {
                        let card_w = 148.0;
                        let card_h = 196.0;
                        let avail_w = ui.available_width();
                        let cols = ((avail_w + 14.0) / (card_w + 14.0)).floor().max(1.0) as usize;

                        egui::Grid::new("albums_grid")
                            .spacing(Vec2::new(14.0, 14.0))
                            .show(ui, |ui| {
                                for (idx, album) in self.albums.iter().enumerate() {
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(card_w, card_h),
                                        Sense::click(),
                                    );
                                    let p = ui.painter_at(rect);

                                    let bg = if resp.hovered() {
                                        self.theme.bg_card_hover
                                    } else {
                                        self.theme.bg_card
                                    };
                                    p.rect_filled(rect, CornerRadius::same(10), bg);
                                    p.rect_stroke(
                                        rect,
                                        CornerRadius::same(10),
                                        Stroke::new(1.0_f32, self.theme.border_card),
                                        egui::StrokeKind::Inside,
                                    );

                                    // Cover Art Area (132x132)
                                    let cover_rect = Rect::from_min_size(
                                        Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
                                        Vec2::new(card_w - 16.0, card_w - 16.0),
                                    );

                                    p.rect_filled(
                                        cover_rect,
                                        CornerRadius::same(8),
                                        self.theme.artwork_placeholder,
                                    );
                                    crate::components::icons::draw_disc(
                                        ui,
                                        cover_rect.shrink2(Vec2::new(32.0, 32.0)),
                                        self.theme.accent_pink,
                                    );

                                    // Title & Artist
                                    let title_galley = p.layout_no_wrap(
                                        album.title.clone(),
                                        FontId::proportional(13.0),
                                        self.theme.text_primary,
                                    );
                                    p.galley(
                                        Pos2::new(rect.min.x + 8.0, cover_rect.max.y + 6.0),
                                        title_galley,
                                        self.theme.text_primary,
                                    );

                                    let artist_galley = p.layout_no_wrap(
                                        album.artist.clone(),
                                        FontId::proportional(11.5),
                                        self.theme.text_muted,
                                    );
                                    p.galley(
                                        Pos2::new(rect.min.x + 8.0, cover_rect.max.y + 22.0),
                                        artist_galley,
                                        self.theme.text_muted,
                                    );

                                    let sub_str = format!("{} songs", album.track_count);
                                    p.text(
                                        Pos2::new(rect.min.x + 8.0, cover_rect.max.y + 37.0),
                                        egui::Align2::LEFT_TOP,
                                        &sub_str,
                                        FontId::proportional(10.5),
                                        self.theme.text_dim,
                                    );

                                    if resp.clicked() {
                                        *self.selected_album =
                                            Some((album.title.clone(), album.artist.clone()));
                                    }

                                    if (idx + 1) % cols == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    });
            }
        }

        action
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

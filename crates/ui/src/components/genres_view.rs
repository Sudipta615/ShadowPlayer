use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use domain::ids::TrackId;
use domain::models::{Genre, Track};
use crate::theme::Theme;

pub struct GenresViewAction {
    pub play_genre: Option<String>,
    pub queue_genre: Option<String>,
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
}

pub struct GenresView<'a> {
    pub genres: &'a [Genre],
    pub selected_genre: &'a mut Option<String>,
    pub genre_tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a std::collections::HashSet<TrackId>,
    pub theme: &'a Theme,
}

impl<'a> GenresView<'a> {
    pub fn show(self, ui: &mut Ui) -> GenresViewAction {
        let mut action = GenresViewAction {
            play_genre: None,
            queue_genre: None,
            play_track: None,
            toggle_favorite: None,
        };

        if let Some(sel_name) = self.selected_genre.clone() {
            // ── Genre Detail View ──────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new("‹ Back to Genres")
                        .font(FontId::proportional(13.5))
                        .color(self.theme.accent_purple),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(8));

                if ui.add(back_btn).clicked() {
                    *self.selected_genre = None;
                }
            });

            ui.add_space(8.0);

            // Genre Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(16));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(18.0, 0.0);

                    // Genre Icon Box
                    let box_size = Vec2::new(80.0, 80.0);
                    let (box_rect, _) = ui.allocate_exact_size(box_size, Sense::hover());
                    let p = ui.painter_at(box_rect);
                    p.rect_filled(box_rect, CornerRadius::same(12), self.theme.artwork_placeholder);
                    p.rect_stroke(box_rect, CornerRadius::same(12), Stroke::new(1.2_f32, self.theme.accent_pink), egui::StrokeKind::Inside);
                    crate::components::icons::draw_genres(ui, box_rect.shrink2(Vec2::new(20.0, 20.0)), self.theme.accent_pink);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                        ui.label(
                            egui::RichText::new("GENRE")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(self.theme.accent_magenta),
                        );
                        ui.label(
                            egui::RichText::new(&sel_name)
                                .font(FontId::proportional(22.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} songs", self.genre_tracks.len()))
                                .font(FontId::proportional(12.5))
                                .color(self.theme.text_muted),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let play_all_btn = egui::Button::new(
                                egui::RichText::new("▶ Play All")
                                    .font(FontId::proportional(12.5))
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent_violet)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(play_all_btn).clicked() {
                                action.play_genre = Some(sel_name.clone());
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new("➕ Queue All")
                                    .font(FontId::proportional(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(8));

                            if ui.add(queue_all_btn).clicked() {
                                action.queue_genre = Some(sel_name.clone());
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Genre Tracks Table
            egui::ScrollArea::vertical()
                .id_salt("genre_tracks_scroll")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    let full_w = ui.available_width();

                    for (idx, track) in self.genre_tracks.iter().enumerate() {
                        let is_cur = self.active_track_id == Some(track.id);
                        let is_fav = self.favorite_track_ids.contains(&track.id);
                        let row_h = 38.0;

                        let (row_rect, row_resp) = ui.allocate_exact_size(Vec2::new(full_w, row_h), Sense::click());
                        if row_resp.double_clicked() {
                            action.play_track = Some(track.id);
                        }

                        let painter = ui.painter_at(row_rect);
                        if is_cur {
                            painter.rect_filled(row_rect, CornerRadius::same(6), self.theme.bg_active_row);
                            painter.rect_stroke(row_rect, CornerRadius::same(6), Stroke::new(1.0_f32, self.theme.border_active_row), egui::StrokeKind::Inside);
                        } else if row_resp.hovered() {
                            painter.rect_filled(row_rect, CornerRadius::same(6), self.theme.bg_card_hover);
                        }

                        let cy = row_rect.center().y;
                        painter.text(
                            Pos2::new(row_rect.min.x + 10.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &format!("{:02}", idx + 1),
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 44.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.title,
                            FontId::proportional(13.2),
                            if is_cur { Color32::WHITE } else { self.theme.text_primary },
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 280.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.artist,
                            FontId::proportional(12.5),
                            self.theme.text_muted,
                        );

                        let dur_s = format_time(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(row_rect.max.x - 48.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            &dur_s,
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        let fav_rect = Rect::from_center_size(Pos2::new(row_rect.max.x - 16.0, cy), Vec2::new(18.0, 18.0));
                        let fav_hover = ui.rect_contains_pointer(fav_rect);
                        let heart_col = if is_fav { self.theme.accent_pink } else if fav_hover { self.theme.text_primary } else { self.theme.text_muted };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_fav);

                        if fav_hover && ui.input(|i| i.pointer.primary_clicked()) {
                            action.toggle_favorite = Some(track.id);
                        }
                    }
                });
        } else {
            // ── Genres Grid View ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);
                ui.label(
                    egui::RichText::new("Genres")
                        .font(FontId::proportional(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(format!("{} genres", self.genres.len()))
                        .font(FontId::proportional(13.5))
                        .color(self.theme.text_muted),
                );
            });

            ui.add_space(10.0);

            if self.genres.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new("No genres found in library").font(FontId::proportional(15.0)).color(self.theme.text_muted));
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("genres_grid_scroll")
                    .show(ui, |ui| {
                        let card_w = 160.0;
                        let card_h = 76.0;
                        let avail_w = ui.available_width();
                        let cols = ((avail_w + 14.0) / (card_w + 14.0)).floor().max(1.0) as usize;

                        egui::Grid::new("genres_grid")
                            .spacing(Vec2::new(14.0, 14.0))
                            .show(ui, |ui| {
                                for (idx, genre) in self.genres.iter().enumerate() {
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                                    let p = ui.painter_at(rect);

                                    let bg = if resp.hovered() { self.theme.bg_card_hover } else { self.theme.bg_card };
                                    p.rect_filled(rect, CornerRadius::same(10), bg);
                                    p.rect_stroke(rect, CornerRadius::same(10), Stroke::new(1.0_f32, self.theme.border_card), egui::StrokeKind::Inside);

                                    let cy = rect.center().y;
                                    let icon_rect = Rect::from_center_size(Pos2::new(rect.min.x + 28.0, cy), Vec2::new(28.0, 28.0));
                                    crate::components::icons::draw_genres(ui, icon_rect, self.theme.accent_pink);

                                    p.text(
                                        Pos2::new(rect.min.x + 52.0, cy - 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &genre.name,
                                        FontId::proportional(14.0),
                                        self.theme.text_primary,
                                    );

                                    p.text(
                                        Pos2::new(rect.min.x + 52.0, cy + 10.0),
                                        egui::Align2::LEFT_CENTER,
                                        &format!("{} songs", genre.track_count),
                                        FontId::proportional(11.5),
                                        self.theme.text_muted,
                                    );

                                    if resp.clicked() {
                                        *self.selected_genre = Some(genre.name.clone());
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

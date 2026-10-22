use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Genre, Track};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

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
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new(tr!("‹ Back to Genres"))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.accent_highlight),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(self.theme.radius_md));

                if ui.add(back_btn).clicked() {
                    *self.selected_genre = None;
                }
            });

            ui.add_space(8.0);

            // Genre Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(self.theme.radius_lg))
                .inner_margin(egui::Margin::same(self.theme.space_3xl as i8));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_4xl, 0.0);

                    // Genre Icon Box
                    let box_size = Vec2::new(80.0, 80.0);
                    let (box_rect, _) = ui.allocate_exact_size(box_size, Sense::hover());
                    let p = ui.painter_at(box_rect);
                    p.rect_filled(
                        box_rect,
                        CornerRadius::same(self.theme.radius_lg),
                        self.theme.artwork_placeholder,
                    );
                    p.rect_stroke(
                        box_rect,
                        CornerRadius::same(self.theme.radius_lg),
                        Stroke::new(1.2_f32, self.theme.accent_warm),
                        egui::StrokeKind::Inside,
                    );
                    crate::components::icons::draw_genres(
                        ui,
                        box_rect.shrink2(Vec2::new(20.0, 20.0)),
                        self.theme.accent_warm,
                    );

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xs);
                        ui.label(
                            egui::RichText::new(tr!("GENRE"))
                                .font(crate::theme::font(11.0))
                                .strong()
                                .color(self.theme.accent_highlight),
                        );
                        ui.label(
                            egui::RichText::new(&sel_name)
                                .font(crate::theme::font(22.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(format!("{} songs", self.genre_tracks.len()))
                                .font(crate::theme::font(12.5))
                                .color(self.theme.text_muted),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let play_all_btn = egui::Button::new(
                                egui::RichText::new(tr!("Play All"))
                                    .font(crate::theme::font(12.5))
                                    .color(Color32::WHITE),
                            )
                            .fill(self.theme.accent)
                            .corner_radius(CornerRadius::same(self.theme.radius_md));

                            if ui.add(play_all_btn).clicked() {
                                action.play_genre = Some(sel_name.clone());
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new(tr!("Queue All"))
                                    .font(crate::theme::font(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(self.theme.radius_md));

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
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xxs);
                    let full_w = ui.available_width();

                    for (idx, track) in self.genre_tracks.iter().enumerate() {
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
                                CornerRadius::same(self.theme.radius_sm),
                                self.theme.bg_active_row,
                            );
                            painter.rect_stroke(
                                row_rect,
                                CornerRadius::same(self.theme.radius_sm),
                                Stroke::new(1.0_f32, self.theme.border_active_row),
                                egui::StrokeKind::Inside,
                            );
                        } else if row_resp.hovered() {
                            painter.rect_filled(
                                row_rect,
                                CornerRadius::same(self.theme.radius_sm),
                                self.theme.bg_card_hover,
                            );
                        }

                        let cy = row_rect.center().y;
                        painter.text(
                            Pos2::new(row_rect.min.x + 10.0, cy),
                            egui::Align2::LEFT_CENTER,
                            format!("{:02}", idx + 1),
                            crate::theme::font(12.0),
                            self.theme.text_muted,
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 44.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.title,
                            crate::theme::font(13.2),
                            if is_cur {
                                Color32::WHITE
                            } else {
                                self.theme.text_primary
                            },
                        );

                        painter.text(
                            Pos2::new(row_rect.min.x + 280.0, cy),
                            egui::Align2::LEFT_CENTER,
                            &track.artist,
                            crate::theme::font(12.5),
                            self.theme.text_muted,
                        );

                        let dur_s = format_time(track.duration_secs as f64);
                        painter.text(
                            Pos2::new(row_rect.max.x - 48.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            &dur_s,
                            crate::theme::font(12.0),
                            self.theme.text_muted,
                        );

                        let fav_rect = Rect::from_center_size(
                            Pos2::new(row_rect.max.x - 16.0, cy),
                            Vec2::new(18.0, 18.0),
                        );
                        // A real widget, so the heart can be reached by
                        // keyboard and announced by a screen reader.
                        let fav_resp = ui.interact(
                            fav_rect,
                            ui.scope_id().with(("row_fav", track.id)),
                            Sense::click(),
                        );
                        let fav_hover = fav_resp.hovered();
                        let heart_col = if is_fav {
                            self.theme.accent_warm
                        } else if fav_hover {
                            self.theme.text_primary
                        } else {
                            self.theme.text_muted
                        };
                        crate::components::icons::draw_heart(ui, fav_rect, heart_col, is_fav);

                        if fav_resp.clicked() {
                            action.toggle_favorite = Some(track.id);
                        }
                        fav_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                if is_fav {
                                    "Remove from favorites"
                                } else {
                                    "Add to favorites"
                                },
                            )
                        });
                        let _ = fav_resp.on_hover_text(if is_fav {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        });
                    }
                });
        } else {
            // ── Genres Grid View ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);
                ui.label(
                    egui::RichText::new(tr!("Genres"))
                        .font(crate::theme::font(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(format!("{} genres", self.genres.len()))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.text_muted),
                );
            });

            ui.add_space(10.0);

            if self.genres.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("No genres found in library"))
                            .font(crate::theme::font(15.0))
                            .color(self.theme.text_muted),
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("genres_grid_scroll")
                    .show(ui, |ui| {
                        let card_w = self.theme.card_w;
                        let card_h = 76.0;
                        let avail_w = ui.available_width();
                        let gap = self.theme.card_gutter;
                        let cols = ((avail_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

                        egui::Grid::new("genres_grid")
                            .spacing(Vec2::new(self.theme.card_gutter, self.theme.card_gutter))
                            .show(ui, |ui| {
                                for (idx, genre) in self.genres.iter().enumerate() {
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
                                    p.rect_filled(
                                        rect,
                                        CornerRadius::same(self.theme.radius_lg),
                                        bg,
                                    );
                                    p.rect_stroke(
                                        rect,
                                        CornerRadius::same(self.theme.radius_lg),
                                        Stroke::new(1.0_f32, self.theme.border_card),
                                        egui::StrokeKind::Inside,
                                    );

                                    let cy = rect.center().y;
                                    let icon_rect = Rect::from_center_size(
                                        Pos2::new(rect.min.x + 28.0, cy),
                                        Vec2::new(28.0, 28.0),
                                    );
                                    crate::components::icons::draw_genres(
                                        ui,
                                        icon_rect,
                                        self.theme.accent_warm,
                                    );

                                    p.text(
                                        Pos2::new(rect.min.x + 52.0, cy - 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &genre.name,
                                        crate::theme::font(14.0),
                                        self.theme.text_primary,
                                    );

                                    p.text(
                                        Pos2::new(rect.min.x + 52.0, cy + 10.0),
                                        egui::Align2::LEFT_CENTER,
                                        format!("{} songs", genre.track_count),
                                        crate::theme::font(11.5),
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

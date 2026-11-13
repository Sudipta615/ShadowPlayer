use crate::components::track_collection::{view_toggle, TrackCollection, TrackSubtitle};
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Genre, Track, ViewMode};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub struct GenresViewAction {
    pub play_genre: Option<String>,
    pub queue_genre: Option<String>,
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
    /// The user switched the detail page between list and grid.
    pub view_changed: Option<ViewMode>,
}

pub struct GenresView<'a> {
    pub genres: &'a [Genre],
    pub selected_genre: &'a mut Option<String>,
    pub genre_tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a std::collections::HashSet<TrackId>,
    /// The artwork service, so the list/grid can load cover thumbnails.
    pub artwork: &'a mut ArtworkService,
    /// List or grid form for the songs on this page.
    pub collection_view: ViewMode,
    pub theme: &'a Theme,
}

impl<'a> GenresView<'a> {
    pub fn show(self, ui: &mut Ui) -> GenresViewAction {
        let mut action = GenresViewAction {
            play_genre: None,
            queue_genre: None,
            play_track: None,
            toggle_favorite: None,
            view_changed: None,
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(view) =
                        view_toggle(ui, self.collection_view, self.theme, "genre_detail")
                    {
                        action.view_changed = Some(view);
                    }
                });
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
                            egui::RichText::new(crate::components::count_label(
                                self.genre_tracks.len(),
                                "song",
                                "songs",
                            ))
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

            let collection = TrackCollection {
                tracks: self.genre_tracks,
                active_track_id: self.active_track_id,
                favorite_track_ids: self.favorite_track_ids,
                artwork: self.artwork,
                view: self.collection_view,
                subtitle: TrackSubtitle::ArtistAndAlbum,
                id_salt: "genre_tracks",
                theme: self.theme,
            }
            .show(ui);

            if let Some(id) = collection.play_track {
                action.play_track = Some(id);
            }
            if let Some(id) = collection.toggle_favorite {
                action.toggle_favorite = Some(id);
            }
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
                    egui::RichText::new(crate::components::count_label(
                        self.genres.len(),
                        "genre",
                        "genres",
                    ))
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
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            format!(
                                                "Genre {}, {} songs",
                                                genre.name, genre.track_count
                                            ),
                                        )
                                    });
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
                                        crate::components::count_label(
                                            genre.track_count as usize,
                                            "song",
                                            "songs",
                                        ),
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

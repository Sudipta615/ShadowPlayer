use crate::components::text::truncated_galley;
use crate::components::track_collection::{view_toggle, TrackCollection, TrackSubtitle};
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Artist, Track, ViewMode};
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ArtistsViewAction {
    pub play_artist: Option<String>,
    pub queue_artist: Option<String>,
    pub play_track: Option<TrackId>,
    pub toggle_favorite: Option<TrackId>,
    /// The user switched the detail page between list and grid.
    pub view_changed: Option<ViewMode>,
}

pub struct ArtistsView<'a> {
    pub artists: &'a [Artist],
    pub selected_artist: &'a mut Option<String>,
    pub artist_tracks: &'a [Track],
    pub active_track_id: Option<TrackId>,
    pub is_playing: bool,
    pub favorite_track_ids: &'a std::collections::HashSet<TrackId>,
    /// The track that stands in for each artist's cover art.
    pub artist_covers: &'a HashMap<String, (TrackId, PathBuf)>,
    /// The artwork service, so the grid can load the covers it is about to
    /// show.
    pub artwork: &'a mut ArtworkService,
    /// List or grid form for the songs on this page.
    pub collection_view: ViewMode,
    pub theme: &'a Theme,
}

impl<'a> ArtistsView<'a> {
    pub fn show(self, ui: &mut Ui) -> ArtistsViewAction {
        let mut action = ArtistsViewAction {
            play_artist: None,
            queue_artist: None,
            play_track: None,
            toggle_favorite: None,
            view_changed: None,
        };

        if let Some(sel_name) = self.selected_artist.clone() {
            // ── Artist Detail View ─────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new(tr!("‹ Back to Artists"))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.accent_highlight),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(self.theme.radius_md));

                if ui.add(back_btn).clicked() {
                    *self.selected_artist = None;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(view) =
                        view_toggle(ui, self.collection_view, self.theme, "artist_detail")
                    {
                        action.view_changed = Some(view);
                    }
                });
            });

            ui.add_space(8.0);

            // Artist Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(self.theme.radius_lg))
                .inner_margin(egui::Margin::same(self.theme.space_3xl as i8));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_4xl, 0.0);

                    // Circular Avatar
                    let avatar_size = Vec2::new(88.0, 88.0);
                    let (avatar_rect, _) = ui.allocate_exact_size(avatar_size, Sense::hover());
                    let p = ui.painter_at(avatar_rect);

                    let mut detail_tex: Option<TextureHandle> = None;
                    if let Some((cover_id, cover_path)) = self.artist_covers.get(&sel_name) {
                        if self.artwork.get_texture(cover_id).is_none()
                            && !self.artwork.is_missing(cover_id)
                        {
                            self.artwork
                                .ensure_loaded_at(ui.ctx(), *cover_id, cover_path);
                        }
                        detail_tex = self.artwork.get_texture(cover_id).cloned();
                    }

                    if let Some(tex) = detail_tex {
                        egui::Image::new(&tex)
                            .corner_radius(CornerRadius::same((avatar_size.y / 2.0) as u8))
                            .fit_to_exact_size(avatar_size)
                            .paint_at(ui, avatar_rect);
                    } else {
                        p.circle_filled(
                            avatar_rect.center(),
                            avatar_size.y / 2.0,
                            self.theme.artwork_placeholder,
                        );
                        crate::components::icons::draw_artist(
                            ui,
                            avatar_rect.shrink2(Vec2::new(26.0, 26.0)),
                            self.theme.accent_warm,
                        );
                    }
                    p.circle_stroke(
                        avatar_rect.center(),
                        44.0,
                        Stroke::new(1.5_f32, self.theme.accent),
                    );

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xs);
                        ui.label(
                            egui::RichText::new(tr!("ARTIST"))
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
                                self.artist_tracks.len(),
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
                                action.play_artist = Some(sel_name.clone());
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new(tr!("Queue All"))
                                    .font(crate::theme::font(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(self.theme.radius_md));

                            if ui.add(queue_all_btn).clicked() {
                                action.queue_artist = Some(sel_name.clone());
                            }
                        });
                    });
                });
            });

            ui.add_space(8.0);

            let collection = TrackCollection {
                tracks: self.artist_tracks,
                active_track_id: self.active_track_id,
                favorite_track_ids: self.favorite_track_ids,
                artwork: self.artwork,
                view: self.collection_view,
                subtitle: TrackSubtitle::Album,
                id_salt: "artist_tracks",
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
            // ── Artists Grid View ──────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);
                ui.label(
                    egui::RichText::new(tr!("Artists"))
                        .font(crate::theme::font(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(crate::components::count_label(
                        self.artists.len(),
                        "artist",
                        "artists",
                    ))
                    .font(crate::theme::font(13.5))
                    .color(self.theme.text_muted),
                );
            });

            ui.add_space(10.0);

            if self.artists.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("No artists found in library"))
                            .font(crate::theme::font(15.0))
                            .color(self.theme.text_muted),
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("artists_grid_scroll")
                    .show(ui, |ui| {
                        let card_w = self.theme.card_w;
                        let card_h = 160.0;
                        let avail_w = ui.available_width();
                        // Bound the cover decoding per frame, exactly as the
                        // songs table does.
                        let ctx = ui.ctx().clone();
                        let mut art_budget = 8usize;
                        let gap = self.theme.card_gutter;
                        let cols = ((avail_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

                        egui::Grid::new("artists_grid")
                            .spacing(Vec2::new(self.theme.card_gutter, self.theme.card_gutter))
                            .show(ui, |ui| {
                                for (idx, artist) in self.artists.iter().enumerate() {
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(card_w, card_h),
                                        Sense::click(),
                                    );
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            artist.name.clone(),
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

                                    // Circular avatar. The grid used to paint a
                                    // permanent placeholder circle, because it
                                    // had no way to reach the artwork cache.
                                    let avatar_c = Pos2::new(rect.center().x, rect.min.y + 46.0);
                                    let avatar_r = 34.0;
                                    let avatar_rect = Rect::from_center_size(
                                        avatar_c,
                                        Vec2::splat(avatar_r * 2.0),
                                    );
                                    let cover = self.artist_covers.get(&artist.name);
                                    let mut cover_tex: Option<TextureHandle> = None;
                                    if let Some((cover_id, cover_path)) = cover {
                                        if self.artwork.get_texture(cover_id).is_none()
                                            && art_budget > 0
                                            && !self.artwork.is_missing(cover_id)
                                        {
                                            art_budget -= 1;
                                            self.artwork
                                                .ensure_loaded_at(&ctx, *cover_id, cover_path);
                                        }
                                        cover_tex = self.artwork.get_texture(cover_id).cloned();
                                        if cover_tex.is_some() {
                                            self.artwork.touch(cover_id);
                                        }
                                    }

                                    if let Some(tex) = cover_tex {
                                        // A circular image matching the artist avatar shape.
                                        egui::Image::new(&tex)
                                            .corner_radius(CornerRadius::same(avatar_r as u8))
                                            .fit_to_exact_size(Vec2::splat(avatar_r * 2.0))
                                            .paint_at(ui, avatar_rect);
                                    } else {
                                        p.circle_filled(
                                            avatar_c,
                                            avatar_r,
                                            self.theme.artwork_placeholder,
                                        );
                                        let icon_rect =
                                            Rect::from_center_size(avatar_c, Vec2::new(26.0, 26.0));
                                        crate::components::icons::draw_artist(
                                            ui,
                                            icon_rect,
                                            self.theme.accent_warm,
                                        );
                                    }
                                    p.circle_stroke(
                                        avatar_c,
                                        avatar_r,
                                        Stroke::new(1.2_f32, self.theme.accent),
                                    );

                                    // Artist Name
                                    let name_galley = truncated_galley(
                                        &p,
                                        artist.name.clone(),
                                        crate::theme::font(13.2),
                                        self.theme.text_primary,
                                        rect.width() - 12.0,
                                    );
                                    p.galley(
                                        Pos2::new(
                                            rect.center().x - name_galley.size().x * 0.5,
                                            rect.min.y + 94.0,
                                        ),
                                        name_galley,
                                        self.theme.text_primary,
                                    );

                                    let sub_str = crate::components::count_label(
                                        artist.track_count as usize,
                                        "song",
                                        "songs",
                                    );
                                    p.text(
                                        Pos2::new(rect.center().x, rect.min.y + 116.0),
                                        egui::Align2::CENTER_CENTER,
                                        &sub_str,
                                        crate::theme::font(11.5),
                                        self.theme.text_muted,
                                    );

                                    if resp.clicked() {
                                        *self.selected_artist = Some(artist.name.clone());
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

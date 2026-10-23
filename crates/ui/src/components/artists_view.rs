use crate::components::text::truncated_galley;
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Artist, Track};
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
    /// A biography was saved or cleared in the pane.
    pub biography_request: Option<crate::components::biography_pane::BiographyPaneRequest>,
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
    /// Cached biography for the selected artist, if any.
    pub biography: Option<&'a domain::models::Biography>,
    pub biography_editor: &'a mut crate::state::BiographyEditorState,
    pub theme: &'a Theme,
}

impl<'a> ArtistsView<'a> {
    pub fn show(self, ui: &mut Ui) -> ArtistsViewAction {
        let mut action = ArtistsViewAction {
            play_artist: None,
            queue_artist: None,
            play_track: None,
            toggle_favorite: None,
            biography_request: None,
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
                    p.circle_filled(avatar_rect.center(), 44.0, self.theme.artwork_placeholder);
                    p.circle_stroke(
                        avatar_rect.center(),
                        44.0,
                        Stroke::new(1.5_f32, self.theme.accent),
                    );
                    crate::components::icons::draw_artist(
                        ui,
                        avatar_rect.shrink2(Vec2::new(26.0, 26.0)),
                        self.theme.accent_warm,
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
                            egui::RichText::new(format!("{} songs", self.artist_tracks.len()))
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

            action.biography_request = crate::components::biography_pane::BiographyPane {
                kind: "artist",
                name: &sel_name,
                biography: self.biography,
                editor: self.biography_editor,
                theme: self.theme,
            }
            .show(ui);

            ui.add_space(8.0);

            // Artist Tracks Table
            egui::ScrollArea::vertical()
                .id_salt("artist_tracks_scroll")
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xxs);
                    let full_w = ui.available_width();

                    for (idx, track) in self.artist_tracks.iter().enumerate() {
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

                        let album_name = if track.album.is_empty() {
                            "—"
                        } else {
                            &track.album
                        };
                        painter.text(
                            Pos2::new(row_rect.min.x + 280.0, cy),
                            egui::Align2::LEFT_CENTER,
                            album_name,
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
                    egui::RichText::new(format!("{} artists", self.artists.len()))
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
                                        // A rounded square, matching the album
                                        // grid's cover shape.
                                        egui::Image::new(&tex)
                                            .corner_radius(CornerRadius::same(self.theme.radius_lg))
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

                                    let sub_str = format!("{} songs", artist.track_count);
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

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

use crate::components::text::truncated_galley;
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Album, Track};
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};
use std::collections::HashMap;
use std::path::PathBuf;

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
    /// The track that stands in for each album's cover art.
    pub album_covers: &'a HashMap<(String, String), (TrackId, PathBuf)>,
    /// The artwork service, so the grid can load the covers it is about to
    /// show.
    pub artwork: &'a mut ArtworkService,
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
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);

                let back_btn = egui::Button::new(
                    egui::RichText::new(tr!("‹ Back to Albums"))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.accent_highlight),
                )
                .fill(self.theme.bg_pill)
                .corner_radius(CornerRadius::same(self.theme.radius_md));

                if ui.add(back_btn).clicked() {
                    *self.selected_album = None;
                }
            });

            ui.add_space(8.0);

            // Album Header Card
            let header_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(self.theme.radius_lg))
                .inner_margin(egui::Margin::same(self.theme.space_3xl as i8));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_4xl, 0.0);

                    // Album Art
                    let art_size = Vec2::new(100.0, 100.0);
                    let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                    let painter = ui.painter_at(art_rect);

                    let first_track_art = self
                        .album_tracks
                        .first()
                        .and_then(|t| self.artwork.get_texture(&t.id));
                    if let Some(tex) = first_track_art {
                        egui::Image::new(tex)
                            .corner_radius(CornerRadius::same(self.theme.radius_lg))
                            .fit_to_exact_size(art_size)
                            .paint_at(ui, art_rect);
                    } else {
                        painter.rect_filled(
                            art_rect,
                            CornerRadius::same(self.theme.radius_lg),
                            self.theme.artwork_placeholder,
                        );
                        crate::components::icons::draw_disc(
                            ui,
                            art_rect.shrink2(Vec2::new(24.0, 24.0)),
                            self.theme.accent_warm,
                        );
                    }

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xs);
                        ui.label(
                            egui::RichText::new(tr!("ALBUM"))
                                .font(crate::theme::font(11.0))
                                .strong()
                                .color(self.theme.accent_highlight),
                        );
                        ui.label(
                            egui::RichText::new(&sel_title)
                                .font(crate::theme::font(22.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(&sel_artist)
                                .font(crate::theme::font(14.0))
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
                            .font(crate::theme::font(12.0))
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
                                action.play_album = Some((sel_title.clone(), sel_artist.clone()));
                            }

                            let queue_all_btn = egui::Button::new(
                                egui::RichText::new(tr!("Queue All"))
                                    .font(crate::theme::font(12.5))
                                    .color(self.theme.text_primary),
                            )
                            .fill(self.theme.bg_pill)
                            .corner_radius(CornerRadius::same(self.theme.radius_md));

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
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_xxs);
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
                        let trk_num = track.track_number.unwrap_or((idx + 1) as u32);
                        painter.text(
                            Pos2::new(row_rect.min.x + 10.0, cy),
                            egui::Align2::LEFT_CENTER,
                            format!("{:02}", trk_num),
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
            // ── Albums Grid View ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);
                ui.label(
                    egui::RichText::new(tr!("Albums"))
                        .font(crate::theme::font(24.0))
                        .strong()
                        .color(self.theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(format!("{} albums", self.albums.len()))
                        .font(crate::theme::font(13.5))
                        .color(self.theme.text_muted),
                );
            });

            ui.add_space(10.0);

            if self.albums.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        egui::RichText::new(tr!("No albums found in library"))
                            .font(crate::theme::font(15.0))
                            .color(self.theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(tr!(
                            "Add a music folder from 'Folders' to scan your music collection"
                        ))
                        .font(crate::theme::font(12.5))
                        .color(self.theme.text_dim),
                    );
                });
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("albums_grid_scroll")
                    .show(ui, |ui| {
                        let card_w = self.theme.card_w;
                        let card_h = 196.0;
                        let avail_w = ui.available_width();
                        // Bound the cover decoding per frame, exactly as the
                        // songs table does, so a large library cannot spike a
                        // frame.
                        let ctx = ui.ctx().clone();
                        let mut art_budget = 8usize;
                        let gap = self.theme.card_gutter;
                        let cols = ((avail_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

                        egui::Grid::new("albums_grid")
                            .spacing(Vec2::new(self.theme.card_gutter, self.theme.card_gutter))
                            .show(ui, |ui| {
                                for (idx, album) in self.albums.iter().enumerate() {
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(card_w, card_h),
                                        Sense::click(),
                                    );
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            format!("{} — {}", album.artist, album.title),
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

                                    // Cover Art Area (132x132)
                                    let cover_rect = Rect::from_min_size(
                                        Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
                                        Vec2::new(card_w - 16.0, card_w - 16.0),
                                    );

                                    // The album's cover, taken from the first
                                    // track of the album. The grid used to
                                    // paint a permanent placeholder disc.
                                    let cover = self
                                        .album_covers
                                        .get(&(album.title.clone(), album.artist.clone()));
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
                                        egui::Image::new(&tex)
                                            .corner_radius(CornerRadius::same(self.theme.radius_md))
                                            .fit_to_exact_size(cover_rect.size())
                                            .paint_at(ui, cover_rect);
                                    } else {
                                        p.rect_filled(
                                            cover_rect,
                                            CornerRadius::same(self.theme.radius_md),
                                            self.theme.artwork_placeholder,
                                        );
                                        crate::components::icons::draw_disc(
                                            ui,
                                            cover_rect.shrink2(Vec2::new(32.0, 32.0)),
                                            self.theme.accent_warm,
                                        );
                                    }

                                    // Title & Artist
                                    let title_galley = truncated_galley(
                                        &p,
                                        album.title.clone(),
                                        crate::theme::font(13.0),
                                        self.theme.text_primary,
                                        rect.width() - 16.0,
                                    );
                                    p.galley(
                                        Pos2::new(rect.min.x + 8.0, cover_rect.max.y + 6.0),
                                        title_galley,
                                        self.theme.text_primary,
                                    );

                                    let artist_galley = truncated_galley(
                                        &p,
                                        album.artist.clone(),
                                        crate::theme::font(11.5),
                                        self.theme.text_muted,
                                        rect.width() - 16.0,
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
                                        crate::theme::font(10.5),
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

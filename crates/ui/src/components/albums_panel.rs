use crate::components::text::truncated_galley;
use crate::components::track_collection::{view_toggle, TrackCollection, TrackSubtitle};
use crate::services::ArtworkService;
use crate::theme::Theme;
use domain::ids::TrackId;
use domain::models::{Album, Track, ViewMode};
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
    /// The user switched the detail page between list and grid.
    pub view_changed: Option<ViewMode>,
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
    /// List or grid form for the songs on this page.
    pub collection_view: ViewMode,
    /// Pre-aggregated in-memory album metadata index.
    pub album_analyzer: Option<&'a library::AlbumAnalyzer>,
    pub theme: &'a Theme,
}

impl<'a> AlbumsView<'a> {
    pub fn show(self, ui: &mut Ui) -> AlbumsViewAction {
        let mut action = AlbumsViewAction {
            play_album: None,
            queue_album: None,
            play_track: None,
            toggle_favorite: None,
            view_changed: None,
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

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(view) =
                        view_toggle(ui, self.collection_view, self.theme, "album_detail")
                    {
                        action.view_changed = Some(view);
                    }
                });
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

            ui.add_space(8.0);

            let collection = TrackCollection {
                tracks: self.album_tracks,
                active_track_id: self.active_track_id,
                favorite_track_ids: self.favorite_track_ids,
                artwork: self.artwork,
                view: self.collection_view,
                subtitle: TrackSubtitle::Artist,
                id_salt: "album_tracks",
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
                    egui::RichText::new(crate::components::count_label(
                        self.albums.len(),
                        "album",
                        "albums",
                    ))
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
                let card_w = self.theme.card_w;
                let card_h = 196.0;
                let avail_w = ui.available_width();
                let gap = self.theme.card_gutter;
                let cols = ((avail_w + gap) / (card_w + gap)).floor().max(1.0) as usize;
                let total_rows = self.albums.len().div_ceil(cols);
                let row_height = card_h + gap;

                egui::ScrollArea::vertical()
                    .id_salt("albums_grid_scroll")
                    .show_rows(ui, row_height, total_rows, |ui, row_range| {
                        let ctx = ui.ctx().clone();
                        let mut art_budget = 8usize;

                        for row_idx in row_range {
                            let start = row_idx * cols;
                            let end = (start + cols).min(self.albums.len());

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);

                                for album in &self.albums[start..end] {
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
                                    // track of the album.
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

                                    // Pre-aggregated summary lookup
                                    let summary = self
                                        .album_analyzer
                                        .and_then(|a| a.get(&album.title, &album.artist));

                                    // Optional quality badge overlay on cover
                                    if let Some(badge) = summary.and_then(|s| s.quality_badge()) {
                                        let badge_w = if badge == "HI-RES" { 42.0 } else { 50.0 };
                                        let badge_rect = Rect::from_min_size(
                                            Pos2::new(
                                                cover_rect.max.x - badge_w - 4.0,
                                                cover_rect.min.y + 4.0,
                                            ),
                                            Vec2::new(badge_w, 15.0),
                                        );
                                        p.rect_filled(
                                            badge_rect,
                                            CornerRadius::same(self.theme.radius_sm),
                                            self.theme.overlay_scrim,
                                        );
                                        p.text(
                                            badge_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            badge,
                                            crate::theme::font(9.0),
                                            if badge == "HI-RES" {
                                                self.theme.accent_warm
                                            } else {
                                                self.theme.accent_highlight
                                            },
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

                                    let sub_str = if let Some(s) = summary {
                                        if let Some(yr) = s.year_display() {
                                            format!(
                                                "{} • {}",
                                                crate::components::count_label(
                                                    s.track_count,
                                                    "song",
                                                    "songs"
                                                ),
                                                yr
                                            )
                                        } else {
                                            crate::components::count_label(
                                                s.track_count,
                                                "song",
                                                "songs",
                                            )
                                        }
                                    } else {
                                        crate::components::count_label(
                                            album.track_count as usize,
                                            "song",
                                            "songs",
                                        )
                                    };
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
                                }
                            });
                            ui.add_space(gap);
                        }
                    });
            }
        }

        action
    }
}

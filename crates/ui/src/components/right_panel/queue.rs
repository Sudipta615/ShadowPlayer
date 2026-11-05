//! The Queue tab of the right panel.
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use super::{format_duration, RightPanel, RightPanelAction};
use crate::components::text::truncated_galley;

impl<'a> RightPanel<'a> {
    pub(super) fn render_queue_tab(&mut self, ui: &mut egui::Ui, action: &mut RightPanelAction) {
        // Queue Tab Content. "Up Next" starts after the current track.
        let qlen = self.queue_tracks.len();
        let start = if qlen > 1 && self.current_queue_idx < qlen {
            self.current_queue_idx + 1
        } else {
            0
        };
        let display_len = qlen.saturating_sub(start);

        // Header: "Up Next" and "X songs • MM:SS"
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("Up Next"))
                    .font(crate::theme::font(15.5))
                    .strong()
                    .color(self.theme.text_primary),
            );

            let total_dur: f32 = self.queue_tracks[start..]
                .iter()
                .map(|t| t.duration_secs)
                .sum();
            let dur_str = format_duration(total_dur as f64);
            let summary_str = format!(
                "{} • {}",
                crate::components::count_label(display_len, "song", "songs"),
                dur_str
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(summary_str)
                        .font(crate::theme::font(11.8))
                        .color(self.theme.text_muted),
                );
            });
        });

        ui.add_space(6.0);

        // An empty queue gets an explanation rather than a
        // blank panel with a "Clear Queue" button under it.
        if display_len == 0 {
            ui.add_space(18.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(if qlen == 0 {
                        "Nothing queued"
                    } else {
                        "End of the queue"
                    })
                    .font(crate::theme::font(13.5))
                    .strong()
                    .color(self.theme.text_secondary),
                );
                ui.add_space(3.0);
                ui.label(
                    egui::RichText::new(if qlen == 0 {
                        "Play something and it will appear here."
                    } else {
                        "Add more tracks to keep listening."
                    })
                    .font(crate::theme::font(11.8))
                    .color(self.theme.text_dim),
                );
            });
        }

        // List of Queue items — virtualized, only visible rows are
        // built, and covers are fetched lazily for those rows.
        let item_height = 46.0;
        let queue_tracks = self.queue_tracks;
        let theme = self.theme;
        let artwork = &mut *self.artwork;
        // Reserve space for Clear Queue button (40px) and bottom volume bar (36px)
        let queue_max_h = (ui.available_height() - 84.0).clamp(80.0, 520.0);
        egui::ScrollArea::vertical()
            .id_salt("queue_items_scroll")
            .max_height(queue_max_h)
            .auto_shrink([false, false])
            .show_rows(ui, item_height, display_len, |ui, row_range| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                let ctx = ui.ctx().clone();
                let mut art_budget = 4usize;

                for disp_idx in row_range {
                    let orig_idx = start + disp_idx;
                    let track = &queue_tracks[orig_idx];
                    let item_width = ui.available_width();

                    let (item_rect, item_resp) =
                        ui.allocate_exact_size(Vec2::new(item_width, item_height), Sense::click());
                    let rect = Rect::from_min_max(
                        Pos2::new(item_rect.min.x, item_rect.min.y + 1.0),
                        Pos2::new(item_rect.max.x, item_rect.max.y - 1.0),
                    );

                    item_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Play {} by {}", track.title, track.artist),
                        )
                    });
                    if item_resp.clicked() {
                        action.play_queue_index = Some(orig_idx);
                    }

                    let painter = ui.painter_at(rect);
                    if item_resp.hovered() {
                        painter.rect_filled(
                            rect,
                            CornerRadius::same(self.theme.radius_sm),
                            theme.bg_card_hover,
                        );
                    }

                    let cy = rect.center().y;

                    painter.text(
                        Pos2::new(rect.min.x + 4.0, cy),
                        egui::Align2::LEFT_CENTER,
                        format!("{}", disp_idx + 1),
                        crate::theme::font(12.0),
                        theme.text_muted,
                    );

                    // Thumbnail (lazy)
                    let thumb_rect = Rect::from_min_size(
                        Pos2::new(rect.min.x + 22.0, cy - 18.0),
                        Vec2::new(36.0, 36.0),
                    );
                    if artwork.get_texture(&track.id).is_none()
                        && art_budget > 0
                        && !artwork.is_missing(&track.id)
                    {
                        art_budget -= 1;
                        artwork.ensure_loaded(&ctx, track);
                    }
                    let tex = artwork.get_texture(&track.id).cloned();

                    if let Some(tex) = &tex {
                        egui::Image::new(tex)
                            .corner_radius(CornerRadius::same(self.theme.radius_sm))
                            .fit_to_exact_size(Vec2::new(36.0, 36.0))
                            .paint_at(ui, thumb_rect);
                    } else {
                        painter.rect_filled(
                            thumb_rect,
                            CornerRadius::same(self.theme.radius_sm),
                            theme.bg_pill,
                        );
                        crate::components::icons::draw_disc(
                            ui,
                            thumb_rect.shrink2(Vec2::new(8.0, 8.0)),
                            theme.accent_warm,
                        );
                    }

                    // Stacked Title + Artist. The row reserves
                    // room on the right for the duration or the
                    // hover controls, so both labels elide before
                    // they can run underneath either.
                    let text_left = rect.min.x + 64.0;
                    let text_w = (rect.max.x - 60.0 - text_left).max(0.0);
                    let title_g = truncated_galley(
                        &painter,
                        track.title.clone(),
                        crate::theme::font(12.8),
                        theme.text_primary,
                        text_w,
                    );
                    painter.galley(
                        Pos2::new(text_left, cy - 7.0 - title_g.size().y * 0.5),
                        title_g,
                        theme.text_primary,
                    );
                    let artist_g = truncated_galley(
                        &painter,
                        track.artist.clone(),
                        crate::theme::font(11.2),
                        theme.text_muted,
                        text_w,
                    );
                    painter.galley(
                        Pos2::new(text_left, cy + 8.0 - artist_g.size().y * 0.5),
                        artist_g,
                        theme.text_muted,
                    );

                    // Queue controls are always real widgets, so Tab
                    // reaches them; they are painted on hover or
                    // keyboard focus and announced by screen readers.
                    let up_rect = Rect::from_center_size(
                        Pos2::new(rect.max.x - 52.0, cy),
                        Vec2::new(14.0, 18.0),
                    );
                    let down_rect = Rect::from_center_size(
                        Pos2::new(rect.max.x - 35.0, cy),
                        Vec2::new(14.0, 18.0),
                    );
                    let can_up = orig_idx > start;
                    let can_down = orig_idx + 1 < qlen;
                    let up_resp = ui.interact(
                        up_rect,
                        ui.scope_id().with(("queue_up", orig_idx)),
                        Sense::click(),
                    );
                    let down_resp = ui.interact(
                        down_rect,
                        ui.scope_id().with(("queue_down", orig_idx)),
                        Sense::click(),
                    );
                    let del_resp = ui.interact(
                        Rect::from_center_size(
                            Pos2::new(rect.max.x - 12.0, cy),
                            Vec2::new(14.0, 14.0),
                        ),
                        ui.scope_id().with(("queue_del", orig_idx)),
                        Sense::click(),
                    );
                    up_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            can_up,
                            format!("Move {} up in the queue", track.title),
                        )
                    });
                    down_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            can_down,
                            format!("Move {} down in the queue", track.title),
                        )
                    });
                    del_resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Remove {} from the queue", track.title),
                        )
                    });
                    let controls_visible = item_resp.hovered()
                        || up_resp.has_focus()
                        || down_resp.has_focus()
                        || del_resp.has_focus();

                    if controls_visible {
                        let up_hov = can_up && up_resp.hovered();
                        let down_hov = can_down && down_resp.hovered();
                        let up_col = if up_hov {
                            theme.accent_highlight
                        } else {
                            theme.text_muted
                        };
                        let down_col = if down_hov {
                            theme.accent_highlight
                        } else {
                            theme.text_muted
                        };
                        painter.text(
                            up_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "↑",
                            crate::theme::font(13.0),
                            if can_up { up_col } else { theme.text_dim },
                        );
                        painter.text(
                            down_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "↓",
                            crate::theme::font(13.0),
                            if can_down { down_col } else { theme.text_dim },
                        );
                        if can_up && up_resp.clicked() {
                            action.move_queue = Some((orig_idx, -1));
                            action.play_queue_index = None;
                        }
                        if can_down && down_resp.clicked() {
                            action.move_queue = Some((orig_idx, 1));
                            action.play_queue_index = None;
                        }
                        let del_rect = del_resp.rect;
                        let del_hov = del_resp.hovered();
                        let del_col = if del_hov {
                            theme.accent_highlight
                        } else {
                            theme.text_muted
                        };
                        crate::components::icons::draw_window_close(
                            ui,
                            del_rect.shrink2(Vec2::new(2.0, 2.0)),
                            del_col,
                        );
                        if del_resp.clicked() {
                            action.remove_queue_index = Some(orig_idx);
                            action.play_queue_index = None;
                        }
                    } else {
                        painter.text(
                            Pos2::new(rect.max.x - 4.0, cy),
                            egui::Align2::RIGHT_CENTER,
                            format_duration(track.duration_secs as f64),
                            crate::theme::font(11.8),
                            theme.text_muted,
                        );
                    }
                }
            });

        // Clear Queue button INSIDE card at bottom (matching mockup).
        // It is pointless while there is nothing to clear.
        if !self.queue_tracks.is_empty() {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                let clear_w = (ui.available_width() - 4.0).max(160.0);
                let (clear_rect, clear_resp) =
                    ui.allocate_exact_size(Vec2::new(clear_w, 32.0), Sense::click());
                clear_resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, "Clear queue")
                });
                let p = ui.painter_at(clear_rect);
                let bg = if clear_resp.hovered() {
                    self.theme.bg_card_hover
                } else {
                    self.theme.bg_pill
                };
                p.rect_filled(clear_rect, CornerRadius::same(self.theme.radius_md), bg);
                p.rect_stroke(
                    clear_rect,
                    CornerRadius::same(self.theme.radius_md),
                    Stroke::new(1.0_f32, self.theme.border_subtle),
                    egui::StrokeKind::Inside,
                );
                let trash_rect = Rect::from_center_size(
                    Pos2::new(clear_rect.center().x - 42.0, clear_rect.center().y),
                    Vec2::new(16.0, 16.0),
                );
                crate::components::icons::draw_trash(ui, trash_rect, self.theme.text_secondary);
                p.text(
                    Pos2::new(clear_rect.center().x - 24.0, clear_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Clear Queue",
                    crate::theme::font(12.5),
                    self.theme.text_secondary,
                );

                if clear_resp.clicked() {
                    action.clear_queue = true;
                }
            });
        }
    }
}

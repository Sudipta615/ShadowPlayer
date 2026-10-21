//! The full-screen Now Playing view.
//!
//! The mini player is a 320x110 strip, which is the right size for a corner
//! of the screen and the wrong size for looking at. This is the other end of
//! the range: the artwork, the visualiser and the lyrics, with the transport
//! underneath, filling the window.

use crate::components::visualizer::VisualizerWidget;
use crate::theme::Theme;
use domain::models::SyncedLyricLine;
use eframe::egui::{self, Color32, CornerRadius, Rect, Sense, Stroke, Vec2};

/// What the full-screen view did.
#[derive(Debug, Clone, Copy, Default)]
pub struct NowPlayingFullEvents {
    pub toggle_play: bool,
    pub previous: bool,
    pub next: bool,
    pub seek_secs: Option<f64>,
    pub close: bool,
}

/// Everything the full-screen view needs to draw itself.
pub struct NowPlayingFull<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub artwork: Option<&'a egui::TextureHandle>,
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub spectrum_db: &'a [f32],
    /// The synced lyric lines, if the track has any.
    pub lyrics_lines: &'a [SyncedLyricLine],
    /// The index of the line that is playing right now.
    pub active_lyric: Option<usize>,
    pub theme: &'a Theme,
}

impl<'a> NowPlayingFull<'a> {
    pub fn show(self, ui: &mut egui::Ui) -> NowPlayingFullEvents {
        let mut events = NowPlayingFullEvents::default();
        let theme = self.theme;

        ui.vertical(|ui| {
            // ── The top bar ────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                ui.label(
                    egui::RichText::new("NOW PLAYING")
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(theme.text_dim),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                    let color = if resp.hovered() {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    };
                    crate::components::icons::draw_window_close(ui, rect, color);
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, "Leave full screen")
                    });
                    let _ = resp.clone().on_hover_text("Leave full screen (F or Esc)");
                    if resp.clicked() {
                        events.close = true;
                    }
                });
            });

            ui.add_space(theme.space_lg);

            // The transport is the last thing on screen, so the body gets
            // everything above it. Laying the body out to an explicit height
            // is what pins the transport to the bottom of the viewport instead
            // of leaving it stranded after the artwork.
            const TRANSPORT_BLOCK_H: f32 = 116.0;
            let avail = ui.available_size();
            let body_h = (avail.y - TRANSPORT_BLOCK_H).max(180.0);
            let body_w = avail.x.max(320.0);

            let has_lyrics = !self.lyrics_lines.is_empty();
            let gap = theme.space_xl;
            let art_size = (body_w * 0.24)
                .clamp(160.0, 340.0)
                .min((body_h - 16.0).max(140.0));
            let lyrics_w = if has_lyrics {
                (body_w * 0.26).clamp(200.0, 360.0)
            } else {
                0.0
            };
            let lyrics_gap = if has_lyrics { gap } else { 0.0 };
            let details_w = (body_w - art_size - lyrics_w - gap - lyrics_gap).max(200.0);

            ui.allocate_ui_with_layout(
                Vec2::new(body_w, body_h),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                    // ── The cover ──────────────────────────────────────────
                    ui.allocate_ui_with_layout(
                        Vec2::new(art_size, body_h),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            let top_pad = ((body_h - art_size) * 0.22).max(0.0);
                            ui.add_space(top_pad);
                            let (art_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(art_size), Sense::hover());
                            let painter = ui.painter_at(art_rect);
                            match self.artwork {
                                Some(tex) => {
                                    egui::Image::new(tex)
                                        .corner_radius(CornerRadius::same(theme.radius_xl))
                                        .fit_to_exact_size(art_rect.size())
                                        .paint_at(ui, art_rect);
                                }
                                None => {
                                    painter.rect_filled(
                                        art_rect,
                                        CornerRadius::same(theme.radius_xl),
                                        theme.artwork_placeholder,
                                    );
                                    crate::components::icons::draw_music_note(
                                        ui,
                                        Rect::from_center_size(
                                            art_rect.center(),
                                            Vec2::splat(art_size * 0.28),
                                        ),
                                        theme.accent_highlight,
                                    );
                                }
                            }
                        },
                    );

                    ui.add_space(gap);

                    // ── Title, artist, album and the visualiser ────────────
                    ui.allocate_ui_with_layout(
                        Vec2::new(details_w, body_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xxs, 0.0);
                            let top_pad = ((body_h - art_size) * 0.22).max(0.0);
                            ui.add_space(top_pad);
                            ui.label(
                                egui::RichText::new(self.title)
                                    .font(crate::theme::font(28.0))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                egui::RichText::new(self.artist)
                                    .font(crate::theme::font(16.0))
                                    .color(theme.text_secondary),
                            );
                            if !self.album.is_empty() {
                                ui.label(
                                    egui::RichText::new(self.album)
                                        .font(crate::theme::font(13.0))
                                        .color(theme.text_dim),
                                );
                            }

                            // The visualiser takes the details column's full
                            // width. It used to be a 240 px strip under a much
                            // wider title block, which is why the right-hand
                            // side of the window looked empty.
                            ui.add_space(theme.space_lg);
                            let used = ui.min_rect().height();
                            let vis_h = (body_h - used - theme.space_lg).clamp(72.0, 260.0);
                            VisualizerWidget::new(self.spectrum_db, self.is_playing, theme)
                                .show(ui, Vec2::new(details_w, vis_h));
                        },
                    );

                    // ── The lyrics, beside the artwork rather than below ──
                    if has_lyrics {
                        ui.add_space(lyrics_gap);
                        ui.allocate_ui_with_layout(
                            Vec2::new(lyrics_w, body_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("LYRICS")
                                        .font(crate::theme::font(11.0))
                                        .strong()
                                        .color(theme.text_dim),
                                );
                                ui.add_space(theme.space_sm);
                                let list_h = (body_h - theme.space_xl).max(80.0);
                                egui::ScrollArea::vertical()
                                    .id_salt("full_lyrics")
                                    .auto_shrink([false, false])
                                    .max_height(list_h)
                                    .show(ui, |ui| {
                                        ui.set_width(lyrics_w);
                                        ui.spacing_mut().item_spacing =
                                            Vec2::new(0.0, theme.space_xs);
                                        for (idx, line) in self.lyrics_lines.iter().enumerate() {
                                            let is_active = self.active_lyric == Some(idx);
                                            let color = if is_active {
                                                theme.accent_highlight
                                            } else {
                                                theme.text_dim
                                            };
                                            let mut text = egui::RichText::new(line.text.clone())
                                                .font(crate::theme::font(if is_active {
                                                    15.0
                                                } else {
                                                    13.0
                                                }))
                                                .color(color);
                                            if is_active {
                                                text = text.strong();
                                            }
                                            ui.add(
                                                egui::Label::new(text).wrap().sense(Sense::hover()),
                                            );
                                            ui.add_space(if is_active { 6.0 } else { 2.0 });
                                        }
                                    });
                            },
                        );
                    }
                },
            );

            // ── The transport, at the bottom ───────────────────────────────
            ui.allocate_ui_with_layout(
                Vec2::new(body_w, TRANSPORT_BLOCK_H),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let slack = (ui.available_height() - 92.0).max(0.0);
                    ui.add_space(slack);
                    self.transport(ui, &mut events);
                    ui.add_space(theme.space_xs);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("F or Esc to leave")
                                .font(crate::theme::font(10.5))
                                .color(theme.text_dim),
                        );
                    });
                },
            );
        });

        events
    }

    /// The transport row: elapsed/remaining time, a seek bar and the controls.
    fn transport(&self, ui: &mut egui::Ui, events: &mut NowPlayingFullEvents) {
        let theme = self.theme;
        let group_w = 620.0_f32.min(ui.available_width() - 32.0).max(260.0);
        let dur = self.duration_secs.max(0.1);
        let pos = self.position_secs.clamp(0.0, dur);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            let free = (ui.available_width() - group_w) * 0.5;
            if free > 0.0 {
                ui.add_space(free);
            }
            ui.allocate_ui_with_layout(
                Vec2::new(group_w, 88.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // Elapsed on the left, remaining on the right, with the
                    // seek bar between them. Both used to be hidden inside
                    // the slider's custom formatter, which only appears while
                    // the user is dragging it.
                    let time_w = 54.0;
                    let slider_w =
                        (ui.available_width() - time_w * 2.0 - theme.space_md * 2.0).max(80.0);
                    let mut new_pos = pos;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(time_w, 20.0),
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format_time(pos))
                                        .font(crate::theme::mono_font(10.5))
                                        .color(theme.text_muted),
                                );
                            },
                        );
                        let resp = ui.add_sized(
                            [slider_w, 20.0],
                            egui::Slider::new(&mut new_pos, 0.0..=dur)
                                .show_value(false)
                                .trailing_fill(true),
                        );
                        resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Slider, true, "Playback position")
                        });
                        if resp.changed() {
                            events.seek_secs = Some(new_pos);
                        }
                        ui.allocate_ui_with_layout(
                            Vec2::new(time_w, 20.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!("-{}", format_time(dur - pos)))
                                        .font(crate::theme::mono_font(10.5))
                                        .color(theme.text_dim),
                                );
                            },
                        );
                    });

                    ui.add_space(theme.space_xs);

                    // The controls are centred under the seek bar. Each is a
                    // real widget with an accessible name; the glyphs come
                    // from the SVG set because Inter has no transport icons.
                    let controls_w = 176.0;
                    ui.horizontal(|ui| {
                        let left = ((ui.available_width() - controls_w) * 0.5).max(0.0);
                        ui.add_space(left);
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

                        let (prev_slot, prev_resp) =
                            ui.allocate_exact_size(Vec2::new(40.0, 40.0), Sense::click());
                        let prev_rect =
                            Rect::from_center_size(prev_slot.center(), Vec2::splat(24.0));
                        let prev_col = if prev_resp.hovered() {
                            theme.accent_highlight
                        } else {
                            theme.text_primary
                        };
                        crate::components::icons::draw_skip_prev(ui, prev_rect, prev_col);
                        let _ = prev_resp.clone().on_hover_text("Previous track");
                        prev_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, "Previous track")
                        });
                        if prev_resp.clicked() {
                            events.previous = true;
                        }

                        let (play_slot, play_resp) =
                            ui.allocate_exact_size(Vec2::splat(48.0), Sense::click());
                        let painter = ui.painter_at(play_slot);
                        let fill = if play_resp.hovered() {
                            theme.accent.gamma_multiply(1.18)
                        } else {
                            theme.accent
                        };
                        painter.circle_filled(play_slot.center(), 24.0, fill);
                        painter.circle_stroke(
                            play_slot.center(),
                            24.0,
                            Stroke::new(1.0_f32, theme.accent_highlight.gamma_multiply(0.55)),
                        );
                        let icon_rect =
                            Rect::from_center_size(play_slot.center(), Vec2::splat(18.0));
                        if self.is_playing {
                            crate::components::icons::draw_pause(ui, icon_rect, Color32::WHITE);
                        } else {
                            crate::components::icons::draw_play(
                                ui,
                                icon_rect,
                                Color32::WHITE,
                                true,
                            );
                        }
                        let _ = play_resp.clone().on_hover_text(if self.is_playing {
                            "Pause"
                        } else {
                            "Play"
                        });
                        play_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                if self.is_playing { "Pause" } else { "Play" },
                            )
                        });
                        if play_resp.clicked() {
                            events.toggle_play = true;
                        }

                        let (next_slot, next_resp) =
                            ui.allocate_exact_size(Vec2::new(40.0, 40.0), Sense::click());
                        let next_rect =
                            Rect::from_center_size(next_slot.center(), Vec2::splat(24.0));
                        let next_col = if next_resp.hovered() {
                            theme.accent_highlight
                        } else {
                            theme.text_primary
                        };
                        crate::components::icons::draw_skip_next(ui, next_rect, next_col);
                        let _ = next_resp.clone().on_hover_text("Next track");
                        next_resp.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, "Next track")
                        });
                        if next_resp.clicked() {
                            events.next = true;
                        }
                    });
                },
            );
        });
    }
}

/// `m:ss`, or `h:mm:ss` for anything over an hour.
fn format_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00".to_string();
    }
    let total = secs as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A hairline for the full-screen view's own separators.
pub fn hairline(theme: &Theme) -> Stroke {
    Stroke::new(1.0, theme.border_subtle)
}

/// The colour of a lyric line that is not playing.
pub fn dim(theme: &Theme) -> Color32 {
    theme.text_dim
}

#[cfg(test)]
mod tests {
    use super::format_time;

    #[test]
    fn times_are_formatted_without_a_leading_zero() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(5.9), "0:05");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3_661.0), "1:01:01");
        assert_eq!(format_time(f64::NAN), "0:00");
        assert_eq!(format_time(-4.0), "0:00");
    }
}

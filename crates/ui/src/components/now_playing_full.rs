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

            ui.add_space(theme.space_xl);

            // ── The artwork and the details ─────────────────────────────────
            let avail = ui.available_size();
            let art_size = (avail.x * 0.34)
                .clamp(160.0, 400.0)
                .min((avail.y * 0.52).clamp(160.0, 400.0));
            let details_w = (art_size * 1.3).max(240.0);
            let group_w = art_size + theme.space_2xl + details_w;

            // A horizontal layout fills the width, so the group is given an
            // explicit width and centred with a spacer on the left.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                let free = (ui.available_width() - group_w) * 0.5;
                if free > 0.0 {
                    ui.add_space(free);
                }
                ui.allocate_ui_with_layout(
                    Vec2::new(group_w, art_size),
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        // The cover.
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

                        // The details, and the visualiser.
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xxs, 0.0);
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

                            // The visualiser, under the details.
                            ui.add_space(theme.space_lg);
                            let vis_h = (art_size * 0.26).clamp(60.0, 130.0);
                            let vis_w = (art_size * 1.3).max(240.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(vis_w, vis_h),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    VisualizerWidget::new(self.spectrum_db, self.is_playing, theme)
                                        .show(ui, Vec2::new(vis_w, vis_h));
                                },
                            );
                        });
                    },
                );
            });

            ui.add_space(theme.space_lg);

            // ── The lyrics ──────────────────────────────────────────────────
            if !self.lyrics_lines.is_empty() {
                let lyrics_h = (ui.available_height() * 0.22).clamp(60.0, 150.0);
                let lyrics_w = (ui.available_width() * 0.6).max(240.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                    let free = (ui.available_width() - lyrics_w) * 0.5;
                    if free > 0.0 {
                        ui.add_space(free);
                    }
                    ui.allocate_ui_with_layout(
                        Vec2::new(lyrics_w, lyrics_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("full_lyrics")
                                .auto_shrink([false, false])
                                .max_height(lyrics_h)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
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
                                        ui.label(text);
                                        ui.add_space(if is_active { 6.0 } else { 2.0 });
                                    }
                                });
                        },
                    );
                });
            }

            ui.add_space(theme.space_lg);

            // ── The transport ───────────────────────────────────────────────
            self.transport(ui, &mut events);

            ui.add_space(theme.space_md);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                let hint_w = 160.0;
                let free = (ui.available_width() - hint_w) * 0.5;
                if free > 0.0 {
                    ui.add_space(free);
                }
                ui.label(
                    egui::RichText::new("F or Esc to leave")
                        .font(crate::theme::font(10.5))
                        .color(theme.text_dim),
                );
            });
        });

        events
    }

    /// The transport row: a seek bar and the play controls.
    fn transport(&self, ui: &mut egui::Ui, events: &mut NowPlayingFullEvents) {
        let theme = self.theme;
        // The same trick as the artwork group: an explicit width, centred
        // with a spacer.
        let group_w = 520.0_f32.min(ui.available_width() - 32.0).max(220.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            let free = (ui.available_width() - group_w) * 0.5;
            if free > 0.0 {
                ui.add_space(free);
            }
            ui.allocate_ui_with_layout(
                Vec2::new(group_w, 92.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // egui's slider has a fixed width of its own; this one
                    // should fill the group.
                    ui.spacing_mut().slider_width = ui.available_width();
                    // The seek bar.
                    let dur = self.duration_secs.max(0.1);
                    let pos = self.position_secs.clamp(0.0, dur);
                    let mut new_pos = pos;
                    // The slider sizes to its content, so it is given the
                    // group's width explicitly.
                    let resp = ui.add_sized(
                        [ui.available_width(), 20.0],
                        egui::Slider::new(&mut new_pos, 0.0..=dur)
                            .show_value(false)
                            .trailing_fill(true)
                            .custom_formatter(|v, _| {
                                format!("{} / {}", format_time(v), format_time(dur))
                            }),
                    );
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Slider, true, "Playback position")
                    });
                    if resp.changed() {
                        events.seek_secs = Some(new_pos);
                    }
                    ui.add_space(theme.space_xs);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                        if ui.button("⏮").on_hover_text("Previous track").clicked() {
                            events.previous = true;
                        }
                        if ui
                            .button(if self.is_playing { "⏸" } else { "▶" })
                            .on_hover_text(if self.is_playing { "Pause" } else { "Play" })
                            .clicked()
                        {
                            events.toggle_play = true;
                        }
                        if ui.button("⏭").on_hover_text("Next track").clicked() {
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

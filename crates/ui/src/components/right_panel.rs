use crate::services::{ArtworkService, LyricsSource};
use crate::theme::Theme;
use domain::models::{SyncedLyricLine, Track};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Queue,
    Lyrics,
}

pub struct RightPanelAction {
    pub tab_changed: Option<RightTab>,
    pub play_queue_index: Option<usize>,
    pub remove_queue_index: Option<usize>,
    /// Move an upcoming queue row up/down by one slot.
    pub move_queue: Option<(usize, isize)>,
    pub clear_queue: bool,
    pub volume_changed: Option<f32>,
    pub toggle_mute: bool,
}

pub struct RightPanel<'a> {
    pub active_tab: RightTab,
    pub queue_tracks: &'a [Track],
    pub current_queue_idx: usize,
    pub lyrics_lines: &'a [SyncedLyricLine],
    pub plain_lyrics: Option<&'a str>,
    /// Where the current track's lyrics came from; drives the caption.
    pub lyrics_source: Option<LyricsSource>,
    pub active_lyric_idx: Option<usize>,
    pub volume: f32,
    pub is_muted: bool,
    pub artwork: &'a mut ArtworkService,
    pub theme: &'a Theme,
}

impl<'a> RightPanel<'a> {
    pub fn show(self, ui: &mut Ui) -> RightPanelAction {
        let mut action = RightPanelAction {
            tab_changed: None,
            play_queue_index: None,
            remove_queue_index: None,
            move_queue: None,
            clear_queue: false,
            volume_changed: None,
            toggle_mute: false,
        };

        ui.vertical(|ui| {
            ui.set_width(ui.available_width().max(220.0));

            // 1. Container Card for Queue/Lyrics (fits 8 items + Clear Queue button)
            let card_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(egui::Margin::symmetric(14, 14));

            card_frame.show(ui, |ui| {
                // Tab Switcher Pill — 2-way: Queue | Lyrics (matching mockup)
                ui.horizontal(|ui| {
                    let switch_w = ui.available_width();
                    let (switch_rect, _) =
                        ui.allocate_exact_size(Vec2::new(switch_w, 34.0), Sense::hover());
                    let painter = ui.painter_at(switch_rect);

                    // Pill container background
                    painter.rect_filled(switch_rect, CornerRadius::same(9), self.theme.bg_pill);

                    let half_w = switch_rect.width() / 2.0;
                    let queue_rect = Rect::from_min_size(
                        switch_rect.min,
                        Vec2::new(half_w, switch_rect.height()),
                    );
                    let lyrics_rect = Rect::from_min_size(
                        Pos2::new(switch_rect.min.x + half_w, switch_rect.min.y),
                        Vec2::new(half_w, switch_rect.height()),
                    );

                    let q_hover = ui.rect_contains_pointer(queue_rect);
                    let l_hover = ui.rect_contains_pointer(lyrics_rect);

                    // Active Tab Indicator
                    if self.active_tab == RightTab::Queue {
                        let active_pill = queue_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(active_pill, CornerRadius::same(8), self.theme.accent);
                    } else if self.active_tab == RightTab::Lyrics {
                        let active_pill = lyrics_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(active_pill, CornerRadius::same(8), self.theme.accent);
                    }

                    let q_color = if self.active_tab == RightTab::Queue {
                        Color32::WHITE
                    } else {
                        self.theme.text_secondary
                    };
                    let l_color = if self.active_tab == RightTab::Lyrics {
                        Color32::WHITE
                    } else {
                        self.theme.text_secondary
                    };

                    painter.text(
                        queue_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Queue",
                        crate::theme::font(12.8),
                        q_color,
                    );
                    painter.text(
                        lyrics_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Lyrics",
                        crate::theme::font(12.8),
                        l_color,
                    );

                    if q_hover && ui.input(|i| i.pointer.primary_clicked()) {
                        action.tab_changed = Some(RightTab::Queue);
                    }
                    if l_hover && ui.input(|i| i.pointer.primary_clicked()) {
                        action.tab_changed = Some(RightTab::Lyrics);
                    }
                });

                ui.add_space(10.0);

                match self.active_tab {
                    RightTab::Queue => {
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
                                egui::RichText::new("Up Next")
                                    .font(crate::theme::font(15.5))
                                    .strong()
                                    .color(self.theme.text_primary),
                            );

                            let total_dur: f32 = self.queue_tracks[start..]
                                .iter()
                                .map(|t| t.duration_secs)
                                .sum();
                            let dur_str = format_duration(total_dur as f64);
                            let summary_str = format!("{} songs • {}", display_len, dur_str);

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(summary_str)
                                            .font(crate::theme::font(11.8))
                                            .color(self.theme.text_muted),
                                    );
                                },
                            );
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
                        egui::ScrollArea::vertical()
                            .id_salt("queue_items_scroll")
                            .max_height(370.0)
                            .auto_shrink([false, false])
                            .show_rows(ui, item_height, display_len, |ui, row_range| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
                                let ctx = ui.ctx().clone();
                                let mut art_budget = 4usize;

                                for disp_idx in row_range {
                                    let orig_idx = start + disp_idx;
                                    let track = &queue_tracks[orig_idx];
                                    let item_width = ui.available_width();

                                    let (item_rect, item_resp) = ui.allocate_exact_size(
                                        Vec2::new(item_width, item_height),
                                        Sense::click(),
                                    );
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
                                            CornerRadius::same(6),
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
                                            .corner_radius(CornerRadius::same(6))
                                            .fit_to_exact_size(Vec2::new(36.0, 36.0))
                                            .paint_at(ui, thumb_rect);
                                    } else {
                                        painter.rect_filled(
                                            thumb_rect,
                                            CornerRadius::same(6),
                                            theme.bg_pill,
                                        );
                                        crate::components::icons::draw_disc(
                                            ui,
                                            thumb_rect.shrink2(Vec2::new(8.0, 8.0)),
                                            theme.accent_warm,
                                        );
                                    }

                                    // Stacked Title + Artist
                                    let text_left = rect.min.x + 64.0;
                                    painter.text(
                                        Pos2::new(text_left, cy - 7.0),
                                        egui::Align2::LEFT_CENTER,
                                        &track.title,
                                        crate::theme::font(12.8),
                                        theme.text_primary,
                                    );
                                    painter.text(
                                        Pos2::new(text_left, cy + 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &track.artist,
                                        crate::theme::font(11.2),
                                        theme.text_muted,
                                    );

                                    // Queue reordering + remove controls appear on hover.
                                    if item_resp.hovered() {
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
                                        let up_hov = can_up && ui.rect_contains_pointer(up_rect);
                                        let down_hov =
                                            can_down && ui.rect_contains_pointer(down_rect);
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
                                        if up_hov && ui.input(|i| i.pointer.primary_clicked()) {
                                            action.move_queue = Some((orig_idx, -1));
                                            action.play_queue_index = None;
                                        }
                                        if down_hov && ui.input(|i| i.pointer.primary_clicked()) {
                                            action.move_queue = Some((orig_idx, 1));
                                            action.play_queue_index = None;
                                        }

                                        let del_rect = Rect::from_center_size(
                                            Pos2::new(rect.max.x - 12.0, cy),
                                            Vec2::new(14.0, 14.0),
                                        );
                                        let del_hov = ui.rect_contains_pointer(del_rect);
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
                                        if del_hov && ui.input(|i| i.pointer.primary_clicked()) {
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
                                let (clear_rect, clear_resp) = ui
                                    .allocate_exact_size(Vec2::new(clear_w, 32.0), Sense::click());
                                let p = ui.painter_at(clear_rect);
                                let bg = if clear_resp.hovered() {
                                    self.theme.bg_card_hover
                                } else {
                                    self.theme.bg_pill
                                };
                                p.rect_filled(clear_rect, CornerRadius::same(8), bg);
                                p.rect_stroke(
                                    clear_rect,
                                    CornerRadius::same(8),
                                    Stroke::new(1.0_f32, self.theme.border_subtle),
                                    egui::StrokeKind::Inside,
                                );
                                let trash_rect = Rect::from_center_size(
                                    Pos2::new(clear_rect.center().x - 42.0, clear_rect.center().y),
                                    Vec2::new(16.0, 16.0),
                                );
                                crate::components::icons::draw_trash(
                                    ui,
                                    trash_rect,
                                    self.theme.text_secondary,
                                );
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

                    RightTab::Lyrics => {
                        // Lyrics Tab Content. A missing source means the lookup
                        // has not finished yet: tag lyrics are read off-thread,
                        // sidecars resolve synchronously.
                        let lyrics_resolved = self.lyrics_source.is_some();
                        let caption = match self.lyrics_source {
                            Some(s) if s.has_lyrics() => s.caption(),
                            _ => None,
                        };
                        if let Some(caption) = caption {
                            ui.label(
                                egui::RichText::new(caption)
                                    .font(crate::theme::font(11.0))
                                    .color(self.theme.text_dim),
                            );
                            ui.add_space(6.0);
                        }

                        if self.lyrics_lines.is_empty() {
                            if let Some(plain) = self.plain_lyrics {
                                egui::ScrollArea::vertical()
                                    .id_salt("plain_lyrics_scroll")
                                    .max_height(360.0)
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
                                        for line in plain.lines() {
                                            ui.label(
                                                egui::RichText::new(line)
                                                    .font(crate::theme::font(13.0))
                                                    .color(self.theme.text_secondary),
                                            );
                                        }
                                    });
                            } else if !lyrics_resolved {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(
                                        egui::RichText::new("Looking for lyrics...")
                                            .font(crate::theme::font(13.5))
                                            .color(self.theme.text_muted),
                                    );
                                });
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(
                                        egui::RichText::new("No lyrics available")
                                            .font(crate::theme::font(13.5))
                                            .color(self.theme.text_muted),
                                    );
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Place a .lrc or .txt file beside the track,",
                                        )
                                        .font(crate::theme::font(11.5))
                                        .color(self.theme.text_dim),
                                    );
                                    ui.label(
                                        egui::RichText::new("or embed lyrics in the file's tags.")
                                            .font(crate::theme::font(11.5))
                                            .color(self.theme.text_dim),
                                    );
                                });
                            }
                        } else {
                            let active_idx = self.active_lyric_idx.unwrap_or(0);
                            let scroll_h = 360.0;

                            egui::ScrollArea::vertical()
                                .id_salt("lyrics_scroll_area")
                                .max_height(scroll_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                                    for (idx, line) in self.lyrics_lines.iter().enumerate() {
                                        let is_current = idx == active_idx;

                                        let (font_size, color) = if is_current {
                                            (16.0, self.theme.accent_highlight)
                                        } else if idx < active_idx {
                                            (12.5, self.theme.text_muted)
                                        } else {
                                            (13.0, self.theme.text_secondary)
                                        };

                                        let rich = if is_current {
                                            egui::RichText::new(&line.text)
                                                .font(crate::theme::font(font_size))
                                                .strong()
                                                .color(color)
                                        } else {
                                            egui::RichText::new(&line.text)
                                                .font(crate::theme::font(font_size))
                                                .color(color)
                                        };

                                        let label_resp = ui.label(rich);
                                        if is_current {
                                            label_resp.scroll_to_me(Some(egui::Align::Center));
                                        }
                                    }
                                });
                        }
                    }
                }
            });

            // 2. Dedicated Bottom Volume Control below Card (speaker + full-width slider)
            let space_to_bottom = (ui.available_height() - 24.0).max(8.0);
            ui.add_space(space_to_bottom);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                // Speaker Icon Button
                let is_muted = self.is_muted || self.volume <= 0.0;
                let (spk_rect, spk_resp) =
                    ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                crate::components::icons::draw_speaker(
                    ui,
                    spk_rect,
                    self.theme.text_secondary,
                    is_muted,
                );

                if spk_resp.clicked() {
                    action.toggle_mute = true;
                }

                // Full-width volume slider with a single jade fill.
                let slider_w = (ui.available_width() - 4.0).max(60.0);
                let (slider_rect, slider_resp) =
                    ui.allocate_exact_size(Vec2::new(slider_w, 20.0), Sense::click_and_drag());

                slider_resp.widget_info(|| {
                    egui::WidgetInfo::slider(
                        true,
                        if self.is_muted {
                            0.0
                        } else {
                            self.volume as f64
                        },
                        "Volume",
                    )
                });
                let painter = ui.painter_at(slider_rect);
                let cy = slider_rect.center().y;

                // Background track (clearly visible in both dark & light modes)
                let track_rect = Rect::from_min_max(
                    Pos2::new(slider_rect.min.x, cy - 2.0),
                    Pos2::new(slider_rect.max.x, cy + 2.0),
                );
                painter.rect_filled(track_rect, CornerRadius::same(2), self.theme.slider_track);

                let effective_vol = if self.is_muted {
                    0.0
                } else {
                    self.volume.clamp(0.0, 1.0)
                };
                let filled_w = slider_w * effective_vol;

                if filled_w > 0.0 {
                    let filled_rect = Rect::from_min_max(
                        Pos2::new(slider_rect.min.x, cy - 2.0),
                        Pos2::new(slider_rect.min.x + filled_w, cy + 2.0),
                    );
                    painter.rect_filled(
                        filled_rect,
                        CornerRadius::same(2),
                        self.theme.accent_highlight,
                    );
                }

                if slider_resp.hovered() || slider_resp.dragged() {
                    let thumb_pos = Pos2::new(slider_rect.min.x + filled_w, cy);
                    painter.circle_filled(thumb_pos, 5.0, self.theme.text_primary);
                    painter.circle_stroke(
                        thumb_pos,
                        5.0,
                        Stroke::new(1.5_f32, self.theme.bg_sidebar),
                    );
                }

                if slider_resp.dragged() || slider_resp.clicked() {
                    if let Some(pos) = slider_resp.interact_pointer_pos() {
                        let norm = ((pos.x - slider_rect.min.x) / slider_w).clamp(0.0, 1.0);
                        action.volume_changed = Some(norm);
                    }
                }
            });
        });

        action
    }
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

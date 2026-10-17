use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};
use domain::ids::TrackId;
use domain::models::{SyncedLyricLine, Track};
use crate::theme::Theme;
use crate::components::settings_tab::{show_settings_tab, SettingsTabActions, SettingsTabState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Queue,
    Lyrics,
    Settings,
}

pub struct RightPanelAction {
    pub tab_changed: Option<RightTab>,
    pub play_queue_index: Option<usize>,
    pub remove_queue_index: Option<usize>,
    pub clear_queue: bool,
    pub volume_changed: Option<f32>,
    pub toggle_mute: bool,
    pub settings: SettingsTabActions,
}

pub struct RightPanel<'a> {
    pub active_tab: RightTab,
    pub queue_tracks: &'a [Track],
    pub current_queue_idx: usize,
    pub lyrics_lines: &'a [SyncedLyricLine],
    pub plain_lyrics: Option<&'a str>,
    pub active_lyric_idx: Option<usize>,
    pub volume: f32,
    pub is_muted: bool,
    pub artwork_map: &'a std::collections::HashMap<TrackId, TextureHandle>,
    pub settings_state: &'a mut SettingsTabState,
    pub theme: &'a Theme,
}

impl<'a> RightPanel<'a> {
    pub fn show(self, ui: &mut Ui) -> RightPanelAction {
        let mut action = RightPanelAction {
            tab_changed: None,
            play_queue_index: None,
            remove_queue_index: None,
            clear_queue: false,
            volume_changed: None,
            toggle_mute: false,
            settings: SettingsTabActions::default(),
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
                    let (switch_rect, _) = ui.allocate_exact_size(Vec2::new(switch_w, 34.0), Sense::hover());
                    let painter = ui.painter_at(switch_rect);

                    // Pill container background
                    painter.rect_filled(switch_rect, CornerRadius::same(9), self.theme.bg_pill);

                    let half_w = switch_rect.width() / 2.0;
                    let queue_rect  = Rect::from_min_size(switch_rect.min, Vec2::new(half_w, switch_rect.height()));
                    let lyrics_rect = Rect::from_min_size(Pos2::new(switch_rect.min.x + half_w, switch_rect.min.y), Vec2::new(half_w, switch_rect.height()));

                    let q_hover = ui.rect_contains_pointer(queue_rect);
                    let l_hover = ui.rect_contains_pointer(lyrics_rect);

                    // Active Tab Indicator
                    if self.active_tab == RightTab::Queue {
                        let active_pill = queue_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(active_pill, CornerRadius::same(8), self.theme.accent_violet);
                    } else if self.active_tab == RightTab::Lyrics {
                        let active_pill = lyrics_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(active_pill, CornerRadius::same(8), self.theme.accent_violet);
                    }

                    let q_color = if self.active_tab == RightTab::Queue  { Color32::WHITE } else { self.theme.text_secondary };
                    let l_color = if self.active_tab == RightTab::Lyrics { Color32::WHITE } else { self.theme.text_secondary };

                    painter.text(queue_rect.center(),  egui::Align2::CENTER_CENTER, "Queue",  FontId::proportional(12.8), q_color);
                    painter.text(lyrics_rect.center(), egui::Align2::CENTER_CENTER, "Lyrics", FontId::proportional(12.8), l_color);

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
                        // Queue Tab Content
                        let (display_items, _total_queue_dur): (Vec<(usize, &Track)>, f32) = if self.queue_tracks.len() > 1 && self.current_queue_idx < self.queue_tracks.len() {
                            let items: Vec<(usize, &Track)> = self.queue_tracks.iter().enumerate().skip(self.current_queue_idx + 1).collect();
                            let dur = items.iter().map(|(_, t)| t.duration_secs).sum();
                            (items, dur)
                        } else {
                            let items: Vec<(usize, &Track)> = self.queue_tracks.iter().enumerate().collect();
                            let dur = items.iter().map(|(_, t)| t.duration_secs).sum();
                            (items, dur)
                        };

                        // Header: "Up Next" and "X songs • MM:SS"
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Up Next")
                                    .font(FontId::proportional(15.5))
                                    .strong()
                                    .color(self.theme.text_primary),
                            );

                            let dur_str = format_duration(_total_queue_dur as f64);
                            let summary_str = format!("{} songs • {}", display_items.len(), dur_str);

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(summary_str)
                                        .font(FontId::proportional(11.8))
                                        .color(self.theme.text_muted),
                                );
                            });
                        });

                        ui.add_space(6.0);

                        // List of Queue items (fits 8 items cleanly)
                        egui::ScrollArea::vertical()
                            .id_salt("queue_items_scroll")
                            .max_height(370.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

                                for (disp_idx, &(orig_idx, track)) in display_items.iter().enumerate() {
                                    let item_height = 44.0;
                                    let item_width = ui.available_width();

                                    let (item_rect, item_resp) = ui.allocate_exact_size(
                                        Vec2::new(item_width, item_height),
                                        Sense::click(),
                                    );

                                    if item_resp.double_clicked() || item_resp.clicked() {
                                        action.play_queue_index = Some(orig_idx);
                                    }

                                    let painter = ui.painter_at(item_rect);
                                    if item_resp.hovered() {
                                        painter.rect_filled(item_rect, CornerRadius::same(6), self.theme.bg_card_hover);
                                    }

                                    let cy = item_rect.center().y;

                                    // Display 1-based index (1 to 8)
                                    painter.text(
                                        Pos2::new(item_rect.min.x + 4.0, cy),
                                        egui::Align2::LEFT_CENTER,
                                        &format!("{}", disp_idx + 1),
                                        FontId::proportional(12.0),
                                        self.theme.text_muted,
                                    );

                                    // Thumbnail
                                    let thumb_rect = Rect::from_min_size(
                                        Pos2::new(item_rect.min.x + 22.0, cy - 18.0),
                                        Vec2::new(36.0, 36.0),
                                    );

                                    if let Some(tex) = self.artwork_map.get(&track.id) {
                                        egui::Image::new(tex)
                                            .corner_radius(CornerRadius::same(6))
                                            .fit_to_exact_size(Vec2::new(36.0, 36.0))
                                            .paint_at(ui, thumb_rect);
                                    } else {
                                        painter.rect_filled(thumb_rect, CornerRadius::same(6), Color32::from_rgb(25, 22, 40));
                                        crate::components::icons::draw_disc(ui, thumb_rect.shrink2(Vec2::new(8.0, 8.0)), self.theme.accent_pink);
                                    }

                                    // Stacked Title + Artist
                                    let text_left = item_rect.min.x + 64.0;
                                    painter.text(
                                        Pos2::new(text_left, cy - 7.0),
                                        egui::Align2::LEFT_CENTER,
                                        &track.title,
                                        FontId::proportional(12.8),
                                        self.theme.text_primary,
                                    );

                                    painter.text(
                                        Pos2::new(text_left, cy + 8.0),
                                        egui::Align2::LEFT_CENTER,
                                        &track.artist,
                                        FontId::proportional(11.2),
                                        self.theme.text_muted,
                                    );

                                    // Duration or delete button on hover
                                    if item_resp.hovered() {
                                        let del_rect = Rect::from_center_size(Pos2::new(item_rect.max.x - 12.0, cy), Vec2::new(14.0, 14.0));
                                        let del_hov = ui.rect_contains_pointer(del_rect);
                                        let del_col = if del_hov { self.theme.accent_magenta } else { self.theme.text_muted };
                                        crate::components::icons::draw_window_close(ui, del_rect.shrink2(Vec2::new(2.0, 2.0)), del_col);
                                        if del_hov && ui.input(|i| i.pointer.primary_clicked()) {
                                            action.remove_queue_index = Some(orig_idx);
                                        }
                                    } else {
                                        painter.text(
                                            Pos2::new(item_rect.max.x - 4.0, cy),
                                            egui::Align2::RIGHT_CENTER,
                                            &format_duration(track.duration_secs as f64),
                                            FontId::proportional(11.8),
                                            self.theme.text_muted,
                                        );
                                    }
                                }
                            });

                        // Clear Queue button INSIDE card at bottom (matching mockup)
                        ui.add_space(8.0);
                        ui.vertical_centered(|ui| {
                            let clear_w = (ui.available_width() - 4.0).max(160.0);
                            let (clear_rect, clear_resp) = ui.allocate_exact_size(Vec2::new(clear_w, 32.0), Sense::click());
                            let p = ui.painter_at(clear_rect);
                            let bg = if clear_resp.hovered() { self.theme.bg_card_hover } else { self.theme.bg_pill };
                            p.rect_filled(clear_rect, CornerRadius::same(8), bg);
                            p.rect_stroke(clear_rect, CornerRadius::same(8), Stroke::new(1.0_f32, self.theme.border_subtle), egui::StrokeKind::Inside);
                            let trash_rect = Rect::from_center_size(Pos2::new(clear_rect.center().x - 42.0, clear_rect.center().y), Vec2::new(16.0, 16.0));
                            crate::components::icons::draw_trash(ui, trash_rect, self.theme.text_secondary);
                            p.text(
                                Pos2::new(clear_rect.center().x - 24.0, clear_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                "Clear Queue",
                                FontId::proportional(12.5),
                                self.theme.text_secondary,
                            );

                            if clear_resp.clicked() {
                                action.clear_queue = true;
                            }
                        });
                    }

                    RightTab::Lyrics => {
                        // Lyrics Tab Content
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
                                                    .font(FontId::proportional(13.0))
                                                    .color(self.theme.text_secondary),
                                            );
                                        }
                                    });
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(
                                        egui::RichText::new("No lyrics available")
                                            .font(FontId::proportional(13.5))
                                            .color(self.theme.text_muted),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new("Place a .lrc or .txt sidecar file beside the audio track")
                                            .font(FontId::proportional(11.5))
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
                                            (16.0, self.theme.accent_magenta)
                                        } else if idx < active_idx {
                                            (12.5, self.theme.text_muted)
                                        } else {
                                            (13.0, self.theme.text_secondary)
                                        };

                                        let rich = if is_current {
                                            egui::RichText::new(&line.text).font(FontId::proportional(font_size)).strong().color(color)
                                        } else {
                                            egui::RichText::new(&line.text).font(FontId::proportional(font_size)).color(color)
                                        };

                                        let label_resp = ui.label(rich);
                                        if is_current {
                                            label_resp.scroll_to_me(Some(egui::Align::Center));
                                        }
                                     }
                                });
                        }
                    }

                    RightTab::Settings => {
                        let settings_actions = show_settings_tab(ui, self.settings_state, self.theme);
                        action.settings = settings_actions;
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
                let (spk_rect, spk_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                crate::components::icons::draw_speaker(ui, spk_rect, self.theme.text_secondary, is_muted);

                if spk_resp.clicked() {
                    action.toggle_mute = true;
                }

                // Gradient Volume Slider filling entire remaining width
                let slider_w = (ui.available_width() - 4.0).max(60.0);
                let (slider_rect, slider_resp) = ui.allocate_exact_size(
                    Vec2::new(slider_w, 20.0),
                    Sense::click_and_drag(),
                );

                let painter = ui.painter_at(slider_rect);
                let cy = slider_rect.center().y;

                // Background track
                let track_rect = Rect::from_min_max(
                    Pos2::new(slider_rect.min.x, cy - 2.0),
                    Pos2::new(slider_rect.max.x, cy + 2.0),
                );
                painter.rect_filled(track_rect, CornerRadius::same(2), self.theme.bg_input);

                // Filled portion with gradient (electric indigo -> luminous lavender)
                let effective_vol = if self.is_muted { 0.0 } else { self.volume.clamp(0.0, 1.0) };
                let filled_w = slider_w * effective_vol;

                if filled_w > 0.0 {
                    let num_slices = (filled_w / 3.0).clamp(1.0, 40.0) as usize;
                    for i in 0..num_slices {
                        let t0 = i as f32 / num_slices as f32;
                        let t1 = (i + 1) as f32 / num_slices as f32;
                        let x0 = slider_rect.min.x + filled_w * t0;
                        let x1 = slider_rect.min.x + filled_w * t1;
                        let r = (236.0 * (1.0 - t0) + 129.0 * t0) as u8;
                        let g = (72.0 * (1.0 - t0) + 140.0 * t0) as u8;
                        let b = (153.0 * (1.0 - t0) + 248.0 * t0) as u8;
                        let col = Color32::from_rgb(r, g, b);
                        let slice_rect = Rect::from_min_max(Pos2::new(x0, cy - 2.0), Pos2::new(x1, cy + 2.0));
                        painter.rect_filled(slice_rect, CornerRadius::same(if i == 0 { 2 } else { 0 }), col);
                    }
                }

                // White circular handle
                let thumb_pos = Pos2::new(slider_rect.min.x + filled_w, cy);
                painter.circle_filled(thumb_pos, 6.0, Color32::WHITE);

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

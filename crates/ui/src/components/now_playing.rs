use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2};
use crate::components::visualizer::VisualizerWidget;
use crate::theme::Theme;

pub struct NowPlayingEvents {
    pub toggle_play: bool,
    pub next_track: bool,
    pub prev_track: bool,
    pub toggle_shuffle: bool,
    pub toggle_repeat: bool,
    pub toggle_favorite: bool,
    pub seek_secs: Option<f64>,
    pub open_eq_panel: bool,
    pub open_properties: bool,
    pub open_bit_perfect: bool,
    /// A sleep-timer choice was made.
    pub sleep_timer_request: Option<SleepTimerRequest>,
}

/// Sleep-timer selection made from the Now Playing transport bar.
#[derive(Debug, Clone, Copy)]
pub enum SleepTimerRequest {
    Off,
    AfterSeconds(f64),
    EndOfTrack,
}

pub struct NowPlayingCard<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub album: &'a str,
    pub is_playing: bool,
    pub is_favorite: bool,
    pub is_shuffle: bool,
    pub is_repeat: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub spectrum_db: &'a [f32],
    pub artwork: Option<&'a TextureHandle>,
    pub format_str: Option<&'a str>,
    pub bit_perfect: bool,
    pub is_eq_open: bool,
    /// Label of the active sleep timer, if any (shown on the timer button).
    pub sleep_timer_label: Option<&'a str>,
    pub theme: &'a Theme,
}

impl<'a> NowPlayingCard<'a> {
    pub fn show(self, ui: &mut Ui) -> NowPlayingEvents {
        let mut events = NowPlayingEvents {
            toggle_play: false,
            next_track: false,
            prev_track: false,
            toggle_shuffle: false,
            toggle_repeat: false,
            toggle_favorite: false,
            seek_secs: None,
            open_eq_panel: false,
            open_properties: false,
            open_bit_perfect: false,
            sleep_timer_request: None,
        };

        let card_frame = egui::Frame::NONE
            .fill(self.theme.bg_card)
            .stroke(Stroke::new(1.0_f32, self.theme.border_card))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(egui::Margin::symmetric(16, 16));

        card_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(18.0, 0.0);

                // 1. Album Artwork (Rounded Square)
                let art_size = Vec2::new(170.0, 170.0);
                let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                let painter = ui.painter_at(art_rect);

                if let Some(texture) = self.artwork {
                    // Render loaded artwork texture with rounding & glowing border
                    painter.rect_filled(art_rect, CornerRadius::same(14), Color32::BLACK);
                    egui::Image::new(texture)
                        .corner_radius(CornerRadius::same(14))
                        .fit_to_exact_size(art_size)
                        .paint_at(ui, art_rect);
                    painter.rect_stroke(art_rect, CornerRadius::same(14), Stroke::new(1.0_f32, self.theme.border_card), egui::StrokeKind::Inside);
                } else {
                    painter.rect_filled(art_rect, CornerRadius::same(14), self.theme.artwork_placeholder);
                    let moon_center = Pos2::new(art_rect.center().x, art_rect.center().y - 12.0);
                    painter.circle_filled(moon_center, 34.0, self.theme.accent_pink);
                    // The bundled fonts have no "♫" glyph, so the note is
                    // drawn from a vector icon instead.
                    crate::components::icons::draw_music_note(
                        ui,
                        Rect::from_center_size(art_rect.center(), Vec2::splat(46.0)),
                        self.theme.accent_purple,
                    );
                }

                // 2. Right info and controls
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);

                    // Row: "NOW PLAYING" tag + Heart & Menu (matching mockup)
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("NOW PLAYING")
                                .font(FontId::proportional(11.0))
                                .strong()
                                .color(self.theme.text_muted),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

                            // Menu button - opens Track Properties
                            let (menu_rect, menu_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            let menu_col = if menu_resp.hovered() { Color32::WHITE } else { self.theme.text_secondary };
                            crate::components::icons::draw_more_menu(ui, menu_rect, menu_col);
                            if menu_resp.on_hover_text("Track Details & Properties").clicked() {
                                events.open_properties = true;
                            }

                            // Heart favorite toggle
                            let (heart_rect, heart_resp) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            let heart_color = if self.is_favorite {
                                self.theme.accent_pink
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_heart(ui, heart_rect, heart_color, self.is_favorite);

                            if heart_resp.clicked() {
                                events.toggle_favorite = true;
                            }
                        });
                    });

                    // Title & Artist & Album
                    ui.label(
                        egui::RichText::new(self.title)
                            .font(FontId::proportional(23.0))
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(self.artist)
                            .font(FontId::proportional(14.5))
                            .color(self.theme.text_secondary),
                    );
                    ui.label(
                        egui::RichText::new(self.album)
                            .font(FontId::proportional(12.5))
                            .color(self.theme.text_muted),
                    );

                    ui.add_space(3.0);

                    // 3. Live Spectrum Visualizer (precisely aligned with the progress bar track below!)
                    let time_label_w = 32.0;
                    let time_gap = 8.0;
                    let bar_width = (ui.available_width() - (time_label_w * 2.0 + time_gap * 2.0)).max(60.0);

                    ui.horizontal(|ui| {
                        ui.add_space(time_label_w + time_gap);
                        VisualizerWidget::new(self.spectrum_db, self.is_playing, self.theme)
                            .show(ui, Vec2::new(bar_width, 42.0));
                    });

                    ui.add_space(3.0);

                    // 4. Progress Scrubber Row (Elapsed - Bar - Total)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(time_gap, 0.0);

                        let elapsed_str = format_time(self.position_secs);
                        let duration_str = format_time(self.duration_secs);

                        let (lbl1, _) = ui.allocate_exact_size(Vec2::new(time_label_w, 16.0), Sense::hover());
                        ui.painter_at(lbl1).text(
                            lbl1.center(),
                            egui::Align2::CENTER_CENTER,
                            &elapsed_str,
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );

                        // Scrubber Slider
                        let (bar_rect, bar_resp) = ui.allocate_exact_size(Vec2::new(bar_width, 16.0), Sense::click_and_drag());
                        let painter = ui.painter_at(bar_rect);

                        let track_y = bar_rect.center().y;
                        let track_rect = Rect::from_min_max(
                            Pos2::new(bar_rect.min.x, track_y - 2.0),
                            Pos2::new(bar_rect.max.x, track_y + 2.0),
                        );
                        // Background track
                        painter.rect_filled(track_rect, CornerRadius::same(2), self.theme.bg_input);

                        // Progress fraction
                        let fraction = if self.duration_secs > 0.0 {
                            (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };

                        let filled_width = bar_width * fraction as f32;
                        if filled_width > 0.0 {
                            let num_slices = (filled_width / 3.0).clamp(1.0, 50.0) as usize;
                            for i in 0..num_slices {
                                let t0 = i as f32 / num_slices as f32;
                                let t1 = (i + 1) as f32 / num_slices as f32;
                                let x0 = bar_rect.min.x + filled_width * t0;
                                let x1 = bar_rect.min.x + filled_width * t1;
                                // Vibrant dual-tone gradient matching mockup and volume slider (Rose Pink -> Luminous Soft Indigo)
                                let r = (236.0 * (1.0 - t0) + 129.0 * t0) as u8;
                                let g = (72.0 * (1.0 - t0) + 140.0 * t0) as u8;
                                let b = (153.0 * (1.0 - t0) + 248.0 * t0) as u8;
                                let col = Color32::from_rgb(r, g, b);
                                let slice_rect = Rect::from_min_max(Pos2::new(x0, track_y - 2.0), Pos2::new(x1, track_y + 2.0));
                                painter.rect_filled(slice_rect, CornerRadius::same(if i == 0 { 2 } else { 0 }), col);
                            }
                        }

                        // Glowing White Thumb Knob
                        let thumb_pos = Pos2::new(bar_rect.min.x + filled_width, track_y);
                        painter.circle_filled(thumb_pos, 6.0, Color32::WHITE);

                        // Dragging or clicking progress bar to seek
                        if bar_resp.dragged() || bar_resp.clicked() {
                            if let Some(mouse_pos) = bar_resp.interact_pointer_pos() {
                                let norm = ((mouse_pos.x - bar_rect.min.x) / bar_width).clamp(0.0, 1.0);
                                events.seek_secs = Some(norm as f64 * self.duration_secs);
                            }
                        }

                        let (lbl2, _) = ui.allocate_exact_size(Vec2::new(time_label_w, 16.0), Sense::hover());
                        ui.painter_at(lbl2).text(
                            lbl2.center(),
                            egui::Align2::CENTER_CENTER,
                            &duration_str,
                            FontId::proportional(12.0),
                            self.theme.text_muted,
                        );
                    });

                    ui.add_space(4.0);

                    // 5. Playback Controls Row: Spacious center group + right-aligned EQ
                    let row_h = 48.0;
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let avail_w = ui.available_width();
                            let center_group_w = 230.0;
                            let left_pad = ((avail_w - center_group_w - 32.0) / 2.0).max(0.0);
                            ui.add_space(left_pad);
                            ui.spacing_mut().item_spacing = Vec2::new(20.0, 0.0);

                            // Shuffle Button
                            let (shuf_slot, shuf_resp) = ui.allocate_exact_size(Vec2::new(22.0, row_h), Sense::click());
                            let shuf_rect = Rect::from_center_size(shuf_slot.center(), Vec2::new(20.0, 20.0));
                            let shuffle_color = if self.is_shuffle {
                                self.theme.accent_magenta
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_shuffle(ui, shuf_rect, shuffle_color);
                            if shuf_resp.clicked() {
                                events.toggle_shuffle = true;
                            }

                            // Previous Track Button
                            let (prev_slot, prev_resp) = ui.allocate_exact_size(Vec2::new(24.0, row_h), Sense::click());
                            let prev_rect = Rect::from_center_size(prev_slot.center(), Vec2::new(22.0, 22.0));
                            crate::components::icons::draw_skip_prev(ui, prev_rect, self.theme.text_primary);
                            if prev_resp.clicked() {
                                events.prev_track = true;
                            }

                            // Play/Pause Big Circular Button (48x48)
                            let play_size = Vec2::new(48.0, 48.0);
                            let (play_rect, play_resp) = ui.allocate_exact_size(play_size, Sense::click());
                            let painter = ui.painter_at(play_rect);
                            let pc = play_rect.center();
                            let pr = 24.0_f32;

                            // Circular gradient by horizontal scanlines clipped precisely to the circle
                            let scan_steps = 48;
                            for s in 0..scan_steps {
                                let dy = (s as f32 - 24.0) + 0.5;
                                let sq = (pr * pr - dy * dy).max(0.0);
                                let dx = sq.sqrt();
                                if dx > 0.0 {
                                    let t = s as f32 / scan_steps as f32;
                                    // Radiant neon pink/magenta (#ec4899) to royal purple (#7c3aed) matching mockup
                                    let r = (236.0 * (1.0 - t) + 124.0 * t) as u8;
                                    let g = (72.0 * (1.0 - t) + 58.0 * t) as u8;
                                    let b = (153.0 * (1.0 - t) + 237.0 * t) as u8;
                                    let col = Color32::from_rgb(r, g, b);
                                    painter.line_segment([Pos2::new(pc.x - dx, pc.y + dy), Pos2::new(pc.x + dx, pc.y + dy)], Stroke::new(1.2_f32, col));
                                }
                            }
                            painter.circle_stroke(pc, pr, Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(236, 72, 153, 90)));

                            let inner_icon_rect = Rect::from_center_size(pc, Vec2::new(18.0, 18.0));
                            if self.is_playing {
                                crate::components::icons::draw_pause(ui, inner_icon_rect, Color32::WHITE);
                            } else {
                                crate::components::icons::draw_play(ui, inner_icon_rect, Color32::WHITE, true);
                            }
                            if play_resp.clicked() {
                                events.toggle_play = true;
                            }

                            // Next Track Button
                            let (next_slot, next_resp) = ui.allocate_exact_size(Vec2::new(24.0, row_h), Sense::click());
                            let next_rect = Rect::from_center_size(next_slot.center(), Vec2::new(22.0, 22.0));
                            crate::components::icons::draw_skip_next(ui, next_rect, self.theme.text_primary);
                            if next_resp.clicked() {
                                events.next_track = true;
                            }

                            // Repeat Button
                            let (rep_slot, rep_resp) = ui.allocate_exact_size(Vec2::new(22.0, row_h), Sense::click());
                            let rep_rect = Rect::from_center_size(rep_slot.center(), Vec2::new(20.0, 20.0));
                            let repeat_color = if self.is_repeat {
                                self.theme.accent_pink
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_repeat(ui, rep_rect, repeat_color);
                            if rep_resp.clicked() {
                                events.toggle_repeat = true;
                            }

                            // EQ Panel Button - RIGHT ALIGNED
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (eq_slot, eq_resp) = ui.allocate_exact_size(Vec2::new(28.0, row_h), Sense::click());
                                let eq_rect = Rect::from_center_size(eq_slot.center(), Vec2::new(28.0, 28.0));
                                let ep = ui.painter_at(eq_rect);
                                let is_active = self.is_eq_open;
                                let bg = if is_active {
                                    self.theme.accent_magenta.linear_multiply(0.25)
                                } else if eq_resp.hovered() {
                                    self.theme.bg_card_hover
                                } else {
                                    self.theme.bg_pill
                                };
                                let border_color = if is_active {
                                    self.theme.accent_magenta
                                } else {
                                    self.theme.border_subtle
                                };
                                let icon_color = if is_active {
                                    self.theme.accent_pink
                                } else {
                                    self.theme.text_secondary
                                };
                                ep.rect_filled(eq_rect, CornerRadius::same(7), bg);
                                ep.rect_stroke(eq_rect, CornerRadius::same(7), Stroke::new(1.0_f32, border_color), egui::StrokeKind::Inside);
                                let icon_rect = eq_rect.shrink2(Vec2::new(4.0, 4.0));
                                crate::components::icons::draw_equalizer_sliders(ui, icon_rect, icon_color);
                                if eq_resp.on_hover_text("Equalizer & Audio DSP Studio (Ctrl+E)").clicked() {
                                    events.open_eq_panel = true;
                                }

                                // Sleep Timer Button (with a small preset popup)
                                let (tm_slot, tm_resp) = ui.allocate_exact_size(Vec2::new(30.0, row_h), Sense::click());
                                let tm_rect = Rect::from_center_size(tm_slot.center(), Vec2::new(22.0, 22.0));
                                let timer_active = self.sleep_timer_label.is_some();
                                let tm_icon_col = if timer_active {
                                    self.theme.accent_pink
                                } else {
                                    self.theme.text_secondary
                                };
                                crate::components::icons::draw_clock(ui, tm_rect, tm_icon_col);

                                let menu_id = ui.make_persistent_id("sleep_timer_menu");
                                let mut menu_open = ui.data(|d| d.get_temp::<bool>(menu_id).unwrap_or(false));
                                if tm_resp.clicked() {
                                    menu_open = !menu_open;
                                }
                                let hover_text = match self.sleep_timer_label {
                                    Some(l) => format!("Sleep timer: {}", l),
                                    None => "Sleep timer".to_string(),
                                };
                                let _ = tm_resp.clone().on_hover_text(hover_text);

                                if menu_open {
                                    let popup_pos = tm_rect.left_bottom() + Vec2::new(0.0, 6.0);
                                    let mut close = false;
                                    let mut popup_rect: Option<Rect> = None;
                                    egui::Area::new(menu_id.with("popup"))
                                        .order(egui::Order::Foreground)
                                        .fixed_pos(popup_pos)
                                        .show(ui.ctx(), |ui| {
                                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                                ui.set_min_width(170.0);
                                                ui.label(
                                                    egui::RichText::new("Sleep Timer")
                                                        .strong()
                                                        .color(self.theme.text_primary),
                                                );
                                                for (label, secs) in [
                                                    ("5 minutes", 300.0),
                                                    ("15 minutes", 900.0),
                                                    ("30 minutes", 1800.0),
                                                    ("1 hour", 3600.0),
                                                ] {
                                                    if ui.button(label).clicked() {
                                                        events.sleep_timer_request =
                                                            Some(SleepTimerRequest::AfterSeconds(secs));
                                                        close = true;
                                                    }
                                                }
                                                if ui.button("End of current track").clicked() {
                                                    events.sleep_timer_request =
                                                        Some(SleepTimerRequest::EndOfTrack);
                                                    close = true;
                                                }
                                                if timer_active && ui.button("Turn off timer").clicked() {
                                                    events.sleep_timer_request =
                                                        Some(SleepTimerRequest::Off);
                                                    close = true;
                                                }
                                            });
                                            popup_rect = Some(ui.min_rect());
                                        });

                                    let clicked_elsewhere = ui.input(|i| i.pointer.any_click())
                                        && !tm_resp.hovered()
                                        && !popup_rect
                                            .map(|r| ui.rect_contains_pointer(r))
                                            .unwrap_or(false);
                                    if close || clicked_elsewhere {
                                        menu_open = false;
                                    }
                                }
                                ui.data_mut(|d| d.insert_temp(menu_id, menu_open));
                            });
                        },
                    );
                });
            });
        });

        events
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

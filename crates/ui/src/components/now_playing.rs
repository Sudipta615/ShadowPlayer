use crate::components::visualizer::VisualizerWidget;
use crate::theme::Theme;
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};

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
    /// The loudness inspector was requested.
    pub open_loudness: bool,
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
            open_loudness: false,
            sleep_timer_request: None,
        };

        let compact = ui.available_width() < 620.0;
        let dense = ui.available_height() < 460.0;
        let compact = compact || dense;
        let card_frame = egui::Frame::NONE
            .fill(self.theme.bg_card)
            .stroke(Stroke::new(1.0_f32, self.theme.border_card))
            .corner_radius(CornerRadius::same(self.theme.radius_xl))
            .inner_margin(egui::Margin::symmetric(
                if dense {
                    10
                } else if compact {
                    14
                } else {
                    16
                },
                if dense {
                    10
                } else if compact {
                    12
                } else {
                    16
                },
            ));

        let card_response = card_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(
                    if dense {
                        12.0
                    } else if compact {
                        14.0
                    } else {
                        18.0
                    },
                    0.0,
                );

                // 1. Album Artwork (Rounded Square)
                //
                // The artwork scales with the width the card has, instead of
                // jumping between three fixed sizes, so the card keeps its
                // proportions at every window size.
                let art_size = Vec2::splat((ui.available_width() * 0.30).clamp(64.0, 170.0));
                let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                let painter = ui.painter_at(art_rect);

                if let Some(texture) = self.artwork {
                    // Render loaded artwork texture with rounding & a hairline border.
                    painter.rect_filled(
                        art_rect,
                        CornerRadius::same(self.theme.radius_lg),
                        Color32::BLACK,
                    );
                    egui::Image::new(texture)
                        .corner_radius(CornerRadius::same(self.theme.radius_lg))
                        .fit_to_exact_size(art_size)
                        .paint_at(ui, art_rect);
                    painter.rect_stroke(
                        art_rect,
                        CornerRadius::same(self.theme.radius_lg),
                        Stroke::new(1.0_f32, self.theme.border_card),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    painter.rect_filled(
                        art_rect,
                        CornerRadius::same(self.theme.radius_lg),
                        self.theme.artwork_placeholder,
                    );
                    let disc_center = Pos2::new(art_rect.center().x, art_rect.center().y - 8.0);
                    painter.circle_filled(
                        disc_center,
                        art_size.x * 0.20,
                        self.theme.accent.gamma_multiply(0.35),
                    );
                    // The bundled fonts have no "♫" glyph, so the note is
                    // drawn from a vector icon instead.
                    crate::components::icons::draw_music_note(
                        ui,
                        Rect::from_center_size(art_rect.center(), Vec2::splat(art_size.x * 0.27)),
                        self.theme.accent_highlight,
                    );
                }

                // 2. Right info and controls
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, if dense { 2.0 } else { 3.0 });
                    // Horizontal rows otherwise inherit the default 36px
                    // interactive minimum, which makes a dense card far taller
                    // than its content. The explicit control sizes below still
                    // set their own hit areas.
                    ui.spacing_mut().interact_size.y = 12.0;

                    // Row: "NOW PLAYING" tag + Heart & Menu (matching mockup)
                    ui.horizontal(|ui| {
                        if !dense {
                            ui.label(
                                egui::RichText::new("NOW PLAYING")
                                    .font(crate::theme::font(11.0))
                                    .strong()
                                    .color(self.theme.text_muted),
                            );
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_xl, 0.0);

                            // Menu button - opens Track Properties
                            let (menu_rect, menu_resp) =
                                ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            let menu_col = if menu_resp.hovered() {
                                self.theme.text_primary
                            } else {
                                self.theme.text_secondary
                            };
                            crate::components::icons::draw_more_menu(ui, menu_rect, menu_col);
                            menu_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    "Track details and properties",
                                )
                            });
                            if menu_resp
                                .on_hover_text("Track Details & Properties")
                                .clicked()
                            {
                                events.open_properties = true;
                            }

                            // Heart favorite toggle
                            let (heart_rect, heart_resp) =
                                ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
                            let heart_color = if self.is_favorite {
                                self.theme.accent_warm
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_heart(
                                ui,
                                heart_rect,
                                heart_color,
                                self.is_favorite,
                            );
                            let _ = heart_resp.clone().on_hover_text(if self.is_favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            });

                            heart_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    if self.is_favorite {
                                        "Remove from favorites"
                                    } else {
                                        "Add to favorites"
                                    },
                                )
                            });
                            if heart_resp.clicked() {
                                events.toggle_favorite = true;
                            }
                        });
                    });

                    // Title & Artist & Album
                    ui.label(
                        egui::RichText::new(self.title)
                            .font(crate::theme::font(if dense {
                                16.0
                            } else if compact {
                                19.0
                            } else {
                                23.0
                            }))
                            .strong()
                            .color(self.theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(self.artist)
                            .font(crate::theme::font(if dense {
                                11.5
                            } else if compact {
                                13.0
                            } else {
                                14.5
                            }))
                            .color(self.theme.text_secondary),
                    );
                    if !compact {
                        ui.label(
                            egui::RichText::new(self.album)
                                .font(crate::theme::font(12.5))
                                .color(self.theme.text_muted),
                        );
                    }

                    ui.add_space(if compact { 2.0 } else { 3.0 });

                    // 3. Live spectrum visualizer. It is decorative and the
                    // compact layout needs the vertical space for the table, so
                    // it is omitted below the responsive breakpoint.
                    let time_label_w = 32.0;
                    let time_gap = 8.0;
                    let bar_width =
                        (ui.available_width() - (time_label_w * 2.0 + time_gap * 2.0)).max(60.0);
                    if !compact {
                        ui.horizontal(|ui| {
                            ui.add_space(time_label_w + time_gap);
                            VisualizerWidget::new(self.spectrum_db, self.is_playing, self.theme)
                                .show(ui, Vec2::new(bar_width, 42.0));
                        });

                        ui.add_space(3.0);
                    }

                    // 4. Progress Scrubber Row (Elapsed - Bar - Total)
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(time_gap, 0.0);

                        let elapsed_str = format_time(self.position_secs);
                        let duration_str = format_time(self.duration_secs);

                        let (lbl1, _) = ui.allocate_exact_size(
                            Vec2::new(time_label_w, if dense { 12.0 } else { 16.0 }),
                            Sense::hover(),
                        );
                        ui.painter_at(lbl1).text(
                            lbl1.center(),
                            egui::Align2::CENTER_CENTER,
                            &elapsed_str,
                            crate::theme::font(12.0),
                            self.theme.text_muted,
                        );

                        // Scrubber Slider
                        let (bar_rect, bar_resp) = ui.allocate_exact_size(
                            Vec2::new(bar_width, if dense { 12.0 } else { 16.0 }),
                            Sense::click_and_drag(),
                        );
                        let painter = ui.painter_at(bar_rect);

                        let track_y = bar_rect.center().y;
                        let track_rect = Rect::from_min_max(
                            Pos2::new(bar_rect.min.x, track_y - 2.0),
                            Pos2::new(bar_rect.max.x, track_y + 2.0),
                        );
                        // Background track (clearly visible in both dark & light modes)
                        painter.rect_filled(
                            track_rect,
                            CornerRadius::same(self.theme.radius_xs),
                            self.theme.slider_track,
                        );

                        // Progress fraction
                        let fraction = if self.duration_secs > 0.0 {
                            (self.position_secs / self.duration_secs).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };

                        let filled_width = bar_width * fraction as f32;
                        if filled_width > 0.0 {
                            let fill_rect = Rect::from_min_max(
                                Pos2::new(bar_rect.min.x, track_y - 2.0),
                                Pos2::new(bar_rect.min.x + filled_width, track_y + 2.0),
                            );
                            painter.rect_filled(
                                fill_rect,
                                CornerRadius::same(self.theme.radius_xs),
                                self.theme.accent_highlight,
                            );
                        }

                        // A quiet thumb, visible on hover/drag instead of a
                        // permanent white dot.
                        if bar_resp.hovered() || bar_resp.dragged() {
                            let thumb_pos = Pos2::new(bar_rect.min.x + filled_width, track_y);
                            painter.circle_filled(thumb_pos, 5.0, self.theme.text_primary);
                            painter.circle_stroke(
                                thumb_pos,
                                5.0,
                                Stroke::new(1.5_f32, self.theme.bg_card),
                            );
                        }

                        bar_resp.widget_info(|| {
                            egui::WidgetInfo::slider(true, fraction, "Playback position")
                        });

                        // Dragging or clicking progress bar to seek
                        if bar_resp.dragged() || bar_resp.clicked() {
                            if let Some(mouse_pos) = bar_resp.interact_pointer_pos() {
                                let norm =
                                    ((mouse_pos.x - bar_rect.min.x) / bar_width).clamp(0.0, 1.0);
                                events.seek_secs = Some(norm as f64 * self.duration_secs);
                            }
                        }

                        let (lbl2, _) = ui.allocate_exact_size(
                            Vec2::new(time_label_w, if dense { 12.0 } else { 16.0 }),
                            Sense::hover(),
                        );
                        ui.painter_at(lbl2).text(
                            lbl2.center(),
                            egui::Align2::CENTER_CENTER,
                            &duration_str,
                            crate::theme::font(12.0),
                            self.theme.text_muted,
                        );
                    });

                    ui.add_space(4.0);

                    // 5. Playback Controls Row: Spacious center group + right-aligned EQ
                    let row_h = if dense {
                        34.0
                    } else if compact {
                        42.0
                    } else {
                        48.0
                    };
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let avail_w = ui.available_width();
                            // The right-hand group (the EQ and sleep-timer
                            // buttons) is a fixed 68 px; the transport group
                            // takes what is left. The padding is what centres
                            // the transport group in the space between the
                            // artwork and the right-hand group, and it
                            // shrinks rather than letting the row overlap.
                            let right_group_w = 68.0;
                            let transport_w = (avail_w - right_group_w).max(120.0);
                            let left_pad = ((transport_w - 210.0) / 2.0).max(0.0);
                            ui.add_space(left_pad);
                            let gap = if avail_w < 380.0 {
                                8.0
                            } else if compact {
                                12.0
                            } else {
                                20.0
                            };
                            ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);

                            // Shuffle Button
                            let (shuf_slot, shuf_resp) =
                                ui.allocate_exact_size(Vec2::new(22.0, row_h), Sense::click());
                            let shuf_rect =
                                Rect::from_center_size(shuf_slot.center(), Vec2::new(20.0, 20.0));
                            let shuffle_color = if self.is_shuffle {
                                self.theme.accent_highlight
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_shuffle(ui, shuf_rect, shuffle_color);
                            let _ = shuf_resp.clone().on_hover_text(if self.is_shuffle {
                                "Shuffle on — click to turn off"
                            } else {
                                "Shuffle — click to turn on"
                            });
                            shuf_resp.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::Button,
                                    true,
                                    self.is_shuffle,
                                    "Shuffle",
                                )
                            });
                            if shuf_resp.clicked() {
                                events.toggle_shuffle = true;
                            }

                            // Previous Track Button
                            let (prev_slot, prev_resp) =
                                ui.allocate_exact_size(Vec2::new(24.0, row_h), Sense::click());
                            let prev_rect =
                                Rect::from_center_size(prev_slot.center(), Vec2::new(22.0, 22.0));
                            crate::components::icons::draw_skip_prev(
                                ui,
                                prev_rect,
                                self.theme.text_primary,
                            );
                            let _ = prev_resp.clone().on_hover_text("Previous track");
                            prev_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    "Previous track",
                                )
                            });
                            if prev_resp.clicked() {
                                events.prev_track = true;
                            }

                            // Play/Pause Big Circular Button, sized from the
                            // artwork so the two stay in proportion.
                            let play_size = Vec2::splat((art_size.x * 0.30).clamp(32.0, 48.0));
                            let (play_rect, play_resp) =
                                ui.allocate_exact_size(play_size, Sense::click());
                            let painter = ui.painter_at(play_rect);
                            let pc = play_rect.center();
                            let pr = play_size.x * 0.5;

                            let play_fill = if play_resp.hovered() {
                                self.theme.accent.gamma_multiply(1.18)
                            } else {
                                self.theme.accent
                            };
                            painter.circle_filled(pc, pr, play_fill);
                            painter.circle_stroke(
                                pc,
                                pr,
                                Stroke::new(
                                    1.0_f32,
                                    self.theme.accent_highlight.gamma_multiply(0.55),
                                ),
                            );

                            let inner_icon_rect = Rect::from_center_size(
                                pc,
                                Vec2::splat(if dense {
                                    13.0
                                } else if compact {
                                    16.0
                                } else {
                                    18.0
                                }),
                            );
                            if self.is_playing {
                                crate::components::icons::draw_pause(
                                    ui,
                                    inner_icon_rect,
                                    Color32::WHITE,
                                );
                            } else {
                                crate::components::icons::draw_play(
                                    ui,
                                    inner_icon_rect,
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

                            // Next Track Button
                            let (next_slot, next_resp) =
                                ui.allocate_exact_size(Vec2::new(24.0, row_h), Sense::click());
                            let next_rect =
                                Rect::from_center_size(next_slot.center(), Vec2::new(22.0, 22.0));
                            crate::components::icons::draw_skip_next(
                                ui,
                                next_rect,
                                self.theme.text_primary,
                            );
                            let _ = next_resp.clone().on_hover_text("Next track");
                            next_resp.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Button, true, "Next track")
                            });
                            if next_resp.clicked() {
                                events.next_track = true;
                            }

                            // Repeat Button
                            let (rep_slot, rep_resp) =
                                ui.allocate_exact_size(Vec2::new(22.0, row_h), Sense::click());
                            let rep_rect =
                                Rect::from_center_size(rep_slot.center(), Vec2::new(20.0, 20.0));
                            let repeat_color = if self.is_repeat {
                                self.theme.accent_highlight
                            } else {
                                self.theme.text_muted
                            };
                            crate::components::icons::draw_repeat(ui, rep_rect, repeat_color);
                            let _ = rep_resp.clone().on_hover_text(if self.is_repeat {
                                "Repeat on — click to change mode"
                            } else {
                                "Repeat — click to repeat all"
                            });
                            rep_resp.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::Button,
                                    true,
                                    self.is_repeat,
                                    "Repeat",
                                )
                            });
                            if rep_resp.clicked() {
                                events.toggle_repeat = true;
                            }

                            // EQ Panel Button - RIGHT ALIGNED
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let (eq_slot, eq_resp) = ui.allocate_exact_size(
                                        Vec2::new(28.0, row_h),
                                        Sense::click(),
                                    );
                                    let eq_rect = Rect::from_center_size(
                                        eq_slot.center(),
                                        Vec2::new(28.0, 28.0),
                                    );
                                    let ep = ui.painter_at(eq_rect);
                                    let is_active = self.is_eq_open;
                                    let bg = if is_active {
                                        self.theme.accent_highlight.linear_multiply(0.25)
                                    } else if eq_resp.hovered() {
                                        self.theme.bg_card_hover
                                    } else {
                                        self.theme.bg_pill
                                    };
                                    let border_color = if is_active {
                                        self.theme.accent_highlight
                                    } else {
                                        self.theme.border_subtle
                                    };
                                    let icon_color = if is_active {
                                        self.theme.accent_highlight
                                    } else {
                                        self.theme.text_secondary
                                    };
                                    ep.rect_filled(
                                        eq_rect,
                                        CornerRadius::same(self.theme.radius_sm),
                                        bg,
                                    );
                                    ep.rect_stroke(
                                        eq_rect,
                                        CornerRadius::same(self.theme.radius_sm),
                                        Stroke::new(1.0_f32, border_color),
                                        egui::StrokeKind::Inside,
                                    );
                                    let icon_rect = eq_rect.shrink2(Vec2::new(4.0, 4.0));
                                    crate::components::icons::draw_equalizer_sliders(
                                        ui, icon_rect, icon_color,
                                    );
                                    eq_resp.widget_info(|| {
                                        egui::WidgetInfo::selected(
                                            egui::Role::Button,
                                            true,
                                            is_active,
                                            "Equalizer and audio DSP studio",
                                        )
                                    });
                                    if eq_resp
                                        .on_hover_text("Equalizer & Audio DSP Studio (Ctrl+E)")
                                        .clicked()
                                    {
                                        events.open_eq_panel = true;
                                    }

                                    // Loudness Inspector Button
                                    let (ld_slot, ld_resp) = ui.allocate_exact_size(
                                        Vec2::new(30.0, row_h),
                                        Sense::click(),
                                    );
                                    let ld_rect = Rect::from_center_size(
                                        ld_slot.center(),
                                        Vec2::new(22.0, 22.0),
                                    );
                                    crate::components::icons::draw_bar_chart(
                                        ui,
                                        ld_rect,
                                        self.theme.text_secondary,
                                    );
                                    ld_resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            "Loudness inspector",
                                        )
                                    });
                                    let _ = ld_resp.clone().on_hover_text(
                                        "Loudness inspector — how loud this track is",
                                    );
                                    if ld_resp.clicked() {
                                        events.open_loudness = true;
                                    }

                                    // Sleep Timer Button (with a small preset popup)
                                    let (tm_slot, tm_resp) = ui.allocate_exact_size(
                                        Vec2::new(30.0, row_h),
                                        Sense::click(),
                                    );
                                    let tm_rect = Rect::from_center_size(
                                        tm_slot.center(),
                                        Vec2::new(22.0, 22.0),
                                    );
                                    let timer_active = self.sleep_timer_label.is_some();
                                    let tm_icon_col = if timer_active {
                                        self.theme.accent_warm
                                    } else {
                                        self.theme.text_secondary
                                    };
                                    crate::components::icons::draw_clock(ui, tm_rect, tm_icon_col);

                                    let menu_id = ui.make_persistent_id("sleep_timer_menu");
                                    let mut menu_open =
                                        ui.data(|d| d.get_temp::<bool>(menu_id).unwrap_or(false));
                                    if tm_resp.clicked() {
                                        menu_open = !menu_open;
                                    }
                                    let hover_text = match self.sleep_timer_label {
                                        Some(l) => format!("Sleep timer: {}", l),
                                        None => "Sleep timer".to_string(),
                                    };
                                    tm_resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            true,
                                            hover_text.clone(),
                                        )
                                    });
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
                                                            events.sleep_timer_request = Some(
                                                                SleepTimerRequest::AfterSeconds(
                                                                    secs,
                                                                ),
                                                            );
                                                            close = true;
                                                        }
                                                    }
                                                    if ui.button("End of current track").clicked() {
                                                        events.sleep_timer_request =
                                                            Some(SleepTimerRequest::EndOfTrack);
                                                        close = true;
                                                    }
                                                    if timer_active
                                                        && ui.button("Turn off timer").clicked()
                                                    {
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
                                },
                            );
                        },
                    );
                });
            });
        });
        let _ = card_response;

        events
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

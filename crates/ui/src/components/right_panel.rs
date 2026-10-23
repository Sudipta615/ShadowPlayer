use crate::components::text::truncated_galley;
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
    /// Write the lyrics editor's draft to the current track's `.lrc` sidecar.
    pub save_lyrics: bool,
}

pub struct RightPanel<'a> {
    pub active_tab: RightTab,
    pub queue_tracks: &'a [Track],
    pub current_queue_idx: usize,
    pub current_track: Option<&'a Track>,
    pub lyrics_lines: &'a [SyncedLyricLine],
    pub plain_lyrics: Option<&'a str>,
    /// Where the current track's lyrics came from; drives the caption.
    pub lyrics_source: Option<LyricsSource>,
    pub active_lyric_idx: Option<usize>,
    pub lyrics_editor: &'a mut crate::state::LyricsEditorState,
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
            save_lyrics: false,
        };

        ui.vertical(|ui| {
            ui.set_width(ui.available_width().max(220.0));

            // 1. Container Card for Queue/Lyrics (fits 8 items + Clear Queue button)
            let card_frame = egui::Frame::NONE
                .fill(self.theme.bg_card)
                .stroke(Stroke::new(1.0_f32, self.theme.border_card))
                .corner_radius(CornerRadius::same(self.theme.radius_xl))
                .inner_margin(egui::Margin::symmetric(
                    self.theme.space_2xl as i8,
                    self.theme.space_2xl as i8,
                ));

            card_frame.show(ui, |ui| {
                // Tab Switcher Pill — 2-way: Queue | Lyrics (matching mockup)
                ui.horizontal(|ui| {
                    let switch_w = ui.available_width();
                    let (switch_rect, _) =
                        ui.allocate_exact_size(Vec2::new(switch_w, 34.0), Sense::hover());
                    let painter = ui.painter_at(switch_rect);

                    // Pill container background
                    painter.rect_filled(
                        switch_rect,
                        CornerRadius::same(self.theme.radius_md),
                        self.theme.bg_pill,
                    );

                    let half_w = switch_rect.width() / 2.0;
                    let queue_rect = Rect::from_min_size(
                        switch_rect.min,
                        Vec2::new(half_w, switch_rect.height()),
                    );
                    let lyrics_rect = Rect::from_min_size(
                        Pos2::new(switch_rect.min.x + half_w, switch_rect.min.y),
                        Vec2::new(half_w, switch_rect.height()),
                    );

                    // The two halves are real widgets, not raw pointer
                    // hit-tests: a tab switcher that cannot be reached by
                    // keyboard or by a screen reader is not a tab switcher.
                    let q_resp = ui.interact(
                        queue_rect,
                        ui.make_persistent_id("right_tab_queue"),
                        Sense::click(),
                    );
                    let l_resp = ui.interact(
                        lyrics_rect,
                        ui.make_persistent_id("right_tab_lyrics"),
                        Sense::click(),
                    );

                    let q_hover = q_resp.hovered();
                    let l_hover = l_resp.hovered();

                    // The active pill slides between the two halves rather
                    // than jumping. The animation id is fixed, so the state
                    // stays bounded no matter how long the app runs.
                    let pill_t = ui.ctx().animate_bool_with_time(
                        ui.make_persistent_id("right_tab_pill"),
                        self.active_tab == RightTab::Lyrics,
                        0.16,
                    );
                    let active_pill = Rect::from_min_size(
                        Pos2::new(
                            queue_rect.min.x
                                + (lyrics_rect.min.x - queue_rect.min.x) * pill_t
                                + 2.0,
                            switch_rect.min.y + 2.0,
                        ),
                        Vec2::new(half_w - 4.0, switch_rect.height() - 4.0),
                    );
                    painter.rect_filled(
                        active_pill,
                        CornerRadius::same(self.theme.radius_md),
                        self.theme.accent,
                    );

                    // Hover feedback for the half that is *not* the active one.
                    if self.active_tab != RightTab::Queue && q_hover {
                        let hovered_pill = queue_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(
                            hovered_pill,
                            CornerRadius::same(self.theme.radius_md),
                            self.theme.bg_card_hover,
                        );
                    }
                    if self.active_tab != RightTab::Lyrics && l_hover {
                        let hovered_pill = lyrics_rect.shrink2(Vec2::new(2.0, 2.0));
                        painter.rect_filled(
                            hovered_pill,
                            CornerRadius::same(self.theme.radius_md),
                            self.theme.bg_card_hover,
                        );
                    }

                    let q_color =
                        crate::theme::lerp_color(Color32::WHITE, self.theme.text_secondary, pill_t);
                    let l_color =
                        crate::theme::lerp_color(self.theme.text_secondary, Color32::WHITE, pill_t);

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

                    if q_resp.clicked() {
                        action.tab_changed = Some(RightTab::Queue);
                    }
                    if l_resp.clicked() {
                        action.tab_changed = Some(RightTab::Lyrics);
                    }
                    q_resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            self.active_tab == RightTab::Queue,
                            "Queue",
                        )
                    });
                    l_resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            self.active_tab == RightTab::Lyrics,
                            "Lyrics",
                        )
                    });
                    let _ = q_resp.on_hover_text(tr!(Msg::S2F46849AD1D28614));
                    let _ = l_resp.on_hover_text(tr!(Msg::S2321681328DA5F17));
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
                            .max_height((ui.available_height() - 40.0).clamp(120.0, 520.0))
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
                                            format!(
                                                "Move {} up in the queue",
                                                track.title
                                            ),
                                        )
                                    });
                                    down_resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::Role::Button,
                                            can_down,
                                            format!(
                                                "Move {} down in the queue",
                                                track.title
                                            ),
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
                                let (clear_rect, clear_resp) = ui
                                    .allocate_exact_size(Vec2::new(clear_w, 32.0), Sense::click());
                                clear_resp.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        "Clear queue",
                                    )
                                });
                                let p = ui.painter_at(clear_rect);
                                let bg = if clear_resp.hovered() {
                                    self.theme.bg_card_hover
                                } else {
                                    self.theme.bg_pill
                                };
                                p.rect_filled(
                                    clear_rect,
                                    CornerRadius::same(self.theme.radius_md),
                                    bg,
                                );
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
                        let track_id = self.current_track.map(|t| t.id);

                        // A draft belongs to exactly one track: never let it
                        // be saved onto the next song.
                        if self.lyrics_editor.track_id.is_some()
                            && self.lyrics_editor.track_id != track_id
                        {
                            self.lyrics_editor.close();
                        }
                        let editing_here =
                            self.lyrics_editor.editing && self.lyrics_editor.track_id == track_id;

                        let caption = match self.lyrics_source {
                            Some(s) if s.has_lyrics() => s.caption(),
                            _ => None,
                        };
                        ui.horizontal(|ui| {
                            if let Some(caption) = caption {
                                ui.label(
                                    egui::RichText::new(caption)
                                        .font(crate::theme::font(11.0))
                                        .color(self.theme.text_dim),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if editing_here {
                                        if ui.small_button(tr!("Save")).clicked() {
                                            action.save_lyrics = true;
                                        }
                                        if ui.small_button(tr!("Cancel")).clicked() {
                                            self.lyrics_editor.close();
                                        }
                                    } else if let Some(id) = track_id {
                                        if ui.small_button(tr!("Edit")).clicked() {
                                            self.lyrics_editor.begin(
                                                id,
                                                self.plain_lyrics.unwrap_or("").to_string(),
                                            );
                                        }
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);

                        if editing_here {
                            ui.label(
                                egui::RichText::new(tr!(
                                    "Edit the raw lyrics. Timestamps such as [00:12.50] stay synced; save writes a .lrc file beside the track."
                                ))
                                .font(crate::theme::font(11.0))
                                .color(self.theme.text_dim),
                            );
                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .id_salt("lyrics_editor_scroll")
                                .max_height(420.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(
                                            &mut self.lyrics_editor.draft,
                                        )
                                        .desired_rows(16)
                                        .desired_width(f32::INFINITY)
                                        .hint_text(tr!(
                                            "[00:00.00] First line\n[00:04.20] Second line"
                                        ))
                                        .font(egui::TextStyle::Monospace),
                                    );
                                });
                            if let Some(error) = &self.lyrics_editor.error {
                                ui.label(
                                    egui::RichText::new(error)
                                        .font(crate::theme::font(11.0))
                                        .color(self.theme.destructive),
                                );
                            }
                        } else if self.lyrics_lines.is_empty() {
                            if let Some(plain) = self.plain_lyrics {
                                egui::ScrollArea::vertical()
                                    .id_salt("plain_lyrics_scroll")
                                    .max_height((ui.available_height() - 40.0).clamp(120.0, 520.0))
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing =
                                            Vec2::new(0.0, self.theme.space_sm);
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
                                        egui::RichText::new(tr!("Looking for lyrics..."))
                                            .font(crate::theme::font(13.5))
                                            .color(self.theme.text_muted),
                                    );
                                });
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(
                                        egui::RichText::new(tr!("No lyrics available"))
                                            .font(crate::theme::font(13.5))
                                            .color(self.theme.text_muted),
                                    );
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(tr!(
                                            "Place a .lrc or .txt file beside the track,"
                                        ))
                                        .font(crate::theme::font(11.5))
                                        .color(self.theme.text_dim),
                                    );
                                    ui.label(
                                        egui::RichText::new(tr!(
                                            "or embed lyrics in the file's tags."
                                        ))
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
                                    ui.spacing_mut().item_spacing =
                                        Vec2::new(0.0, self.theme.space_lg);

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
            //
            // Pinned to the bottom of the drawer. It used to be pushed down
            // by whatever height the queue happened to have, so it drifted
            // around as the queue grew and shrank.
            let space_to_bottom = (ui.available_height() - 24.0).max(8.0);
            ui.add_space(space_to_bottom);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_lg, 0.0);

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

                spk_resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        if is_muted { "Unmute" } else { "Mute" },
                    )
                });
                let _ = spk_resp
                    .clone()
                    .on_hover_text(if is_muted { "Unmute" } else { "Mute" });
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
                painter.rect_filled(
                    track_rect,
                    CornerRadius::same(self.theme.radius_xs),
                    self.theme.slider_track,
                );

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
                        CornerRadius::same(self.theme.radius_xs),
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

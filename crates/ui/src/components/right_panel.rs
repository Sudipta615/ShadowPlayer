use crate::services::{ArtworkService, LyricsSource};
use crate::theme::Theme;
use domain::models::{SyncedLyricLine, Track};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

mod lyrics;
mod queue;

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
    pub fn show(mut self, ui: &mut Ui) -> RightPanelAction {
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
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: self.theme.shadow,
                })
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
                        self.theme.motion_time(0.16),
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
                    RightTab::Queue => self.render_queue_tab(ui, &mut action),
                    RightTab::Lyrics => self.render_lyrics_tab(ui, &mut action),
                }
            });

            // 2. Dedicated Bottom Volume Control below Card (speaker + full-width slider)
            //
            // Pinned to the bottom of the drawer. It used to be pushed down
            // by whatever height the queue happened to have, so it drifted
            // around as the queue grew and shrank.
            let space_to_bottom = (ui.available_height() - 28.0).clamp(0.0, 16.0);
            if space_to_bottom > 0.0 {
                ui.add_space(space_to_bottom);
            }
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

pub(super) fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

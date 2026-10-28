use crate::theme::Theme;
use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, TextureHandle, Ui, Vec2,
};

pub struct MiniPlayerEvents {
    pub toggle_play: bool,
    pub previous: bool,
    pub next: bool,
    pub restore: bool,
    pub seek_secs: Option<f64>,
}

pub struct MiniPlayer<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub artwork: Option<&'a TextureHandle>,
    pub is_playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub theme: &'a Theme,
}

impl MiniPlayer<'_> {
    pub fn show(self, ui: &mut Ui) -> MiniPlayerEvents {
        let mut events = MiniPlayerEvents {
            toggle_play: false,
            previous: false,
            next: false,
            restore: false,
            seek_secs: None,
        };

        let rect = ui.max_rect();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, self.theme.bg_app);

        // Dragging the empty space moves the frameless-ish window, matching
        // the way lightweight mini players behave on every platform.
        let drag = ui.interact(
            rect,
            ui.scope_id().with("mini_drag"),
            Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let margin = 8.0;
        let top_h = rect.height() - margin * 2.0 - 18.0;
        let art_size = top_h.min(64.0);
        let content = Rect::from_min_max(
            Pos2::new(rect.min.x + margin, rect.min.y + margin),
            Pos2::new(rect.max.x - margin, rect.min.y + margin + top_h),
        );

        let art_rect = Rect::from_min_size(content.min, Vec2::splat(art_size));
        if let Some(texture) = self.artwork {
            painter.rect_filled(
                art_rect,
                CornerRadius::same(self.theme.radius_md),
                self.theme.bg_pill,
            );
            egui::Image::new(texture)
                .corner_radius(CornerRadius::same(self.theme.radius_md))
                .fit_to_exact_size(Vec2::splat(art_size))
                .paint_at(ui, art_rect);
        } else {
            painter.rect_filled(
                art_rect,
                CornerRadius::same(self.theme.radius_md),
                self.theme.artwork_placeholder,
            );
            crate::components::icons::draw_music_note(
                ui,
                Rect::from_center_size(art_rect.center(), Vec2::splat(art_size * 0.36)),
                self.theme.accent_highlight,
            );
        }

        let actions_w = 96.0;
        let info_left = art_rect.max.x + 9.0;
        let info_rect = Rect::from_min_max(
            Pos2::new(info_left, content.min.y),
            Pos2::new(content.max.x - actions_w - 6.0, content.max.y),
        );
        let info = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(info_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let mut info = info;
        info.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        info.add(
            egui::Label::new(
                egui::RichText::new(self.title)
                    .font(crate::theme::font(13.5))
                    .strong()
                    .color(self.theme.text_primary),
            )
            .truncate(),
        );
        info.add(
            egui::Label::new(
                egui::RichText::new(self.artist)
                    .font(crate::theme::font(10.5))
                    .color(self.theme.text_muted),
            )
            .truncate(),
        );

        let actions = Rect::from_min_max(
            Pos2::new(content.max.x - actions_w, content.min.y),
            content.max,
        );
        let mut actions = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(actions)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        actions.spacing_mut().item_spacing = Vec2::new(self.theme.space_xxs, 0.0);
        let cy = actions.max_rect().center().y;
        let button = |ui: &mut Ui, x: f32, size: f32, icon: fn(&Ui, Rect, Color32)| {
            let r = Rect::from_center_size(Pos2::new(x, cy), Vec2::splat(size));
            let resp = ui.interact(r, ui.scope_id().with(x as i32), Sense::click());
            icon(
                ui,
                r.shrink2(Vec2::splat(size * 0.22)),
                if resp.hovered() {
                    self.theme.text_primary
                } else {
                    self.theme.text_secondary
                },
            );
            resp
        };

        let left = actions.max_rect().min.x;
        let previous = button(
            &mut actions,
            left + 10.0,
            24.0,
            crate::components::icons::draw_skip_prev,
        );
        let _ = previous.clone().on_hover_text(tr!(Msg::S9C2CFDED8ECFF47F));
        previous
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Previous track"));
        if previous.clicked() {
            events.previous = true;
        }
        // Play/pause
        let play_rect = Rect::from_center_size(Pos2::new(left + 38.0, cy), Vec2::splat(30.0));
        let play_id = actions.scope_id().with("mini_play");
        let play_resp = crate::components::play_button::play_button_at(
            &mut actions,
            play_rect,
            play_id,
            self.is_playing,
            self.theme,
        );
        let _ = play_resp
            .clone()
            .on_hover_text(if self.is_playing { "Pause" } else { "Play" });
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
        let next = button(
            &mut actions,
            left + 64.0,
            24.0,
            crate::components::icons::draw_skip_next,
        );
        let _ = next.clone().on_hover_text(tr!(Msg::SDC4932D8AABB325F));
        next.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Next track"));
        if next.clicked() {
            events.next = true;
        }
        let restore_rect = Rect::from_center_size(Pos2::new(left + 88.0, cy), Vec2::splat(22.0));
        let restore = actions.interact(
            restore_rect,
            actions.scope_id().with("mini_restore"),
            Sense::click(),
        );
        let rc = if restore.hovered() {
            self.theme.text_primary
        } else {
            self.theme.text_muted
        };
        let p = actions.painter();
        let outer = Rect::from_center_size(restore_rect.center(), Vec2::new(15.0, 11.0));
        p.rect_stroke(
            outer,
            CornerRadius::same(self.theme.radius_xs),
            Stroke::new(1.2_f32, rc),
            egui::StrokeKind::Inside,
        );
        p.rect_filled(
            Rect::from_center_size(outer.center() + Vec2::new(1.0, 1.0), Vec2::new(7.0, 5.0)),
            CornerRadius::same(self.theme.radius_xs),
            rc,
        );
        let _ = restore.clone().on_hover_text(tr!(Msg::S22296B89B750827C));
        restore.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, "Restore full player")
        });
        if restore.clicked() {
            events.restore = true;
        }

        // Progress line spans the window width, keeping the tiny layout calm.
        let bar = Rect::from_min_max(
            Pos2::new(rect.min.x + margin, rect.max.y - 12.0),
            Pos2::new(rect.max.x - margin, rect.max.y - 10.0),
        );
        let bar_resp = ui.interact(
            bar.expand2(Vec2::new(0.0, 6.0)),
            ui.scope_id().with("mini_seek"),
            Sense::click_and_drag(),
        );
        painter.rect_filled(
            bar,
            CornerRadius::same(self.theme.radius_xs),
            self.theme.slider_track,
        );
        let frac = if self.duration_secs > 0.0 {
            (self.position_secs / self.duration_secs).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        if frac > 0.0 {
            let fill = Rect::from_min_max(
                bar.min,
                Pos2::new(bar.min.x + bar.width() * frac, bar.max.y),
            );
            painter.rect_filled(
                fill,
                CornerRadius::same(self.theme.radius_xs),
                self.theme.accent_highlight,
            );
        }
        bar_resp.widget_info(|| {
            egui::WidgetInfo::slider(
                true,
                if self.duration_secs > 0.0 {
                    self.position_secs / self.duration_secs
                } else {
                    0.0
                },
                "Playback position",
            )
        });
        if bar_resp.dragged() || bar_resp.clicked() {
            if let Some(pos) = bar_resp.interact_pointer_pos() {
                let norm = ((pos.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
                events.seek_secs = Some(norm as f64 * self.duration_secs);
            }
        }

        painter.text(
            Pos2::new(rect.min.x + margin, rect.max.y - 4.0),
            egui::Align2::LEFT_BOTTOM,
            format_time(self.position_secs),
            crate::theme::font(9.0),
            self.theme.text_muted,
        );
        painter.text(
            Pos2::new(rect.max.x - margin, rect.max.y - 4.0),
            egui::Align2::RIGHT_BOTTOM,
            format_time(self.duration_secs),
            crate::theme::font(9.0),
            self.theme.text_muted,
        );

        if self.is_playing {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }

        events
    }
}

fn format_time(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

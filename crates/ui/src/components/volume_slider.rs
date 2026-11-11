use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Dedicated, independent volume slider widget.
/// Sits below the right sidebar and remains visible whether the sidebar is toggled on or off.
pub struct VolumeSlider<'a> {
    pub volume: f32,
    pub is_muted: bool,
    pub theme: &'a Theme,
    pub standalone: bool,
}

pub struct VolumeSliderAction {
    pub volume_changed: Option<f32>,
    pub toggle_mute: bool,
}

impl<'a> VolumeSlider<'a> {
    pub fn new(volume: f32, is_muted: bool, theme: &'a Theme) -> Self {
        Self {
            volume,
            is_muted,
            theme,
            standalone: false,
        }
    }

    pub fn standalone(mut self, standalone: bool) -> Self {
        self.standalone = standalone;
        self
    }

    pub fn show(self, ui: &mut Ui) -> VolumeSliderAction {
        let mut action = VolumeSliderAction {
            volume_changed: None,
            toggle_mute: false,
        };

        let is_muted = self.is_muted || self.volume <= 0.0;
        let effective_vol = if is_muted {
            0.0
        } else {
            self.volume.clamp(0.0, 1.0)
        };

        let rect = ui.available_rect_before_wrap();
        let painter = ui.painter();

        if self.standalone {
            // Sleek card container when rendered independently (e.g. when right sidebar is toggled off)
            painter.rect_filled(
                rect,
                CornerRadius::same(self.theme.radius_md),
                self.theme.bg_card,
            );
            painter.rect_stroke(
                rect,
                CornerRadius::same(self.theme.radius_md),
                Stroke::new(1.0, self.theme.border_subtle),
                egui::StrokeKind::Inside,
            );
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(self.theme.space_md, 0.0);

            if self.standalone {
                ui.add_space(8.0);
            }

            // Speaker Icon Button
            let (spk_rect, spk_resp) =
                ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::click());
            let spk_color = if spk_resp.hovered() {
                self.theme.text_primary
            } else {
                self.theme.text_secondary
            };
            crate::components::icons::draw_speaker(ui, spk_rect, spk_color, is_muted);

            let mute_label = if is_muted { "Unmute" } else { "Mute" };
            spk_resp
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, mute_label));
            let _ = spk_resp.clone().on_hover_text(mute_label);
            if spk_resp.clicked() {
                action.toggle_mute = true;
            }

            // Volume Slider track
            let right_pad = if self.standalone { 44.0 } else { 38.0 };
            let slider_w = (ui.available_width() - right_pad).max(60.0);
            let (slider_rect, slider_resp) =
                ui.allocate_exact_size(Vec2::new(slider_w, 20.0), Sense::click_and_drag());

            slider_resp
                .widget_info(|| egui::WidgetInfo::slider(true, effective_vol as f64, "Volume"));

            // Scroll wheel on slider adjusts volume
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if slider_resp.hovered() && scroll_delta.abs() > 0.0 {
                let step = if scroll_delta > 0.0 { 0.02 } else { -0.02 };
                let new_vol = (effective_vol + step).clamp(0.0, 1.0);
                action.volume_changed = Some(new_vol);
            }

            let painter = ui.painter_at(slider_rect);
            let cy = slider_rect.center().y;

            // Background track
            let track_rect = Rect::from_min_max(
                Pos2::new(slider_rect.min.x, cy - 2.0),
                Pos2::new(slider_rect.max.x, cy + 2.0),
            );
            painter.rect_filled(
                track_rect,
                CornerRadius::same(self.theme.radius_xs),
                self.theme.slider_track,
            );

            // Filled track
            let filled_w = slider_w * effective_vol;
            if filled_w > 0.0 {
                let filled_rect = Rect::from_min_max(
                    Pos2::new(slider_rect.min.x, cy - 2.0),
                    Pos2::new(slider_rect.min.x + filled_w, cy + 2.0),
                );
                painter.rect_filled(
                    filled_rect,
                    CornerRadius::same(self.theme.radius_xs),
                    self.theme.accent,
                );
            }

            // Knob / Thumb
            let thumb_pos = Pos2::new(slider_rect.min.x + filled_w, cy);
            if slider_resp.hovered() || slider_resp.dragged() {
                painter.circle_filled(thumb_pos, 5.5, self.theme.text_primary);
                painter.circle_stroke(thumb_pos, 5.5, Stroke::new(1.5, self.theme.bg_card));
            } else {
                painter.circle_filled(thumb_pos, 3.5, self.theme.accent);
            }

            if slider_resp.dragged() || slider_resp.clicked() {
                if let Some(pos) = slider_resp.interact_pointer_pos() {
                    let norm = ((pos.x - slider_rect.min.x) / slider_w).clamp(0.0, 1.0);
                    action.volume_changed = Some(norm);
                }
            }

            // Percentage label
            let pct_text = format!("{:.0}%", effective_vol * 100.0);
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(pct_text)
                    .font(crate::theme::font(11.0))
                    .color(self.theme.text_muted),
            );
        });

        action
    }
}

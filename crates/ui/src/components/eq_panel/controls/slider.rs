//! The gradient slider shared by every panel tab.
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

pub(in crate::components::eq_panel) fn gradient_horizontal_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_val: f32,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(ui.available_width(), 18.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    let min = *range.start();
    let max = *range.end();
    resp.widget_info(|| {
        egui::WidgetInfo::slider(
            true,
            f64::from(((*value - min) / (max - min)).clamp(0.0, 1.0)),
            format!("Setting slider, {value:.2}"),
        )
    });

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let new_val = min + t * (max - min);
            if (new_val - *value).abs() > 0.001 {
                *value = new_val;
                changed = true;
            }
        }
    }

    if resp.double_clicked() {
        *value = default_val;
        changed = true;
    }

    let is_active = resp.hovered() || resp.dragged();
    let anim_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_slider_active"),
        is_active,
        theme.motion_time(0.12),
    );

    // Paint background track using theme token
    let track_h = egui::lerp(4.0..=5.0, anim_t);
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), track_h));
    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(theme.radius_xs),
        theme.slider_track,
    );

    // Center 0.0 dB tick mark if 0.0 dB lies within range
    if min < 0.0 && max > 0.0 {
        let zero_t = ((0.0 - min) / (max - min)).clamp(0.0, 1.0);
        let zero_x = rect.left() + zero_t * rect.width();
        ui.painter().line_segment(
            [
                Pos2::new(zero_x, rect.center().y - 4.0),
                Pos2::new(zero_x, rect.center().y + 4.0),
            ],
            Stroke::new(1.0, theme.text_muted.gamma_multiply(0.4)),
        );
    }

    // Paint active track up to thumb
    let thumb_t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let thumb_x = rect.left() + thumb_t * rect.width();
    let filled_w = (thumb_x - track_rect.min.x).max(0.0);
    if filled_w > 0.0 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(track_rect.min.x, track_rect.min.y),
            Pos2::new(track_rect.min.x + filled_w, track_rect.max.y),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(theme.radius_xs),
            theme.accent_highlight,
        );
    }

    // Paint circular thumb with animated halo
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let halo_radius = egui::lerp(7.0..=9.5, anim_t);
    let halo_alpha = egui::lerp(0.18..=0.35, anim_t);
    let inner_radius = egui::lerp(5.0..=6.0, anim_t);

    ui.painter().circle_filled(
        thumb_pos,
        halo_radius,
        theme.accent_highlight.gamma_multiply(halo_alpha),
    );
    ui.painter()
        .circle_filled(thumb_pos, inner_radius, Color32::WHITE);
    ui.painter().circle_stroke(
        thumb_pos,
        inner_radius,
        Stroke::new(1.6, theme.accent_highlight),
    );

    changed
}

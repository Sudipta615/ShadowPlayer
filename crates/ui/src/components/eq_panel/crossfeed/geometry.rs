//! 2D interactive soundstage geometry canvas simulating listener and virtual speakers at 60°.

use super::types::{calculate_acoustic_path_cm, calculate_itd_delay_us};
use crate::components::eq_panel::EqPanelState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// Interactive 2D Soundstage Geometry Canvas simulating listener and virtual speakers at 60°.
pub fn render_soundstage_geometry_canvas(ui: &mut egui::Ui, state: &EqPanelState, theme: &Theme) {
    let avail_w = ui.available_width();
    let canvas_h = 160.0;
    let (canvas_rect, resp) = ui.allocate_exact_size(Vec2::new(avail_w, canvas_h), Sense::hover());
    let painter = ui.painter_at(canvas_rect);

    // Canvas background
    painter.rect_filled(
        canvas_rect,
        CornerRadius::same(theme.radius_md),
        theme.bg_pill,
    );
    painter.rect_stroke(
        canvas_rect,
        CornerRadius::same(theme.radius_md),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let center_x = canvas_rect.center().x;
    let head_y = canvas_rect.bottom() - 38.0;
    let head_center = Pos2::new(center_x, head_y);
    let head_radius = 20.0;

    // Head Shadow Zone (behind listener head)
    let shadow_rect = Rect::from_min_max(
        Pos2::new(center_x - 48.0, head_y),
        Pos2::new(center_x + 48.0, canvas_rect.bottom() - 6.0),
    );
    painter.rect_filled(
        shadow_rect,
        CornerRadius::same(theme.radius_sm),
        theme.shadow,
    );

    // Virtual Speakers at ±30°
    let spk_y = canvas_rect.top() + 32.0;
    let spk_offset_x = (avail_w * 0.32).clamp(70.0, 140.0);
    let left_spk = Pos2::new(center_x - spk_offset_x, spk_y);
    let right_spk = Pos2::new(center_x + spk_offset_x, spk_y);

    let ear_l = Pos2::new(center_x - head_radius - 2.0, head_y);
    let ear_r = Pos2::new(center_x + head_radius + 2.0, head_y);

    // 1. Direct Sound Propagation Lines
    let direct_color = if state.crossfeed_enabled {
        theme.bit_perfect_green
    } else {
        theme.text_secondary
    };
    painter.line_segment([left_spk, ear_l], Stroke::new(1.8, direct_color));
    painter.line_segment([right_spk, ear_r], Stroke::new(1.8, direct_color));

    // 2. Contralateral Crossfeed Wavefront Arcs (if enabled)
    if state.crossfeed_enabled {
        let cross_color = theme.accent;

        // Curved arc from Left Speaker to Right Ear
        let cp_l = Pos2::new(center_x - 8.0, head_y - 28.0);
        let pts_l = bezier_curve(left_spk, cp_l, ear_r, 16);
        for pair in pts_l.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(1.4, cross_color));
        }

        // Curved arc from Right Speaker to Left Ear
        let cp_r = Pos2::new(center_x + 8.0, head_y - 28.0);
        let pts_r = bezier_curve(right_spk, cp_r, ear_l, 16);
        for pair in pts_r.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(1.4, cross_color));
        }

        // Delay badge in center
        let badge_text = format!("+{} µs ITD", (state.crossfeed_delay_ms * 1000.0).round());
        let badge_pos = Pos2::new(center_x, head_y - 36.0);
        painter.text(
            badge_pos,
            egui::Align2::CENTER_CENTER,
            badge_text,
            crate::theme::font(10.0),
            theme.accent_warm,
        );
    }

    // 3. Draw Speaker Cabinets
    draw_speaker_cabinet(&painter, left_spk, -0.52, "L (30°)", theme);
    draw_speaker_cabinet(&painter, right_spk, 0.52, "R (30°)", theme);

    // 4. Draw Listener Head & Ears
    painter.circle_filled(head_center, head_radius, theme.bg_card_hover);
    painter.circle_stroke(
        head_center,
        head_radius,
        Stroke::new(1.5, theme.text_secondary),
    );

    // Left and Right Ears
    painter.rect_filled(
        Rect::from_center_size(ear_l, Vec2::new(5.0, 10.0)),
        CornerRadius::same(theme.radius_xs),
        theme.text_primary,
    );
    painter.rect_filled(
        Rect::from_center_size(ear_r, Vec2::new(5.0, 10.0)),
        CornerRadius::same(theme.radius_xs),
        theme.text_primary,
    );

    // Nose pointer (facing upward towards speakers)
    painter.line_segment(
        [
            Pos2::new(center_x - 4.0, head_y - head_radius + 2.0),
            Pos2::new(center_x, head_y - head_radius - 5.0),
        ],
        Stroke::new(1.5, theme.text_primary),
    );
    painter.line_segment(
        [
            Pos2::new(center_x, head_y - head_radius - 5.0),
            Pos2::new(center_x + 4.0, head_y - head_radius + 2.0),
        ],
        Stroke::new(1.5, theme.text_primary),
    );

    // Badges in corners
    painter.text(
        Pos2::new(canvas_rect.left() + 10.0, canvas_rect.top() + 8.0),
        egui::Align2::LEFT_TOP,
        tr!("Virtual 60° Soundstage"),
        crate::theme::font(10.5),
        theme.text_primary,
    );

    let itd_summary = format!(
        "ITD: {:.0} µs ({:.1} cm)",
        calculate_itd_delay_us(state.crossfeed_delay_ms),
        calculate_acoustic_path_cm(state.crossfeed_delay_ms),
    );
    painter.text(
        Pos2::new(canvas_rect.right() - 10.0, canvas_rect.top() + 8.0),
        egui::Align2::RIGHT_TOP,
        itd_summary,
        crate::theme::font(10.5),
        theme.text_muted,
    );

    if resp.hovered() {
        resp.on_hover_text(tr!(
            "Visualizes 60° virtual loudspeaker placement. Green vectors show direct arrival; purple curves show diffracted contralateral wavefronts arriving at opposite ear."
        ));
    }
}

/// Helper to draw a speaker cabinet box with driver cone.
fn draw_speaker_cabinet(
    painter: &egui::Painter,
    pos: Pos2,
    _angle_rad: f32,
    label: &str,
    theme: &Theme,
) {
    let size = Vec2::new(26.0, 22.0);
    let rect = Rect::from_center_size(pos, size);
    painter.rect_filled(
        rect,
        CornerRadius::same(theme.radius_xs),
        theme.bg_card_hover,
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_xs),
        Stroke::new(1.2, theme.text_secondary),
        egui::StrokeKind::Inside,
    );

    // Glowing driver cone ring
    painter.circle_filled(pos, 6.0, theme.bg_pill);
    painter.circle_stroke(pos, 6.0, Stroke::new(1.0, theme.accent));
    painter.circle_filled(pos, 2.5, theme.accent);

    painter.text(
        Pos2::new(pos.x, pos.y - 15.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        crate::theme::font(9.5),
        theme.text_muted,
    );
}

/// Quadratic bezier curve interpolation.
fn bezier_curve(p0: Pos2, p1: Pos2, p2: Pos2, steps: usize) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let one_minus_t = 1.0 - t;
        let x = one_minus_t * one_minus_t * p0.x + 2.0 * one_minus_t * t * p1.x + t * t * p2.x;
        let y = one_minus_t * one_minus_t * p0.y + 2.0 * one_minus_t * t * p1.y + t * t * p2.y;
        pts.push(Pos2::new(x, y));
    }
    pts
}

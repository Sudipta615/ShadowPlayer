//! 2D polar soundstage geometry canvas.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use super::types::{DistanceUnit, SpeakerSetupModalState};
use crate::theme::Theme;

/// Render the 2D soundstage geometry with polar concentric distance rings.
pub fn render_soundstage_canvas(
    ui: &mut egui::Ui,
    state: &mut SpeakerSetupModalState,
    theme: &Theme,
    changed: &mut bool,
) {
    let desired_size = Vec2::new(ui.available_width(), 220.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        CornerRadius::same(theme.radius_lg),
        theme.bg_app.gamma_multiply(0.7),
    );
    painter.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, theme.border_card),
        egui::StrokeKind::Inside,
    );

    let center = Pos2::new(rect.center().x, rect.min.y + 130.0);

    // Concentric polar distance rings
    let (max_val, step_val) = match state.unit {
        DistanceUnit::Meters => (4.0, 1.0),
        DistanceUnit::Feet => (12.0, 3.0),
    };
    let scale = 95.0 / max_val;

    let mut current_ring = step_val;
    while current_ring <= max_val + 0.1 {
        let r_px = current_ring * scale;
        painter.circle_stroke(
            center,
            r_px,
            Stroke::new(1.0, theme.border_subtle.gamma_multiply(0.35)),
        );
        painter.text(
            Pos2::new(center.x + 4.0, center.y - r_px - 2.0),
            egui::Align2::LEFT_BOTTOM,
            state.unit.format_distance(current_ring),
            crate::theme::font(9.0),
            theme.text_muted.gamma_multiply(0.6),
        );
        current_ring += step_val;
    }

    // Directional guidelines
    for deg in [-150.0_f32, -105.0, -30.0, 0.0, 30.0, 105.0, 150.0] {
        let rad = deg.to_radians();
        let end_pt = Pos2::new(center.x + 105.0 * rad.sin(), center.y - 105.0 * rad.cos());
        painter.line_segment(
            [center, end_pt],
            Stroke::new(1.0, theme.border_subtle.gamma_multiply(0.18)),
        );
    }

    // Listener (sweet spot) icon
    painter.circle_filled(center, 12.0, theme.bg_card);
    painter.circle_stroke(center, 12.0, Stroke::new(1.5, theme.accent));
    // Ears
    painter.circle_filled(Pos2::new(center.x - 13.0, center.y), 3.0, theme.accent);
    painter.circle_filled(Pos2::new(center.x + 13.0, center.y), 3.0, theme.accent);
    // Nose / Direction cue
    painter.line_segment(
        [center, Pos2::new(center.x, center.y - 9.0)],
        Stroke::new(2.0, theme.accent),
    );
    painter.text(
        Pos2::new(center.x, center.y + 17.0),
        egui::Align2::CENTER_TOP,
        tr!("Listener (Sweet Spot)"),
        crate::theme::font(9.0),
        theme.text_secondary,
    );

    // Speakers
    let click_pos = if response.clicked() {
        response.hover_pos()
    } else {
        None
    };

    for (idx, speaker) in state.speakers.iter().enumerate() {
        let rad = speaker.role.polar_angle_deg().to_radians();
        let r_px = (speaker.distance * scale).clamp(24.0, 108.0);
        let spk_center = Pos2::new(center.x + r_px * rad.sin(), center.y - r_px * rad.cos());

        let is_selected = idx == state.selected_idx;
        let box_rect = Rect::from_center_size(spk_center, Vec2::new(36.0, 24.0));

        // Click detection
        if let Some(cp) = click_pos {
            if box_rect.expand(6.0).contains(cp) {
                state.selected_idx = idx;
                *changed = true;
            }
        }

        // Vector line from listener to speaker
        painter.line_segment(
            [center, spk_center],
            Stroke::new(
                if is_selected { 1.5 } else { 1.0 },
                if is_selected {
                    theme.accent.gamma_multiply(0.7)
                } else {
                    theme.border_subtle.gamma_multiply(0.4)
                },
            ),
        );

        // Speaker box styling
        let bg_col = if is_selected {
            theme.accent.gamma_multiply(0.25)
        } else {
            theme.bg_card
        };
        let stroke_col = if is_selected {
            theme.accent
        } else {
            theme.border_card
        };

        painter.rect_filled(box_rect, CornerRadius::same(theme.radius_sm), bg_col);
        painter.rect_stroke(
            box_rect,
            CornerRadius::same(theme.radius_sm),
            Stroke::new(if is_selected { 2.0 } else { 1.0 }, stroke_col),
            egui::StrokeKind::Inside,
        );

        painter.text(
            spk_center,
            egui::Align2::CENTER_CENTER,
            speaker.role.label(),
            crate::theme::font(11.0),
            if is_selected {
                theme.text_primary
            } else {
                theme.text_secondary
            },
        );

        // Distance & Delay readout below/above box
        let delay_ms = state.delay_ms_for(speaker);
        let text_offset_y = if spk_center.y > center.y { 14.0 } else { -14.0 };
        let align = if spk_center.y > center.y {
            egui::Align2::CENTER_TOP
        } else {
            egui::Align2::CENTER_BOTTOM
        };
        let delay_label = if delay_ms > 0.05 {
            format!("+{:0.1}ms", delay_ms)
        } else {
            "0ms".to_string()
        };
        painter.text(
            Pos2::new(spk_center.x, spk_center.y + text_offset_y),
            align,
            format!(
                "{} ({})",
                state.unit.format_distance(speaker.distance),
                delay_label
            ),
            crate::theme::font(8.5),
            if is_selected {
                theme.bit_perfect_green
            } else {
                theme.text_muted
            },
        );
    }
}

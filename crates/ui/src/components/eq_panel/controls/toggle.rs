//! The panel's switch and choice pill.
//!
//! Shared with the creative-effects panel, so both look and feel the same.
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Sense, Stroke, Vec2};

/// The panel's switch: a rounded track with a sliding thumb.
///
/// Shared with the creative-effects panel, so both switches look and feel
/// the same.
pub fn custom_toggle_switch(
    ui: &mut egui::Ui,
    value: &mut bool,
    tooltip: &str,
    theme: &Theme,
) -> bool {
    let desired_size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, tooltip));
    let mut changed = false;

    if resp.clicked() {
        *value = !*value;
        changed = true;
    }

    let anim_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_toggle_thumb"),
        *value,
        theme.motion_time(0.12),
    );
    let hover_t = ui.ctx().animate_bool_with_time(
        resp.id.with("eq_toggle_hover"),
        resp.hovered(),
        theme.motion_time(0.12),
    );

    let p = ui.painter_at(rect);
    let bg_off = crate::theme::lerp_color(theme.border_card, theme.bg_pill_hover, hover_t);
    let bg = crate::theme::lerp_color(bg_off, theme.accent, anim_t);
    p.rect_filled(rect, CornerRadius::same(theme.radius_lg), bg);

    let stroke_col = crate::theme::lerp_color(theme.border_card, theme.accent_highlight, anim_t);
    p.rect_stroke(
        rect,
        CornerRadius::same(theme.radius_lg),
        Stroke::new(1.0, stroke_col),
        egui::StrokeKind::Inside,
    );

    let thumb_x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), anim_t);
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);

    if hover_t > 0.01 {
        let halo_r = egui::lerp(7.5..=9.5, hover_t);
        let halo_col =
            crate::theme::lerp_color(theme.border_card, theme.accent, anim_t).gamma_multiply(0.25);
        p.circle_filled(thumb_pos, halo_r, halo_col);
    }

    p.circle_filled(thumb_pos, 7.5, Color32::WHITE);

    resp.on_hover_text(tooltip);
    changed
}

/// One option in a small mutually-exclusive set (limiter mode, compressor
/// band, crossfeed view).
///
/// `Button::fill` applies to every widget state, so a filled selected pill in
/// the same row also suppressed egui's hover feedback on its unselected
/// neighbours. Painting the pill by hand restores that feedback and lets the
/// selected label pick an ink colour that stays readable on the accent.
pub(in crate::components::eq_panel) fn choice_pill(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    accent: Color32,
    theme: &Theme,
) -> egui::Response {
    let text_color = if selected {
        crate::theme::on_fill(accent)
    } else {
        theme.text_secondary
    };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), crate::theme::font(10.8), text_color);
    let size = Vec2::new(galley.size().x + 18.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));

    // Hover snaps out instead of crossfading, so the pill the pointer left and
    // the one it entered are never lit at the same time.
    let hover_t = if resp.hovered() && !selected {
        ui.ctx()
            .animate_bool_with_time(resp.id.with("choice_hover"), true, theme.motion_time(0.1))
    } else {
        0.0
    };

    let painter = ui.painter_at(rect);
    let radius = CornerRadius::same(theme.radius_sm);
    if selected {
        painter.rect_filled(rect, radius, accent);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, crate::theme::with_alpha(accent, 200)),
            egui::StrokeKind::Inside,
        );
    } else if hover_t > 0.01 {
        painter.rect_filled(rect, radius, theme.bg_card_hover.gamma_multiply(hover_t));
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, theme.border_card.gamma_multiply(hover_t)),
            egui::StrokeKind::Inside,
        );
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.5, theme.accent_highlight),
            egui::StrokeKind::Inside,
        );
    }
    painter.galley(
        Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );

    resp
}

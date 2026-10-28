//! The primary transport play/pause button.
//!
//! One widget paints it everywhere so the silhouette promised by a theme is
//! actually honoured in the Now Playing card, the full-screen player, the mini
//! player and the theme preview. Hovering eases the button up by a few percent
//! and lights a soft halo; no layout changes, so nothing moves under the
//! pointer.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

use crate::theme::{lerp_color, with_alpha, Theme};

/// Paint a play/pause control of `size` and return its response.
///
/// The caller owns the tooltip, accessibility label and click handling; this
/// helper only guarantees a consistent silhouette and hover treatment.
pub fn play_button(ui: &mut Ui, size: f32, is_playing: bool, theme: &Theme) -> Response {
    let size = size.max(20.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    paint(ui, rect, &response, is_playing, theme);
    response
}

/// Paint the same control into a rectangle the caller already owns.
///
/// Used by the mini player, whose transport sits on a hand-laid strip rather
/// than in a normal allocate-as-you-go layout.
pub fn play_button_at(
    ui: &mut Ui,
    rect: Rect,
    id: egui::Id,
    is_playing: bool,
    theme: &Theme,
) -> Response {
    let response = ui.interact(rect, id, Sense::click());
    paint(ui, rect, &response, is_playing, theme);
    response
}

fn paint(ui: &Ui, rect: Rect, response: &Response, is_playing: bool, theme: &Theme) {
    let size = rect.width().max(20.0);
    let painter = ui.painter_at(rect.expand(8.0));

    // Ease the hover in and out. `animate_bool` keeps requesting repaints only
    // while the value is actually changing.
    let hover_t = ui.ctx().animate_bool_with_time(
        response.id.with("play_hover"),
        response.hovered(),
        theme.motion_time(0.12),
    );

    let scale = 1.0 + hover_t * 0.045;
    let shape = theme.play_button_shape;
    let slot = size * scale;
    // A pill is wider than it is tall. Shrinking the height inside the same
    // square slot gives it the silhouette the gallery promises without
    // changing the transport row's layout in any view.
    let button_size = match shape {
        crate::theme::PlayButtonShape::Pill => Vec2::new(slot, slot * 0.62),
        _ => Vec2::splat(slot),
    };
    let button_rect = Rect::from_center_size(rect.center(), button_size);
    let radius = shape.corner_radius(button_rect.size(), theme);

    // Outer glow / halo. This is what makes the control feel responsive
    // without moving anything around it.
    if hover_t > 0.0 {
        let halo = button_rect.expand(size * 0.10 * hover_t);
        painter.rect_filled(
            halo,
            shape.corner_radius(halo.size(), theme),
            with_alpha(theme.accent_highlight, (34.0 * hover_t) as u8),
        );
    }

    let fill = lerp_color(theme.accent, theme.accent_highlight, hover_t * 0.16);
    painter.rect_filled(button_rect, radius, fill);
    painter.rect_stroke(
        button_rect,
        radius,
        Stroke::new(
            1.0,
            with_alpha(theme.accent_highlight, 110 + (hover_t * 90.0) as u8),
        ),
        StrokeKind::Inside,
    );

    // A tiny specular highlight on the top-left of the control, matching the
    // glass treatment used by the surrounding panes.
    if theme.frosted {
        painter.circle_filled(
            Pos2::new(
                button_rect.center().x - button_rect.width() * 0.18,
                button_rect.center().y - button_rect.height() * 0.22,
            ),
            button_rect.width() * 0.16,
            Color32::from_rgba_unmultiplied(255, 255, 255, 26),
        );
    }

    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(button_rect.height() * 0.42));
    if is_playing {
        crate::components::icons::draw_pause(ui, icon_rect, Color32::WHITE);
    } else {
        crate::components::icons::draw_play(ui, icon_rect, Color32::WHITE, true);
    }
}

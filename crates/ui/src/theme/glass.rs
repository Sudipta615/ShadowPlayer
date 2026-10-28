//! Frosted-glass surface support.
//!
//! egui paints immediate-mode rectangles, so a true backdrop-filter cannot be
//! sampled from the framebuffer. Instead the shell paints a deliberately
//! defocused artwork backdrop once, before any panel, and then uses translucent
//! surfaces with a bright micro-bevel and a soft inner sheen. The result is the
//! same optical recipe real glass uses: a blurred colour field, a tinted pane,
//! and a specular edge that catches the light.

use eframe::egui::{
    self, Color32, CornerRadius, Rect, Stroke, StrokeKind, TextureHandle, Ui, Vec2,
};

use super::tokens::{with_alpha, Theme};

/// Paint the blurred colour field behind the main application shell.
///
/// Called only while `theme.frosted` is true, before the sidebar and central
/// panels. `backdrop` is the current track's downsampled, blurred artwork (see
/// `ArtworkService::backdrop_texture`).
pub fn paint_backdrop(ui: &Ui, theme: &Theme, backdrop: Option<&TextureHandle>) {
    let rect = ui.max_rect();
    if !rect.is_positive() {
        return;
    }

    let painter = ui.painter().with_clip_rect(rect);

    // A gentle vertical wash keeps the top of the window lighter, the way a
    // real pane catches ambient light from above.
    let (top, bottom) = if theme.is_light {
        (lighten(theme.bg_app, 10), darken(theme.bg_app, 12))
    } else {
        (lighten(theme.bg_app, 10), darken(theme.bg_app, 22))
    };
    vertical_gradient(&painter, rect, top, bottom);

    if let Some(texture) = backdrop {
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            with_alpha(Color32::WHITE, if theme.is_light { 128 } else { 176 }),
        );
    }

    // Tint the image back towards the theme. Dark themes need a slightly
    // denser scrim so body text keeps its contrast over bright album covers.
    painter.rect_filled(
        rect,
        CornerRadius::ZERO,
        with_alpha(theme.bg_app, if theme.is_light { 132 } else { 116 }),
    );

    // Two slow accent glows give the glass something to refract. They are
    // deliberately broad and low-contrast; this is atmosphere, not decoration.
    radial_glow(
        &painter,
        egui::Pos2::new(
            rect.left() + rect.width() * 0.12,
            rect.top() + rect.height() * 0.18,
        ),
        rect.width().max(rect.height()) * 0.62,
        theme.accent_highlight,
        if theme.is_light { 16 } else { 30 },
    );
    radial_glow(
        &painter,
        egui::Pos2::new(
            rect.right() - rect.width() * 0.08,
            rect.bottom() - rect.height() * 0.12,
        ),
        rect.width().max(rect.height()) * 0.52,
        theme.accent_warm,
        if theme.is_light { 10 } else { 18 },
    );

    // A very soft vignette stops the edges from looking flat.
    let top_veil = with_alpha(theme.bg_app, if theme.is_light { 20 } else { 44 });
    vertical_gradient(
        &painter,
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() * 0.28)),
        top_veil,
        Color32::TRANSPARENT,
    );
}

/// Add the specular rim and top sheen that turns a translucent card into glass.
/// Call after the card's content has been drawn so the highlight sits above the
/// fill but below no text (it only touches the outer 1–2 px).
pub fn paint_glass_sheen(ui: &Ui, rect: Rect, radius: CornerRadius, theme: &Theme) {
    if !theme.frosted || !rect.is_positive() {
        return;
    }

    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));

    // Broad, low-alpha highlight across the top third of the pane.
    let sheen = Rect::from_min_max(
        rect.min,
        egui::Pos2::new(rect.max.x, rect.min.y + (rect.height() * 0.42).min(64.0)),
    );
    vertical_gradient(
        &painter,
        sheen,
        with_alpha(Color32::WHITE, if theme.is_light { 26 } else { 18 }),
        Color32::TRANSPARENT,
    );

    // A brighter 1 px line along the top edge. The corners are left alone so
    // the line follows the rounded silhouette rather than cutting across it.
    let inset = radius.nw.max(radius.ne) as f32 + 1.0;
    if rect.width() > inset * 2.0 {
        painter.line_segment(
            [
                egui::Pos2::new(rect.left() + inset, rect.top() + 1.0),
                egui::Pos2::new(rect.max.x - inset, rect.top() + 1.0),
            ],
            Stroke::new(1.0, with_alpha(Color32::WHITE, 46)),
        );
    }

    // Micro-bevel on the remaining edges: slightly brighter than the normal
    // border, but not enough to read as a second outline.
    painter.rect_stroke(
        rect.shrink(0.5),
        radius,
        Stroke::new(1.0, with_alpha(Color32::WHITE, 18)),
        StrokeKind::Inside,
    );
}

fn lighten(color: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r().saturating_add(amount),
        color.g().saturating_add(amount),
        color.b().saturating_add(amount),
        color.a(),
    )
}

fn darken(color: Color32, amount: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r().saturating_sub(amount),
        color.g().saturating_sub(amount),
        color.b().saturating_sub(amount),
        color.a(),
    )
}

fn vertical_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    if !rect.is_positive() {
        return;
    }
    let mut mesh = egui::epaint::Mesh::default();
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_top(),
        uv: egui::epaint::WHITE_UV,
        color: top,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_top(),
        uv: egui::epaint::WHITE_UV,
        color: top,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: bottom,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: bottom,
    });
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

fn radial_glow(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    color: Color32,
    max_alpha: u8,
) {
    if radius <= 0.0 || max_alpha == 0 {
        return;
    }
    // Approximate a radial gradient with a small stack of translucent discs.
    // Twenty-four fills is negligible next to the widget work of a frame.
    const STEPS: usize = 24;
    for i in 0..STEPS {
        let t = i as f32 / (STEPS - 1) as f32;
        let r = radius * (1.0 - t * 0.92);
        let alpha = (max_alpha as f32 * (1.0 - t).powi(2) * 0.16) as u8;
        if alpha > 0 {
            painter.circle_filled(center, r, with_alpha(color, alpha));
        }
    }
}

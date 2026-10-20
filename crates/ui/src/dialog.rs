//! Helpers shared by the app's floating dialogs.

use eframe::egui::{self, Vec2};

/// The viewport rectangle, with a sensible fallback for the frames before the
/// platform has reported one.
pub fn viewport_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.input(|i| i.raw.screen_rect)
        .unwrap_or(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            Vec2::new(1280.0, 830.0),
        ))
}

/// Clamp a dialog's preferred size to what the viewport can actually show.
///
/// The app's minimum window is 640x360, and several dialogs were authored
/// larger than that with `resizable(false)`. Their title bar sat near the top
/// of the window and the rest of the dialog fell outside it, with no way to
/// scroll to the content or drag the dialog back into view.
pub fn fit_to_viewport(ctx: &egui::Context, desired: Vec2) -> Vec2 {
    let screen = viewport_rect(ctx);
    Vec2::new(
        desired.x.min((screen.width() - 32.0).max(240.0)),
        desired.y.min((screen.height() - 32.0).max(160.0)),
    )
}

/// The smallest size a dialog may be dragged to.
///
/// Kept modest on purpose: a dialog that cannot be shrunk below this stops
/// being usable on a small screen.
pub fn min_dialog_size() -> Vec2 {
    Vec2::new(320.0, 200.0)
}

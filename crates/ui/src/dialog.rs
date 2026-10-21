//! Helpers shared by the app's floating dialogs.

use eframe::egui::{self, CornerRadius, Sense, Stroke, Vec2};

use crate::theme::Theme;

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

/// Show a modal dialog whose body scrolls.
///
/// A dialog taller than the viewport is anchored in the middle, so its top
/// ends up off-screen and unreachable. This variant gives the body a scroll
/// area sized to the viewport, so the whole dialog always fits.
pub fn show_modal_scrollable(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    theme: &Theme,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut close_requested = false;
    let screen = viewport_rect(ctx);
    // The title row, the separator and the frame's margins.
    let max_body_h = (screen.height() - 110.0).max(120.0);

    egui::Modal::new(egui::Id::unique(id))
        .frame(
            egui::Frame::popup(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(egui::Margin::same(theme.space_3xl as i8)),
        )
        .backdrop_color(theme.modal_backdrop)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_max_width(width);

            // ── Title row ───────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .font(crate::theme::font(15.5))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                    let color = if resp.hovered() {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    };
                    crate::components::icons::draw_window_close(ui, rect.shrink(5.0), color);
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, "Close")
                    });
                    let _ = resp.clone().on_hover_text("Close");
                    if resp.clicked() {
                        close_requested = true;
                    }
                });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            egui::ScrollArea::vertical()
                .id_salt("modal_body_scroll")
                // Grow with the content, and only scroll once the content
                // is taller than the window allows: a two-line dialog should
                // not be a full-screen panel.
                .auto_shrink([false, true])
                .max_height(max_body_h)
                .show(ui, body);

            // Shrink-to-fit measures the body before the scroll area's own
            // margin is applied; a few pixels here keep the last line of a
            // short dialog off the frame's edge.
            ui.add_space(6.0);
        });

    close_requested
}

/// Show a modal dialog with no title row.
///
/// A command palette is the case for this: the title bar of a modal is
/// wasted space there, because the field is obviously what the dialog is
/// for, and every convention for a palette is a bare field.
pub fn show_modal_bare(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    theme: &Theme,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let response = egui::Modal::new(egui::Id::unique(id))
        .frame(
            egui::Frame::popup(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_lg))
                .inner_margin(egui::Margin::same(theme.space_lg as i8)),
        )
        .backdrop_color(theme.modal_backdrop)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_max_width(width);
            body(ui);
        });
    // A click on the backdrop dismisses the dialog.
    response.backdrop_response.clicked()
}

/// Show a modal dialog in the app's own chrome.
///
/// egui's `Window` brings a title bar that does not match the app's card
/// style, and it leaves the background UI live: a click behind the dialog
/// still lands on the library, and Tab walks straight out of the dialog into
/// the sidebar. `egui::Modal` gives the backdrop and the focus trap; this
/// helper adds the title and the close button in the app's visual language.
///
/// Returns `true` when the user asked to close the dialog.
pub fn show_modal(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    theme: &Theme,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut close_requested = false;
    let modal_id = egui::Id::unique(id);

    egui::Modal::new(modal_id)
        .frame(
            egui::Frame::popup(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0, theme.border_card))
                .corner_radius(CornerRadius::same(theme.radius_xl))
                .inner_margin(egui::Margin::same(theme.space_3xl as i8)),
        )
        .backdrop_color(theme.modal_backdrop)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.set_max_width(width);

            // ── Title row ───────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .font(crate::theme::font(15.5))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                    let color = if resp.hovered() {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    };
                    crate::components::icons::draw_window_close(ui, rect.shrink(5.0), color);
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, "Close")
                    });
                    let _ = resp.clone().on_hover_text("Close");
                    if resp.clicked() {
                        close_requested = true;
                    }
                });
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            body(ui);
        });

    close_requested
}

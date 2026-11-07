//! Shared panel controls: cards, the footer preamp, the gradient slider, and
//! the toggle switch.
//!
//! The drawing half of each control lives in a submodule; this façade keeps
//! the two primitives every tab builds on ([`card`] and the footer strip)
//! and re-exports the rest, so `super::controls::name` still resolves.

use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Margin, Stroke, Vec2};

/// A rounded, bordered card with the panel's standard inner spacing.
///
/// The fill is the app background rather than a raised surface: sections on
/// the panel already sit on `bg_window`, so one step *down* separates them
/// without stacking two nearly identical greys on top of each other.
pub(super) fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme.bg_app)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_3xl as i8,
            theme.space_2xl as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                add_contents(ui);
            });
        });
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Footer Controls: Preamp Slider & Tab-Specific Reset Button
// ─────────────────────────────────────────────────────────────────────────────

/// Slider excursion that can represent the current shared preamp.
///
/// Normal mode is ±12 dB and Extended Range is ±24 dB. A curve computed
/// outside the panel (AutoEQ) can need more; the control widens just enough to
/// show it instead of clamping the knob while the readout tells the truth.
pub(in crate::components::eq_panel) fn preamp_display_limit(
    preamp_db: f32,
    extended_range: bool,
) -> f32 {
    if preamp_db.abs() > 24.0 {
        30.0
    } else if extended_range || preamp_db.abs() > 12.0 {
        24.0
    } else {
        12.0
    }
}

pub(super) fn render_footer_controls(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    tab: EqTab,
) {
    // The preamp card and the reset button fit side by side on a normal
    // window. Below that they stack, so the footer never imposes a minimum
    // width on a panel the user deliberately narrowed.
    let stacked = ui.available_width() < 480.0;

    if stacked {
        render_preamp_card(ui, state, actions, theme, None);
        ui.add_space(theme.space_xs);
        render_reset_button(ui, state, actions, theme, tab, None);
    } else {
        let reset_w = 96.0;
        let preamp_w = (ui.available_width() - reset_w - theme.space_md).max(180.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
            render_preamp_card(ui, state, actions, theme, Some(preamp_w));
            render_reset_button(ui, state, actions, theme, tab, Some(reset_w));
        });
    }
}

mod preamp;
mod slider;
mod tabs;
#[cfg(test)]
mod tests;
mod toggle;

use preamp::{render_preamp_card, render_reset_button};
pub(super) use slider::gradient_horizontal_slider;
pub(super) use tabs::render_tab_navigation;
pub(super) use toggle::choice_pill;
pub use toggle::custom_toggle_switch;

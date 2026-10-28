//! The Filters tab: headphone crossfeed and measured room correction.
//!
//! Both stages existed in the engine and were persisted in the EQ state, but
//! the studio never rendered their controls. The only way to change them was a
//! chain preset or a direct engine call. This tab makes the active state and
//! the reasons for bypass visible.

use super::controls::{custom_toggle_switch, gradient_horizontal_slider, render_footer_controls};
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Margin, Stroke, Vec2};
use std::ops::RangeInclusive;

pub(super) fn render_filters_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        render_crossfeed_card(ui, state, actions, theme);
        render_correction_card(ui, state, actions, theme);
        render_speaker_setup_card(ui, actions, theme);

        render_footer_controls(ui, state, actions, theme, EqTab::Filters);
    });
}

fn render_speaker_setup_card(ui: &mut egui::Ui, actions: &mut EqPanelActions, theme: &Theme) {
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("Speaker Setup & Alignment"))
                    .font(crate::theme::font(14.0))
                    .strong()
                    .color(theme.text_primary),
            );
        });
        ui.label(
            egui::RichText::new(tr!(
                "Calibrate speaker arrival delays, distances, channel trims, and subwoofer crossover."
            ))
            .font(crate::theme::font(11.0))
            .color(theme.text_secondary),
        );
        if ui
            .button(tr!("Open Speaker Setup & Alignment Wizard…"))
            .clicked()
        {
            actions.open_speaker_setup = true;
        }
    });
}

fn render_crossfeed_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    super::crossfeed::render_crossfeed_card(ui, state, actions, theme);
}

fn render_correction_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    card(ui, theme, |ui| {
        header_with_toggle(
            ui,
            &tr!("Correction"),
            "correction",
            &crate::components::info_box::INFO_CORRECTION,
            &mut state.correction_enabled,
            &tr!("Enable Correction"),
            theme,
            |enabled| actions.correction_enabled_changed = Some(enabled),
        );
        ui.label(
            egui::RichText::new(tr!("Convolves playback with a measured impulse response. Depth blends dry and corrected signal."))
                .font(crate::theme::font(11.0))
                .color(theme.text_secondary),
        );
        if state.correction_enabled && state.correction_ir_path.trim().is_empty() {
            ui.label(
                egui::RichText::new(tr!(
                    "No impulse response loaded. The correction stage stays bypassed."
                ))
                .font(crate::theme::font(10.5))
                .strong()
                .color(theme.accent_warm),
            );
        }

        slider_row(
            ui,
            &tr!("Correction depth"),
            &mut state.correction_depth,
            0.0..=1.0,
            1.0,
            |v| format!("{:.0}%", v * 100.0),
            theme,
            |v| actions.correction_depth_changed = Some(v),
        );

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("Impulse response file"))
                    .font(crate::theme::font(11.0))
                    .color(theme.text_secondary),
            );
            ui.add(
                egui::TextEdit::singleline(&mut state.correction_ir_path)
                    .desired_width((ui.available_width() - 100.0).max(120.0))
                    .hint_text(tr!("Path to .wav IR")),
            );
            if ui.button(tr!("Load IR…")).clicked() {
                actions.load_correction_ir = true;
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn header_with_toggle(
    ui: &mut egui::Ui,
    title: &str,
    info_id: &str,
    info: &crate::components::info_box::InfoBoxContent,
    value: &mut bool,
    toggle_tooltip: &str,
    theme: &Theme,
    mut on_changed: impl FnMut(bool),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
        ui.label(
            egui::RichText::new(title)
                .font(crate::theme::font(14.0))
                .strong()
                .color(theme.text_primary),
        );
        crate::components::info_box::render_info_icon_button(ui, info_id, info, theme);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if custom_toggle_switch(ui, value, toggle_tooltip, theme) {
                on_changed(*value);
            }
            ui.label(
                egui::RichText::new(toggle_tooltip)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_secondary),
            );
        });
    });
}

fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
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

#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    format: impl Fn(f32) -> String,
    theme: &Theme,
    mut on_changed: impl FnMut(f32),
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .font(crate::theme::font(11.0))
                .color(theme.text_secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format(*value))
                    .font(crate::theme::font(10.5))
                    .color(theme.text_muted),
            );
            let width = (ui.available_width() - 8.0).max(80.0);
            ui.allocate_ui_with_layout(
                Vec2::new(width, 18.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if gradient_horizontal_slider(ui, value, range, default, theme) {
                        on_changed(*value);
                    }
                },
            );
        });
    });
}

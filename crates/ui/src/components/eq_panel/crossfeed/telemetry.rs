//! Telemetry readouts, slider controls, and card wrappers for crossfeed.

use super::types::{
    calculate_acoustic_path_cm, calculate_fatigue_reduction_pct, calculate_itd_delay_us,
};
use crate::components::eq_panel::controls::gradient_horizontal_slider;
use crate::components::eq_panel::EqPanelState;
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};
use std::ops::RangeInclusive;

/// Psychoacoustic metric telemetry boxes.
pub fn render_telemetry_strip(ui: &mut egui::Ui, state: &EqPanelState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
        let box_w = (ui.available_width() - theme.space_xs * 3.0) / 4.0;

        let delay_us = calculate_itd_delay_us(state.crossfeed_delay_ms);
        let path_cm = calculate_acoustic_path_cm(state.crossfeed_delay_ms);
        let relief_pct =
            calculate_fatigue_reduction_pct(state.crossfeed_freq, state.crossfeed_delay_ms);

        telemetry_box(
            ui,
            tr!("ITD Delay"),
            &format!("{delay_us:.0} µs"),
            theme.accent_warm,
            box_w,
            theme,
        );
        telemetry_box(
            ui,
            tr!("Path Offset"),
            &format!("{path_cm:.1} cm"),
            theme.text_primary,
            box_w,
            theme,
        );
        telemetry_box(
            ui,
            tr!("Head Cutoff"),
            &format!("{:.0} Hz", state.crossfeed_freq),
            theme.text_primary,
            box_w,
            theme,
        );
        telemetry_box(
            ui,
            tr!("Fatigue Relief"),
            &format!("{relief_pct}%"),
            theme.bit_perfect_green,
            box_w,
            theme,
        );
    });
}

fn telemetry_box(
    ui: &mut egui::Ui,
    label: String,
    value: &str,
    val_color: Color32,
    width: f32,
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_pill)
        .corner_radius(CornerRadius::same(theme.radius_sm))
        .inner_margin(Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(width.max(50.0));
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(9.5))
                        .color(theme.text_muted),
                );
                ui.label(
                    egui::RichText::new(value)
                        .font(crate::theme::font(11.5))
                        .strong()
                        .color(val_color),
                );
            });
        });
}

pub fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
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
pub fn slider_row(
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

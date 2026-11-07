//! The Dynamics tab: the limiter and multiband compressor that shape the
//! output after the EQ.
//!
//! These controls existed in persisted state and in the engine command
//! surface, but no tab rendered them. Keeping them behind an invisible path
//! made the UI claim an EQ-only chain while a limiter could still be active.
//! Every control here writes the existing typed action, and the dispatcher
//! forwards it to the engine.

use super::controls::{
    card, custom_toggle_switch, gradient_horizontal_slider, render_footer_controls,
};
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use audio_session::engine::dsp::limiter::LimiterMode;
use eframe::egui::{self, Vec2};
use std::ops::RangeInclusive;

pub(super) fn render_dynamics_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        render_limiter_card(ui, state, actions, theme);
        render_compressor_card(ui, state, actions, theme);

        render_footer_controls(ui, state, actions, theme, EqTab::Dynamics);
    });
}

fn render_limiter_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
            ui.label(
                egui::RichText::new(tr!("Limiter"))
                    .font(crate::theme::font(14.0))
                    .strong()
                    .color(theme.text_primary),
            );
            crate::components::info_box::render_info_icon_button(
                ui,
                "limiter",
                &crate::components::info_box::INFO_LIMITER,
                theme,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut enabled = state.limiter_enabled;
                if custom_toggle_switch(ui, &mut enabled, &tr!("Enable Limiter"), theme) {
                    state.limiter_enabled = enabled;
                    actions.limiter_enabled_changed = Some(enabled);
                }
                ui.label(
                    egui::RichText::new(tr!("Enable Limiter"))
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            });
        });

        ui.label(
            egui::RichText::new(tr!(
                "True-peak guard catches peaks after every boost in the chain."
            ))
            .font(crate::theme::font(11.0))
            .color(theme.text_secondary),
        );

        // Mode — label on its own row so the five pills have room to breathe.
        ui.label(
            egui::RichText::new(tr!("Limiter mode"))
                .font(crate::theme::font(10.5))
                .color(theme.text_muted),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xxs);
            for (mode, label) in [
                (LimiterMode::Transparent, tr!("Transparent")),
                (LimiterMode::Saturate, tr!("Saturate")),
                (LimiterMode::Safety, tr!("Safety")),
                (LimiterMode::Mastering, tr!("Mastering")),
                (LimiterMode::ClipperLimiter, tr!("Clipper + Limiter")),
            ] {
                let selected = state.limiter_mode == mode;
                if super::controls::choice_pill(ui, &label, selected, theme.accent, theme).clicked()
                    && !selected
                {
                    state.limiter_mode = mode;
                    actions.limiter_mode_changed = Some(mode);
                }
            }
        });

        toggle_row(
            ui,
            &tr!("True Peak"),
            &mut state.limiter_true_peak,
            theme,
            |enabled| actions.limiter_true_peak_changed = Some(enabled),
        );

        slider_row(
            ui,
            &tr!("Ceiling"),
            &mut state.limiter_ceiling_db,
            -6.0..=0.0,
            -1.0,
            |v| format!("{v:+.1} dBFS"),
            theme,
            || actions.limiter_params_changed = true,
        );
        slider_row(
            ui,
            &tr!("Lookahead"),
            &mut state.limiter_lookahead_ms,
            1.0..=20.0,
            5.0,
            |v| format!("{v:.1} ms"),
            theme,
            || actions.limiter_params_changed = true,
        );
        slider_row(
            ui,
            &tr!("Attack"),
            &mut state.limiter_attack_ms,
            0.1..=10.0,
            0.3,
            |v| format!("{v:.2} ms"),
            theme,
            || actions.limiter_params_changed = true,
        );
        slider_row(
            ui,
            &tr!("Release"),
            &mut state.limiter_release_ms,
            20.0..=500.0,
            100.0,
            |v| format!("{v:.0} ms"),
            theme,
            || actions.limiter_params_changed = true,
        );
        toggle_row(
            ui,
            &tr!("Soft Clip"),
            &mut state.limiter_soft_clip,
            theme,
            |_| actions.limiter_params_changed = true,
        );
    });
}

fn render_compressor_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    card(ui, theme, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
            ui.label(
                egui::RichText::new(tr!("Compressor"))
                    .font(crate::theme::font(14.0))
                    .strong()
                    .color(theme.text_primary),
            );
            crate::components::info_box::render_info_icon_button(
                ui,
                "compressor",
                &crate::components::info_box::INFO_COMPRESSOR,
                theme,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut enabled = state.compressor_enabled;
                if custom_toggle_switch(ui, &mut enabled, &tr!("Enable Compressor"), theme) {
                    state.compressor_enabled = enabled;
                    actions.compressor_enabled_changed = Some(enabled);
                }
                ui.label(
                    egui::RichText::new(tr!("Enable Compressor"))
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            });
        });

        ui.label(
            egui::RichText::new(tr!(
                "Three independent bands. Makeup gain runs before the output limiter."
            ))
            .font(crate::theme::font(11.0))
            .color(theme.text_muted),
        );

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
            for (band, label) in [(0, tr!("Low")), (1, tr!("Mid")), (2, tr!("High"))] {
                let selected = state.comp_band == band;
                if super::controls::choice_pill(ui, &label, selected, theme.accent, theme).clicked()
                {
                    state.comp_band = band;
                }
            }
        });

        let band = state.comp_band.min(2);
        slider_row(
            ui,
            &tr!("Threshold"),
            &mut state.comp_threshold[band],
            -60.0..=0.0,
            -24.0,
            |v| format!("{v:+.1} dB"),
            theme,
            || actions.comp_band_params_changed = Some(band),
        );
        slider_row(
            ui,
            &tr!("Ratio"),
            &mut state.comp_ratio[band],
            1.0..=20.0,
            4.0,
            |v| format!("{v:.1}:1"),
            theme,
            || actions.comp_band_params_changed = Some(band),
        );
        slider_row(
            ui,
            &tr!("Attack"),
            &mut state.comp_attack_ms[band],
            0.1..=100.0,
            10.0,
            |v| format!("{v:.1} ms"),
            theme,
            || actions.comp_band_params_changed = Some(band),
        );
        slider_row(
            ui,
            &tr!("Release"),
            &mut state.comp_release_ms[band],
            10.0..=1000.0,
            100.0,
            |v| format!("{v:.0} ms"),
            theme,
            || actions.comp_band_params_changed = Some(band),
        );
        slider_row(
            ui,
            &tr!("Makeup Gain"),
            &mut state.comp_makeup_db[band],
            -12.0..=12.0,
            0.0,
            |v| format!("{v:+.1} dB"),
            theme,
            || actions.comp_band_params_changed = Some(band),
        );
        slider_row(
            ui,
            &tr!("Knee"),
            &mut state.comp_knee_db[band],
            0.0..=24.0,
            3.0,
            |v| format!("{v:.1} dB"),
            theme,
            || actions.comp_band_features_changed = Some(band),
        );
        toggle_row(
            ui,
            &tr!("Stereo Link"),
            &mut state.comp_stereo_link[band],
            theme,
            |_| actions.comp_band_features_changed = Some(band),
        );
    });
}

fn toggle_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut bool,
    theme: &Theme,
    mut on_changed: impl FnMut(bool),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

        // Fixed label column matching slider_row so toggle and slider rows
        // share the same left-aligned label column.
        ui.allocate_ui_with_layout(
            Vec2::new(SLIDER_LABEL_W, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            },
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if custom_toggle_switch(ui, value, label, theme) {
                on_changed(*value);
            }
        });
    });
}

/// A labelled slider row with a fixed label column so all sliders within a
/// card start at the same x position regardless of label length.
const SLIDER_LABEL_W: f32 = 90.0;
const SLIDER_READOUT_W: f32 = 56.0;

#[allow(clippy::too_many_arguments)]
fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    default: f32,
    format: impl Fn(f32) -> String,
    theme: &Theme,
    mut on_changed: impl FnMut(),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

        // Fixed-width label column — all sliders align at the same left edge.
        ui.allocate_ui_with_layout(
            Vec2::new(SLIDER_LABEL_W, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            },
        );

        // Slider fills the remaining space minus a fixed readout column.
        let readout_text = format(*value);
        let slider_w = (ui.available_width() - SLIDER_READOUT_W - theme.space_sm).max(60.0);
        ui.allocate_ui_with_layout(
            Vec2::new(slider_w, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if gradient_horizontal_slider(ui, value, range, default, theme) {
                    on_changed();
                }
            },
        );

        // Right-aligned readout in its own fixed column.
        ui.allocate_ui_with_layout(
            Vec2::new(SLIDER_READOUT_W, 18.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(readout_text)
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                );
            },
        );
    });
}

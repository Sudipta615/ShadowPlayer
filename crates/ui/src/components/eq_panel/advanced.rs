//! The Advanced tab: extended range, headroom, mid/side, dithering, and the
//! AutoEQ calibration entry point.

use super::controls::{custom_toggle_switch, gradient_horizontal_slider, render_footer_controls};
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 6. Tab 3: Advanced Settings (Max Dynamic Range, Headroom, M/S, Dither, AutoEQ)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_advanced_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let two_col = avail_w >= 540.0;
    let card_w = if two_col {
        (avail_w - 12.0) * 0.5
    } else {
        avail_w
    };

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        // Row 1: Extended Gain Range & Auto Headroom
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 1: Extended Dynamic Range (±24 dB Max Values Mode)
            render_advanced_toggle_card(
                ui,
                card_w,
                "Extended Gain Range",
                &crate::components::info_box::INFO_EXTENDED_RANGE,
                "Unlocks full ±24.0 dB boost/cut across Graphic EQ bands, Tone shelves, and Preamp.",
                if state.extended_range { "±24 dB Unlocked" } else { "±12 dB Standard" },
                state.extended_range,
                &mut state.extended_range,
                |_| {},
                theme,
            );

            // Card 2: Automatic Headroom Management
            if two_col {
                let mut ah = state.auto_headroom;
                render_advanced_toggle_card(
                    ui,
                    card_w,
                    "Auto Headroom Management",
                    &crate::components::info_box::INFO_AUTO_HEADROOM,
                    "Dynamically lowers master digital gain when EQ boosts exceed 0 dBFS to prevent clipping.",
                    if ah { "Auto Attenuation ON" } else { "Headroom Bypass" },
                    ah,
                    &mut ah,
                    |v| {
                        state.auto_headroom = v;
                        actions.eq_auto_headroom_changed = Some(v);
                    },
                    theme,
                );
            }
        });

        if !two_col {
            let mut ah = state.auto_headroom;
            render_advanced_toggle_card(
                ui,
                card_w,
                "Auto Headroom Management",
                &crate::components::info_box::INFO_AUTO_HEADROOM,
                "Dynamically lowers master digital gain when EQ boosts exceed 0 dBFS to prevent clipping.",
                if ah { "Auto Attenuation ON" } else { "Headroom Bypass" },
                ah,
                &mut ah,
                |v| {
                    state.auto_headroom = v;
                    actions.eq_auto_headroom_changed = Some(v);
                },
                theme,
            );
        }

        // Row 2: Mid/Side Processing & TPDF Dithering
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Card 3: Mid/Side Processing (M/S Mode)
            let mut ms = state.midside_eq;
            render_advanced_toggle_card(
                ui,
                card_w,
                "Mid/Side Processing (M/S)",
                &crate::components::info_box::INFO_MIDSIDE_EQ,
                "Splits audio into Mid (center) and Side (stereo field) channels for independent acoustic shaping.",
                if ms { "M/S Matrix Active" } else { "Stereo L/R Mode" },
                ms,
                &mut ms,
                |v| {
                    state.midside_eq = v;
                    actions.midside_eq_changed = Some(v);
                },
                theme,
            );

            // Card 4: TPDF Triangular Dithering
            if two_col {
                let mut dit = state.dither_enabled;
                render_advanced_toggle_card(
                    ui,
                    card_w,
                    "TPDF Triangular Dithering",
                    &crate::components::info_box::INFO_DITHER,
                    "High-resolution noise shaping that eliminates low-level quantization distortion on 32-bit float output.",
                    if dit { "32-bit TPDF Active" } else { "Truncation Mode" },
                    dit,
                    &mut dit,
                    |v| {
                        state.dither_enabled = v;
                        actions.dither_enabled_changed = Some(v);
                    },
                    theme,
                );
            }
        });

        if !two_col {
            let mut dit = state.dither_enabled;
            render_advanced_toggle_card(
                ui,
                card_w,
                "TPDF Triangular Dithering",
                &crate::components::info_box::INFO_DITHER,
                "High-resolution noise shaping that eliminates low-level quantization distortion on 32-bit float output.",
                if dit { "32-bit TPDF Active" } else { "Truncation Mode" },
                dit,
                &mut dit,
                |v| {
                    state.dither_enabled = v;
                    actions.dither_enabled_changed = Some(v);
                },
                theme,
            );
        }

        // Row 3: Headphone Calibration (AutoEQ Curve Database) Banner Card
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_lg))
            .inner_margin(Margin::symmetric(theme.space_3xl as i8, theme.space_xl as i8))
            .show(ui, |ui| {
                ui.set_width(avail_w - 32.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_2xl, 0.0);

                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::hover());
                    crate::components::icons::draw_headphones(ui, icon_rect, theme.accent_highlight);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                            ui.label(
                                egui::RichText::new(tr!("AutoEQ Headphone Target Calibration"))
                                    .font(crate::theme::font(13.5))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            crate::components::info_box::render_info_icon_button(
                                ui,
                                "autoeq_calibration_card",
                                &crate::components::info_box::INFO_AUTOEQ,
                                theme,
                            );
                        });
                        ui.label(
                            egui::RichText::new(tr!("Calibrate your headphones or IEMs using Harman/Diffuse-field acoustic targets from 4,000+ lab measurements."))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            egui::RichText::new(tr!("Open AutoEQ"))
                                .font(crate::theme::font(12.0))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(theme.accent)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(theme.radius_md))
                        .min_size(Vec2::new(110.0, 30.0));

                        if ui.add(btn).on_hover_text(tr!(Msg::S628A0870D33BFE83)).clicked() {
                            actions.open_autoeq_dialog = true;
                        }
                    });
                });
            });

        // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────
        render_footer_controls(ui, state, actions, theme, EqTab::Advanced);
    });
}

#[allow(clippy::too_many_arguments)]
fn render_advanced_toggle_card(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    info: &crate::components::info_box::InfoBoxContent,
    description: &str,
    badge_text: &str,
    is_active: bool,
    value: &mut bool,
    mut on_changed: impl FnMut(bool),
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(
            1.0,
            if is_active {
                theme.accent.linear_multiply(0.4)
            } else {
                theme.border_card
            },
        ))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_xl as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);

                // Row 1: Title, Clickable Info Icon, and Toggle Switch
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(ui, title, info, theme);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if custom_toggle_switch(ui, value, title, theme) {
                            on_changed(*value);
                        }
                    });
                });

                // Row 2: Status Pill Badge
                ui.horizontal(|ui| {
                    let badge_bg = if is_active {
                        theme.accent.linear_multiply(0.2)
                    } else {
                        theme.bg_pill
                    };
                    let badge_fg = if is_active {
                        theme.accent_warm
                    } else {
                        theme.text_muted
                    };
                    egui::Frame::NONE
                        .fill(badge_bg)
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .inner_margin(Margin::symmetric(
                            theme.space_sm as i8,
                            theme.space_xxs as i8,
                        ))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(badge_text)
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(badge_fg),
                            );
                        });
                });

                // Row 3: Description text
                ui.label(
                    egui::RichText::new(description)
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
            });
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_slider_card(
    ui: &mut egui::Ui,
    width: f32,
    title: &str,
    info: &crate::components::info_box::InfoBoxContent,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default_val: f32,
    left_lbl: &str,
    center_lbl: &str,
    right_lbl: &str,
    format_val: impl Fn(f32) -> String,
    mut on_changed: impl FnMut(f32),
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_lg as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(width - 28.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);

                // Row 1: Title + Clickable Info Icon
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(13.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(ui, title, info, theme);
                });

                // Row 2: Range Labels (-12dB ... 0dB ... +12dB or -24dB ... 0dB ... +24dB)
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(left_lbl)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                    let avail_mid = (ui.available_width() - 48.0).max(40.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(avail_mid, 14.0),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new(center_lbl)
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_muted),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(right_lbl)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                    });
                });

                // Row 3: Gradient Slider
                ui.add_space(2.0);
                if gradient_horizontal_slider(ui, value, range, default_val, theme) {
                    on_changed(*value);
                }

                // Row 4: Numeric Readout Centered Below Slider
                ui.add_space(1.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(format_val(*value))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}

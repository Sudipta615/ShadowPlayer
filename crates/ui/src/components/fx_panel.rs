//! The creative-effects panel: chorus, flanger, phaser, delay and saturation.
//!
//! These are the sound-design effects. They are not corrective — the
//! equalizer, the compressor and the filters fix a recording; these change
//! what the recording sounds like. Every one of them is off until the user
//! asks for it, because an effect the listener did not ask for is a defect,
//! not a feature.
//!
//! The panel edits one [`FxConfig`], which is also what the engine's insert
//! takes: the panel cannot describe a state the engine cannot run.

use crate::theme::Theme;
use config::{DistortionShape, FxConfig};
use eframe::egui::{self, CornerRadius, Margin, Stroke, Vec2};

/// Render the creative-effects tab.
///
/// Returns whether anything changed, so the caller can push the new
/// settings to the engine.
pub fn render_fx_tab(ui: &mut egui::Ui, fx: &mut FxConfig, theme: &Theme) -> bool {
    let mut changed = false;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        ui.label(
            egui::RichText::new(tr!("Creative Effects"))
                .font(crate::theme::font(14.0))
                .strong()
                .color(theme.text_primary),
        );
        ui.label(
            egui::RichText::new(tr!(
                "The insert sits after the stereo stage. An effect with the switch off does not touch the signal."
            ))
            .font(crate::theme::font(12.0))
            .color(theme.text_muted),
        );

        let avail_w = ui.available_width();
        let two_col = avail_w >= 540.0;
        let card_w = if two_col {
            (avail_w - 12.0) * 0.5
        } else {
            avail_w
        };

        // Row 1: Chorus & Flanger
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

            changed |= render_effect_card(
                ui,
                card_w,
                tr!("Chorus"),
                &mut fx.chorus.enabled,
                theme,
                |ui| {
                    slider(ui, tr!("Rate"), &mut fx.chorus.rate_hz, 0.05..=8.0, " Hz", theme);
                    slider(
                        ui,
                        tr!("Depth"),
                        &mut fx.chorus.depth_ms,
                        0.0..=20.0,
                        " ms",
                        theme,
                    );
                    slider(
                        ui,
                        tr!("Base Delay"),
                        &mut fx.chorus.base_delay_ms,
                        1.0..=40.0,
                        " ms",
                        theme,
                    );
                    slider(ui, tr!("Feedback"), &mut fx.chorus.feedback, 0.0..=0.9, "", theme);
                    mix_sliders(ui, &mut fx.chorus.wet, &mut fx.chorus.dry, theme);
                },
            );

            if two_col {
                changed |= render_effect_card(
                    ui,
                    card_w,
                    tr!("Flanger"),
                    &mut fx.flanger.enabled,
                    theme,
                    |ui| {
                        slider(ui, tr!("Rate"), &mut fx.flanger.rate_hz, 0.05..=8.0, " Hz", theme);
                        slider(
                            ui,
                            tr!("Depth"),
                            &mut fx.flanger.depth_ms,
                            0.0..=10.0,
                            " ms",
                            theme,
                        );
                        slider(
                            ui,
                            tr!("Manual Offset"),
                            &mut fx.flanger.manual_ms,
                            0.0..=10.0,
                            " ms",
                            theme,
                        );
                        slider(ui, tr!("Feedback"), &mut fx.flanger.feedback, 0.0..=0.95, "", theme);
                        mix_sliders(ui, &mut fx.flanger.wet, &mut fx.flanger.dry, theme);
                    },
                );
            }
        });

        // Row 2: Phaser & Delay
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

            changed |= render_effect_card(
                ui,
                card_w,
                tr!("Phaser"),
                &mut fx.phaser.enabled,
                theme,
                |ui| {
                    slider(ui, tr!("Rate"), &mut fx.phaser.rate_hz, 0.05..=8.0, " Hz", theme);
                    slider(
                        ui,
                        tr!("Base Frequency"),
                        &mut fx.phaser.base_freq_hz,
                        40.0..=2000.0,
                        " Hz",
                        theme,
                    );
                    slider(
                        ui,
                        tr!("Sweep"),
                        &mut fx.phaser.sweep_octaves,
                        0.5..=6.0,
                        " oct",
                        theme,
                    );
                    let mut stages = fx.phaser.stages as f32;
                    if slider(ui, tr!("Stages"), &mut stages, 2.0..=8.0, "", theme) {
                        fx.phaser.stages = stages.round().clamp(2.0, 8.0) as u8;
                    }
                    slider(ui, tr!("Feedback"), &mut fx.phaser.feedback, 0.0..=0.95, "", theme);
                    mix_sliders(ui, &mut fx.phaser.wet, &mut fx.phaser.dry, theme);
                },
            );

            if two_col {
                changed |= render_effect_card(
                    ui,
                    card_w,
                    tr!("Delay"),
                    &mut fx.delay.enabled,
                    theme,
                    |ui| {
                        slider(ui, tr!("Time"), &mut fx.delay.time_ms, 10.0..=1000.0, " ms", theme);
                        slider(ui, tr!("Feedback"), &mut fx.delay.feedback, 0.0..=0.9, "", theme);
                        slider(ui, tr!("Damping"), &mut fx.delay.damping, 0.0..=0.9, "", theme);
                        mix_sliders(ui, &mut fx.delay.wet, &mut fx.delay.dry, theme);
                    },
                );
            }
        });

        // Row 3: Saturation
        changed |= render_effect_card(
            ui,
            avail_w,
            tr!("Saturation"),
            &mut fx.distortion.enabled,
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Shape"))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    let mut shape = fx.distortion.shape;
                    egui::ComboBox::from_id_salt("fx_distortion_shape")
                        .selected_text(shape.name())
                        .show_ui(ui, |ui| {
                            for option in DistortionShape::ALL {
                                if ui.selectable_label(shape == option, option.name()).clicked() {
                                    shape = option;
                                }
                            }
                        });
                    fx.distortion.shape = shape;
                });
                slider(ui, tr!("Drive"), &mut fx.distortion.drive, 0.1..=10.0, "×", theme);
                slider(ui, tr!("Bias"), &mut fx.distortion.bias, -1.0..=1.0, "", theme);
                slider(ui, tr!("Ceiling"), &mut fx.distortion.ceiling, 0.1..=2.0, "", theme);
                mix_sliders(
                    ui,
                    &mut fx.distortion.wet,
                    &mut fx.distortion.dry,
                    theme,
                );
            },
        );
    });

    changed
}

/// One effect's card: the name, the switch and its controls.
fn render_effect_card(
    ui: &mut egui::Ui,
    width: f32,
    title: impl AsRef<str>,
    enabled: &mut bool,
    theme: &Theme,
    controls: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut changed = false;
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(
            1.0,
            if *enabled {
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
            ui.set_width((width - 28.0).max(120.0));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);

                let title = title.as_ref();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::components::eq_panel::custom_toggle_switch(
                            ui, enabled, title, theme,
                        ) {
                            changed = true;
                        }
                    });
                });

                ui.add_enabled_ui(*enabled, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);
                    controls(ui);
                });
            });
        });
    changed
}

/// A labelled slider that reports whether it moved.
fn slider(
    ui: &mut egui::Ui,
    label: impl AsRef<str>,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    theme: &Theme,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label.as_ref())
                .font(crate::theme::font(12.0))
                .color(theme.text_secondary),
        );
        let response = ui.add(egui::Slider::new(value, range).suffix(suffix));
        changed = response.changed();
    });
    changed
}

/// The wet and dry levels of an effect.
fn mix_sliders(ui: &mut egui::Ui, wet: &mut f32, dry: &mut f32, theme: &Theme) {
    slider(ui, tr!("Mix"), wet, 0.0..=1.0, "", theme);
    slider(ui, tr!("Dry"), dry, 0.0..=1.0, "", theme);
}

use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Vec2, Window};
use config::EnginePreset;
use crate::theme::Theme;

pub struct AdvancedAudioState {
    pub is_open: bool,
    pub current_preset: EnginePreset,
    pub eq_enabled: bool,
    pub eq_gains_db: [f32; 10], // 31, 63, 125, 250, 500, 1k, 2k, 4k, 8k, 16k
    pub bass_boost_db: f32,
    pub spatial_enabled: bool,
    pub spatial_width: f32,
    pub loudness_norm_enabled: bool,
}

impl Default for AdvancedAudioState {
    fn default() -> Self {
        Self {
            is_open: false,
            current_preset: EnginePreset::Fidelity,
            eq_enabled: false,
            eq_gains_db: [0.0; 10],
            bass_boost_db: 0.0,
            spatial_enabled: false,
            spatial_width: 1.0,
            loudness_norm_enabled: false,
        }
    }
}

pub struct AdvancedAudioActions {
    pub preset_changed: Option<EnginePreset>,
    pub eq_band_changed: Option<(usize, f32)>,
    pub eq_reset: bool,
    pub eq_enabled_changed: Option<bool>,
    pub spatial_enabled_changed: Option<bool>,
    pub spatial_width_changed: Option<f32>,
    pub bass_boost_changed: Option<f32>,
    pub loudness_norm_changed: Option<bool>,
}

pub fn show_advanced_audio_modal(
    ctx: &egui::Context,
    state: &mut AdvancedAudioState,
    bit_perfect: bool,
    bit_perfect_cause_str: Option<&str>,
    theme: &Theme,
) -> AdvancedAudioActions {
    let mut actions = AdvancedAudioActions {
        preset_changed: None,
        eq_band_changed: None,
        eq_reset: false,
        eq_enabled_changed: None,
        spatial_enabled_changed: None,
        spatial_width_changed: None,
        bass_boost_changed: None,
        loudness_norm_changed: None,
    };

    if !state.is_open {
        return actions;
    }

    let mut is_open = state.is_open;

    Window::new("Advanced Audio & DSP Settings")
        .open(&mut is_open)
        .resizable(false)
        .collapsible(false)
        .default_size(Vec2::new(560.0, 480.0))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 14.0);

            // 1. Bit-Perfect Telemetry Banner
            let banner_frame = egui::Frame::NONE
                .fill(theme.bg_pill)
                .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(12));

            banner_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    if bit_perfect {
                        ui.label(
                            egui::RichText::new("• BIT-PERFECT DIRECT STREAM")
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(theme.bit_perfect_green),
                        );
                        ui.label(
                            egui::RichText::new("(DSP bypassed for uncompromised audiophile fidelity)")
                                .font(FontId::proportional(11.5))
                                .color(theme.text_secondary),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("• DSP ACTIVE")
                                .font(FontId::proportional(12.5))
                                .strong()
                                .color(theme.warning_yellow),
                        );
                        let cause = bit_perfect_cause_str.unwrap_or("Processing enabled");
                        ui.label(
                            egui::RichText::new(format!("({})", cause))
                                .font(FontId::proportional(11.5))
                                .color(theme.text_secondary),
                        );
                    }
                });
            });

            // 2. Engine Presets
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new("Audio Engine Preset")
                        .font(FontId::proportional(14.0))
                        .strong()
                        .color(theme.text_primary),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(12.0, 0.0);

                    let presets = [
                        (EnginePreset::Fidelity, "Audiophile Fidelity"),
                        (EnginePreset::Consumer, "Consumer Dynamic"),
                        (EnginePreset::LegacyLowPower, "Low-Power Legacy"),
                    ];

                    for (p, label) in presets {
                        let is_selected = state.current_preset == p;
                        let btn = egui::Button::new(
                            egui::RichText::new(label)
                                .font(FontId::proportional(12.0))
                                .color(if is_selected { Color32::WHITE } else { theme.text_secondary }),
                        )
                        .fill(if is_selected { theme.accent_violet } else { theme.bg_pill })
                        .corner_radius(CornerRadius::same(8));

                        if ui.add(btn).clicked() {
                            state.current_preset = p;
                            actions.preset_changed = Some(p);
                        }
                    }
                });
            });

            ui.separator();

            // 3. 10-Band Graphic Equalizer
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("10-Band Equalizer")
                            .font(FontId::proportional(14.0))
                            .strong()
                            .color(theme.text_primary),
                    );

                    let mut eq_on = state.eq_enabled;
                    if ui.checkbox(&mut eq_on, "Enable").changed() {
                        state.eq_enabled = eq_on;
                        actions.eq_enabled_changed = Some(eq_on);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset Flat").clicked() {
                            state.eq_gains_db = [0.0; 10];
                            actions.eq_reset = true;
                        }
                    });
                });

                ui.add_space(8.0);

                // 10 vertical sliders
                let freqs = ["31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k"];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(14.0, 0.0);

                    for i in 0..10 {
                        ui.vertical_centered(|ui| {
                            let mut val = state.eq_gains_db[i];
                            let slider = egui::Slider::new(&mut val, -12.0..=12.0)
                                .vertical()
                                .show_value(false)
                                .step_by(0.5);

                            if ui.add(slider).changed() {
                                state.eq_gains_db[i] = val;
                                actions.eq_band_changed = Some((i, val));
                            }

                            ui.label(
                                egui::RichText::new(format!("{:+.1}", val))
                                    .font(FontId::proportional(9.5))
                                    .color(theme.text_muted),
                            );

                            ui.label(
                                egui::RichText::new(freqs[i])
                                    .font(FontId::proportional(10.5))
                                    .color(theme.text_secondary),
                            );
                        });
                    }
                });
            });

            ui.separator();

            // 4. Spatial Audio & Bass Boost
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(240.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Spatial Audio")
                                .font(FontId::proportional(13.0))
                                .strong()
                                .color(theme.text_primary),
                        );
                        let mut sp_on = state.spatial_enabled;
                        if ui.checkbox(&mut sp_on, "On").changed() {
                            state.spatial_enabled = sp_on;
                            actions.spatial_enabled_changed = Some(sp_on);
                        }
                    });

                    let mut width = state.spatial_width;
                    if ui.add(egui::Slider::new(&mut width, 0.0..=2.0).text("Width").step_by(0.05)).changed() {
                        state.spatial_width = width;
                        actions.spatial_width_changed = Some(width);
                    }
                });

                ui.add_space(20.0);

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Bass Boost")
                            .font(FontId::proportional(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );

                    let mut bb = state.bass_boost_db;
                    if ui.add(egui::Slider::new(&mut bb, 0.0..=12.0).suffix(" dB").step_by(0.5)).changed() {
                        state.bass_boost_db = bb;
                        actions.bass_boost_changed = Some(bb);
                    }
                });
            });
        });

    state.is_open = is_open;
    actions
}

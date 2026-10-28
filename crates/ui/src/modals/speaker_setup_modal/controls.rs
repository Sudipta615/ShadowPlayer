//! Channel inspector and bass management controls.

use config::{CrossoverFilterType, CrossoverSlope};
use eframe::egui::{self, CornerRadius, Margin, Stroke};

use super::types::{DistanceUnit, SpeakerChannelConfig, SpeakerSetupModalState, SpeakerSizePreset};
use crate::theme::Theme;

/// Selected channel calibration card with distance, delay, and trim controls.
pub fn render_selected_speaker_card(
    ui: &mut egui::Ui,
    speaker: &mut SpeakerChannelConfig,
    unit: DistanceUnit,
    theme: &Theme,
    changed: &mut bool,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} — {} (Ch {})",
                        tr!("Selected Channel"),
                        speaker.role.name(),
                        speaker.channel
                    ))
                    .font(crate::theme::font(12.5))
                    .strong()
                    .color(theme.text_primary),
                );
            });

            ui.columns(3, |cols| {
                // Distance slider
                cols[0].vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Distance"))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    );
                    let mut d = speaker.distance;
                    if ui
                        .add(
                            egui::Slider::new(&mut d, unit.slider_range())
                                .step_by(unit.step() as f64),
                        )
                        .changed()
                    {
                        speaker.distance = d;
                        *changed = true;
                    }
                });

                // Level trim dB
                cols[1].vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Level Trim"))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    );
                    let mut trim = speaker.trim_db;
                    if ui
                        .add(
                            egui::Slider::new(&mut trim, -12.0..=12.0)
                                .step_by(0.1)
                                .suffix(" dB"),
                        )
                        .changed()
                    {
                        speaker.trim_db = trim;
                        *changed = true;
                    }
                });

                // Polarity Inversion
                cols[2].vertical(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Polarity Invert"))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    );
                    let mut inv = speaker.invert;
                    let label = if inv {
                        tr!("Inverted (180°)")
                    } else {
                        tr!("Normal")
                    };
                    if ui.checkbox(&mut inv, label).changed() {
                        speaker.invert = inv;
                        *changed = true;
                    }
                });
            });
        });
}

/// Bass management crossover and subwoofer integration card.
pub fn render_bass_management_card(
    ui: &mut egui::Ui,
    state: &mut SpeakerSetupModalState,
    theme: &Theme,
    changed: &mut bool,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::same(theme.space_md as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Subwoofer Crossover & Bass Management"))
                        .font(crate::theme::font(12.5))
                        .strong()
                        .color(theme.text_primary),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut enabled = state.bass_management_enabled;
                    if ui
                        .checkbox(&mut enabled, tr!("Enable Bass Management"))
                        .changed()
                    {
                        state.bass_management_enabled = enabled;
                        *changed = true;
                    }
                });
            });

            if !state.bass_management_enabled {
                return;
            }

            // Quick speaker size presets
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Quick Size Preset"))
                        .font(crate::theme::font(11.0))
                        .color(theme.text_secondary),
                );
                for (preset, label, freq) in [
                    (
                        SpeakerSizePreset::Floorstander,
                        tr!("Floorstanders (40 Hz)"),
                        40.0,
                    ),
                    (
                        SpeakerSizePreset::Bookshelf,
                        tr!("Bookshelf (80 Hz THX)"),
                        80.0,
                    ),
                    (
                        SpeakerSizePreset::Satellite,
                        tr!("Satellite (120 Hz)"),
                        120.0,
                    ),
                ] {
                    let active =
                        state.speaker_size == preset && (state.crossover_hz - freq).abs() < 1.0;
                    if ui
                        .add(crate::components::settings_tab::pill_btn(
                            label, active, theme,
                        ))
                        .clicked()
                    {
                        state.speaker_size = preset;
                        state.crossover_hz = freq;
                        *changed = true;
                    }
                }
            });

            ui.columns(2, |cols| {
                // Left column: Crossover frequency and slopes
                cols[0].vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Crossover Frequency"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let mut hz = state.crossover_hz;
                        if ui
                            .add(
                                egui::Slider::new(&mut hz, 40.0..=200.0)
                                    .step_by(1.0)
                                    .suffix(" Hz"),
                            )
                            .changed()
                        {
                            state.crossover_hz = hz;
                            state.speaker_size = SpeakerSizePreset::Custom;
                            *changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Filter Slope"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let options = [
                            (CrossoverSlope::Db12, "12 dB/oct"),
                            (CrossoverSlope::Db24, "24 dB/oct"),
                            (CrossoverSlope::Db48, "48 dB/oct"),
                        ];
                        if crate::components::settings_tab::segment_picker(
                            ui,
                            &mut state.slope,
                            &options,
                            theme,
                        ) {
                            *changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Filter Topology"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let options = [
                            (CrossoverFilterType::LinkwitzRiley, "Linkwitz-Riley"),
                            (CrossoverFilterType::Butterworth, "Butterworth"),
                        ];
                        if crate::components::settings_tab::segment_picker(
                            ui,
                            &mut state.filter_type,
                            &options,
                            theme,
                        ) {
                            *changed = true;
                        }
                    });
                });

                // Right column: Mains highpass, Sub phase, polarity, fine delay
                cols[1].vertical(|ui| {
                    let mut hp = state.mains_highpass;
                    if ui
                        .checkbox(&mut hp, tr!("Mains High-Pass"))
                        .on_hover_text(tr!("Roll off mains below crossover"))
                        .changed()
                    {
                        state.mains_highpass = hp;
                        *changed = true;
                    }

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Subwoofer Phase"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let mut phase = state.sub_phase_deg;
                        if ui
                            .add(
                                egui::Slider::new(&mut phase, 0.0..=180.0)
                                    .step_by(1.0)
                                    .suffix("°"),
                            )
                            .changed()
                        {
                            state.sub_phase_deg = phase;
                            *changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Sub Polarity"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let mut inv = state.sub_polarity_invert;
                        if ui.checkbox(&mut inv, tr!("Polarity Invert")).changed() {
                            state.sub_polarity_invert = inv;
                            *changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(tr!("Sub Fine Delay"))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        let mut delay = state.sub_fine_delay_ms;
                        if ui
                            .add(
                                egui::Slider::new(&mut delay, 0.0..=25.0)
                                    .step_by(0.1)
                                    .suffix(" ms"),
                            )
                            .changed()
                        {
                            state.sub_fine_delay_ms = delay;
                            *changed = true;
                        }
                    });
                });
            });
        });
}

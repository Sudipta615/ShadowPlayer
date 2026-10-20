use audio_session::AudioSession;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2, Window};
use engine::dsp::autoeq::{AutoEq, AutoEqParams, AutoEqResult, FrequencyResponse, TargetCurve};

use crate::components::eq_panel::{EqPanelState, ParametricBand};
use crate::theme::Theme;

pub struct AutoEqModelProfile {
    pub brand: &'static str,
    pub model: &'static str,
    pub is_iem: bool,
    pub measurement: &'static [(f32, f32)],
}

/// Curated database of measured audiophile headphones and IEMs
pub fn get_curated_autoeq_profiles() -> &'static [AutoEqModelProfile] {
    static PROFILES: &[AutoEqModelProfile] = &[
        // Sennheiser
        AutoEqModelProfile {
            brand: "Sennheiser",
            model: "HD 600",
            is_iem: false,
            measurement: &[
                (20.0, -8.5),
                (31.5, -6.5),
                (50.0, -4.5),
                (80.0, -2.5),
                (100.0, -1.0),
                (160.0, 0.5),
                (250.0, 0.0),
                (1000.0, 0.0),
                (3000.0, 1.5),
                (4500.0, -1.2),
                (6000.0, -2.0),
                (8000.0, 1.0),
                (10000.0, -1.5),
                (16000.0, -4.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Sennheiser",
            model: "HD 650 / HD 6XX",
            is_iem: false,
            measurement: &[
                (20.0, -9.0),
                (31.5, -6.0),
                (50.0, -3.8),
                (80.0, -1.8),
                (100.0, -0.5),
                (160.0, 1.0),
                (250.0, 0.5),
                (1000.0, 0.0),
                (3000.0, 1.2),
                (4500.0, -1.5),
                (6000.0, -2.5),
                (8000.0, 0.5),
                (10000.0, -2.0),
                (16000.0, -5.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Sennheiser",
            model: "HD 800 S",
            is_iem: false,
            measurement: &[
                (20.0, -6.0),
                (40.0, -4.5),
                (80.0, -2.0),
                (200.0, 0.0),
                (1000.0, 0.0),
                (3000.0, 1.0),
                (5500.0, 4.5),
                (6000.0, 6.0),
                (7000.0, 2.0),
                (10000.0, 0.0),
                (16000.0, -2.0),
            ],
        },
        // Sony
        AutoEqModelProfile {
            brand: "Sony",
            model: "WH-1000XM4",
            is_iem: false,
            measurement: &[
                (20.0, 6.5),
                (40.0, 6.0),
                (80.0, 5.0),
                (150.0, 3.5),
                (300.0, 1.5),
                (1000.0, 0.0),
                (2000.0, -1.0),
                (3500.0, -3.0),
                (6000.0, -1.5),
                (9000.0, 3.0),
                (15000.0, -4.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Sony",
            model: "WH-1000XM5",
            is_iem: false,
            measurement: &[
                (20.0, 5.0),
                (40.0, 4.8),
                (80.0, 4.0),
                (150.0, 2.5),
                (300.0, 1.0),
                (1000.0, 0.0),
                (2500.0, -1.5),
                (4000.0, -2.5),
                (6500.0, 1.0),
                (9500.0, 2.0),
                (16000.0, -3.5),
            ],
        },
        // Beyerdynamic
        AutoEqModelProfile {
            brand: "Beyerdynamic",
            model: "DT 770 Pro (80 Ohm)",
            is_iem: false,
            measurement: &[
                (20.0, 4.0),
                (40.0, 3.5),
                (80.0, 2.0),
                (200.0, -1.0),
                (500.0, -1.5),
                (1000.0, 0.0),
                (2500.0, -0.5),
                (4000.0, 1.5),
                (6000.0, 5.0),
                (8500.0, 7.5),
                (10000.0, 4.0),
                (16000.0, -1.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Beyerdynamic",
            model: "DT 990 Pro (250 Ohm)",
            is_iem: false,
            measurement: &[
                (20.0, -2.0),
                (40.0, 0.0),
                (80.0, 1.5),
                (200.0, -0.5),
                (1000.0, 0.0),
                (3000.0, 1.0),
                (6000.0, 6.0),
                (8500.0, 9.0),
                (10000.0, 5.0),
                (15000.0, 1.0),
            ],
        },
        // Moondrop
        AutoEqModelProfile {
            brand: "Moondrop",
            model: "Aria",
            is_iem: true,
            measurement: &[
                (20.0, 4.5),
                (50.0, 4.2),
                (100.0, 3.8),
                (200.0, 2.0),
                (1000.0, 0.0),
                (2500.0, 6.5),
                (4000.0, 3.0),
                (6000.0, -0.5),
                (8000.0, 2.0),
                (12000.0, -2.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Moondrop",
            model: "Blessing 2 / Dusk",
            is_iem: true,
            measurement: &[
                (20.0, 3.8),
                (50.0, 3.5),
                (100.0, 2.5),
                (200.0, 1.0),
                (1000.0, 0.0),
                (2700.0, 7.0),
                (4500.0, 2.5),
                (6500.0, 0.0),
                (8000.0, 1.5),
                (13000.0, -1.0),
            ],
        },
        // Apple
        AutoEqModelProfile {
            brand: "Apple",
            model: "AirPods Pro 2",
            is_iem: true,
            measurement: &[
                (20.0, 3.5),
                (50.0, 3.5),
                (100.0, 3.0),
                (200.0, 1.5),
                (1000.0, 0.0),
                (2500.0, 5.5),
                (4500.0, 1.0),
                (6000.0, -1.0),
                (8000.0, 0.5),
                (14000.0, -3.0),
            ],
        },
        AutoEqModelProfile {
            brand: "Apple",
            model: "AirPods Max",
            is_iem: false,
            measurement: &[
                (20.0, 4.0),
                (50.0, 3.8),
                (100.0, 2.5),
                (250.0, 1.0),
                (1000.0, 0.0),
                (2500.0, 2.0),
                (4000.0, 1.0),
                (6000.0, -2.0),
                (8000.0, 2.5),
                (11000.0, -3.0),
            ],
        },
        // Hifiman
        AutoEqModelProfile {
            brand: "Hifiman",
            model: "Sundara",
            is_iem: false,
            measurement: &[
                (20.0, -4.5),
                (40.0, -3.0),
                (80.0, -1.0),
                (200.0, 0.0),
                (1000.0, 0.0),
                (2000.0, 1.0),
                (3500.0, -1.5),
                (6000.0, 1.0),
                (9000.0, 2.5),
                (12000.0, -1.0),
            ],
        },
        // Audio-Technica
        AutoEqModelProfile {
            brand: "Audio-Technica",
            model: "ATH-M50x",
            is_iem: false,
            measurement: &[
                (20.0, 3.5),
                (50.0, 3.0),
                (100.0, 2.5),
                (200.0, 1.0),
                (400.0, -1.5),
                (1000.0, 0.0),
                (2500.0, -1.0),
                (4000.0, 1.5),
                (6500.0, -2.5),
                (9000.0, 4.0),
                (15000.0, -3.0),
            ],
        },
        // AKG
        AutoEqModelProfile {
            brand: "AKG",
            model: "K371",
            is_iem: false,
            measurement: &[
                (20.0, 5.0),
                (50.0, 4.8),
                (100.0, 4.0),
                (200.0, 1.5),
                (1000.0, 0.0),
                (2500.0, 4.0),
                (4000.0, 1.0),
                (6000.0, -1.5),
                (8500.0, 0.0),
                (14000.0, -2.0),
            ],
        },
        // Audeze
        AutoEqModelProfile {
            brand: "Audeze",
            model: "LCD-2 Classic",
            is_iem: false,
            measurement: &[
                (20.0, 0.0),
                (50.0, 0.0),
                (100.0, 0.0),
                (500.0, 0.0),
                (1000.0, 0.0),
                (2000.0, -2.5),
                (3500.0, -4.5),
                (5000.0, -3.0),
                (8000.0, 1.5),
                (12000.0, -2.0),
            ],
        },
    ];
    PROFILES
}

#[derive(Default)]
pub struct AutoEqModalState {
    pub is_open: bool,
    pub search_query: String,
    pub selected_index: Option<usize>,
    pub target_curve_idx: usize, // 0 = Harman, 1 = Flat
    pub max_gain_db: f32,
    pub last_result: Option<AutoEqResult>,
}

/// Render the AutoEQ Headphone Profile Selector and Optimizer dialogue
pub fn show_autoeq_modal(
    ctx: &egui::Context,
    state: &mut AutoEqModalState,
    eq_panel_state: &mut EqPanelState,
    session: &AudioSession,
    theme: &Theme,
) {
    if !state.is_open {
        return;
    }

    if state.max_gain_db == 0.0 {
        state.max_gain_db = 12.0;
    }

    let mut is_open = state.is_open;
    let profiles = get_curated_autoeq_profiles();

    Window::new("AutoEQ Headphone & IEM Calibration")
        .open(&mut is_open)
        .resizable(true)
        .collapsible(false)
        .default_size(crate::dialog::fit_to_viewport(ctx, Vec2::new(580.0, 520.0)))
        .min_size(crate::dialog::min_dialog_size())
        .frame(
            egui::Frame::window(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.5_f32, theme.accent_highlight))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(egui::Margin::same(18)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("autoeq_body")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 12.0);

                    ui.label(
                        egui::RichText::new(
                            "Auto-compensate frequency response using the AutoEQ database",
                        )
                        .font(crate::theme::font(12.5))
                        .color(theme.text_secondary),
                    );

                    // Search Bar
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Search:")
                                .font(crate::theme::font(13.0))
                                .color(theme.text_primary),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut state.search_query)
                                .desired_width(ui.available_width() - 80.0),
                        );
                        if ui.button("Clear").clicked() {
                            state.search_query.clear();
                        }
                    });

                    // Target Curve & Gain Clamp Options
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(16.0, 0.0);
                        ui.label(
                            egui::RichText::new("Target Curve:")
                                .font(crate::theme::font(12.0))
                                .color(theme.text_secondary),
                        );

                        let targets = ["Harman 2018", "Flat Reference"];
                        for (idx, label) in targets.iter().enumerate() {
                            let active = state.target_curve_idx == idx;
                            if ui.selectable_label(active, *label).clicked() {
                                state.target_curve_idx = idx;
                            }
                        }

                        ui.label(
                            egui::RichText::new("Max Boost:")
                                .font(crate::theme::font(12.0))
                                .color(theme.text_secondary),
                        );
                        ui.add(
                            egui::Slider::new(&mut state.max_gain_db, 3.0..=15.0)
                                .suffix(" dB")
                                .step_by(1.0),
                        );
                    });

                    ui.separator();

                    // Headphone List & Selection
                    let q = state.search_query.to_lowercase();
                    let filtered_profiles: Vec<(usize, &AutoEqModelProfile)> = profiles
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| {
                            q.is_empty()
                                || p.brand.to_lowercase().contains(&q)
                                || p.model.to_lowercase().contains(&q)
                        })
                        .collect();

                    egui::ScrollArea::vertical()
                        .id_salt("autoeq_list")
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);

                            for (orig_idx, prof) in filtered_profiles {
                                let is_sel = state.selected_index == Some(orig_idx);
                                let kind_badge = if prof.is_iem { "IEM" } else { "Over-Ear" };
                                let display_text =
                                    format!("{} — {} [{}]", prof.brand, prof.model, kind_badge);

                                let btn = egui::Button::new(
                                    egui::RichText::new(display_text)
                                        .font(crate::theme::font(12.5))
                                        .color(if is_sel {
                                            Color32::WHITE
                                        } else {
                                            theme.text_primary
                                        }),
                                )
                                .fill(if is_sel { theme.accent } else { theme.bg_pill })
                                .corner_radius(CornerRadius::same(6))
                                .min_size(Vec2::new(ui.available_width(), 26.0));

                                if ui.add(btn).clicked() {
                                    state.selected_index = Some(orig_idx);

                                    // Trigger AutoEq optimization
                                    let target = if state.target_curve_idx == 0 {
                                        if prof.is_iem {
                                            TargetCurve::HarmanIem2018
                                        } else {
                                            TargetCurve::HarmanHeadphone2018
                                        }
                                    } else {
                                        TargetCurve::Flat
                                    };

                                    let params = AutoEqParams {
                                        target,
                                        max_gain_db: state.max_gain_db,
                                        ..Default::default()
                                    };

                                    let freq_resp =
                                        FrequencyResponse::new(prof.measurement.to_vec());
                                    let result = AutoEq::optimize(
                                        &format!("{} {}", prof.brand, prof.model),
                                        &freq_resp,
                                        &params,
                                    );
                                    state.last_result = Some(result);
                                }
                            }
                        });

                    // Results & Application Section
                    if let Some(ref res) = state.last_result {
                        ui.separator();
                        let prof_name = state
                            .selected_index
                            .map(|idx| format!("{} {}", profiles[idx].brand, profiles[idx].model))
                            .unwrap_or_else(|| "Headphone".into());

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("• Profile Ready: {}", prof_name))
                                    .font(crate::theme::font(13.0))
                                    .strong()
                                    .color(theme.bit_perfect_green),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "Preamp: {:+0.1} dB | Fit RMS Error: {:.2} dB",
                                    res.preamp_db, res.rms_error_db
                                ))
                                .font(crate::theme::font(11.5))
                                .color(theme.text_secondary),
                            );
                        });

                        ui.horizontal(|ui| {
                            let apply_btn = egui::Button::new(
                                egui::RichText::new("Apply Calibration to Audio Engine")
                                    .font(crate::theme::font(13.0))
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(Vec2::new(ui.available_width() - 90.0, 32.0));

                            if ui.add(apply_btn).clicked() {
                                // Apply directly to AudioEngine via AudioSession
                                session.set_eq_preset(res.preset.clone());
                                session.set_preamp(res.preamp_db);
                                session.set_eq_enabled(true);
                                session.set_eq_auto_headroom(true);

                                // Sync to UI state
                                eq_panel_state.eq_enabled = true;
                                eq_panel_state.preamp_db = res.preamp_db;
                                eq_panel_state.auto_headroom = true;

                                for (i, band) in res.preset.bands.iter().take(10).enumerate() {
                                    eq_panel_state.bands[i] = ParametricBand {
                                        freq: band.frequency,
                                        gain_db: band.gain_db,
                                        q: band.q,
                                        filter_type: band.filter_type.into(),
                                        enabled: band.enabled,
                                    };
                                }

                                state.is_open = false;
                            }

                            if ui.button("Close").clicked() {
                                state.is_open = false;
                            }
                        });
                    }
                });
        });

    state.is_open = is_open;
}

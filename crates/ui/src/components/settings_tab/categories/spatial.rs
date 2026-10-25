//! The Spatial 3D category: soundstage radar, HRTF, room acoustics, and the
//! listener's head model.

use super::super::widgets::{
    badge_label, card_header, dual_tone_slider, pill_btn, segment_picker, setting_card,
    setting_item, setting_item_with_info, toggle_switch,
};
use super::super::{SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use config::SpatialQuality;
use eframe::egui::{self, CornerRadius, Stroke, Vec2};

pub(in crate::components::settings_tab) fn render_spatial_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    actions: &mut SettingsTabActions,
) {
    // Master bypass. Every spatial control below writes into the same stage,
    // so without this card the UI could edit HRTF and room settings that the
    // engine was dutifully skipping.
    setting_card(ui, theme, |ui| {
        let status = if state.spatial_enabled {
            tr!("ACTIVE")
        } else {
            tr!("BYPASSED")
        };
        card_header(
            ui,
            crate::components::icons::draw_headphones,
            &tr!("Spatial Audio Master"),
            &tr!("One bypass for HRTF, room, listener and head-tracking settings"),
            Some((
                status.as_str(),
                if state.spatial_enabled {
                    theme.accent
                } else {
                    theme.bg_pill
                },
                if state.spatial_enabled {
                    theme.accent_warm
                } else {
                    theme.text_muted
                },
            )),
            theme,
        );
        setting_item(
            ui,
            &tr!("Enable Spatial Audio"),
            &tr!("Bypass all spatial processing"),
            theme,
            |ui| {
                let mut enabled = state.spatial_enabled;
                if toggle_switch(ui, &mut enabled, theme) {
                    state.spatial_enabled = enabled;
                    actions.spatial_enabled_changed = Some(enabled);
                }
            },
        );
        ui.label(
            egui::RichText::new(if state.spatial_enabled {
                tr!("Spatial rendering is active for the current output.")
            } else {
                tr!(
                    "Spatial audio is bypassed. Enable it to hear HRTF, room and listener settings."
                )
            })
            .font(crate::theme::font(11.0))
            .color(theme.text_secondary),
        );
    });

    let spatial_enabled = state.spatial_enabled;
    ui.add_enabled_ui(spatial_enabled, |ui| {
        // Card 1: 3D Soundstage Radar Canvas
        setting_card(ui, theme, |ui| {
            card_header(
                ui,
                crate::components::icons::draw_bar_chart,
                &tr!("3D Soundstage & Spatial Radar (Interactive)"),
                &tr!("Drag the virtual sound source across the circular acoustic field to position audio in 3D space"),
                None,
                theme,
            );

            ui.vertical_centered(|ui| {
                // The drag moves the **virtual centre** — that is the one spatial
                // parameter the engine actually exposes (`set_spatial_screen`).
                // This used to also write a separate "source azimuth" and then copy
                // it into the centre on every frame, so the two readouts could
                // never differ.
                let radar_resp = crate::widgets::SpatialRadarWidget::new(
                    &mut state.screen_center_az,
                    state.listener_yaw_deg,
                    theme,
                )
                .room_dimensions(state.room_width, state.room_depth)
                .size(Vec2::new(ui.available_width().min(480.0), 240.0))
                .show(ui);

                if radar_resp.changed {
                    actions.spatial_screen_changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
                badge_label(
                    ui,
                    format!("Virtual Center: {:+.1}°", state.screen_center_az),
                    theme,
                );
                badge_label(
                    ui,
                    format!("Listener Facing: {:+.1}°", state.listener_yaw_deg),
                    theme,
                );
            });

            setting_item_with_info(
                ui,
                &tr!("Listener Orientation"),
                &tr!("Rotates the listener's head independently of the virtual centre, so the soundstage can be auditioned from an angle"),
                None,
                theme,
                |ui| {
                    let mut yaw = state.listener_yaw_deg;
                    if dual_tone_slider(ui, &mut yaw, -180.0..=180.0, 1.0, 0.0, theme) {
                        state.listener_yaw_deg = yaw;
                        actions.spatial_listener_changed = true;
                    }
                },
            );

            setting_item_with_info(
                ui,
                &tr!("Air Absorption"),
                &tr!("Distance-dependent high-frequency roll-off along each path, as air itself absorbs treble over distance"),
                None,
                theme,
                |ui| {
                    let mut enabled = state.air_absorption_enabled;
                    if toggle_switch(ui, &mut enabled, theme) {
                        state.air_absorption_enabled = enabled;
                        actions.air_absorption_changed = Some(enabled);
                    }
                },
            );
        });

        // Card 2: HRTF & Precision
        setting_card(ui, theme, |ui| {
            card_header(
                ui,
                crate::components::icons::draw_disc,
                &tr!("Binaural HRTF & Head Tracking"),
                &tr!("Head-Related Transfer Function filters model human pinna and ear-canal acoustics for headphones"),
                None,
                theme,
            );

            setting_item_with_info(
                ui,
                &tr!("Spatial Synthesis Quality"),
                &tr!("Spherical harmonics order and convolution precision"),
                Some(&crate::components::info_box::INFO_SPATIAL_HRTF),
                theme,
                |ui| {
                    let options = [
                        (SpatialQuality::Low, "Low"),
                        (SpatialQuality::Medium, "Medium"),
                        (SpatialQuality::High, "High (Recommended)"),
                        (SpatialQuality::Ultra, "Ultra High-Res"),
                    ];
                    if segment_picker(ui, &mut state.spatial_quality, &options, theme) {
                        actions.spatial_quality_changed = Some(state.spatial_quality);
                    }
                },
            );

            setting_item(
                ui,
                &tr!("HRTF Profile ID"),
                &tr!("SOFA dataset profile identifier (e.g. KEMAR_diffuse, IRC_1002)"),
                theme,
                |ui| {
                    let edit = egui::TextEdit::singleline(&mut state.hrtf_profile)
                        .hint_text(tr!(Msg::SE0F13CACC590D830))
                        .desired_width(180.0)
                        .font(crate::theme::font(12.0));
                    ui.add(edit);
                    if ui
                        .add(pill_btn(tr!(Msg::S7CB1A12491596FB2), false, theme))
                        .clicked()
                    {
                        actions.hrtf_profile_changed = Some(state.hrtf_profile.clone());
                    }
                },
            );

            setting_item_with_info(
                ui,
                &tr!("Listener Motion Smoothing"),
                &tr!("Low-pass filter time constant for head motion tracker interpolation"),
                Some(&crate::components::info_box::INFO_LISTENER_TRACKING),
                theme,
                |ui| {
                    let mut v = state.listener_smoothing_ms;
                    if dual_tone_slider(ui, &mut v, 0.0..=500.0, 5.0, 0.0, theme) {
                        state.listener_smoothing_ms = v;
                        actions.listener_tracking_changed = true;
                    }
                    badge_label(ui, format!("{:.0} ms", state.listener_smoothing_ms), theme);
                },
            );

            setting_item(
                ui,
                &tr!("Listener Max Angular Velocity"),
                &tr!("Clamps extreme head rotations (0 = uncapped natural tracking)"),
                theme,
                |ui| {
                    let mut r = state.listener_max_rate;
                    if dual_tone_slider(ui, &mut r, 0.0..=720.0, 10.0, 0.0, theme) {
                        state.listener_max_rate = r;
                        actions.listener_tracking_changed = true;
                    }
                    badge_label(ui, format!("{:.0} °/s", state.listener_max_rate), theme);
                },
            );
        });

        // Card 3: Virtual Room Acoustics
        setting_card(ui, theme, |ui| {
            card_header(
                ui,
                crate::components::icons::draw_folder,
                &tr!("Virtual Room Acoustics Simulation"),
                &tr!("Simulates listening room wall reflections, RT60 reverberation decay, and air absorption"),
                Some((
                    if state.room_enabled { "REVERB ACTIVE" } else { "BYPASSED" },
                    theme.bg_pill,
                    if state.room_enabled { theme.accent } else { theme.text_muted },
                )),
                theme,
            );

            setting_item_with_info(
                ui,
                &tr!("Enable Room Acoustics"),
                &tr!("Apply virtual listening room reverberation model"),
                Some(&crate::components::info_box::INFO_ROOM_ACOUSTICS),
                theme,
                |ui| {
                    let mut on = state.room_enabled;
                    if toggle_switch(ui, &mut on, theme) {
                        state.room_enabled = on;
                        actions.spatial_room_changed = true;
                    }
                },
            );

            if state.room_enabled {
                ui.add_space(4.0);
                let room_grid = egui::Frame::NONE
                    .fill(theme.bg_pill.linear_multiply(0.6))
                    .stroke(Stroke::new(1.0, theme.border_card))
                    .corner_radius(CornerRadius::same(theme.radius_lg))
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_2xl as i8,
                        theme.space_xl as i8,
                    ));

                room_grid.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

                    setting_item(
                        ui,
                        &tr!("Reverb Wet/Dry Mix"),
                        &tr!("Ratio of reverberant room reflection to direct dry audio"),
                        theme,
                        |ui| {
                            let mut v = state.room_wet;
                            if dual_tone_slider(ui, &mut v, 0.0..=1.0, 0.01, 0.0, theme) {
                                state.room_wet = v;
                                actions.spatial_room_changed = true;
                            }
                            badge_label(ui, format!("{:.0}%", state.room_wet * 100.0), theme);
                        },
                    );

                    setting_item(
                        ui,
                        &tr!("RT60 Decay Time"),
                        &tr!("Time required for acoustic reflections to decay by 60 dB"),
                        theme,
                        |ui| {
                            let mut v = state.room_rt60_ms;
                            if dual_tone_slider(ui, &mut v, 50.0..=3000.0, 25.0, 0.0, theme) {
                                state.room_rt60_ms = v;
                                actions.spatial_room_changed = true;
                            }
                            badge_label(ui, format!("{:.0} ms", state.room_rt60_ms), theme);
                        },
                    );

                    setting_item(
                        ui,
                        &tr!("Room Dimensions (W x D x H)"),
                        &tr!("Physical space modeling in meters"),
                        theme,
                        |ui| {
                            ui.label(tr!(Msg::S09380C07B5C97BE4));
                            let mut w = state.room_width;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut w)
                                        .speed(0.1)
                                        .range(1.0..=30.0)
                                        .min_size(Vec2::new(76.0, 24.0)),
                                )
                                .changed()
                            {
                                state.room_width = w;
                                actions.spatial_room_changed = true;
                            }
                            ui.label(tr!(Msg::S08FE8607B598D2EB));
                            let mut d = state.room_depth;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut d)
                                        .speed(0.1)
                                        .range(1.0..=30.0)
                                        .min_size(Vec2::new(76.0, 24.0)),
                                )
                                .changed()
                            {
                                state.room_depth = d;
                                actions.spatial_room_changed = true;
                            }
                            ui.label(tr!(Msg::S09270E07B5BB0C17));
                            let mut h = state.room_height;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut h)
                                        .speed(0.1)
                                        .range(1.5..=10.0)
                                        .min_size(Vec2::new(76.0, 24.0)),
                                )
                                .changed()
                            {
                                state.room_height = h;
                                actions.spatial_room_changed = true;
                            }
                        },
                    );
                });
            }
        });
    });
}

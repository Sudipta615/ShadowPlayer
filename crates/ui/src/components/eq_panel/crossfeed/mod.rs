//! Headphone spatialization, calibrated crossfeed presets, and psychoacoustic
//! soundstage visualization.
//!
//! Models interaural time difference (ITD) and head-shadow interaural level
//! difference (ILD) to eliminate super-stereo listening fatigue on headphones.

mod geometry;
mod spectrum;
mod telemetry;
mod types;

#[cfg(test)]
mod tests;

pub use geometry::render_soundstage_geometry_canvas;
pub use spectrum::render_transfer_spectrum_canvas;
pub use telemetry::{card, render_telemetry_strip, slider_row};
pub use types::{
    biquad_crossfeed_gain_db, calculate_acoustic_path_cm, calculate_fatigue_reduction_pct,
    calculate_itd_delay_us, CalibratedCrossfeedPreset, CrossfeedViewMode, SPEED_OF_SOUND_CM_PER_MS,
};

use super::controls::custom_toggle_switch;
use super::{EqPanelActions, EqPanelState};
use crate::theme::Theme;
use config::CrossfeedProfile;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

/// Render the complete Headphone Crossfeed Studio card.
pub fn render_crossfeed_card(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    card(ui, theme, |ui| {
        // ── Card Header ────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
            ui.label(
                egui::RichText::new(tr!("Crossfeed"))
                    .font(crate::theme::font(14.0))
                    .strong()
                    .color(theme.text_primary),
            );
            crate::components::info_box::render_info_icon_button(
                ui,
                "crossfeed",
                &crate::components::info_box::INFO_CROSSFEED,
                theme,
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if custom_toggle_switch(
                    ui,
                    &mut state.crossfeed_enabled,
                    &tr!("Enable Crossfeed"),
                    theme,
                ) {
                    actions.crossfeed_enabled_changed = Some(state.crossfeed_enabled);
                }
                let status_text = if state.crossfeed_enabled {
                    tr!("Crossfeed Active")
                } else {
                    tr!("Direct Stereo (Bypassed)")
                };
                let status_color = if state.crossfeed_enabled {
                    theme.bit_perfect_green
                } else {
                    theme.text_muted
                };
                ui.label(
                    egui::RichText::new(status_text)
                        .font(crate::theme::font(11.0))
                        .color(status_color),
                );
            });
        });

        ui.label(
            egui::RichText::new(tr!(
                "Models the interaural delay and shadowing that headphones remove."
            ))
            .font(crate::theme::font(11.0))
            .color(theme.text_secondary),
        );

        // ── View Mode Switcher ─────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
            for (mode, label) in [
                (
                    CrossfeedViewMode::SoundstageGeometry,
                    tr!("Soundstage Geometry (60°)"),
                ),
                (
                    CrossfeedViewMode::TransferSpectrum,
                    tr!("Acoustic Response (ITD/ILD)"),
                ),
            ] {
                let selected = state.crossfeed_view_mode == mode;
                let btn = egui::Button::new(
                    egui::RichText::new(label)
                        .font(crate::theme::font(10.5))
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme.text_secondary
                        }),
                )
                .fill(if selected {
                    theme.accent
                } else {
                    theme.bg_pill
                })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm));

                if ui.add(btn).clicked() && !selected {
                    state.crossfeed_view_mode = mode;
                }
            }
        });

        // ── Visual Canvas ──────────────────────────────────────────────────
        match state.crossfeed_view_mode {
            CrossfeedViewMode::SoundstageGeometry => {
                render_soundstage_geometry_canvas(ui, state, theme);
            }
            CrossfeedViewMode::TransferSpectrum => {
                render_transfer_spectrum_canvas(ui, state, theme);
            }
        }

        // ── Psychoacoustic Telemetry Strip ─────────────────────────────────
        render_telemetry_strip(ui, state, theme);

        ui.add_space(theme.space_xxs);

        // ── Calibrated Presets Strip ───────────────────────────────────────
        let active_preset = CalibratedCrossfeedPreset::detect(
            state.crossfeed_profile,
            state.crossfeed_freq,
            state.crossfeed_q,
            state.crossfeed_delay_ms,
        );

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xs);
            for preset in CalibratedCrossfeedPreset::ALL {
                let selected = active_preset == preset;
                let btn = egui::Button::new(
                    egui::RichText::new(preset.label())
                        .font(crate::theme::font(10.5))
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme.text_secondary
                        }),
                )
                .fill(if selected {
                    theme.accent
                } else {
                    theme.bg_pill
                })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm));

                if ui.add(btn).clicked() && !selected {
                    let (freq, q, delay) = preset.params();
                    state.crossfeed_profile = preset.engine_profile();
                    state.crossfeed_freq = freq;
                    state.crossfeed_q = q;
                    state.crossfeed_delay_ms = delay;

                    actions.crossfeed_profile_changed = Some(state.crossfeed_profile);
                    if state.crossfeed_profile == CrossfeedProfile::Custom {
                        actions.crossfeed_custom_changed = true;
                    }
                }
            }
        });

        // Preset explanation label
        ui.label(
            egui::RichText::new(active_preset.description())
                .font(crate::theme::font(10.5))
                .italics()
                .color(theme.text_muted),
        );

        // ── Parameter Tuning Sliders ───────────────────────────────────────
        ui.add_space(theme.space_xs);
        slider_row(
            ui,
            &tr!("Cutoff"),
            &mut state.crossfeed_freq,
            300.0..=1500.0,
            700.0,
            |v| format!("{v:.0} Hz"),
            theme,
            |_| {
                state.crossfeed_profile = CrossfeedProfile::Custom;
                actions.crossfeed_profile_changed = Some(CrossfeedProfile::Custom);
                actions.crossfeed_custom_changed = true;
            },
        );
        slider_row(
            ui,
            &tr!("Q"),
            &mut state.crossfeed_q,
            0.2..=2.0,
            0.707,
            |v| format!("{v:.2}"),
            theme,
            |_| {
                state.crossfeed_profile = CrossfeedProfile::Custom;
                actions.crossfeed_profile_changed = Some(CrossfeedProfile::Custom);
                actions.crossfeed_custom_changed = true;
            },
        );
        slider_row(
            ui,
            &tr!("Delay"),
            &mut state.crossfeed_delay_ms,
            0.05..=0.80,
            0.30,
            |v| format!("{:.0} µs ({:.2} ms)", v * 1000.0, v),
            theme,
            |_| {
                state.crossfeed_profile = CrossfeedProfile::Custom;
                actions.crossfeed_profile_changed = Some(CrossfeedProfile::Custom);
                actions.crossfeed_custom_changed = true;
            },
        );
    });
}

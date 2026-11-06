//! The Quick Tune acoustic dashboard for fast, satisfying listening adjustments.
//!
//! Designed for everyday listening and audiophiles who want to quickly shape their
//! soundstage without diving into 64 individual faders or multiband compressor knees.

use super::controls::render_footer_controls;
use super::presets::{apply_active_preset, EqPreset};
use super::{EqPanelActions, EqPanelState, EqTab, EqViewMode};
use crate::theme::Theme;
use config::CrossfeedProfile;
use eframe::egui::{self, CornerRadius, Margin, Stroke, Vec2};

pub(super) fn render_quick_tune_view(
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
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

        // ── 1. Quick Presets Strip ───────────────────────────────────────────
        super::controls::card(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);
                ui.label(
                    egui::RichText::new("Acoustic Profile:")
                        .font(crate::theme::font(11.5))
                        .strong()
                        .color(theme.text_secondary),
                );

                let quick_presets = [
                    (EqPreset::Flat, "Flat / Neutral"),
                    (EqPreset::Jazz, "Jazz & Acoustic"),
                    (EqPreset::BassBoost, "Bass Boost"),
                    (EqPreset::VocalBoost, "Vocal Presence"),
                    (EqPreset::Rock, "Rock & Pop"),
                    (EqPreset::Electronic, "Electronic / EDM"),
                    (EqPreset::Classical, "Classical"),
                ];

                for (preset, label) in quick_presets {
                    let is_active = state.selected_preset == preset
                        && state.selected_custom_preset_idx.is_none();
                    if super::controls::choice_pill(ui, label, is_active, theme.accent_warm, theme)
                        .clicked()
                        && !is_active
                    {
                        state.push_undo();
                        state.selected_preset = preset;
                        state.selected_custom_preset_idx = None;
                        apply_active_preset(state, actions);
                    }
                }
            });
        });

        // ── 2. Primary Acoustic Tone Sliders (Bass & Treble) ─────────────────
        let tone_max = if state.extended_range { 24.0 } else { 12.0 };
        let tone_range = -tone_max..=tone_max;
        let min_lbl = if state.extended_range { "-24dB" } else { "-12dB" };
        let max_lbl = if state.extended_range { "+24dB" } else { "+12dB" };

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Bass Punch Shelf (80-100 Hz)
            super::advanced::render_slider_card(
                ui,
                card_w,
                "Bass Punch & Depth",
                &crate::components::info_box::INFO_BASS_SHELF,
                &mut state.bass_shelf_db,
                tone_range.clone(),
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.bass_shelf_changed = Some(v),
                theme,
            );

            // Treble Air Shelf (7.5-10 kHz)
            if two_col {
                super::advanced::render_slider_card(
                    ui,
                    card_w,
                    "Treble Clarity & Air",
                    &crate::components::info_box::INFO_TREBLE_SHELF,
                    &mut state.treble_shelf_db,
                    tone_range.clone(),
                    0.0,
                    min_lbl,
                    "0dB",
                    max_lbl,
                    |v| format!("{:+.1} dB", v),
                    |v| actions.treble_shelf_changed = Some(v),
                    theme,
                );
            }
        });

        if !two_col {
            super::advanced::render_slider_card(
                ui,
                card_w,
                "Treble Clarity & Air",
                &crate::components::info_box::INFO_TREBLE_SHELF,
                &mut state.treble_shelf_db,
                tone_range,
                0.0,
                min_lbl,
                "0dB",
                max_lbl,
                |v| format!("{:+.1} dB", v),
                |v| actions.treble_shelf_changed = Some(v),
                theme,
            );
        }

        // ── 3. Spatial Acoustics (Soundstage Width & Headphone Crossfeed) ────
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Soundstage Width Slider
            super::advanced::render_slider_card(
                ui,
                card_w,
                "Soundstage Width",
                &crate::components::info_box::INFO_STEREO_WIDTH,
                &mut state.stereo_width,
                0.0..=2.0,
                1.0,
                "Mono (0%)",
                "Natural (100%)",
                "Wide (200%)",
                |v| format!("{:.0}%", v * 100.0),
                |v| actions.stereo_width_changed = Some(v),
                theme,
            );

            // Headphone Crossfeed Quick Card
            if two_col {
                render_crossfeed_quick_card(ui, card_w, state, actions, theme);
            }
        });

        if !two_col {
            render_crossfeed_quick_card(ui, card_w, state, actions, theme);
        }

        // ── 4. Switch to Studio Pro Banner ───────────────────────────────────
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.border_card))
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(Margin::symmetric(
                theme.space_xl as i8,
                theme.space_sm as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Need 64-band control, AutoEQ targets, multiband dynamics, or creative effects?")
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new("Open Studio Pro Mode →")
                                    .font(crate::theme::font(11.5))
                                    .color(theme.accent_highlight),
                            )
                            .clicked()
                        {
                            state.view_mode = EqViewMode::StudioPro;
                        }
                    });
                });
            });

        // ── 5. Shared Preamp Footer ──────────────────────────────────────────
        render_footer_controls(ui, state, actions, theme, EqTab::Tone);
    });
}

fn render_crossfeed_quick_card(
    ui: &mut egui::Ui,
    width: f32,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let mut cf_active = state.crossfeed_enabled;
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(
            1.0,
            if cf_active {
                theme.accent.linear_multiply(0.4)
            } else {
                theme.border_card
            },
        ))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_2xl as i8,
            theme.space_lg as i8,
        ))
        .show(ui, |ui| {
            ui.set_width((width - 28.0).max(0.0));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new("Headphone Crossfeed")
                            .font(crate::theme::font(13.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    crate::components::info_box::render_info_icon_button(
                        ui,
                        "quick_crossfeed",
                        &crate::components::info_box::INFO_CROSSFEED,
                        theme,
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::controls::custom_toggle_switch(
                            ui,
                            &mut cf_active,
                            "Quick Crossfeed",
                            theme,
                        ) {
                            state.crossfeed_enabled = cf_active;
                            actions.crossfeed_enabled_changed = Some(cf_active);
                        }
                    });
                });

                ui.label(
                    egui::RichText::new(
                        "Simulates acoustic speaker room cross-bleed to eliminate headphone fatigue.",
                    )
                    .font(crate::theme::font(10.5))
                    .color(theme.text_muted),
                );

                ui.add_space(2.0);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, theme.space_xxs);
                    let profiles = [
                        (CrossfeedProfile::Bauer, "Bauer"),
                        (CrossfeedProfile::ChuMoy, "Chu Moy"),
                        (CrossfeedProfile::Custom, "Custom"),
                    ];
                    for (prof, lbl) in profiles {
                        let is_active = state.crossfeed_profile == prof;
                        if super::controls::choice_pill(ui, lbl, is_active, theme.accent_highlight, theme)
                            .clicked()
                            && !is_active
                        {
                            state.crossfeed_profile = prof;
                            actions.crossfeed_profile_changed = Some(prof);
                        }
                    }
                });
            });
        });
}

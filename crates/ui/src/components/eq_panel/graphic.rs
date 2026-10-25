//! The Graphic Equalizer tab: band-count selector, preset picker, save box,
//! response canvas, and the shared footer.

use super::controls::render_footer_controls;
use super::curve::render_spline_eq_canvas;
use super::presets::{apply_active_preset, CustomEqPreset, EqPreset};
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use config::{EqMode, GraphicEqLayout};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// 4. Tab 1: Graphic Equalizer (Bands, Canvas, Spline, Presets, Save)
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn render_graphic_eq_tab(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    // ── Curve source: exactly one of Graphic / Parametric may own the stage ──
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
        ui.label(
            egui::RichText::new(tr!("Equalizer mode"))
                .font(crate::theme::font(11.0))
                .color(theme.text_muted),
        );
        for (mode, label) in [
            (EqMode::Graphic, tr!("Graphic")),
            (EqMode::Parametric, tr!("Parametric / AutoEQ")),
        ] {
            let selected = state.eq_mode == mode;
            let button = egui::Button::new(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.0))
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
            if ui.add(button).clicked() && !selected {
                state.set_eq_mode(mode);
                actions.eq_mode_changed = true;
            }
        }
    });

    if state.eq_mode == EqMode::Parametric {
        render_parametric_curve_summary(ui, state, actions, theme);
        render_footer_controls(ui, state, actions, theme, EqTab::Parametric);
        return;
    }

    // ── Sub-bar: Band Count Selector (Left) & Preset Dropdown + Save Button (Right) ─────────
    ui.horizontal(|ui| {
        // 1. Left side: Band Count Buttons (Secondary filter chips: visually distinct from main tabs)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            let band_options = [
                ("10 Bands", GraphicEqLayout::TenBand),
                ("15 Bands", GraphicEqLayout::FifteenBand),
                ("31 Bands", GraphicEqLayout::ThirtyOneBand),
                ("64 Bands", GraphicEqLayout::SixtyFourBand),
            ];

            for (lbl, layout) in band_options {
                let is_active = matches!(
                    (&state.graphic_layout, &layout),
                    (GraphicEqLayout::TenBand, GraphicEqLayout::TenBand)
                        | (GraphicEqLayout::FifteenBand, GraphicEqLayout::FifteenBand)
                        | (
                            GraphicEqLayout::ThirtyOneBand,
                            GraphicEqLayout::ThirtyOneBand
                        )
                        | (
                            GraphicEqLayout::SixtyFourBand,
                            GraphicEqLayout::SixtyFourBand
                        )
                );

                let (bg, text_col, stroke) = if is_active {
                    (theme.accent_warm, Color32::WHITE, Stroke::NONE)
                } else {
                    (
                        theme.bg_pill,
                        theme.text_secondary,
                        Stroke::new(1.0, theme.border_card),
                    )
                };

                let btn = egui::Button::new(
                    egui::RichText::new(lbl)
                        .font(crate::theme::font(11.0))
                        .strong()
                        .color(text_col),
                )
                .fill(bg)
                .stroke(stroke)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(60.0, 24.0));

                if ui.add(btn).clicked() && !is_active {
                    // Preserve the curve the user is looking at. Re-applying
                    // the selected factory preset here used to flatten every
                    // hand-edited ("Custom") curve on a band-count change.
                    state.set_graphic_layout(layout);
                    actions.graphic_eq_changed = true;
                }
            }

            ui.add_space(2.0);
            crate::components::info_box::render_info_icon_button(
                ui,
                "eq_graphic_band_topology",
                &crate::components::info_box::INFO_GRAPHIC_EQ_BANDS,
                theme,
            );
        });

        // 2. Right side: Preset Dropdown & Save Button (No "Preset:" label, moved to right, UI color)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            // Save Preset Button
            let save_btn = egui::Button::new(
                egui::RichText::new(tr!("Save"))
                    .font(crate::theme::font(11.5))
                    .color(theme.text_primary),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(44.0, 24.0));

            if ui
                .add(save_btn)
                .on_hover_text(tr!(Msg::SF9C7F2EDD78EA787))
                .clicked()
            {
                state.is_saving_preset = !state.is_saving_preset;
                if state.is_saving_preset && state.new_preset_name.is_empty() {
                    state.new_preset_name = format!("Preset {}", state.custom_presets.len() + 1);
                }
            }

            // Delete Custom Preset Button (if custom preset active)
            if let Some(idx) = state.selected_custom_preset_idx {
                let del_btn = egui::Button::new(
                    egui::RichText::new(tr!("Delete"))
                        .font(crate::theme::font(11.0))
                        .color(theme.destructive),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(44.0, 24.0));

                if ui
                    .add(del_btn)
                    .on_hover_text(tr!(Msg::S2821880D30CCB161))
                    .clicked()
                    && idx < state.custom_presets.len()
                {
                    state.custom_presets.remove(idx);
                    state.selected_custom_preset_idx = None;
                    state.selected_preset = EqPreset::Flat;
                    apply_active_preset(state, actions);
                }
            }

            // Preset Dropdown ComboBox (with UI colors applied via scope)
            let current_preset_name = if let Some(idx) = state.selected_custom_preset_idx {
                state
                    .custom_presets
                    .get(idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Custom".to_string())
            } else {
                state.selected_preset.name().to_string()
            };

            ui.scope(|ui| {
                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.weak_bg_fill = theme.bg_pill;
                visuals.widgets.inactive.bg_fill = theme.bg_pill;
                visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                visuals.widgets.inactive.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.widgets.hovered.weak_bg_fill = theme.bg_card_hover;
                visuals.widgets.hovered.bg_fill = theme.bg_card_hover;
                visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                visuals.widgets.hovered.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.widgets.open.weak_bg_fill = theme.bg_card_hover;
                visuals.widgets.open.bg_fill = theme.bg_card_hover;
                visuals.widgets.open.bg_stroke = Stroke::NONE;
                visuals.widgets.open.corner_radius = CornerRadius::same(theme.radius_sm);

                visuals.window_fill = theme.bg_card;
                visuals.window_stroke = Stroke::new(1.0, theme.border_card);
                visuals.menu_corner_radius = CornerRadius::same(theme.radius_md);

                egui::ComboBox::from_id_salt("eq_presets_combo")
                    .selected_text(
                        egui::RichText::new(current_preset_name)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_primary),
                    )
                    .width(115.0)
                    .show_ui(ui, |ui| {
                        ui.label(
                            egui::RichText::new(tr!("Factory Presets"))
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                        for preset in EqPreset::ALL {
                            if preset == EqPreset::Custom {
                                continue;
                            }
                            let is_sel = state.selected_preset == preset
                                && state.selected_custom_preset_idx.is_none();
                            if ui.selectable_label(is_sel, preset.name()).clicked() {
                                state.selected_preset = preset;
                                state.selected_custom_preset_idx = None;
                                apply_active_preset(state, actions);
                            }
                        }

                        if !state.custom_presets.is_empty() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(tr!("Saved Presets"))
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_muted),
                            );
                            for idx in 0..state.custom_presets.len() {
                                let is_sel = state.selected_custom_preset_idx == Some(idx);
                                let clicked = {
                                    let cp = &state.custom_presets[idx];
                                    ui.selectable_label(is_sel, &cp.name).clicked()
                                };
                                if clicked {
                                    state.selected_preset = EqPreset::Custom;
                                    state.selected_custom_preset_idx = Some(idx);
                                    apply_active_preset(state, actions);
                                }
                            }
                        }
                    });
            });

            crate::components::info_box::render_info_icon_button(
                ui,
                "eq_preset_curves",
                &crate::components::info_box::INFO_EQ_PRESETS,
                theme,
            );
        });
    });

    // ── Inline Save Box (when Save toggled) ──────────────────────────────────
    if state.is_saving_preset {
        egui::Frame::NONE
            .fill(theme.bg_card_hover)
            .stroke(Stroke::new(1.0, theme.accent_highlight))
            .corner_radius(CornerRadius::same(theme.radius_md))
            .inner_margin(Margin::symmetric(
                theme.space_xl as i8,
                theme.space_sm as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);
                    ui.label(
                        egui::RichText::new(tr!("Preset Name:"))
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_preset_name).desired_width(160.0),
                    );

                    if ui
                        .button(egui::RichText::new(tr!("Confirm Save")).color(Color32::WHITE))
                        .clicked()
                    {
                        let trimmed = state.new_preset_name.trim();
                        if !trimmed.is_empty() {
                            let num = state.graphic_layout.num_bands();
                            let gains = state.graphic_gains[0..num].to_vec();
                            state.custom_presets.push(CustomEqPreset {
                                name: trimmed.to_string(),
                                gains,
                                preamp_db: state.preamp_db,
                                layout: Some(state.graphic_layout.clone()),
                            });
                            state.selected_custom_preset_idx = Some(state.custom_presets.len() - 1);
                            state.selected_preset = EqPreset::Custom;
                            state.new_preset_name.clear();
                            state.is_saving_preset = false;
                        }
                    }

                    if ui.button(tr!(Msg::S2CEB11BE2290BB1B)).clicked() {
                        state.is_saving_preset = false;
                    }
                });
            });
    }

    // ── Spline Canvas with Vertical Stems & Interactive Curve ────────────────
    render_spline_eq_canvas(ui, state, actions, theme);

    // ── Bottom Controls: Preamp + Tab-Specific Reset ─────────────────────────
    render_footer_controls(ui, state, actions, theme, EqTab::Graphic);
}

/// Parametric mode owns the same stage as the graphic sliders, so showing the
/// disabled graphic canvas here would invite edits that cannot be heard.
fn render_parametric_curve_summary(
    ui: &mut egui::Ui,
    state: &EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(theme.space_3xl as i8, theme.space_2xl as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                ui.label(
                    egui::RichText::new(tr!("Parametric / AutoEQ"))
                        .font(crate::theme::font(14.0))
                        .strong()
                        .color(theme.text_primary),
                );
                ui.label(
                    egui::RichText::new(tr!("The EQ stage is driven by parametric bands. Generate or replace the curve in the AutoEQ calibration dialog."))
                        .font(crate::theme::font(11.5))
                        .color(theme.text_secondary),
                );
                ui.label(
                    egui::RichText::new(tr_args!(
                        "{count} bands in the active parametric curve",
                        "count" => state.bands.len()
                    ))
                    .font(crate::theme::font(10.5))
                    .color(theme.text_muted),
                );
                ui.add_space(theme.space_sm);
                let button = egui::Button::new(
                    egui::RichText::new(tr!("Open AutoEQ"))
                        .font(crate::theme::font(12.0))
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_md))
                .min_size(Vec2::new(120.0, 30.0));
                if ui.add(button).clicked() {
                    actions.open_autoeq_dialog = true;
                }
            });
        });
}

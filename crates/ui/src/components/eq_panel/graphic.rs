//! The Graphic Equalizer tab: band-count selector, preset picker, save box,
//! response canvas, and the shared footer.

use super::controls::render_footer_controls;
use super::curve::render_spline_eq_canvas;
use super::presets::{apply_active_preset, CustomEqPreset, EqPreset};
use super::{EqPanelActions, EqPanelState, EqTab};
use crate::theme::Theme;
use config::GraphicEqLayout;
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
    // ── Band topology ────────────────────────────────────────────────────────
    // Wrapping keeps four chips and their help affordance usable at a narrow
    // window width instead of imposing a minimum width on the panel.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);

        let band_options = [
            ("10 Bands", GraphicEqLayout::TenBand),
            ("15 Bands", GraphicEqLayout::FifteenBand),
            ("31 Bands", GraphicEqLayout::ThirtyOneBand),
            ("64 Bands", GraphicEqLayout::SixtyFourBand),
        ];

        for (lbl, layout) in band_options {
            let is_active = state.graphic_layout == layout;

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
            .corner_radius(theme.control_radius())
            .min_size(Vec2::new(60.0, 24.0));

            if ui.add(btn).clicked() && !is_active {
                // Preserve the curve the user is looking at. Re-applying
                // the selected factory preset here used to flatten every
                // hand-edited ("Custom") curve on a band-count change.
                state.set_graphic_layout(layout);
                actions.graphic_eq_changed = true;
            }
        }

        crate::components::info_box::render_info_icon_button(
            ui,
            "eq_graphic_band_topology",
            &crate::components::info_box::INFO_GRAPHIC_EQ_BANDS,
            theme,
        );
    });

    // ── Presets, save, and curve import/export ───────────────────────────────
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_xs);

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
            visuals.widgets.inactive.corner_radius = theme.control_radius();

            visuals.widgets.hovered.weak_bg_fill = theme.bg_card_hover;
            visuals.widgets.hovered.bg_fill = theme.bg_card_hover;
            visuals.widgets.hovered.bg_stroke = Stroke::NONE;
            visuals.widgets.hovered.corner_radius = theme.control_radius();

            visuals.widgets.open.weak_bg_fill = theme.bg_card_hover;
            visuals.widgets.open.bg_fill = theme.bg_card_hover;
            visuals.widgets.open.bg_stroke = Stroke::NONE;
            visuals.widgets.open.corner_radius = theme.control_radius();

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

        let save_btn = egui::Button::new(
            egui::RichText::new(tr!("Save"))
                .font(crate::theme::font(11.5))
                .color(theme.text_primary),
        )
        .fill(theme.bg_pill)
        .stroke(Stroke::NONE)
        .corner_radius(theme.control_radius())
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

        if let Some(idx) = state.selected_custom_preset_idx {
            let del_btn = egui::Button::new(
                egui::RichText::new(tr!("Delete"))
                    .font(crate::theme::font(11.0))
                    .color(theme.destructive),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::NONE)
            .corner_radius(theme.control_radius())
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

        crate::components::info_box::render_info_icon_button(
            ui,
            "eq_preset_curves",
            &crate::components::info_box::INFO_EQ_PRESETS,
            theme,
        );

        for (label, import) in [(tr!("Export"), false), (tr!("Import"), true)] {
            let button = egui::Button::new(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_secondary),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::NONE)
            .corner_radius(theme.control_radius())
            .min_size(Vec2::new(52.0, 24.0));
            if ui
                .add(button)
                .on_hover_text(tr!("AutoEQ, Equalizer APO, REW, and GraphicEQ text"))
                .clicked()
            {
                if import {
                    actions.import_curve = true;
                } else {
                    actions.export_curve = true;
                }
            }
        }
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
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, theme.space_xs);
                    ui.label(
                        egui::RichText::new(tr!("Preset Name:"))
                            .font(crate::theme::font(11.5))
                            .color(theme.text_secondary),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut state.new_preset_name).desired_width(150.0),
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

    // ── Preamp before the canvas ─────────────────────────────────────────────
    // The shared preamp is the level control for the curve that follows. It
    // used to sit after a canvas that could grow to 500 px, which pushed it
    // below the fold in the default window. Keeping it above the canvas makes
    // the control visible on open, and the canvas below now has a firm cap.
    render_footer_controls(ui, state, actions, theme, EqTab::Graphic);

    // ── Spline Canvas with Vertical Stems & Interactive Curve ────────────────
    render_spline_eq_canvas(ui, state, actions, theme);
}

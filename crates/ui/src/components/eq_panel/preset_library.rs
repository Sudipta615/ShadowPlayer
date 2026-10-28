//! The durable preset library, shown beside the parametric curve.
//!
//! The list is backed by `eq_presets` (migration V15) and loaded by the app.
//! The view never writes to the database: it emits typed requests, and the
//! dispatcher saves, renames, deletes or binds the row. That keeps SQL out of
//! draw code and makes a failed write visible as a toast instead of a stale
//! list that looks saved.
//!
//! Rows are drawn through `ScrollArea::show_rows`, so the frame cost is
//! proportional to the visible presets rather than to the whole library.

use super::{EqPanelActions, EqPanelState};
use crate::theme::{font, Theme};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

/// Height of one preset row. Uniform because the scroll area virtualizes on it.
/// Two lines: the identity/actions and the metadata/device actions. That keeps
/// a row usable at the panel's minimum width instead of forcing it wider.
const ROW_HEIGHT: f32 = 46.0;

pub(super) fn render_preset_library(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    {
        let library = &mut state.library;

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(tr!("Preset library"))
                    .font(font(12.5))
                    .strong()
                    .color(theme.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(
                    egui::RichText::new(tr!("Save current curve"))
                        .font(font(11.0))
                        .color(theme.text_secondary),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(128.0, 24.0));
                if ui.add(button).clicked() {
                    library.saving = !library.saving;
                }
            });
        });

        if let Some(error) = library.error.as_deref() {
            let mut dismiss = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(error)
                        .font(font(10.5))
                        .color(theme.destructive),
                );
                dismiss = ui.small_button("×").clicked();
            });
            if dismiss {
                library.error = None;
            }
        }

        // Which preset, if any, is pointed at by the playing track.
        if let Some(id) = library.track_binding.clone() {
            if let Some(name) = library
                .presets
                .iter()
                .find(|preset| preset.id == id)
                .map(|preset| preset.name.clone())
            {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(tr_args!(
                            "Bound to this track: {name}",
                            "name" => name
                        ))
                        .font(font(10.0))
                        .color(theme.accent_highlight),
                    );
                    if ui.small_button(tr!("Unbind")).clicked() {
                        actions.preset_unbind_track = true;
                    }
                });
            }
        }
    }

    if state.library.saving {
        render_save_box(ui, &mut state.library, actions, theme);
    }

    if let Some(id) = state.library.binding_edit.clone() {
        render_binding_editor(ui, &mut state.library, actions, theme, &id);
    }

    let library = &mut state.library;
    crate::components::search_field::search_field_with_width(
        ui,
        &mut library.search,
        tr!("Search presets"),
        theme,
        f32::INFINITY,
    );

    if !library.loaded {
        ui.label(
            egui::RichText::new(tr!("Loading presets…"))
                .font(font(10.5))
                .color(theme.text_muted),
        );
        return;
    }

    let filtered = library.filtered_indices();
    if filtered.is_empty() {
        ui.label(
            egui::RichText::new(tr!("No saved presets yet"))
                .font(font(10.5))
                .color(theme.text_muted),
        );
        return;
    }

    // Collect clicks as ids; mutating the library while its rows are borrowed
    // would require cloning every row on every frame.
    let mut apply_id: Option<String> = None;
    let mut delete_id: Option<String> = None;
    let mut default_id: Option<String> = None;
    let mut bind_row: Option<(String, String)> = None;
    let mut bind_track_id: Option<String> = None;

    egui::ScrollArea::vertical()
        .id_salt("eq_preset_library")
        .max_height(168.0)
        .auto_shrink([false, true])
        .show_rows(ui, ROW_HEIGHT, filtered.len(), |ui, range| {
            for row in range {
                let preset = &library.presets[filtered[row]];
                let meta = format!("{} · {}", preset.curve.point_count(), preset.source_label());
                let device = preset.output_pattern.as_deref();

                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ROW_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 2.0);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);

                            let apply = egui::Button::new(
                                egui::RichText::new(tr!("Apply"))
                                    .font(font(10.5))
                                    .color(theme.text_primary),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(44.0, 20.0));
                            if ui.add(apply).clicked() {
                                apply_id = Some(preset.id.clone());
                            }

                            // Reserve the button column before drawing the
                            // name: `Label::truncate` consumes the width it is
                            // offered, so a later right-to-left group would
                            // otherwise have no room left.
                            let buttons_w = 58.0;
                            let name_w =
                                (ui.available_width() - buttons_w - theme.space_xs).max(48.0);
                            let name = egui::RichText::new(&preset.name).font(font(11.0)).color(
                                if preset.is_default {
                                    theme.accent
                                } else {
                                    theme.text_primary
                                },
                            );
                            ui.add_sized(
                                Vec2::new(name_w, 20.0),
                                egui::Label::new(name).truncate(),
                            )
                            .on_hover_text(&preset.name);

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("×")
                                        .on_hover_text(tr!("Delete this preset"))
                                        .clicked()
                                    {
                                        delete_id = Some(preset.id.clone());
                                    }
                                    let star = if preset.is_default { "★" } else { "☆" };
                                    if ui
                                        .small_button(star)
                                        .on_hover_text(tr!("Make this the global default"))
                                        .clicked()
                                    {
                                        default_id = Some(preset.id.clone());
                                    }
                                },
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);

                            let actions_w = 126.0;
                            let meta_w =
                                (ui.available_width() - actions_w - theme.space_sm).max(48.0);
                            let meta_label = egui::RichText::new(meta)
                                .font(font(9.5))
                                .color(theme.text_muted);
                            let response = ui.add_sized(
                                Vec2::new(meta_w, 18.0),
                                egui::Label::new(meta_label).truncate(),
                            );
                            if let Some(pattern) = device {
                                response.on_hover_text(format!(
                                    "{}: {pattern}",
                                    tr!("Applied automatically when this device is selected")
                                ));
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button(tr!("This track"))
                                        .on_hover_text(tr!("Use this preset for the current track"))
                                        .clicked()
                                    {
                                        bind_track_id = Some(preset.id.clone());
                                    }
                                    if ui.small_button(tr!("Bind")).clicked() {
                                        bind_row = Some((
                                            preset.id.clone(),
                                            preset.output_pattern.clone().unwrap_or_default(),
                                        ));
                                    }
                                },
                            );
                        });
                    },
                );
            }
        });

    if let Some(id) = apply_id {
        actions.preset_apply = Some(id);
    }
    if let Some(id) = delete_id {
        actions.preset_delete = Some(id);
    }
    if let Some(id) = default_id {
        actions.preset_default = Some(id);
    }
    if let Some((id, pattern)) = bind_row {
        library.binding_edit = Some(id);
        library.binding_draft = pattern;
    }
    if let Some(id) = bind_track_id {
        actions.preset_bind_track = Some(id);
    }
}

fn render_save_box(
    ui: &mut egui::Ui,
    library: &mut crate::state::PresetLibraryState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.accent_highlight))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::symmetric(
            theme.space_md as i8,
            theme.space_sm as i8,
        ))
        .show(ui, |ui| {
            // Name and the default flag on one line; the device pattern gets
            // its own line so neither field is squeezed at the minimum width.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                let name_w = (ui.available_width() * 0.5).clamp(120.0, 220.0);
                ui.add(
                    egui::TextEdit::singleline(&mut library.save_name)
                        .hint_text(tr!("Preset Name:"))
                        .desired_width(name_w),
                );
                ui.checkbox(
                    &mut library.save_default,
                    egui::RichText::new(tr!("Default"))
                        .font(font(10.0))
                        .color(theme.text_secondary),
                );
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut library.save_pattern)
                        .hint_text(tr!("Device pattern"))
                        .desired_width(ui.available_width()),
                );
            });
            ui.add_space(theme.space_xs);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                let save = egui::Button::new(
                    egui::RichText::new(tr!("Confirm Save")).color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(96.0, 24.0));
                if ui.add(save).clicked() {
                    let name = library.save_name.trim();
                    if !name.is_empty() {
                        let pattern = library.save_pattern.trim();
                        actions.preset_save = Some(super::PresetSaveRequest {
                            name: name.to_string(),
                            output_pattern: (!pattern.is_empty()).then(|| pattern.to_string()),
                            is_default: library.save_default,
                        });
                    }
                }
                if ui.button(tr!("Cancel")).clicked() {
                    library.saving = false;
                }
                ui.label(
                    egui::RichText::new(tr!(
                        "The pattern matches the device name; the longest match wins"
                    ))
                    .font(font(9.5))
                    .color(theme.text_muted),
                );
            });
        });
}

fn render_binding_editor(
    ui: &mut egui::Ui,
    library: &mut crate::state::PresetLibraryState,
    actions: &mut EqPanelActions,
    theme: &Theme,
    id: &str,
) {
    egui::Frame::NONE
        .fill(theme.bg_card_hover)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::symmetric(
            theme.space_md as i8,
            theme.space_xs as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                ui.label(
                    egui::RichText::new(tr!("Device pattern"))
                        .font(font(10.0))
                        .color(theme.text_secondary),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut library.binding_draft)
                        .hint_text(tr!("e.g. USB DAC"))
                        .desired_width((ui.available_width() - 150.0).clamp(100.0, 180.0)),
                );
                if ui.small_button(tr!("Bind")).clicked() {
                    let pattern = library.binding_draft.trim();
                    actions.preset_bind = Some(super::PresetBindRequest {
                        id: id.to_string(),
                        output_pattern: (!pattern.is_empty()).then(|| pattern.to_string()),
                    });
                }
                if ui.small_button(tr!("Clear binding")).clicked() {
                    actions.preset_bind = Some(super::PresetBindRequest {
                        id: id.to_string(),
                        output_pattern: None,
                    });
                }
                if ui.small_button(tr!("Cancel")).clicked() {
                    library.binding_edit = None;
                }
            });
        });
}

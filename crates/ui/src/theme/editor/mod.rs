mod state;
mod widgets;

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, Stroke, Vec2};

use super::contrast::validate_contrast;
use super::fonts::bundled_font_family_names;
use super::manager::ThemeManager;
use super::tokens::{with_alpha, PlayButtonShape, Theme};
use state::{save_current_theme, PendingAction, SaveOutcome};
use widgets::{
    color_picker_row, field_label, font_role_input, render_live_preview, section_heading,
};

pub use state::ThemeEditorState;

/// Actions emitted by the Theme Editor.
#[derive(Default, Clone, Debug)]
pub struct EditorActions {
    pub theme_saved: Option<String>,
}

/// Render the Theme Editor body. The owning settings card supplies the
/// "Theme Editor" heading, so this function starts with the toolbar rather
/// than drawing a second frame around itself.
pub fn render_theme_editor(
    ui: &mut egui::Ui,
    state: &mut ThemeEditorState,
    manager: &ThemeManager,
    theme: &Theme,
) -> EditorActions {
    let mut actions = EditorActions::default();

    // The Settings scroll area uses zero horizontal spacing between cards.
    // The editor is a dense control surface, so give its own children a
    // consistent rhythm instead of inheriting that card-level spacing.
    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);

    // Compact toolbar. The card header is the page heading; this row only
    // carries the open/close affordance and a one-line state hint.
    ui.horizontal(|ui| {
        let dirty = state.is_open && state.is_dirty();
        let status = if state.is_open {
            if dirty {
                format!("Editing \"{}\" · unsaved changes", state.name)
            } else {
                format!("Editing \"{}\"", state.name)
            }
        } else {
            "Build a theme from any base palette and save it for the gallery".to_string()
        };
        ui.label(
            egui::RichText::new(status)
                .font(crate::theme::font(11.0))
                .color(if dirty {
                    theme.accent_warm
                } else {
                    theme.text_muted
                }),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let toggle_lbl = if state.is_open {
                "Close Editor"
            } else {
                "Open Theme Editor"
            };
            let toggle = egui::Button::new(
                egui::RichText::new(toggle_lbl)
                    .font(crate::theme::font(11.5))
                    .strong()
                    .color(if state.is_open {
                        theme.text_secondary
                    } else {
                        Color32::WHITE
                    }),
            )
            .fill(if state.is_open {
                theme.bg_pill
            } else {
                theme.accent
            })
            .stroke(Stroke::new(1.0, theme.border_subtle))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(132.0, 28.0));
            if ui.add(toggle).clicked() {
                if state.is_open {
                    if state.is_dirty() {
                        state.pending_action = Some(PendingAction::Close);
                    } else {
                        state.is_open = false;
                    }
                } else {
                    state.is_open = true;
                }
            }
        });
    });

    if !state.is_open {
        return actions;
    }

    ui.add_space(theme.space_md);

    // A close or "open another theme" request that would lose edits parks
    // here until the user decides. This is an inline bar rather than a modal:
    // the draft stays visible behind it, so the decision is about the work on
    // screen.
    if let Some(pending) = state.pending_action.clone() {
        let (message, discard_label) = match &pending {
            PendingAction::Close => (
                "You have unsaved changes. Discard them and close the editor?".to_string(),
                "Discard & Close",
            ),
            PendingAction::Load(load) => (
                format!(
                    "Opening \"{}\" will discard your unsaved changes. Continue?",
                    load.name
                ),
                "Discard & Open",
            ),
        };
        let mut discard = false;
        let mut keep_editing = false;
        egui::Frame::NONE
            .fill(with_alpha(theme.accent_warm, 18))
            .stroke(Stroke::new(1.0, with_alpha(theme.accent_warm, 130)))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .inner_margin(Margin::symmetric(
                theme.space_md as i8,
                theme.space_xs as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(message)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_primary),
                    );
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(discard_label)
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(theme.accent_warm),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.accent_warm))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        discard = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Keep editing")
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_secondary),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        keep_editing = true;
                    }
                });
            });
        if discard {
            match state.pending_action.take() {
                Some(PendingAction::Close) => {
                    state.revert_to_saved();
                    state.is_open = false;
                }
                Some(PendingAction::Load(load)) => state.apply_pending_load(*load),
                None => {}
            }
            return actions;
        }
        if keep_editing {
            state.pending_action = None;
        }
        ui.add_space(theme.space_sm);
    }

    if let Some((ref msg, is_err)) = state.status_message {
        let color = if is_err {
            theme.destructive
        } else {
            theme.bit_perfect_green
        };
        egui::Frame::NONE
            .fill(with_alpha(color, 20))
            .stroke(Stroke::new(1.0, with_alpha(color, 110)))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .inner_margin(Margin::symmetric(
                theme.space_md as i8,
                theme.space_xs as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(msg)
                        .font(crate::theme::font(11.0))
                        .color(color),
                );
            });
        ui.add_space(theme.space_sm);
    }

    // Two-column top area: identity on the left, live silhouette on the
    // right. This keeps the metadata compact at every window width.
    ui.horizontal_top(|ui| {
        let preview = state.preview_theme(theme);
        // The preview frame is 178 pt wide plus its margins; reserve room for
        // it and the inter-column gap so the metadata fields never overlap it.
        let left_w = (ui.available_width() - 206.0).max(240.0);
        ui.allocate_ui_with_layout(Vec2::new(left_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(left_w);
            egui::Grid::new("theme_editor_identity")
                .num_columns(2)
                .spacing(Vec2::new(theme.space_sm, theme.space_xs))
                .show(ui, |ui| {
                    field_label(ui, "Theme name", theme);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.name)
                            .desired_width((left_w - 108.0).max(120.0))
                            .font(crate::theme::font(12.0)),
                    );
                    ui.end_row();

                    field_label(ui, "File ID", theme);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.id)
                            .desired_width((left_w - 108.0).max(120.0))
                            .font(crate::theme::mono_font(11.0)),
                    );
                    ui.end_row();

                    field_label(ui, "Author", theme);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.author)
                            .desired_width((left_w - 108.0).max(120.0))
                            .font(crate::theme::font(12.0)),
                    );
                    ui.end_row();

                    field_label(ui, "Description", theme);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.description)
                            .desired_width((left_w - 108.0).max(120.0))
                            .font(crate::theme::font(11.5)),
                    );
                    ui.end_row();
                });

            ui.add_space(theme.space_xs);
            let mut light = state.is_light;
            if ui.checkbox(&mut light, "Light theme").changed() {
                state.is_light = light;
            }
        });

        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            render_live_preview(ui, state, &preview, theme);
        });
    });

    ui.add_space(theme.space_md);

    // Transport silhouette, live preview included.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Play button")
                .font(crate::theme::font(11.0))
                .strong()
                .color(theme.text_secondary),
        );
        ui.add_space(theme.space_xs);
        for shape in PlayButtonShape::ALL {
            let selected = state.play_button_shape == shape;
            let response = ui.add(
                egui::Button::new(
                    egui::RichText::new(shape.label())
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
                .stroke(Stroke::new(
                    1.0,
                    if selected {
                        theme.accent_highlight
                    } else {
                        theme.border_card
                    },
                ))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(0.0, 26.0)),
            );
            if response.clicked() {
                state.play_button_shape = shape;
            }
        }
    });

    ui.add_space(theme.space_md);

    // Base palette loader.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Clone base colors")
                .font(crate::theme::font(11.0))
                .color(theme.text_muted),
        );
        for (label, base) in [
            ("Obsidian Dark", Theme::dark()),
            ("Studio Light", Theme::light()),
            ("OLED Midnight", Theme::oled()),
        ] {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(label)
                            .font(crate::theme::font(11.0))
                            .color(theme.text_secondary),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .min_size(Vec2::new(0.0, 24.0)),
                )
                .clicked()
            {
                state.populate_from_theme(&base);
            }
        }
    });

    ui.add_space(theme.space_md);

    // Grouped color pickers.
    ui.columns(3, |columns| {
        columns[0].vertical(|ui| {
            section_heading(ui, "Surfaces", theme);
            color_picker_row(ui, state, "bg_app", "App Background", theme);
            color_picker_row(ui, state, "bg_sidebar", "Sidebar", theme);
            color_picker_row(ui, state, "bg_card", "Cards & Dialogs", theme);
            color_picker_row(ui, state, "bg_pill", "Pills & Chips", theme);
            color_picker_row(ui, state, "bg_input", "Text Input", theme);
            color_picker_row(ui, state, "border_card", "Card Border", theme);
        });

        columns[1].vertical(|ui| {
            section_heading(ui, "Accents & Status", theme);
            color_picker_row(ui, state, "accent", "Primary Accent", theme);
            color_picker_row(ui, state, "accent_highlight", "Highlight", theme);
            color_picker_row(ui, state, "accent_warm", "Warm Accent", theme);
            color_picker_row(ui, state, "info", "Info Blue", theme);
            color_picker_row(ui, state, "destructive", "Destructive", theme);
            color_picker_row(ui, state, "bit_perfect_green", "Bit-Perfect Green", theme);
        });

        columns[2].vertical(|ui| {
            section_heading(ui, "Typography & Contrast", theme);
            color_picker_row(ui, state, "text_primary", "Primary Text", theme);
            color_picker_row(ui, state, "text_secondary", "Secondary Text", theme);
            color_picker_row(ui, state, "text_muted", "Muted Text", theme);
            color_picker_row(ui, state, "text_dim", "Dim Text", theme);

            ui.add_space(theme.space_sm);
            match validate_contrast(&state.colors) {
                Ok(()) => {
                    ui.label(
                        egui::RichText::new("PASS · WCAG AA (≥ 4.5:1)")
                            .font(crate::theme::font(10.5))
                            .color(theme.bit_perfect_green),
                    );
                }
                Err(ref err) => {
                    ui.label(
                        egui::RichText::new(format!("Warning: {err}"))
                            .font(crate::theme::font(10.5))
                            .color(theme.warning_yellow),
                    );
                }
            }
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Auto-Fix Contrast")
                            .font(crate::theme::font(11.0))
                            .color(theme.info),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_sm))
                    .min_size(Vec2::new(0.0, 24.0)),
                )
                .on_hover_text("Adjust text luminance until every text token clears WCAG AA")
                .clicked()
            {
                state.auto_fix_contrast();
            }
        });
    });

    ui.add_space(theme.space_md);

    // Radii: compact numeric fields rather than six full-width sliders.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Corner radii")
                .font(crate::theme::font(11.0))
                .strong()
                .color(theme.text_secondary),
        );
        for (key, label) in [
            ("radius_xs", "XS"),
            ("radius_sm", "S"),
            ("radius_md", "M"),
            ("radius_lg", "L"),
            ("radius_xl", "XL"),
            ("radius_pill", "Pill"),
        ] {
            if let Some(value) = state.radii.get_mut(key) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.space_xs;
                    ui.label(
                        egui::RichText::new(label)
                            .font(crate::theme::mono_font(9.5))
                            .color(theme.text_muted),
                    );
                    ui.add(
                        egui::DragValue::new(value)
                            .range(0..=64)
                            .speed(0.5)
                            .fixed_decimals(0),
                    );
                });
            }
        }
    });

    ui.add_space(theme.space_md);

    // Per-role typefaces. An empty field means Inter for that role.
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Fonts")
                .font(crate::theme::font(11.0))
                .strong()
                .color(theme.text_secondary),
        );
        font_role_input(ui, &mut state.fonts.display, "Display", "Inter", theme);
        font_role_input(ui, &mut state.fonts.body, "Body", "Inter", theme);
        font_role_input(ui, &mut state.fonts.mono, "Mono", "JetBrains Mono", theme);
    });
    let bundled = bundled_font_family_names().collect::<Vec<_>>().join(", ");
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new("Bundled families")
                .font(crate::theme::font(9.5))
                .color(theme.text_dim),
        )
        .on_hover_text(bundled);
        let missing = state.fonts.missing_fonts();
        if !missing.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "{} unavailable · Inter fallback",
                    missing.join(", ")
                ))
                .font(crate::theme::font(9.5))
                .color(theme.warning_yellow),
            );
        }
    });

    ui.add_space(theme.space_md);

    // Save / export.
    let can_save = !state.id.trim().is_empty() && !state.name.trim().is_empty();
    let mut saved_id = None;
    ui.horizontal(|ui| {
        let save = ui.add_enabled(
            can_save,
            egui::Button::new(
                egui::RichText::new("Save to Gallery")
                    .font(crate::theme::font(12.0))
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(theme.accent)
            .corner_radius(CornerRadius::same(theme.radius_md))
            .min_size(Vec2::new(154.0, 30.0)),
        );
        if save.clicked() {
            match save_current_theme(state, manager, false, false) {
                SaveOutcome::Saved(id) => saved_id = Some(id),
                SaveOutcome::NeedsOverwrite => state.pending_overwrite = true,
                SaveOutcome::Error(message) => {
                    state.status_message = Some((message, true));
                }
            }
        }

        let save_new = ui.add_enabled(
            can_save,
            egui::Button::new(
                egui::RichText::new("Save as New")
                    .font(crate::theme::font(11.5))
                    .color(theme.accent_highlight),
            )
            .fill(theme.bg_pill)
            .stroke(Stroke::new(1.0, theme.border_subtle))
            .corner_radius(CornerRadius::same(theme.radius_md))
            .min_size(Vec2::new(118.0, 30.0)),
        );
        if save_new.clicked() {
            match save_current_theme(state, manager, true, true) {
                SaveOutcome::Saved(id) => saved_id = Some(id),
                SaveOutcome::NeedsOverwrite => state.pending_overwrite = true,
                SaveOutcome::Error(message) => {
                    state.status_message = Some((message, true));
                }
            }
        }

        if !can_save {
            ui.label(
                egui::RichText::new("Name and file ID are required")
                    .font(crate::theme::font(10.5))
                    .color(theme.warning_yellow),
            );
        } else {
            ui.label(
                egui::RichText::new(if state.is_dirty() {
                    "Unsaved changes are not in the gallery yet"
                } else {
                    "The saved theme appears in the gallery immediately"
                })
                .font(crate::theme::font(10.5))
                .color(theme.text_muted),
            );
        }
    });

    if state.pending_overwrite {
        ui.add_space(theme.space_xs);
        let mut overwrite = false;
        let mut cancel = false;
        egui::Frame::NONE
            .fill(with_alpha(theme.accent_warm, 18))
            .stroke(Stroke::new(1.0, with_alpha(theme.accent_warm, 130)))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .inner_margin(Margin::symmetric(
                theme.space_md as i8,
                theme.space_xs as i8,
            ))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "A theme file named \"{}.shadowtheme\" already exists. Overwrite it?",
                            state.id.trim()
                        ))
                        .font(crate::theme::font(11.5))
                        .color(theme.text_primary),
                    );
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Overwrite")
                                    .font(crate::theme::font(10.5))
                                    .strong()
                                    .color(theme.accent_warm),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.accent_warm))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        overwrite = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Cancel")
                                    .font(crate::theme::font(10.5))
                                    .color(theme.text_secondary),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .min_size(Vec2::new(0.0, 22.0)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if overwrite {
            match save_current_theme(state, manager, true, false) {
                SaveOutcome::Saved(id) => saved_id = Some(id),
                SaveOutcome::NeedsOverwrite => state.pending_overwrite = true,
                SaveOutcome::Error(message) => {
                    state.status_message = Some((message, true));
                }
            }
        }
        if cancel {
            state.pending_overwrite = false;
        }
    }

    if let Some(id) = saved_id {
        state.pending_overwrite = false;
        state.status_message = Some(("Theme saved to the gallery.".into(), false));
        actions.theme_saved = Some(id);
    }

    actions
}

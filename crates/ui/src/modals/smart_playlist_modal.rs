//! Rule-based "smart playlist" manager: compose a saved query and watch it match
//! the library live.

use domain::ids::PlaylistId;
use domain::models::Track;
use domain::smart::{SmartField, SmartOp, SmartPlaylist, SmartRule};
use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

use crate::theme::Theme;

/// State for the floating smart-playlist editor window.
pub struct SmartPlaylistEditorState {
    pub is_open: bool,
    /// The definition currently being edited (never `None` while open).
    pub draft: SmartPlaylist,
    /// `true` when the draft is not yet persisted (shows "Create" instead of
    /// "Save", and enables Delete only for existing definitions).
    pub is_new: bool,
    /// Set whenever a widget changes the draft, so the caller re-evaluates.
    pub draft_dirty: bool,
}

impl Default for SmartPlaylistEditorState {
    fn default() -> Self {
        Self {
            is_open: false,
            draft: SmartPlaylist::draft(PlaylistId::new()),
            is_new: true,
            draft_dirty: false,
        }
    }
}

impl SmartPlaylistEditorState {
    /// Open the editor on a brand-new definition.
    pub fn open_new(&mut self) {
        self.draft = SmartPlaylist::draft(PlaylistId::new());
        self.is_new = true;
        self.is_open = true;
        self.draft_dirty = true;
    }

    /// Open the editor on an existing definition.
    pub fn open_existing(&mut self, sp: &SmartPlaylist) {
        self.draft = sp.clone();
        self.is_new = false;
        self.is_open = true;
        self.draft_dirty = true;
    }
}

/// What the caller should persist after the window closes.
#[derive(Default)]
pub struct SmartPlaylistActions {
    pub save: Option<SmartPlaylist>,
    pub delete: Option<PlaylistId>,
    /// The user asked to open a different playlist for editing.
    pub open: Option<PlaylistId>,
}

/// Floating smart-playlist editor.
pub fn show_smart_playlist_editor(
    ctx: &egui::Context,
    state: &mut SmartPlaylistEditorState,
    playlists: &[SmartPlaylist],
    matches: &[Track],
    theme: &Theme,
) -> SmartPlaylistActions {
    let mut actions = SmartPlaylistActions::default();
    if !state.is_open {
        return actions;
    }

    let mut is_open = state.is_open;
    let closed = crate::dialog::show_modal(
        ctx,
        "smart_playlist",
        "Smart Playlist",
        640.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);
            let validation = state.draft.validation_error();

            // Apply consistent theme styling to ComboBoxes and input widgets
            ui.visuals_mut().widgets.inactive.weak_bg_fill = theme.bg_input;
            ui.visuals_mut().widgets.inactive.bg_fill = theme.bg_input;
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border_input);
            ui.visuals_mut().widgets.inactive.corner_radius = CornerRadius::same(theme.radius_md);
            ui.visuals_mut().widgets.hovered.weak_bg_fill = theme.bg_card_hover;
            ui.visuals_mut().widgets.hovered.bg_fill = theme.bg_card_hover;
            ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::new(1.0, theme.accent);
            ui.visuals_mut().widgets.hovered.corner_radius = CornerRadius::same(theme.radius_md);
            ui.visuals_mut().widgets.open.weak_bg_fill = theme.bg_card_active;
            ui.visuals_mut().widgets.open.bg_fill = theme.bg_card_active;
            ui.visuals_mut().widgets.open.bg_stroke = Stroke::new(1.0, theme.accent);
            ui.visuals_mut().widgets.open.corner_radius = CornerRadius::same(theme.radius_md);

            ui.horizontal_top(|ui| {
                // ── Left: saved smart playlists (compact column) ────────────────────────
                let col_w = 130.0;
                ui.allocate_ui_with_layout(
                    Vec2::new(col_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new(tr!("Saved"))
                                .font(crate::theme::font(13.0))
                                .strong()
                                .color(theme.text_primary),
                        );
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .id_salt("smart_pl_list")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 8.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);
                                if playlists.is_empty() {
                                    ui.label(
                                        egui::RichText::new(tr!("None yet."))
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_muted),
                                    );
                                }
                                for sp in playlists {
                                    let is_sel = !state.is_new && state.draft.id == sp.id;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                                        if ui
                                            .selectable_label(
                                                is_sel,
                                                egui::RichText::new(sp.name.clone())
                                                    .font(crate::theme::font(12.0)),
                                            )
                                            .clicked()
                                        {
                                            actions.open = Some(sp.id);
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new(tr!(
                                                        Msg::SAD9F16B467D53F18
                                                    ))
                                                    .font(crate::theme::font(11.0))
                                                    .color(theme.text_muted),
                                                )
                                                .fill(theme.bg_card)
                                                .stroke(Stroke::new(1.0, theme.border_subtle))
                                                .corner_radius(CornerRadius::same(theme.radius_sm)),
                                            )
                                            .on_hover_text(tr!(Msg::SDC673E73B5C13AEA))
                                            .clicked()
                                        {
                                            actions.delete = Some(sp.id);
                                        }
                                    });
                                }
                            });
                    },
                );

                ui.separator();

                // ── Right: rule builder ────────────────────────────────
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                            ui.label(
                                egui::RichText::new(tr!("Name"))
                                    .font(crate::theme::font(12.5))
                                    .color(theme.text_secondary),
                            );
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut state.draft.name)
                                        .hint_text(tr!(Msg::S9D8617B1A686A83D))
                                        .desired_width(ui.available_width() - 80.0),
                                )
                                .changed()
                            {
                                state.draft_dirty = true;
                            }
                            let can_save =
                                !state.draft.name.trim().is_empty() && validation.is_none();
                            let save_resp = ui
                                .add_enabled(
                                    can_save,
                                    egui::Button::new(
                                        egui::RichText::new(if state.is_new {
                                            "Create"
                                        } else {
                                            "Save"
                                        })
                                        .font(crate::theme::font(12.0))
                                        .color(Color32::WHITE),
                                    )
                                    .fill(theme.accent)
                                    .corner_radius(CornerRadius::same(theme.radius_md)),
                                )
                                .on_disabled_hover_text(
                                    validation
                                        .clone()
                                        .unwrap_or_else(|| "Give the playlist a name".to_string()),
                                );
                            if save_resp.clicked() {
                                actions.save = Some(state.draft.clone());
                            }
                        });

                        // AND / OR
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                            ui.label(
                                egui::RichText::new(tr!("Match:"))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_secondary),
                            );
                            if ui
                                .selectable_value(&mut state.draft.match_all, true, "all rules")
                                .clicked()
                            {
                                state.draft_dirty = true;
                            }
                            if ui
                                .selectable_value(&mut state.draft.match_all, false, "any rule")
                                .clicked()
                            {
                                state.draft_dirty = true;
                            }
                        });

                        if let Some(message) = &validation {
                            ui.label(
                                egui::RichText::new(message)
                                    .font(crate::theme::font(11.5))
                                    .color(theme.destructive),
                            )
                            .on_hover_text(tr!(Msg::SABE7A4AB105B00FD));
                        }

                        ui.separator();

                        // Rules
                        let mut remove: Option<usize> = None;
                        egui::ScrollArea::vertical()
                            .id_salt("smart_rules")
                            .auto_shrink([false, false])
                            .max_height((ui.available_height() * 0.45).max(90.0))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
                                for (i, rule) in state.draft.rules.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                                        // Field
                                        egui::ComboBox::from_id_salt(("sf", i))
                                            .selected_text(rule.field.label())
                                            .width(104.0)
                                            .show_ui(ui, |ui| {
                                                for f in SmartField::ALL {
                                                    if ui
                                                        .selectable_value(
                                                            &mut rule.field,
                                                            f,
                                                            f.label(),
                                                        )
                                                        .clicked()
                                                    {
                                                        // Keep the operator valid for the new field.
                                                        if !f.operators().contains(&rule.op) {
                                                            rule.op = f
                                                                .operators()
                                                                .first()
                                                                .copied()
                                                                .unwrap_or(SmartOp::Contains);
                                                        }
                                                        state.draft_dirty = true;
                                                    }
                                                }
                                            });

                                        // Operator
                                        egui::ComboBox::from_id_salt(("so", i))
                                            .selected_text(rule.op.label())
                                            .width(112.0)
                                            .show_ui(ui, |ui| {
                                                for op in rule.field.operators() {
                                                    if ui
                                                        .selectable_value(
                                                            &mut rule.op,
                                                            *op,
                                                            op.label(),
                                                        )
                                                        .clicked()
                                                    {
                                                        state.draft_dirty = true;
                                                    }
                                                }
                                            });

                                        // Value
                                        if rule.op.needs_value() {
                                            let hint = if rule.field.is_date() {
                                                "days"
                                            } else {
                                                "value"
                                            };
                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(&mut rule.value)
                                                        .hint_text(hint)
                                                        .desired_width(85.0),
                                                )
                                                .changed()
                                            {
                                                state.draft_dirty = true;
                                            }
                                        }

                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    egui::RichText::new(tr!(
                                                        Msg::SE0A34419921B022A
                                                    ))
                                                    .font(crate::theme::font(11.0))
                                                    .color(theme.text_muted),
                                                )
                                                .fill(theme.bg_card)
                                                .stroke(Stroke::new(1.0, theme.border_subtle))
                                                .corner_radius(CornerRadius::same(theme.radius_sm)),
                                            )
                                            .on_hover_text(tr!(Msg::S440FB245DF9F0819))
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                    });
                                }
                            });
                        if let Some(i) = remove {
                            state.draft.rules.remove(i);
                            state.draft_dirty = true;
                        }

                        ui.add_space(4.0);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(tr!(Msg::S13E29684902A9319))
                                        .font(crate::theme::font(11.5))
                                        .color(theme.accent),
                                )
                                .fill(theme.bg_pill)
                                .stroke(Stroke::new(1.0, theme.border_subtle))
                                .corner_radius(CornerRadius::same(theme.radius_md)),
                            )
                            .clicked()
                        {
                            state.draft.rules.push(SmartRule::new(SmartField::Artist));
                            state.draft_dirty = true;
                        }

                        ui.separator();

                        // Limit + sort
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                            let mut limited = state.draft.limit_count.is_some();
                            if ui.checkbox(&mut limited, "Limit").changed() {
                                state.draft.limit_count = if limited { Some(50) } else { None };
                                state.draft_dirty = true;
                            }
                            if let Some(n) = state.draft.limit_count.as_mut() {
                                if ui.add(egui::DragValue::new(n).range(1..=10_000)).changed() {
                                    state.draft_dirty = true;
                                }
                                ui.label(
                                    egui::RichText::new(tr!("tracks"))
                                        .font(crate::theme::font(11.5))
                                        .color(theme.text_muted),
                                );
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                            ui.label(
                                egui::RichText::new(tr!("Sort by"))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_secondary),
                            );
                            let label = state
                                .draft
                                .sort_by
                                .map(|f| f.label())
                                .unwrap_or("(default)");
                            egui::ComboBox::from_id_salt("smart_sort")
                                .selected_text(label)
                                .width(130.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            state.draft.sort_by.is_none(),
                                            "(default)",
                                        )
                                        .clicked()
                                    {
                                        state.draft.sort_by = None;
                                        state.draft_dirty = true;
                                    }
                                    for f in SmartField::ALL {
                                        if ui
                                            .selectable_label(
                                                state.draft.sort_by == Some(f),
                                                f.label(),
                                            )
                                            .clicked()
                                        {
                                            state.draft.sort_by = Some(f);
                                            state.draft_dirty = true;
                                        }
                                    }
                                });
                            if ui
                                .selectable_value(&mut state.draft.sort_desc, false, "A-Z")
                                .clicked()
                                || ui
                                    .selectable_value(&mut state.draft.sort_desc, true, "Z-A")
                                    .clicked()
                            {
                                state.draft_dirty = true;
                            }
                        });

                        ui.add_space(6.0);
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("Matches: {} track(s)", matches.len()))
                                .font(crate::theme::font(13.0))
                                .strong()
                                .color(theme.info),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("smart_preview")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 4.0)
                            .show(ui, |ui| {
                                for t in matches.iter().take(100) {
                                    ui.label(
                                        egui::RichText::new(format!("{} — {}", t.title, t.artist))
                                            .font(crate::theme::font(11.5))
                                            .color(theme.text_secondary),
                                    );
                                }
                            });
                    },
                );
            });
        },
    );
    if closed {
        is_open = false;
    }

    state.is_open = is_open;
    actions
}

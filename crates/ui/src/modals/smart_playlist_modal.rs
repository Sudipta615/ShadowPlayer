//! Rule-based "smart playlist" manager: compose a saved query and watch it match
//! the library live.
//!
//! The editor is a single vertical form rather than a saved-list sidebar plus a
//! cramped rule strip: a rule row now has room for three readable controls, the
//! match mode is a visible choice instead of two plain text links, and the
//! primary action sits in a footer the user can always reach.

use domain::ids::PlaylistId;
use domain::models::Track;
use domain::smart::{SmartField, SmartOp, SmartPlaylist, SmartRule};
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

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
    // The app's minimum window is 640 px wide; a fixed 640 px modal would be
    // clipped there. Clamp to the viewport the way the other dialogs do.
    let screen = crate::dialog::viewport_rect(ctx);
    let width = 620.0_f32.min((screen.width() - 48.0).max(320.0));
    let title = tr!(Msg::SDA82BB90A6E11970);

    let mut cancel = false;
    let closed =
        crate::dialog::show_modal_scrollable(ctx, "smart_playlist", &title, width, theme, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);
            theme_form_widgets(ui, theme);

            ui.label(
                egui::RichText::new(tr!(
                    "Rules match the library live — nothing is written until you save."
                ))
                .font(crate::theme::font(11.8))
                .color(theme.text_muted),
            );

            let validation = state.draft.validation_error();
            let can_save = !state.draft.name.trim().is_empty() && validation.is_none();

            // ── Details ───────────────────────────────────────────────
            card(ui, theme, |ui| {
                section_heading(ui, tr!("Details"), theme);

                labelled_row(ui, tr!("Name"), theme, 64.0, |ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut state.draft.name)
                            .hint_text(tr!(Msg::S9D8617B1A686A83D))
                            .desired_width(ui.available_width()),
                    );
                    if edit.changed() {
                        state.draft_dirty = true;
                    }
                });

                ui.add_space(theme.space_xs);

                let mut match_all = state.draft.match_all;
                let all_rules = tr!("All rules");
                let any_rule = tr!("Any rule");
                labelled_row(ui, tr!("Match:"), theme, 64.0, |ui| {
                    if crate::components::settings_tab::segment_picker(
                        ui,
                        &mut match_all,
                        &[(true, all_rules.as_str()), (false, any_rule.as_str())],
                        theme,
                    ) {
                        state.draft.match_all = match_all;
                        state.draft_dirty = true;
                    }
                    ui.label(
                        egui::RichText::new(if match_all {
                            tr!("every rule must be true")
                        } else {
                            tr!("one rule is enough")
                        })
                        .font(crate::theme::font(11.0))
                        .color(theme.text_muted),
                    );
                });

                if let Some(message) = &validation {
                    ui.add_space(theme.space_xs);
                    ui.label(
                        egui::RichText::new(message)
                            .font(crate::theme::font(11.5))
                            .color(theme.destructive),
                    )
                    .on_hover_text(tr!(Msg::SABE7A4AB105B00FD));
                }
            });

            // ── Rules ─────────────────────────────────────────────────
            card(ui, theme, |ui| {
                ui.horizontal(|ui| {
                    section_heading(ui, tr!("Rules"), theme);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(tr_args!(
                                "{n} rules",
                                "n" => state.draft.rules.len()
                            ))
                            .font(crate::theme::font(11.0))
                            .color(theme.text_muted),
                        );
                    });
                });

                if state.draft.rules.is_empty() {
                    ui.label(
                        egui::RichText::new(tr!("No rules yet — every track matches."))
                            .font(crate::theme::font(11.8))
                            .color(theme.text_muted),
                    );
                } else {
                    let mut remove: Option<usize> = None;
                    egui::ScrollArea::vertical()
                        .id_salt("smart_rules")
                        .auto_shrink([false, false])
                        .max_height(228.0)
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
                            for (i, rule) in state.draft.rules.iter_mut().enumerate() {
                                render_rule_row(
                                    ui,
                                    i,
                                    rule,
                                    theme,
                                    &mut state.draft_dirty,
                                    &mut remove,
                                );
                            }
                        });
                    if let Some(i) = remove {
                        state.draft.rules.remove(i);
                        state.draft_dirty = true;
                    }
                }

                ui.add_space(theme.space_xs);
                if add_rule_button(ui, theme).clicked() {
                    state.draft.rules.push(SmartRule::new(SmartField::Artist));
                    state.draft_dirty = true;
                }
            });

            // ── Order & limit ─────────────────────────────────────────
            card(ui, theme, |ui| {
                section_heading(ui, tr!("Order & limit"), theme);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
                    let mut limited = state.draft.limit_count.is_some();
                    if crate::components::settings_tab::toggle_switch(ui, &mut limited, theme) {
                        state.draft.limit_count = if limited { Some(50) } else { None };
                        state.draft_dirty = true;
                    }
                    ui.label(
                        egui::RichText::new(tr!("Limit to"))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    if let Some(n) = state.draft.limit_count.as_mut() {
                        if ui
                            .add(egui::DragValue::new(n).range(1..=10_000).speed(1.0))
                            .changed()
                        {
                            state.draft_dirty = true;
                        }
                        ui.label(
                            egui::RichText::new(tr!("tracks"))
                                .font(crate::theme::font(11.5))
                                .color(theme.text_muted),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(tr!("no limit"))
                                .font(crate::theme::font(11.5))
                                .color(theme.text_muted),
                        );
                    }
                });

                ui.add_space(theme.space_xs);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
                    ui.label(
                        egui::RichText::new(tr!("Sort by"))
                            .font(crate::theme::font(12.0))
                            .color(theme.text_secondary),
                    );
                    let default_sort = tr!("(default)");
                    let label = state
                        .draft
                        .sort_by
                        .map(|f| f.label())
                        .unwrap_or(default_sort.as_str());
                    egui::ComboBox::from_id_salt("smart_sort")
                        .selected_text(label)
                        .width(132.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(state.draft.sort_by.is_none(), tr!("(default)"))
                                .clicked()
                            {
                                state.draft.sort_by = None;
                                state.draft_dirty = true;
                            }
                            for f in SmartField::ALL {
                                if ui
                                    .selectable_label(state.draft.sort_by == Some(f), f.label())
                                    .clicked()
                                {
                                    state.draft.sort_by = Some(f);
                                    state.draft_dirty = true;
                                }
                            }
                        });

                    let enabled = state.draft.sort_by.is_some();
                    ui.add_enabled_ui(enabled, |ui| {
                        let mut ascending = !state.draft.sort_desc;
                        let asc = tr!("Ascending");
                        let desc = tr!("Descending");
                        if crate::components::settings_tab::segment_picker(
                            ui,
                            &mut ascending,
                            &[(true, asc.as_str()), (false, desc.as_str())],
                            theme,
                        ) {
                            state.draft.sort_desc = !ascending;
                            state.draft_dirty = true;
                        }
                    });
                });
            });

            // ── Live match summary ────────────────────────────────────
            render_match_summary(ui, matches, theme);

            ui.add_space(theme.space_xs);
            ui.separator();
            ui.add_space(theme.space_xs);

            // ── Footer ────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);

                // Switching definitions is a secondary action; it stays on the
                // left so it cannot be mistaken for "save".
                let current = if state.is_new {
                    tr!("New playlist")
                } else {
                    state.draft.name.clone()
                };
                egui::ComboBox::from_id_salt("smart_open_existing")
                    .selected_text(current)
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(state.is_new, tr!("New playlist"))
                            .clicked()
                            && !state.is_new
                        {
                            state.open_new();
                        }
                        for sp in playlists {
                            let selected = !state.is_new && state.draft.id == sp.id;
                            if ui.selectable_label(selected, sp.name.clone()).clicked() && !selected
                            {
                                actions.open = Some(sp.id);
                            }
                        }
                    });

                if !state.is_new {
                    let delete = egui::Button::new(
                        egui::RichText::new(tr!(Msg::SDC673E73B5C13AEA))
                            .font(crate::theme::font(11.5))
                            .color(theme.destructive),
                    )
                    .fill(theme.bg_pill)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md));
                    if ui.add(delete).clicked() {
                        actions.delete = Some(state.draft.id);
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let primary_label = if state.is_new {
                        tr!("Create")
                    } else {
                        tr!("Save")
                    };
                    let primary = egui::Button::new(
                        egui::RichText::new(primary_label)
                            .font(crate::theme::font(12.0))
                            .strong()
                            .color(if can_save {
                                crate::theme::on_fill(theme.accent)
                            } else {
                                theme.text_dim
                            }),
                    )
                    .fill(if can_save {
                        theme.accent
                    } else {
                        theme.bg_pill
                    })
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .min_size(Vec2::new(96.0, 30.0));
                    let primary = ui.add_enabled(can_save, primary).on_disabled_hover_text(
                        validation
                            .clone()
                            .unwrap_or_else(|| tr!("Give the playlist a name")),
                    );
                    if primary.clicked() {
                        actions.save = Some(state.draft.clone());
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(tr!(Msg::S2CEB11BE2290BB1B))
                                    .font(crate::theme::font(12.0))
                                    .color(theme.text_secondary),
                            )
                            .fill(theme.bg_pill)
                            .stroke(Stroke::new(1.0, theme.border_subtle))
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .min_size(Vec2::new(84.0, 30.0)),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        });

    if closed || cancel {
        is_open = false;
    }
    state.is_open = is_open;
    actions
}

/// One rule: field, operator, value, and a remove control on a single card.
fn render_rule_row(
    ui: &mut egui::Ui,
    index: usize,
    rule: &mut SmartRule,
    theme: &Theme,
    draft_dirty: &mut bool,
    remove: &mut Option<usize>,
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_subtle))
        .corner_radius(CornerRadius::same(theme.radius_md))
        .inner_margin(Margin::symmetric(
            theme.space_md as i8,
            theme.space_sm as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);

                let total = ui.available_width();
                let field_w = (total * 0.32).clamp(92.0, 150.0);
                let op_w = (total * 0.28).clamp(84.0, 140.0);

                egui::ComboBox::from_id_salt(("sf", index))
                    .selected_text(rule.field.label())
                    .width(field_w)
                    .show_ui(ui, |ui| {
                        for f in SmartField::ALL {
                            if ui.selectable_value(&mut rule.field, f, f.label()).clicked() {
                                // Keep the operator valid for the new field.
                                if !f.operators().contains(&rule.op) {
                                    rule.op =
                                        f.operators().first().copied().unwrap_or(SmartOp::Contains);
                                }
                                *draft_dirty = true;
                            }
                        }
                    });

                egui::ComboBox::from_id_salt(("so", index))
                    .selected_text(rule.op.label())
                    .width(op_w)
                    .show_ui(ui, |ui| {
                        for op in rule.field.operators() {
                            if ui.selectable_value(&mut rule.op, *op, op.label()).clicked() {
                                *draft_dirty = true;
                            }
                        }
                    });

                if rule.op.needs_value() {
                    let hint = if rule.field.is_date() {
                        tr!("days")
                    } else {
                        tr!("value")
                    };
                    let value_w = (ui.available_width() - 32.0).max(56.0);
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut rule.value)
                                .hint_text(hint)
                                .desired_width(value_w),
                        )
                        .changed()
                    {
                        *draft_dirty = true;
                    }
                } else {
                    ui.label(
                        egui::RichText::new(tr!("no value needed"))
                            .font(crate::theme::font(11.5))
                            .color(theme.text_muted),
                    );
                }

                let remove_resp = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(tr!(Msg::SE0A34419921B022A))
                                .font(crate::theme::font(11.0))
                                .color(theme.text_muted),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(theme.radius_sm))
                        .min_size(Vec2::new(26.0, 24.0)),
                    )
                    .on_hover_text(tr!(Msg::S440FB245DF9F0819));
                if remove_resp.clicked() {
                    *remove = Some(index);
                }
            });
        });
}

/// "Add rule" reads as an action on the list, so it is a full-width, quiet
/// button rather than another filled control competing with the footer.
fn add_rule_button(ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), egui::Sense::click());
    let hover_t = if resp.hovered() {
        ui.ctx().animate_bool_with_time(
            resp.id.with("add_rule_hover"),
            true,
            theme.motion_time(0.1),
        )
    } else {
        0.0
    };
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        CornerRadius::same(theme.radius_md),
        crate::theme::lerp_color(theme.bg_card, theme.bg_card_hover, hover_t),
        Stroke::new(1.0, crate::theme::with_alpha(theme.accent, 90)),
        egui::StrokeKind::Inside,
    );
    let color = crate::theme::lerp_color(theme.accent_highlight, theme.accent, hover_t);
    crate::components::icons::draw_plus_circle(
        ui,
        egui::Rect::from_center_size(
            egui::Pos2::new(rect.min.x + 20.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        color,
    );
    let galley = ui.painter().layout_no_wrap(
        tr!("Add rule"),
        crate::theme::font(12.0),
        theme.text_primary,
    );
    painter.galley(
        egui::Pos2::new(rect.min.x + 32.0, rect.center().y - galley.size().y * 0.5),
        galley,
        theme.text_primary,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Add rule"));
    resp
}

/// The live answer to "what will this match?".
fn render_match_summary(ui: &mut egui::Ui, matches: &[Track], theme: &Theme) {
    let dot = if matches.is_empty() {
        theme.warning_yellow
    } else {
        theme.bit_perfect_green
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 16.0), egui::Sense::hover());
        ui.painter().circle_filled(dot_rect.center(), 3.0, dot);
        ui.label(
            egui::RichText::new(if matches.is_empty() {
                tr!("No tracks match yet.")
            } else {
                tr_args!("{n} tracks match", "n" => matches.len())
            })
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_primary),
        );

        // A couple of real titles prove the rule is doing what it says, without
        // turning the footer into a 100-row list nobody scrolls.
        let sample = matches
            .iter()
            .take(2)
            .map(|t| t.title.as_str())
            .collect::<Vec<_>>()
            .join(" · ");
        if !sample.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "{} · {sample}",
                    if matches.len() > 2 { "…" } else { "" }
                ))
                .font(crate::theme::font(11.0))
                .color(theme.text_muted),
            );
        }
    });
}

fn card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme.bg_app)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::symmetric(
            theme.space_xl as i8,
            theme.space_lg as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_sm);
            add_contents(ui);
        });
}

fn section_heading(ui: &mut egui::Ui, text: String, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(crate::theme::font(12.5))
            .strong()
            .color(theme.text_primary),
    );
}

/// A label column of fixed width plus the control, so repeated rows line up
/// instead of nudging apart with label length.
fn labelled_row(
    ui: &mut egui::Ui,
    label: String,
    theme: &Theme,
    label_w: f32,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, theme.space_sm);
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 24.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(12.0))
                        .color(theme.text_secondary),
                );
            },
        );
        add_control(ui);
    });
}

/// Apply the app's form-widget palette to combo boxes and text fields.
fn theme_form_widgets(ui: &mut egui::Ui, theme: &Theme) {
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
}

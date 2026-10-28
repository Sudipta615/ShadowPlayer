//! ZoneSwitch routing rules editor: metadata matching, target zone routing, and priority handling.

use domain::smart::SmartOp;
use domain::zone::{ZoneMatchField, ZoneSwitchCondition, ZoneSwitchRule};
use eframe::egui::{self, CornerRadius, Margin, Stroke, Ui};
use playback::ZoneCoordinator;

use super::actions::ZoneManagerAction;
use super::types::ZoneManagerModalState;
use crate::theme::Theme;

pub fn render_rules_tab(
    ui: &mut Ui,
    state: &mut ZoneManagerModalState,
    coordinator: &ZoneCoordinator,
    theme: &Theme,
    actions: &mut Vec<ZoneManagerAction>,
) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("ZoneSwitch routes playback automatically based on track format, sample rate, or channel count.")
            .color(theme.text_secondary)
            .size(12.0),
    );
    ui.add_space(8.0);

    let all_zones = coordinator.all_zones();
    let rules = coordinator.rules();

    // Existing Rules List
    egui::ScrollArea::vertical()
        .id_salt("rules_list_scroll")
        .max_height(280.0)
        .show(ui, |ui| {
            if rules.is_empty() {
                ui.label(
                    egui::RichText::new("No ZoneSwitch rules configured yet. Create one below.")
                        .italics()
                        .color(theme.text_secondary),
                );
            }

            for rule in rules {
                let rule_id = rule.id.clone();
                let target_name = coordinator
                    .get_zone(rule.target_zone_id)
                    .map(|z| z.config.name.as_str())
                    .unwrap_or("Unknown Zone");

                egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = rule.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                actions.push(ZoneManagerAction::ToggleRule {
                                    rule_id: rule_id.clone(),
                                    enabled,
                                });
                            }

                            ui.label(
                                egui::RichText::new(&rule.name)
                                    .color(theme.text_primary)
                                    .strong()
                                    .size(13.0),
                            );

                            ui.label(
                                egui::RichText::new(format!("→ {target_name}"))
                                    .color(theme.accent)
                                    .strong()
                                    .size(12.0),
                            );

                            ui.label(
                                egui::RichText::new(format!("(Priority: {})", rule.priority))
                                    .color(theme.text_secondary)
                                    .size(11.0),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Delete").clicked() {
                                        actions.push(ZoneManagerAction::DeleteRule(rule_id));
                                    }
                                },
                            );
                        });

                        ui.add_space(4.0);

                        // Conditions summary
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new("Conditions:")
                                    .color(theme.text_secondary)
                                    .size(11.0),
                            );
                            for cond in &rule.conditions {
                                let cond_text = format!(
                                    "{} {} \"{}\"",
                                    cond.field.label(),
                                    cond.op.label(),
                                    cond.value
                                );
                                egui::Frame::NONE
                                    .fill(theme.border_subtle)
                                    .corner_radius(CornerRadius::same(theme.radius_xs))
                                    .inner_margin(Margin::same(4))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(cond_text).size(11.0));
                                    });
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // Add Rule Form
    ui.label(
        egui::RichText::new("Add ZoneSwitch Rule:")
            .strong()
            .size(13.0),
    );
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Rule Name:")
                .color(theme.text_secondary)
                .size(12.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.rule_name)
                .hint_text("e.g. Route DSD or 5.1 Multichannel")
                .desired_width(180.0),
        );

        ui.label(
            egui::RichText::new("Target Zone:")
                .color(theme.text_secondary)
                .size(12.0),
        );
        let selected_target_name = state
            .rule_target_zone
            .and_then(|id| coordinator.get_zone(id))
            .map(|z| z.config.name.as_str())
            .unwrap_or("Select Zone");

        egui::ComboBox::from_id_salt("rule_target_zone_cb")
            .selected_text(selected_target_name)
            .width(150.0)
            .show_ui(ui, |ui| {
                for z in &all_zones {
                    if ui
                        .selectable_label(state.rule_target_zone == Some(z.id), &z.config.name)
                        .clicked()
                    {
                        state.rule_target_zone = Some(z.id);
                    }
                }
            });
    });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Condition:")
                .color(theme.text_secondary)
                .size(12.0),
        );

        // Field dropdown
        egui::ComboBox::from_id_salt("rule_field_cb")
            .selected_text(state.rule_field.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for f in ZoneMatchField::ALL {
                    if ui
                        .selectable_label(state.rule_field == f, f.label())
                        .clicked()
                    {
                        state.rule_field = f;
                    }
                }
            });

        // Op dropdown
        egui::ComboBox::from_id_salt("rule_op_cb")
            .selected_text(state.rule_op.label())
            .width(110.0)
            .show_ui(ui, |ui| {
                let ops = [
                    SmartOp::Equals,
                    SmartOp::NotEquals,
                    SmartOp::Contains,
                    SmartOp::GreaterThan,
                    SmartOp::LessThan,
                    SmartOp::IsTrue,
                    SmartOp::IsFalse,
                ];
                for op in ops {
                    if ui
                        .selectable_label(state.rule_op == op, op.label())
                        .clicked()
                    {
                        state.rule_op = op;
                    }
                }
            });

        // Value text
        ui.add(
            egui::TextEdit::singleline(&mut state.rule_val)
                .hint_text("Value (e.g. DSD, 2, FLAC)")
                .desired_width(120.0),
        );

        // Priority
        ui.label(
            egui::RichText::new("Priority:")
                .color(theme.text_secondary)
                .size(12.0),
        );
        ui.add(egui::DragValue::new(&mut state.rule_priority).range(1..=1000));

        let can_add = !state.rule_name.trim().is_empty()
            && state.rule_target_zone.is_some()
            && (!state.rule_val.trim().is_empty()
                || matches!(state.rule_op, SmartOp::IsTrue | SmartOp::IsFalse));

        if ui
            .add_enabled(can_add, egui::Button::new("+ Add Rule"))
            .clicked()
        {
            if let Some(target_zone_id) = state.rule_target_zone {
                let cond = ZoneSwitchCondition::new(
                    state.rule_field,
                    state.rule_op,
                    state.rule_val.trim(),
                );
                let new_rule = ZoneSwitchRule::new(
                    state.rule_name.trim(),
                    target_zone_id,
                    vec![cond],
                    state.rule_priority,
                );
                actions.push(ZoneManagerAction::SaveRule(new_rule));
                state.rule_name.clear();
            }
        }
    });
}

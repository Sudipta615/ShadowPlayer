//! Zones tab rendering: active zones, master/slave linking, timing alignment, and clock drift.

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Ui};
use playback::ZoneCoordinator;

use super::actions::ZoneManagerAction;
use super::types::ZoneManagerModalState;
use crate::theme::Theme;

pub fn render_zones_tab(
    ui: &mut Ui,
    state: &mut ZoneManagerModalState,
    coordinator: &ZoneCoordinator,
    available_devices: &[String],
    theme: &Theme,
    actions: &mut Vec<ZoneManagerAction>,
) {
    let active_id = coordinator.active_zone_id();
    let all_zones = coordinator.all_zones();

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Independent audio zones with isolated queues, DAC bindings, and master-slave clock sync.")
            .color(theme.text_secondary)
            .size(12.0),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("zones_list_scroll")
        .max_height(340.0)
        .show(ui, |ui| {
            for zone_state in &all_zones {
                let id = zone_state.id;
                let is_active = id == active_id;
                let is_default = zone_state.config.is_default;
                let is_slave = zone_state.config.linked_master_id.is_some();

                egui::Frame::NONE
                    .fill(if is_active {
                        theme.accent.linear_multiply(0.12)
                    } else {
                        theme.bg_card
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if is_active {
                            theme.accent
                        } else {
                            theme.border_subtle
                        },
                    ))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        // Title bar
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&zone_state.config.name)
                                    .color(theme.text_primary)
                                    .strong()
                                    .size(14.0),
                            );

                            if is_default {
                                egui::Frame::NONE
                                    .fill(theme.border_subtle)
                                    .corner_radius(CornerRadius::same(theme.radius_sm))
                                    .inner_margin(Margin {
                                        left: 5,
                                        right: 5,
                                        top: 2,
                                        bottom: 2,
                                    })
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("DEFAULT")
                                                .color(theme.text_secondary)
                                                .size(10.0)
                                                .strong(),
                                        );
                                    });
                            }

                            if is_active {
                                egui::Frame::NONE
                                    .fill(theme.accent)
                                    .corner_radius(CornerRadius::same(theme.radius_sm))
                                    .inner_margin(Margin {
                                        left: 5,
                                        right: 5,
                                        top: 2,
                                        bottom: 2,
                                    })
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("ACTIVE")
                                                .color(Color32::WHITE)
                                                .size(10.0)
                                                .strong(),
                                        );
                                    });
                            }

                            if zone_state.is_playing {
                                egui::Frame::NONE
                                    .fill(Color32::from_rgb(46, 125, 50))
                                    .corner_radius(CornerRadius::same(theme.radius_sm))
                                    .inner_margin(Margin {
                                        left: 5,
                                        right: 5,
                                        top: 2,
                                        bottom: 2,
                                    })
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("PLAYING")
                                                .color(Color32::WHITE)
                                                .size(10.0)
                                                .strong(),
                                        );
                                    });
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !is_default && ui.button("Delete").clicked() {
                                        actions.push(ZoneManagerAction::DeleteZone(id));
                                    }
                                    if !is_active && ui.button("Focus Zone").clicked() {
                                        actions.push(ZoneManagerAction::SetActiveZone(id));
                                    }
                                },
                            );
                        });

                        ui.add_space(8.0);

                        // Device selector row
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Output Device:")
                                    .color(theme.text_secondary)
                                    .size(12.0),
                            );
                            let current_device = zone_state
                                .config
                                .output_device
                                .as_deref()
                                .unwrap_or("System Default");
                            egui::ComboBox::from_id_salt(format!("device_cb_{id}"))
                                .selected_text(current_device)
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            zone_state.config.output_device.is_none(),
                                            "System Default",
                                        )
                                        .clicked()
                                    {
                                        actions.push(ZoneManagerAction::SetOutputDevice {
                                            zone_id: id,
                                            device: None,
                                        });
                                    }
                                    for dev in available_devices {
                                        let selected =
                                            zone_state.config.output_device.as_deref() == Some(dev);
                                        if ui.selectable_label(selected, dev).clicked() {
                                            actions.push(ZoneManagerAction::SetOutputDevice {
                                                zone_id: id,
                                                device: Some(dev.clone()),
                                            });
                                        }
                                    }
                                });
                        });

                        ui.add_space(6.0);

                        // Volume & Mute row
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Volume:")
                                    .color(theme.text_secondary)
                                    .size(12.0),
                            );
                            let mut vol = zone_state.config.volume;
                            if ui
                                .add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false))
                                .changed()
                            {
                                actions.push(ZoneManagerAction::SetVolume {
                                    zone_id: id,
                                    volume: vol,
                                });
                            }
                            ui.label(format!("{:.0}%", vol * 100.0));

                            let muted = zone_state.config.muted;
                            let mute_btn = if muted { "Unmute" } else { "Mute" };
                            if ui.button(mute_btn).clicked() {
                                actions.push(ZoneManagerAction::SetMuted {
                                    zone_id: id,
                                    muted: !muted,
                                });
                            }
                        });

                        ui.add_space(6.0);

                        // Linking & Clock Drift Controls
                        ui.horizontal(|ui| {
                            if let Some(master_id) = zone_state.config.linked_master_id {
                                let master_name = coordinator
                                    .get_zone(master_id)
                                    .map(|z| z.config.name.as_str())
                                    .unwrap_or("Master");
                                ui.label(
                                    egui::RichText::new(format!("Linked Slave to: {master_name}"))
                                        .color(theme.accent)
                                        .size(12.0),
                                );
                                if ui.small_button("Unlink").clicked() {
                                    actions.push(ZoneManagerAction::UnlinkZone(id));
                                }
                            } else {
                                let other_masters: Vec<_> = all_zones
                                    .iter()
                                    .filter(|z| z.id != id && z.config.linked_master_id.is_none())
                                    .collect();
                                if !other_masters.is_empty() {
                                    egui::ComboBox::from_id_salt(format!("link_cb_{id}"))
                                        .selected_text("Link as slave to...")
                                        .show_ui(ui, |ui| {
                                            for m in other_masters {
                                                if ui
                                                    .selectable_label(false, &m.config.name)
                                                    .clicked()
                                                {
                                                    actions.push(ZoneManagerAction::LinkZone {
                                                        master: m.id,
                                                        slave: id,
                                                        offset_ms: 0.0,
                                                    });
                                                }
                                            }
                                        });
                                }
                            }
                        });

                        // Show timing alignment slider & clock drift if slave
                        if is_slave {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Timing Offset:")
                                        .color(theme.text_secondary)
                                        .size(11.0),
                                );
                                let mut offset = zone_state.config.timing_offset_ms;
                                if ui
                                    .add(egui::Slider::new(&mut offset, -50.0..=50.0).suffix(" ms"))
                                    .changed()
                                {
                                    actions.push(ZoneManagerAction::SetTimingOffset {
                                        zone_id: id,
                                        offset_ms: offset,
                                    });
                                }
                            });

                            ui.horizontal(|ui| {
                                let drift = zone_state.config.clock_drift_pct;
                                let ratio = zone_state.clock_drift.resampler_rate_ratio();
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Clock Drift Compensation: {:+.03}% (Rate: {:.5}x)",
                                        drift, ratio
                                    ))
                                    .color(theme.text_secondary)
                                    .size(11.0),
                                );
                            });
                        }
                    });
                ui.add_space(6.0);
            }
        });

    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);

    // Create New Zone Section
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("New Zone:").strong().size(13.0));
        ui.add(
            egui::TextEdit::singleline(&mut state.new_zone_name)
                .hint_text("Zone Name (e.g. Office DAC)")
                .desired_width(180.0),
        );

        let dev_label = if state.new_zone_device.is_empty() {
            "Select Device (Optional)"
        } else {
            &state.new_zone_device
        };

        egui::ComboBox::from_id_salt("new_zone_dev_cb")
            .selected_text(dev_label)
            .width(180.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.new_zone_device.is_empty(), "System Default")
                    .clicked()
                {
                    state.new_zone_device.clear();
                }
                for dev in available_devices {
                    if ui
                        .selectable_label(state.new_zone_device == *dev, dev)
                        .clicked()
                    {
                        state.new_zone_device = dev.clone();
                    }
                }
            });

        let can_create = !state.new_zone_name.trim().is_empty();
        if ui
            .add_enabled(can_create, egui::Button::new("+ Add Zone"))
            .clicked()
        {
            let name = state.new_zone_name.trim().to_string();
            let device = if state.new_zone_device.is_empty() {
                None
            } else {
                Some(state.new_zone_device.clone())
            };
            actions.push(ZoneManagerAction::CreateZone { name, device });
            state.new_zone_name.clear();
            state.new_zone_device.clear();
        }
    });
}

//! The CLAP plugin rack window.
//!
//! A CLAP plugin's editor is a native window and this host cannot embed one, so
//! the rack offers what a host *can* honestly offer instead: every parameter
//! the plugin declares as a live control, its own bypass parameter shown for
//! what it is, the library's plugin list when a file turns out to hold several,
//! and named presets that capture both the values and the plugin's own state.
//!
//! Nothing here talks to the engine. The window reads a [`ClapRackView`] the
//! app already fetched and returns a [`ClapRackActions`] for the app to apply,
//! which keeps "what the rack looks like" and "what the rack does" separable —
//! and testable without a window.

use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Stroke, Vec2, Window};

use crate::state::clap_rack::{ClapRackActions, ClapRackPreset, ClapRackState};
use crate::theme::Theme;

use engine::dsp::clap_rack::ClapRackView;

/// Draw the rack. Returns the actions the user took this frame.
pub fn show_clap_rack_panel(
    ctx: &egui::Context,
    state: &mut ClapRackState,
    view: &ClapRackView,
    theme: &Theme,
) -> ClapRackActions {
    let mut actions = ClapRackActions::default();

    if !state.is_open {
        return actions;
    }

    let screen = ctx
        .input(|i| i.raw.screen_rect)
        .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 830.0)));
    let default_w = 640.0_f32.min((screen.width() - 40.0).max(420.0));
    let default_h = 680.0_f32.min((screen.height() - 40.0).max(360.0));
    let default_x = ((screen.width() - default_w) / 2.0).max(10.0);
    let default_y = ((screen.height() - default_h) / 2.0).max(10.0);
    let initial_pos = state
        .window_pos
        .map(|[x, y]| Pos2::new(x, y))
        .unwrap_or_else(|| Pos2::new(default_x, default_y));

    let window = Window::new("clap_plugin_rack")
        .resizable(true)
        .movable(true)
        .collapsible(false)
        .title_bar(false)
        .default_pos(initial_pos)
        .default_size(Vec2::new(default_w, default_h))
        .min_size(crate::dialog::min_dialog_size())
        .frame(
            egui::Frame::window(&ctx.style_of(ctx.theme()))
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0_f32, theme.border_card))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(18, 14)),
        )
        .show(ctx, |ui| {
            render_header(ui, state, view, theme, &mut actions);

            ui.add_space(10.0);

            if !view.rack.is_loaded() {
                render_unloaded(ui, state, view, theme, &mut actions);
            } else {
                egui::ScrollArea::vertical()
                    .id_salt("clap_rack_body")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
                        render_parameters(ui, view, theme, &mut actions);
                        ui.add_space(6.0);
                        render_presets(ui, state, view, theme, &mut actions);
                    });
            }
        });

    // Remember where the user put it, so the window reopens where it was left.
    if let Some(inner) = window {
        state.window_pos = Some([inner.response.rect.min.x, inner.response.rect.min.y]);
    }

    actions
}

/// Title bar: what is loaded, the host bypass, and the file-level buttons.
fn render_header(
    ui: &mut egui::Ui,
    state: &mut ClapRackState,
    view: &ClapRackView,
    theme: &Theme,
    actions: &mut ClapRackActions,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Plugin Rack")
                .font(crate::theme::font(15.0))
                .color(theme.text_primary)
                .strong(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if pill(ui, "Close", theme, false).clicked() {
                actions.close = true;
            }
        });
    });

    ui.add_space(2.0);

    // Status block: the plugin, or the reason there is not one.
    egui::Frame::NONE
        .fill(theme.bg_input)
        .stroke(Stroke::new(1.0, theme.border_subtle))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);

            match &view.rack.plugin {
                Some(plugin) => {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&plugin.name)
                                .font(crate::theme::font(13.5))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        if !plugin.vendor.trim().is_empty() {
                            ui.label(
                                egui::RichText::new(format!("· {}", plugin.vendor))
                                    .font(crate::theme::font(11.5))
                                    .color(theme.text_secondary),
                            );
                        }
                        if !plugin.version.trim().is_empty() {
                            ui.label(
                                egui::RichText::new(format!("v{}", plugin.version))
                                    .font(crate::theme::font(11.0))
                                    .color(theme.text_muted),
                            );
                        }
                    });
                }
                None => {
                    ui.label(
                        egui::RichText::new("No plugin loaded")
                            .font(crate::theme::font(13.0))
                            .color(theme.text_secondary),
                    );
                }
            }

            if let Some(error) = &view.rack.error {
                let text = egui::RichText::new(error)
                    .font(crate::theme::font(11.0))
                    .color(theme.destructive);
                ui.label(text);
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);

                // Host bypass and the plugin's own bypass are different things,
                // so they are labelled as such: this one takes the insert out of
                // the chain entirely, which is what makes it usable for A/B.
                let mut enabled = !view.rack.bypassed;
                let toggle = ui
                    .add_enabled(
                        view.rack.is_loaded(),
                        egui::Checkbox::new(&mut enabled, "Insert active"),
                    )
                    .on_hover_text(
                        "Take the plugin out of the signal path. Its parameters and state are kept.",
                    );
                if toggle.changed() {
                    actions.set_enabled = Some(enabled);
                }

                if view.rack.is_loaded() && view.rack.has_state {
                    ui.label(
                        egui::RichText::new("· state available")
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                }
                if view.rack.latency_samples > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "· {} samples latency",
                            view.rack.latency_samples
                        ))
                        .font(crate::theme::font(10.5))
                        .color(theme.text_muted),
                    );
                }
            });
        });

    ui.add_space(8.0);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);

        if pill(ui, "Choose Plugin File…", theme, true).clicked() {
            actions.choose_library = true;
        }
        if pill(ui, "Re-read", theme, false)
            .on_hover_text("Ask the engine to republish the plugin and its parameters.")
            .clicked()
        {
            actions.refresh = true;
        }
        if view.rack.is_loaded() && pill(ui, "Unload", theme, false).clicked() {
            actions.unload = true;
        }
    });

    if let Some(notice) = &state.notice {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(notice)
                .font(crate::theme::font(11.0))
                .color(theme.info),
        );
    }
}

/// Nothing loaded: explain the situation and offer whatever a probe found.
fn render_unloaded(
    ui: &mut egui::Ui,
    state: &mut ClapRackState,
    view: &ClapRackView,
    theme: &Theme,
    actions: &mut ClapRackActions,
) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "Load a CLAP plugin to insert it after the volume stage. A plugin's own \
             editor is a native window, so this rack drives every parameter the \
             plugin publishes instead.",
        )
        .font(crate::theme::font(11.5))
        .color(theme.text_secondary),
    );

    let Some(library) = &view.library else {
        return;
    };
    // A probe from an earlier file is not evidence about this window's state.
    if let Some(expected) = &state.probe_path {
        if &library.path != expected {
            return;
        }
    }

    ui.add_space(10.0);

    if let Some(error) = &library.error {
        ui.label(
            egui::RichText::new(format!("{}: {error}", library.path))
                .font(crate::theme::font(11.0))
                .color(theme.destructive),
        );
        ui.add_space(6.0);
        // A probe can fail for a reason outside the player's hands — the file
        // was still being written, say — so retrying it should not mean finding
        // the file all over again.
        if pill(ui, "Try Again", theme, false).clicked() {
            actions.probe_library = Some(library.path.clone());
        }
        return;
    }

    if library.plugins.is_empty() {
        ui.label(
            egui::RichText::new(format!("{} contains no CLAP plugins.", library.path))
                .font(crate::theme::font(11.5))
                .color(theme.text_secondary),
        );
        return;
    }

    ui.label(
        egui::RichText::new(if library.plugins.len() == 1 {
            "This library contains one plugin:".to_string()
        } else {
            format!("{} plugins in this library:", library.plugins.len())
        })
        .font(crate::theme::font(11.5))
        .color(theme.text_primary),
    );
    ui.add_space(4.0);

    for plugin in &library.plugins {
        egui::Frame::NONE
            .fill(theme.bg_pill)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                        ui.label(
                            egui::RichText::new(&plugin.name)
                                .font(crate::theme::font(12.5))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        let detail = if plugin.vendor.trim().is_empty() {
                            plugin.id.clone()
                        } else {
                            format!("{} · {}", plugin.vendor, plugin.id)
                        };
                        ui.label(
                            egui::RichText::new(detail)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if pill(ui, "Load", theme, true).clicked() {
                            actions.load_plugin = Some((library.path.clone(), plugin.id.clone()));
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
}

/// Every editable parameter, as a control.
fn render_parameters(
    ui: &mut egui::Ui,
    view: &ClapRackView,
    theme: &Theme,
    actions: &mut ClapRackActions,
) {
    let editable: Vec<_> = view.rack.editable_params().cloned().collect();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Parameters ({})", editable.len()))
                .font(crate::theme::font(12.5))
                .color(theme.text_primary)
                .strong(),
        );
        if editable.len() < view.rack.params.len() {
            ui.label(
                egui::RichText::new(format!(
                    "· {} hidden or read-only",
                    view.rack.params.len() - editable.len()
                ))
                .font(crate::theme::font(10.5))
                .color(theme.text_muted),
            );
        }
    });

    if editable.is_empty() {
        ui.label(
            egui::RichText::new("This plugin publishes no editable parameters.")
                .font(crate::theme::font(11.5))
                .color(theme.text_secondary),
        );
        return;
    }

    ui.add_space(4.0);

    for param in &editable {
        // An on/off switch is a checkbox, not a slider the user has to drag
        // from one end to the other. CLAP gives no way to enumerate a stepped
        // parameter's labels, so the test is the shape of its range.
        let is_switch = param.is_stepped && param.min == 0.0 && param.max == 1.0;

        ui.horizontal(|ui| {
            param_label(ui, param, theme);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if is_switch {
                    let mut on = param.value >= 0.5;
                    let caption = if param.value_text.trim().is_empty() {
                        if on { "On" } else { "Off" }.to_string()
                    } else {
                        param.value_text.clone()
                    };
                    if ui.checkbox(&mut on, caption).changed() {
                        actions.set_param = Some((param.id, if on { 1.0 } else { 0.0 }));
                    }
                } else if !param.value_text.trim().is_empty() {
                    // The plugin's own rendering of the value is what its editor
                    // would show — for an enumeration, the name of the mode.
                    ui.label(
                        egui::RichText::new(&param.value_text)
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!("{:.3}", param.value))
                            .font(crate::theme::font(10.5))
                            .color(theme.text_muted),
                    );
                }
            });
        });

        if is_switch {
            continue;
        }

        let mut value = param.value;
        let span = param.max - param.min;
        let mut slider = egui::Slider::new(&mut value, param.min..=param.max)
            .show_value(false)
            .trailing_fill(true);
        // A stepped parameter is a list of choices; dragging between them is
        // meaningless and the plugin would quantise it anyway.
        if param.is_stepped && span > 0.0 && span <= 64.0 {
            slider = slider.step_by(1.0);
        }
        let response = ui.add_sized(
            Vec2::new(ui.available_width(), 18.0),
            slider.smart_aim(false),
        );
        if response.changed() {
            actions.set_param = Some((param.id, value));
        }
    }
}

/// One parameter's name, with its grouping path, coloured so a plugin's own
/// bypass switch stands out from the controls that change the sound.
fn param_label(ui: &mut egui::Ui, param: &engine::dsp::clap_rack::ClapParamInfo, theme: &Theme) {
    let label = if param.module.trim().is_empty() {
        param.name.clone()
    } else {
        format!("{}/{}", param.module, param.name)
    };
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(11.5))
            .color(if param.is_bypass {
                theme.accent_warm
            } else {
                theme.text_secondary
            }),
    );
}

/// Presets: named captures of the values plus the plugin's own state.
fn render_presets(
    ui: &mut egui::Ui,
    state: &mut ClapRackState,
    view: &ClapRackView,
    theme: &Theme,
    actions: &mut ClapRackActions,
) {
    ui.separator();
    ui.add_space(4.0);

    let plugin_id = view
        .rack
        .plugin
        .as_ref()
        .map(|p| p.id.clone())
        .unwrap_or_default();
    let relevant = state.presets_for(&plugin_id);

    ui.label(
        egui::RichText::new(format!("Presets for this plugin ({})", relevant.len()))
            .font(crate::theme::font(12.5))
            .color(theme.text_primary)
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "A preset stores the parameter values the host set, and the plugin's own \
             state when it has one.",
        )
        .font(crate::theme::font(10.5))
        .color(theme.text_muted),
    );

    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.add_sized(
            Vec2::new(170.0, 22.0),
            egui::TextEdit::singleline(&mut state.preset_name).hint_text("Preset name"),
        );
        ui.add_sized(
            Vec2::new(150.0, 22.0),
            egui::TextEdit::singleline(&mut state.preset_note).hint_text("Note (optional)"),
        );
        if pill(ui, "Save Preset", theme, true).clicked() {
            actions.save_preset = true;
        }
    });

    ui.add_space(6.0);

    if relevant.is_empty() {
        ui.label(
            egui::RichText::new("No presets saved for this plugin yet.")
                .font(crate::theme::font(11.0))
                .color(theme.text_secondary),
        );
        return;
    }

    // Draw from a copy of the indices so the list can be mutated by the actions
    // without confusing the loop.
    for index in relevant {
        let Some(preset) = state.presets.get(index) else {
            continue;
        };
        render_preset_row(ui, preset, index, state.selected_preset, theme, actions);
    }
}

fn render_preset_row(
    ui: &mut egui::Ui,
    preset: &ClapRackPreset,
    index: usize,
    selected: Option<usize>,
    theme: &Theme,
    actions: &mut ClapRackActions,
) {
    let background = if selected == Some(index) {
        theme.bg_card_active
    } else {
        theme.bg_pill
    };

    egui::Frame::NONE
        .fill(background)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                    ui.label(
                        egui::RichText::new(&preset.name)
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                    );
                    let detail = format!(
                        "{} · {} values{}",
                        preset.summary(),
                        preset.params.len(),
                        if preset.has_state() {
                            " + state"
                        } else {
                            " (no state)"
                        }
                    );
                    ui.label(
                        egui::RichText::new(detail)
                            .font(crate::theme::font(10.0))
                            .color(theme.text_muted),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill(ui, "Delete", theme, false).clicked() {
                        actions.delete_preset = Some(index);
                    }
                    if pill(ui, "Recall", theme, true).clicked() {
                        actions.apply_preset = Some(index);
                    }
                });
            });
        });
    ui.add_space(4.0);
}

/// The app's pill button, reproduced here so the rack does not depend on the
/// settings tab's private helpers.
fn pill(ui: &mut egui::Ui, label: &str, theme: &Theme, primary: bool) -> egui::Response {
    let text = egui::RichText::new(label)
        .font(crate::theme::font(11.5))
        .color(if primary {
            Color32::WHITE
        } else {
            theme.text_primary
        });
    let fill = if primary { theme.accent } else { theme.bg_pill };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .corner_radius(CornerRadius::same(7)),
    )
}

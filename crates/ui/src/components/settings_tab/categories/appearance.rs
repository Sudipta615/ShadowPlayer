//! The Appearance category: theme palette, custom themes, and the editor.

use super::super::widgets::{card_header, color_dot, setting_card, setting_item};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius};

pub(in crate::components::settings_tab) fn render_appearance_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    let Some(manager) = extra.theme_manager else {
        return;
    };

    // Card 1: Active Theme & Quick Preset Switch
    setting_card(ui, theme, |ui| {
        let badge_name = if theme.is_light {
            "STUDIO MINT"
        } else if theme.is_oled() {
            "OLED MIDNIGHT"
        } else {
            "OBSIDIAN JADE"
        };

        card_header(
            ui,
            crate::components::icons::draw_moon,
            &tr!("Theme & Visual Aesthetics"),
            &tr!("Obsidian surfaces, jade highlights, and typography for the player shell"),
            Some((badge_name, theme.bg_pill, theme.accent)),
            theme,
        );

        setting_item(
            ui,
            &tr!("Theme Palette"),
            &tr!("Switch between Obsidian dark and Jade light mode"),
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    let is_obsidian = theme.is_obsidian();
                    let is_light = theme.is_light;
                    let is_oled = theme.is_oled();

                    if ui.selectable_label(is_obsidian, "Obsidian Dark").clicked() {
                        actions.apply_theme = Some("system:dark".to_string());
                    }
                    if ui.selectable_label(is_light, "Studio Light").clicked() {
                        actions.apply_theme = Some("system:light".to_string());
                    }
                    if ui.selectable_label(is_oled, "OLED True Black").clicked() {
                        actions.apply_theme = Some("system:oled".to_string());
                    }
                });
            },
        );

        setting_item(
            ui,
            &tr!("Signature Palette"),
            &tr!("Obsidian & Jade: Jade (#0F766E), Mint (#5EEAD4), Amber (#F59E0B), Emerald (#34D399)"),
            theme,
            |ui| {
                color_dot(ui, theme.bg_app, "App Surface");
                color_dot(ui, theme.accent, "Primary Accent");
                color_dot(ui, theme.accent_highlight, "Highlight Mint");
                color_dot(ui, theme.accent_warm, "Warm Amber");
                color_dot(ui, theme.bit_perfect_green, "Bit-Perfect");
            },
        );

        setting_item(
            ui,
            "Custom Themes Directory",
            &format!(
                "Place .shadowtheme files in {}",
                manager.custom_themes_dir.display()
            ),
            theme,
            |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Import Theme…")
                                    .font(crate::theme::font(12.0))
                                    .color(Color32::WHITE),
                            )
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_sm)),
                        )
                        .on_hover_text("Open file chooser to import .shadowtheme files")
                        .clicked()
                    {
                        actions.import_theme = true;
                    }

                    if ui
                        .button(
                            egui::RichText::new("Open Folder")
                                .font(crate::theme::font(12.0))
                                .color(theme.text_secondary),
                        )
                        .on_hover_text("Open custom-theme directory in your system file manager")
                        .clicked()
                    {
                        actions.open_custom_themes_folder = true;
                    }
                });
            },
        );
    });

    // Card 2: Staged Live Preview Sandbox
    let staged_id = manager.staged_id.as_deref();
    let preview_id = staged_id.unwrap_or_else(|| manager.active_id());
    let preview_theme = manager.resolve_theme(preview_id, theme.is_light);
    let preview_name = match preview_id {
        "system:dark" => "Obsidian Jade",
        "system:light" => "Studio Mint",
        "system:oled" => "OLED Midnight",
        other => other,
    };

    let p_actions = crate::theme::preview::render_theme_preview(
        ui,
        theme,
        &preview_theme,
        preview_id,
        preview_name,
        manager.is_staged(),
    );

    if let Some(apply_id) = p_actions.apply_theme {
        actions.apply_theme = Some(apply_id);
    }
    if p_actions.cancel_preview {
        actions.cancel_theme_preview = true;
    }
    if let Some(edit_id) = p_actions.open_editor {
        state.theme_editor.is_open = true;
        let t = manager.resolve_theme(&edit_id, theme.is_light);
        state.theme_editor.populate_from_theme(&t);
        state.theme_editor.fonts = manager.resolve_fonts(&edit_id);
        state.theme_editor.name = format!("{edit_id} Custom");
        state.theme_editor.id = format!("{edit_id}_custom").to_lowercase().replace(':', "_");
    }

    ui.add_space(theme.space_md);

    // Card 3: Theme Gallery & Browser
    setting_card(ui, theme, |ui| {
        let has_errs = !manager.errors().is_empty();
        card_header(
            ui,
            crate::components::icons::draw_grid_icon,
            "Theme Gallery",
            "Browse curated and user-defined themes with category filters and search",
            Some((
                if has_errs {
                    "ERRORS DETECTED"
                } else {
                    "CATALOG READY"
                },
                theme.bg_pill,
                if has_errs {
                    theme.warning_yellow
                } else {
                    theme.bit_perfect_green
                },
            )),
            theme,
        );

        let g_actions = crate::theme::gallery::render_theme_gallery(
            ui,
            &mut state.theme_gallery,
            manager,
            theme,
        );

        if let Some(prev_id) = g_actions.preview_theme {
            actions.preview_theme = Some(prev_id);
        }
        if let Some(app_id) = g_actions.apply_theme {
            actions.apply_theme = Some(app_id);
        }
        if g_actions.reload_themes {
            actions.reload_themes = true;
        }
        if g_actions.import_theme {
            actions.import_theme = true;
        }
        if g_actions.open_custom_themes_folder {
            actions.open_custom_themes_folder = true;
        }

        if has_errs {
            ui.add_space(theme.space_sm);
            ui.label(
                egui::RichText::new(format!(
                    "Warning: {} theme file(s) failed WCAG or JSON validation",
                    manager.errors().len()
                ))
                .font(crate::theme::font(11.0))
                .color(theme.warning_yellow),
            );
        }
    });

    ui.add_space(theme.space_md);

    // Card 4: In-App Theme Editor
    let e_actions =
        crate::theme::editor::render_theme_editor(ui, &mut state.theme_editor, manager, theme);
    if let Some(saved_id) = e_actions.theme_saved {
        actions.reload_themes = true;
        actions.apply_theme = Some(saved_id);
    }
}

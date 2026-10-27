//! The Appearance category: theme gallery and editor.

use super::super::widgets::{card_header, setting_card, setting_item, toggle_switch};
use super::super::{SettingsExtraContext, SettingsTabActions, SettingsTabState};
use crate::theme::Theme;
use eframe::egui;

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

    // Card 1: Surface Material (Frosted Glass Optics)
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_moon,
            "Surface Material",
            "Translucency, optical specular highlights, and ambient bloom across themes",
            Some((
                if state.frosted_glass_enabled {
                    "ACTIVE"
                } else {
                    "OFF"
                },
                theme.bg_pill,
                if state.frosted_glass_enabled {
                    theme.accent
                } else {
                    theme.text_dim
                },
            )),
            theme,
        );

        let prev = state.frosted_glass_enabled;
        setting_item(
            ui,
            "Frosted Glass Surfaces",
            "Render cards and panels with authentic translucency, micro-bevel specular rims, and ambient artwork bloom across any active theme",
            theme,
            |ui| {
                if toggle_switch(ui, &mut state.frosted_glass_enabled, theme) {
                    actions.frosted_glass_changed = Some(state.frosted_glass_enabled);
                }
            },
        );
        if prev != state.frosted_glass_enabled && actions.frosted_glass_changed.is_none() {
            actions.frosted_glass_changed = Some(state.frosted_glass_enabled);
        }
    });

    ui.add_space(theme.space_md);

    // Card 2: Theme Gallery & Browser
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

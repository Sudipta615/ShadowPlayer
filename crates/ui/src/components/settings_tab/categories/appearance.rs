//! The Appearance category: theme gallery and editor.

use super::super::widgets::{card_header, setting_card, setting_item_toggle};
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

        if setting_item_toggle(
            ui,
            "Frosted Glass Surfaces",
            "Render cards and panels with authentic translucency, micro-bevel specular rims, and ambient artwork bloom across any active theme",
            theme,
            &mut state.frosted_glass_enabled,
        ) {
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
        if g_actions.cancel_preview {
            actions.cancel_theme_preview = true;
        }
        if g_actions.reload_themes {
            actions.reload_themes = true;
        }
        if g_actions.import_theme {
            actions.import_theme = true;
        }
        if let Some(edit_id) = g_actions.open_editor {
            let card = manager
                .all_theme_cards()
                .into_iter()
                .find(|card| card.id == edit_id);
            let display_name = card
                .as_ref()
                .map(|card| card.name.clone())
                .unwrap_or_else(|| edit_id.clone());
            let base = manager.resolve_theme(&edit_id, theme.is_light);
            // A clone keeps everything the gallery card advertises, including
            // the typeface roles the editor would otherwise overwrite on save.
            let fonts = card
                .as_ref()
                .map(|card| card.fonts.clone())
                .unwrap_or_default();
            state
                .theme_editor
                .request_load_for_customizing(&base, &display_name, &edit_id, fonts);
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

    // Card 3: In-App Theme Editor
    let mut saved_theme = None;
    let editor_open = state.theme_editor.is_open;
    let editor_dirty = editor_open && state.theme_editor.is_dirty();
    setting_card(ui, theme, |ui| {
        card_header(
            ui,
            crate::components::icons::draw_palette,
            "Theme Editor",
            "Tune colors, radii, typography and the transport silhouette with a live preview",
            Some((
                if editor_dirty {
                    "UNSAVED"
                } else if editor_open {
                    "OPEN"
                } else {
                    "READY"
                },
                theme.bg_pill,
                if editor_dirty {
                    theme.accent_warm
                } else if editor_open {
                    theme.accent_highlight
                } else {
                    theme.text_muted
                },
            )),
            theme,
        );

        let e_actions =
            crate::theme::editor::render_theme_editor(ui, &mut state.theme_editor, manager, theme);
        saved_theme = e_actions.theme_saved;
    });

    if let Some(saved_id) = saved_theme {
        actions.reload_themes = true;
        actions.apply_theme = Some(saved_id);
    }
}

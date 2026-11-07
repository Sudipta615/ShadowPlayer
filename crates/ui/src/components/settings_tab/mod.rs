//! The Settings panel.
//!
//! The panel is split into focused modules:
//!
//! - [`index`] — the category model and the searchable row index.
//! - [`state`] — persisted state, actions, and borrowed view context.
//! - [`widgets`] — shared cards, rows, sliders, and toggles.
//! - [`nav`] — the category rail, the compact switcher, and the persona toggle.
//! - [`search`] — the search box and its search-as-jump results.
//! - [`jump`] — the one-frame scroll-and-flash request.
//! - [`categories`] — one renderer per settings category.
//! - [`cards`] — larger cards that compose several rows.
//!
//! `mod.rs` owns the tab strip and dispatches to the active category.

use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Vec2};

mod cards;
mod categories;
mod index;
mod jump;
mod nav;
mod search;
mod state;
mod widgets;

use categories::{
    render_appearance_category, render_converter_category, render_library_category,
    render_mixbus_category, render_output_category, render_playback_category,
    render_spatial_category, render_tools_category,
};
use nav::{render_settings_nav, render_settings_nav_compact, render_view_mode_toggle_pill};
use search::{render_settings_search, render_settings_search_results};

pub use index::{search_settings, CategoryGroup, SettingsCategory, SettingsEntry, SETTINGS_INDEX};
pub use jump::{request_settings_jump, settings_jump_pending};
pub use state::{
    CrossfadePreviewContext, RecommendationSettingsContext, SettingsExtraContext,
    SettingsTabActions, SettingsTabState, SettingsViewMode,
};
pub use widgets::{
    card_header, card_header_with_info, dual_tone_slider, pill_btn, segment_picker, setting_card,
    setting_item, setting_item_toggle, setting_item_with_info, toggle_switch,
};

pub fn show_settings_tab_extended(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
) -> SettingsTabActions {
    let mut actions = SettingsTabActions::default();
    // A pending jump should land on its row without a smooth-scroll detour.
    let jump_pending = settings_jump_pending();

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_lg);

        // 0. Search & View Mode Switcher
        let is_wide = ui.available_width() >= SETTINGS_COMPACT_BREAKPOINT;
        if is_wide {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);
                let mode_w = 195.0;
                let search_w = (ui.available_width() - mode_w - theme.space_lg).max(220.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(search_w, 32.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        render_settings_search(ui, state, theme);
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    render_view_mode_toggle_pill(ui, state, theme);
                });
            });
        } else {
            render_settings_search(ui, state, theme);
        }

        if !state.search_query.trim().is_empty() {
            render_settings_search_results(ui, state, theme);
            ui.add_space(32.0);
            return;
        }

        ui.add_space(theme.space_sm);

        // 1. Master-detail layout. Wide panes get the vertical category
        // rail with in-page sub-headings; a narrow pane gets a wrapping chip
        // row instead, because a fixed 224 px rail on a 640 px window would
        // leave the cards less room than a phone.
        if ui.available_width() >= SETTINGS_COMPACT_BREAKPOINT {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

                ui.allocate_ui_with_layout(
                    Vec2::new(SETTINGS_NAV_WIDTH, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| render_settings_nav(ui, state, theme),
                );

                // A hairline divider instead of a full-weight egui separator:
                // the two columns are peers, not sections.
                let divider_x = ui.cursor().left() + theme.space_xl * 0.5;
                let divider_rect = Rect::from_min_max(
                    Pos2::new(divider_x, ui.cursor().top()),
                    Pos2::new(divider_x + 1.0, ui.cursor().top() + ui.available_height()),
                );
                ui.painter()
                    .rect_filled(divider_rect, CornerRadius::ZERO, theme.border_subtle);
                ui.add_space(theme.space_xl);

                let right_w = ui.available_width();
                ui.allocate_ui_with_layout(
                    Vec2::new(right_w, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("settings_main_scroll")
                            .animated(!jump_pending)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_3xl);
                                render_active_category(ui, state, theme, extra, &mut actions);
                                ui.add_space(32.0);
                            });
                    },
                );
            });
        } else {
            render_settings_nav_compact(ui, state, theme);
            ui.add_space(theme.space_md);
            egui::ScrollArea::vertical()
                .id_salt("settings_main_scroll")
                .animated(!jump_pending)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_3xl);
                    render_active_category(ui, state, theme, extra, &mut actions);
                    ui.add_space(32.0);
                });
        }
    });

    actions
}

/// Width of the Settings category rail.
///
/// Wide enough for the longest category name plus its icon; anything longer is
/// elided rather than painted over the content column.
const SETTINGS_NAV_WIDTH: f32 = 224.0;
/// Width below which the vertical rail is replaced by a wrapping chip row.
///
/// The shell owns this value already. Inventing a second breakpoint for
/// Settings was how this page would drift: at one width the nav would
/// already be a row of chips while the cards still thought they were on a
/// narrow pane, or the reverse.
pub const SETTINGS_COMPACT_BREAKPOINT: f32 = crate::layout::SETTINGS_COMPACT_BREAKPOINT;

/// Dispatch the active category. Shared by the wide (rail) and narrow (chips)
/// layouts so both render exactly the same page.
fn render_active_category(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
    extra: &SettingsExtraContext<'_>,
    actions: &mut SettingsTabActions,
) {
    match state.active_category {
        SettingsCategory::Playback => render_playback_category(ui, state, theme, extra, actions),
        SettingsCategory::Output => render_output_category(ui, state, theme, extra, actions),
        SettingsCategory::Spatial => render_spatial_category(ui, state, theme, actions),
        SettingsCategory::MixBus => render_mixbus_category(ui, state, theme, actions),
        SettingsCategory::Tools => render_tools_category(ui, state, theme, extra, actions),
        SettingsCategory::Converter => {
            render_converter_category(ui, state, theme, extra, actions);
        }
        SettingsCategory::Library => render_library_category(ui, state, theme, extra, actions),
        SettingsCategory::Appearance => {
            render_appearance_category(ui, state, theme, extra, actions);
        }
    }
}

#[cfg(test)]
mod tests;

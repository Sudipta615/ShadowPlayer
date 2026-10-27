//! The Settings panel.
//!
//! The panel is split into focused modules:
//!
//! - [`index`] — the category model and the searchable row index.
//! - [`state`] — persisted state, actions, and borrowed view context.
//! - [`widgets`] — shared cards, rows, sliders, and toggles.
//! - [`search`] — the search box and its search-as-jump results.
//! - [`jump`] — the one-frame scroll-and-flash request.
//! - [`categories`] — one renderer per settings category.
//! - [`cards`] — larger cards that compose several rows.
//!
//! `mod.rs` owns the tab strip and dispatches to the active category.

use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

mod cards;
mod categories;
mod index;
mod jump;
mod search;
mod state;
mod widgets;

use categories::{
    render_appearance_category, render_converter_category, render_library_category,
    render_mixbus_category, render_output_category, render_playback_category,
    render_spatial_category, render_tools_category,
};
use search::{render_settings_search, render_settings_search_results};

pub use index::{search_settings, SettingsCategory, SettingsEntry, SETTINGS_INDEX};
pub use jump::{request_settings_jump, settings_jump_pending};
pub use state::{
    CrossfadePreviewContext, SettingsExtraContext, SettingsTabActions, SettingsTabState,
};
pub use widgets::{
    card_header, card_header_with_info, dual_tone_slider, segment_picker, setting_card,
    setting_item, setting_item_with_info, toggle_switch,
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

        // 0. Search. Typing replaces the page with a short list of rows;
        // choosing one switches category and scrolls to the real row.
        render_settings_search(ui, state, theme);
        if !state.search_query.trim().is_empty() {
            render_settings_search_results(ui, state, theme);
            ui.add_space(32.0);
            return;
        }

        ui.add_space(theme.space_sm);

        // 1. Two-column vertical master-detail layout:
        // Left column: Vertical category tabs with open sub-headings under selected tab.
        // Right column: Content cards scroll area.
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            // Left Navigation Column
            let nav_w = 210.0;
            ui.allocate_ui_with_layout(
                Vec2::new(nav_w, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("settings_nav_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(nav_w - 8.0);
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 4.0);

                            for category in SettingsCategory::ALL {
                                let is_active = category == state.active_category;
                                let fill = if is_active {
                                    theme.bg_pill
                                } else {
                                    Color32::TRANSPARENT
                                };
                                let stroke = if is_active {
                                    Stroke::new(1.0, theme.accent)
                                } else {
                                    Stroke::NONE
                                };

                                let (rect, resp) = ui.allocate_exact_size(
                                    Vec2::new(nav_w - 12.0, 36.0),
                                    Sense::click(),
                                );
                                if resp.clicked() {
                                    state.active_category = category;
                                }

                                if is_active {
                                    ui.painter().rect(
                                        rect,
                                        CornerRadius::same(theme.radius_pill),
                                        fill,
                                        stroke,
                                        egui::StrokeKind::Inside,
                                    );
                                } else if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        CornerRadius::same(theme.radius_pill),
                                        theme.bg_card_hover,
                                    );
                                }

                                // Icon
                                let icon_rect = Rect::from_center_size(
                                    Pos2::new(rect.min.x + 20.0, rect.center().y),
                                    Vec2::splat(16.0),
                                );
                                let icon_color = if is_active {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                };
                                category.draw_icon(ui, icon_rect, icon_color);

                                // Title
                                let text_color = if is_active {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                };
                                ui.painter().text(
                                    Pos2::new(rect.min.x + 40.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    category.title(),
                                    if is_active {
                                        crate::theme::font(13.0)
                                    } else {
                                        crate::theme::font(12.5)
                                    },
                                    text_color,
                                );

                                // When active: render open list of sub-headings below
                                if is_active {
                                    ui.add_space(2.0);
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                                        for card in category.cards() {
                                            let display_name = index::card_display_name(card);
                                            let (sub_rect, sub_resp) = ui.allocate_exact_size(
                                                Vec2::new(nav_w - 16.0, 26.0),
                                                Sense::click(),
                                            );
                                            let is_sub_hovered = sub_resp.hovered();
                                            if is_sub_hovered {
                                                ui.painter().rect_filled(
                                                    sub_rect,
                                                    CornerRadius::same(theme.radius_sm),
                                                    theme.bg_card_hover,
                                                );
                                            }
                                            // Bullet dot
                                            ui.painter().circle_filled(
                                                Pos2::new(
                                                    sub_rect.min.x + 24.0,
                                                    sub_rect.center().y,
                                                ),
                                                2.5,
                                                if is_sub_hovered {
                                                    theme.accent
                                                } else {
                                                    theme.text_muted
                                                },
                                            );
                                            // Sub-heading title
                                            ui.painter().text(
                                                Pos2::new(
                                                    sub_rect.min.x + 36.0,
                                                    sub_rect.center().y,
                                                ),
                                                egui::Align2::LEFT_CENTER,
                                                display_name,
                                                crate::theme::font(11.5),
                                                if is_sub_hovered {
                                                    theme.text_primary
                                                } else {
                                                    theme.text_secondary
                                                },
                                            );
                                            if sub_resp.clicked() {
                                                request_settings_jump(card);
                                            }
                                        }
                                    });
                                    ui.add_space(4.0);
                                }
                            }
                        });
                },
            );

            // Separator between navigation and cards
            ui.separator();

            // Right Detail Column: Active Category Content in vertical scroll
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

                            match state.active_category {
                                SettingsCategory::Playback => {
                                    render_playback_category(ui, state, theme, extra, &mut actions);
                                }
                                SettingsCategory::Output => {
                                    render_output_category(ui, state, theme, extra, &mut actions);
                                }
                                SettingsCategory::Spatial => {
                                    render_spatial_category(ui, state, theme, &mut actions);
                                }
                                SettingsCategory::MixBus => {
                                    render_mixbus_category(ui, state, theme, &mut actions);
                                }
                                SettingsCategory::Tools => {
                                    render_tools_category(ui, state, theme, extra, &mut actions);
                                }
                                SettingsCategory::Converter => {
                                    render_converter_category(
                                        ui,
                                        state,
                                        theme,
                                        extra,
                                        &mut actions,
                                    );
                                }
                                SettingsCategory::Library => {
                                    render_library_category(ui, state, theme, extra, &mut actions);
                                }
                                SettingsCategory::Appearance => {
                                    render_appearance_category(
                                        ui,
                                        state,
                                        theme,
                                        extra,
                                        &mut actions,
                                    );
                                }
                            }

                            ui.add_space(32.0);
                        });
                },
            );
        });
    });

    actions
}

#[cfg(test)]
mod search_tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Every source that renders a settings row, paired with the category it
    /// belongs to. The chain-preset card is rendered inside the Spatial
    /// category, so its rows are checked as Spatial rows.
    fn category_sources() -> [(SettingsCategory, String); 8] {
        [
            (
                SettingsCategory::Playback,
                include_str!("categories/playback.rs").to_string(),
            ),
            (
                SettingsCategory::Output,
                include_str!("categories/output.rs").to_string(),
            ),
            (
                SettingsCategory::Spatial,
                [
                    include_str!("categories/spatial.rs"),
                    include_str!("cards/chain_presets.rs"),
                ]
                .concat(),
            ),
            (
                SettingsCategory::MixBus,
                include_str!("categories/mixbus.rs").to_string(),
            ),
            (
                SettingsCategory::Tools,
                include_str!("categories/tools.rs").to_string(),
            ),
            (
                SettingsCategory::Converter,
                include_str!("categories/converter.rs").to_string(),
            ),
            (
                SettingsCategory::Library,
                include_str!("categories/library.rs").to_string(),
            ),
            (
                SettingsCategory::Appearance,
                include_str!("categories/appearance.rs").to_string(),
            ),
        ]
    }

    /// Parse a Rust string literal at the start of `text`.
    fn string_literal(text: &str) -> Option<(String, usize)> {
        let mut chars = text.char_indices();
        let (_, quote) = chars.next()?;
        if quote != '"' {
            return None;
        }
        let mut out = String::new();
        while let Some((index, c)) = chars.next() {
            match c {
                '"' => return Some((out, index + 1)),
                '\\' => {
                    let (_, escaped) = chars.next()?;
                    out.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        '"' => '"',
                        '\\' => '\\',
                        other => other,
                    });
                }
                _ => out.push(c),
            }
        }
        None
    }

    /// Parse the label of a settings row.
    ///
    /// The label is either a plain literal or a translated one: the copy is
    /// in the catalogue, so the render says `&tr!("Label")` while the index
    /// still keys on the English text. Both spellings name the same row, and
    /// the drift test must see through the translation to prove that.
    fn row_label_literal(text: &str) -> Option<(String, usize)> {
        let mut rest = text.trim_start();
        if let Some(stripped) = rest.strip_prefix('&') {
            rest = stripped.trim_start();
        }
        if let Some(stripped) = rest.strip_prefix("tr!(") {
            return string_literal(stripped.trim_start());
        }
        string_literal(rest)
    }

    /// Every `setting_item(ui, "Label", ...)` call and its byte offset.
    ///
    /// A call whose first argument is not `ui` followed by a string literal is
    /// the definition dispatching to `setting_item_with_info`, not a row.
    fn rendered_rows(source: &str) -> Vec<(usize, String)> {
        let mut rows = Vec::new();
        for name in ["setting_item(", "setting_item_with_info("] {
            let mut from = 0;
            while let Some(offset) = source[from..].find(name) {
                let start = from + offset;
                let after = start + name.len();
                let rest = source[after..].trim_start();
                let rest = rest.strip_prefix("ui").unwrap_or("");
                let rest = rest.trim_start();
                let rest = rest.strip_prefix(',').unwrap_or("");
                let rest = rest.trim_start();
                if let Some((label, _)) = row_label_literal(rest) {
                    rows.push((start, label));
                }
                from = after;
            }
        }

        // The diagnostics card renders one row per `OutputFact` in a loop.
        // The labels are static by design; list them here so the search index
        // remains addressable without pretending the runtime list is
        // discoverable by parsing a `for` body.
        // `rustfmt` wraps the iterator expression onto its own line, so search
        // for the call rather than for a single-line `for … in …` spelling.
        if let Some(at) = source.find("crate::components::output_facts::output_facts(") {
            for label in [
                "Requested backend",
                "Negotiated backend",
                "Access mode",
                "Verification",
                "Output device",
                "Negotiated rate / container",
                "Period / buffer",
            ] {
                rows.push((at, label.to_string()));
            }
        }
        rows.sort_by_key(|(at, _)| *at);
        rows
    }

    /// Every card title passed to `card_header*`.
    fn rendered_cards(source: &str) -> Vec<String> {
        let mut cards = Vec::new();
        for name in ["card_header(", "card_header_with_info("] {
            let mut from = 0;
            while let Some(offset) = source[from..].find(name) {
                let after = from + offset + name.len();
                // A real call starts `ui,`; the definition starts `ui:`.
                let rest = source[after..].trim_start();
                if let Some(rest) = rest.strip_prefix("ui,") {
                    // `ui` and the icon contain no string literals, so the
                    // first literal is always the title.
                    if let Some(quote) = rest.find('"') {
                        if let Some((title, _)) = string_literal(&rest[quote..]) {
                            cards.push(title);
                        }
                    }
                }
                from = after;
            }
        }
        cards
    }

    #[test]
    fn every_rendered_setting_is_indexed() {
        let sources = category_sources();
        let indexed: std::collections::BTreeSet<&str> =
            SETTINGS_INDEX.iter().map(|entry| entry.label).collect();
        let mut rendered_count = 0;
        for (_, source) in &sources {
            for (at, label) in rendered_rows(source) {
                rendered_count += 1;
                assert!(
                    indexed.contains(label.as_str()),
                    "`{label}` is rendered at byte {at} but missing from SETTINGS_INDEX"
                );
            }
        }
        assert_eq!(
            rendered_count,
            SETTINGS_INDEX.len(),
            "the render has {rendered_count} setting rows, the index has {}",
            SETTINGS_INDEX.len()
        );
    }

    #[test]
    fn every_indexed_setting_is_rendered_once() {
        let mut rendered: BTreeMap<String, usize> = BTreeMap::new();
        for (_, source) in &category_sources() {
            for (_, label) in rendered_rows(source) {
                *rendered.entry(label).or_default() += 1;
            }
        }
        let mut indexed: BTreeMap<&str, usize> = BTreeMap::new();
        for entry in SETTINGS_INDEX.iter() {
            *indexed.entry(entry.label).or_default() += 1;
        }
        for (label, count) in &indexed {
            assert_eq!(
                rendered.get(*label).copied().unwrap_or(0),
                *count,
                "`{label}` has {count} index rows but {} rendered rows",
                rendered.get(*label).copied().unwrap_or(0)
            );
        }
        for (label, count) in &rendered {
            assert_eq!(
                count, &1,
                "`{label}` is rendered {count} times; the index cannot address one of them"
            );
        }
    }

    #[test]
    fn the_index_carries_the_right_category_and_card() {
        let sources = category_sources();
        let mut cards = Vec::new();
        for (_, source) in &sources {
            cards.extend(rendered_cards(source));
        }
        let card_set: std::collections::BTreeSet<&str> = cards.iter().map(String::as_str).collect();
        let indexed_cards: std::collections::BTreeSet<&str> =
            SETTINGS_INDEX.iter().map(|entry| entry.card).collect();
        assert!(
            indexed_cards.is_subset(&card_set),
            "the index names cards the page does not render: {:?}",
            indexed_cards.difference(&card_set).collect::<Vec<_>>()
        );

        for (category, source) in &sources {
            for (at, label) in rendered_rows(source) {
                let entry = SETTINGS_INDEX
                    .iter()
                    .find(|entry| entry.label == label)
                    .unwrap_or_else(|| panic!("`{label}` is not indexed"));
                assert_eq!(
                    entry.category, *category,
                    "`{label}` is indexed under {:?} but rendered under {category:?} at byte {at}",
                    entry.category
                );
                assert!(
                    cards.iter().any(|card| card == entry.card),
                    "`{label}` names the card `{}`, which the page does not render",
                    entry.card
                );
            }
        }
    }

    #[test]
    fn search_reaches_labels_cards_and_descriptions() {
        assert_eq!(
            search_settings("subwoofer")
                .first()
                .map(|entry| entry.label),
            Some("Subwoofer Phase"),
            "a label match outranks a description match"
        );
        assert!(
            search_settings("crossfeed")
                .iter()
                .any(|entry| entry.label == "Processing Profile"),
            "a description match still finds the row: {:?}",
            search_settings("crossfeed")
                .iter()
                .map(|entry| entry.label)
                .collect::<Vec<_>>()
        );
        assert!(
            search_settings("no such setting exists").is_empty(),
            "a query with no matched characters finds nothing"
        );
        assert!(search_settings("   ").is_empty());
    }

    #[test]
    fn every_index_entry_can_be_found_by_its_own_label() {
        for entry in SETTINGS_INDEX.iter() {
            let results = search_settings(entry.label);
            assert!(
                results.iter().any(|found| found.label == entry.label),
                "`{}` is not found by its own label",
                entry.label
            );
        }
    }
}

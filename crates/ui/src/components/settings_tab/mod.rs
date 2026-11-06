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
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

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

/// Experience persona mode toggle pill (Standard vs Pro Studio)
fn render_view_mode_toggle_pill(ui: &mut egui::Ui, state: &mut SettingsTabState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xs, 0.0);
        let is_pro = state.view_mode == SettingsViewMode::ProStudio;

        let std_btn = ui.add(
            egui::Button::new(
                egui::RichText::new("Standard")
                    .font(crate::theme::font(11.0))
                    .strong()
                    .color(if !is_pro {
                        crate::theme::on_fill(theme.accent)
                    } else {
                        theme.text_secondary
                    }),
            )
            .fill(if !is_pro { theme.accent } else { theme.bg_pill })
            .stroke(Stroke::new(
                1.0,
                if !is_pro {
                    theme.accent_highlight
                } else {
                    theme.border_subtle
                },
            ))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(76.0, 26.0)),
        );
        if std_btn
            .on_hover_text("Standard Mode: Curated playback, hardware DAC, library, and appearance")
            .clicked()
            && is_pro
        {
            state.view_mode = SettingsViewMode::Standard;
        }

        let pro_btn = ui.add(
            egui::Button::new(
                egui::RichText::new("Pro Studio")
                    .font(crate::theme::font(11.0))
                    .strong()
                    .color(if is_pro {
                        crate::theme::on_fill(theme.accent_warm)
                    } else {
                        theme.text_secondary
                    }),
            )
            .fill(if is_pro {
                theme.accent_warm
            } else {
                theme.bg_pill
            })
            .stroke(Stroke::new(
                1.0,
                if is_pro {
                    theme.accent_warm
                } else {
                    theme.border_subtle
                },
            ))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(84.0, 26.0)),
        );
        if pro_btn
            .on_hover_text(
                "Pro Studio Mode: Full audio lab (Spatial 3D radar, Matrix FIR routing, multitrack mix bus, DSD decimation, batch converter)",
            )
            .clicked()
            && !is_pro
        {
            state.view_mode = SettingsViewMode::ProStudio;
        }
    });
}

/// The narrow-pane category switcher: a responsive category dropdown plus mode toggle.
fn render_settings_nav_compact(ui: &mut egui::Ui, state: &mut SettingsTabState, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

        let is_pro = state.view_mode == SettingsViewMode::ProStudio;
        let avail_w = ui.available_width();
        let combo_w = (avail_w - 74.0).max(140.0);

        let current_text = format!(
            "{} {}",
            state.active_category.title(),
            if state.active_category.is_pro() {
                "· PRO"
            } else {
                ""
            }
        );

        egui::ComboBox::from_id_salt("settings_nav_compact_combo")
            .selected_text(current_text)
            .width(combo_w)
            .show_ui(ui, |ui| {
                for group in CategoryGroup::ALL {
                    ui.label(
                        egui::RichText::new(group.title())
                            .font(crate::theme::mono_font(9.0))
                            .strong()
                            .color(theme.text_dim),
                    );
                    for &cat in group.categories() {
                        let is_sel = state.active_category == cat;
                        let text = format!(
                            "{} {}",
                            cat.title(),
                            if cat.is_pro() { "· PRO" } else { "" }
                        );
                        if ui.selectable_label(is_sel, text).clicked() {
                            state.active_category = cat;
                        }
                    }
                    ui.separator();
                }
            });

        let mode_label = if is_pro { "Pro" } else { "Std" };
        let mode_btn = ui.add(
            egui::Button::new(
                egui::RichText::new(mode_label)
                    .font(crate::theme::font(10.5))
                    .strong()
                    .color(if is_pro {
                        crate::theme::on_fill(theme.accent_warm)
                    } else {
                        crate::theme::on_fill(theme.accent)
                    }),
            )
            .fill(if is_pro {
                theme.accent_warm
            } else {
                theme.accent
            })
            .stroke(Stroke::new(1.0, theme.border_subtle))
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(40.0, 24.0)),
        );
        if mode_btn
            .on_hover_text(if is_pro {
                "Active: Pro Studio Mode. Click to switch to Standard Mode"
            } else {
                "Active: Standard Mode. Click to switch to Pro Studio Mode"
            })
            .clicked()
        {
            state.view_mode = if is_pro {
                SettingsViewMode::Standard
            } else {
                SettingsViewMode::ProStudio
            };
        }
    });
}

/// Paint `text` on a single line, eliding with an ellipsis at `rect`'s width.
///
/// The category and card names come from the settings model, not from the
/// layout, so a long one used to spill into the cards. Painting through a
/// one-row `LayoutJob` both clips and truncates it.
fn paint_elided(
    ui: &egui::Ui,
    rect: Rect,
    text: &str,
    font: egui::FontId,
    color: eframe::egui::Color32,
) {
    let painter = ui.painter().with_clip_rect(rect);
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = rect.width();
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    painter.galley(
        Pos2::new(rect.left(), rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

/// The category rail: structured semantic sections with clear visual hierarchy.
fn render_settings_nav(ui: &mut egui::Ui, state: &mut SettingsTabState, theme: &Theme) {
    egui::ScrollArea::vertical()
        .id_salt("settings_nav_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let row_w = ui.available_width();
            ui.set_width(row_w);
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

            for group in CategoryGroup::ALL {
                ui.add_space(theme.space_sm);
                ui.label(
                    egui::RichText::new(group.title())
                        .font(crate::theme::mono_font(9.0))
                        .strong()
                        .color(theme.text_dim.gamma_multiply(0.7)),
                );
                ui.add_space(2.0);

                for &category in group.categories() {
                    let is_active = category == state.active_category;
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(row_w, 36.0), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::Button,
                            true,
                            is_active,
                            category.title(),
                        )
                    });
                    if resp.clicked() {
                        state.active_category = category;
                    }

                    // Hover snaps out instead of fading out. A 120 ms crossfade
                    // left the previous category lit while the next one lit up,
                    // which read as "two tabs highlighted at once".
                    let hover_t = if resp.hovered() && !is_active {
                        ui.ctx().animate_bool_with_time(
                            resp.id.with("cat_hover"),
                            true,
                            theme.motion_time(0.1),
                        )
                    } else {
                        0.0
                    };

                    // Background first, content after: painting a hover fill over
                    // an already-drawn label tints (or hides) the label.
                    if is_active {
                        let fill = crate::theme::lerp_color(theme.bg_pill, theme.accent, 0.14);
                        ui.painter().rect(
                            rect,
                            theme.control_radius(),
                            fill,
                            Stroke::new(1.0, crate::theme::with_alpha(theme.accent, 90)),
                            egui::StrokeKind::Inside,
                        );
                        let pill_rect = Rect::from_min_max(
                            Pos2::new(rect.min.x + 3.0, rect.min.y + 8.0),
                            Pos2::new(rect.min.x + 6.5, rect.max.y - 8.0),
                        );
                        ui.painter().rect_filled(
                            pill_rect,
                            CornerRadius::same(theme.radius_xs),
                            theme.accent,
                        );
                    } else if hover_t > 0.01 {
                        ui.painter().rect_filled(
                            rect,
                            theme.control_radius(),
                            theme.bg_card_hover.gamma_multiply(hover_t),
                        );
                        ui.painter().rect_stroke(
                            rect,
                            theme.control_radius(),
                            Stroke::new(1.0, theme.border_subtle.gamma_multiply(hover_t)),
                            egui::StrokeKind::Inside,
                        );
                    }

                    let icon_rect = Rect::from_center_size(
                        Pos2::new(rect.min.x + 20.0, rect.center().y),
                        Vec2::splat(15.0),
                    );
                    let icon_color = if is_active {
                        theme.accent
                    } else {
                        crate::theme::lerp_color(theme.text_muted, theme.text_secondary, hover_t)
                    };
                    category.draw_icon(ui, icon_rect, icon_color);

                    let text_color = if is_active {
                        theme.text_primary
                    } else {
                        crate::theme::lerp_color(theme.text_secondary, theme.text_primary, hover_t)
                    };
                    let max_right = if category.is_pro() {
                        rect.max.x - 34.0
                    } else {
                        rect.max.x - 8.0
                    };
                    paint_elided(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(rect.min.x + 38.0, rect.min.y),
                            Pos2::new(max_right, rect.max.y),
                        ),
                        category.title(),
                        crate::theme::font(if is_active { 12.5 } else { 12.0 }),
                        text_color,
                    );

                    // PRO badge on Pro categories
                    if category.is_pro() {
                        let pro_r = Rect::from_min_size(
                            Pos2::new(rect.max.x - 30.0, rect.center().y - 7.0),
                            Vec2::new(24.0, 14.0),
                        );
                        ui.painter().rect_filled(
                            pro_r,
                            CornerRadius::same(theme.radius_xs),
                            if is_active {
                                theme.accent_warm.linear_multiply(0.2)
                            } else {
                                theme.bg_pill
                            },
                        );
                        ui.painter().text(
                            pro_r.center(),
                            egui::Align2::CENTER_CENTER,
                            "PRO",
                            crate::theme::mono_font(7.5),
                            if is_active {
                                theme.accent_warm
                            } else {
                                theme.text_dim
                            },
                        );
                    }

                    if is_active {
                        ui.add_space(2.0);
                        render_category_contents(ui, category, theme, row_w);
                        ui.add_space(4.0);
                    }
                }
            }

            ui.add_space(theme.space_sm);
        });
}

/// The active category's sub-headings, with a guide spine.
fn render_category_contents(
    ui: &mut egui::Ui,
    category: SettingsCategory,
    theme: &Theme,
    row_w: f32,
) {
    let cards = category.cards();
    let card_count = cards.len();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
        for (idx, &card) in cards.iter().enumerate() {
            let display_name = index::card_display_name(card);
            let (sub_rect, sub_resp) =
                ui.allocate_exact_size(Vec2::new(row_w, 26.0), Sense::click());
            sub_resp
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, display_name));
            let sub_hover_t = if sub_resp.hovered() {
                ui.ctx().animate_bool_with_time(
                    sub_resp.id.with("sub_hover"),
                    true,
                    theme.motion_time(0.1),
                )
            } else {
                0.0
            };

            if sub_hover_t > 0.01 {
                ui.painter().rect_filled(
                    sub_rect,
                    CornerRadius::same(theme.radius_sm),
                    theme.bg_card_hover.gamma_multiply(sub_hover_t),
                );
            }

            // Guide spine + branch tick, drawn behind the bullet.
            let spine_x = sub_rect.min.x + 18.0;
            let top_y = if idx == 0 {
                sub_rect.center().y
            } else {
                sub_rect.min.y
            };
            let bottom_y = if idx == card_count - 1 {
                sub_rect.center().y
            } else {
                sub_rect.max.y
            };
            ui.painter().line_segment(
                [Pos2::new(spine_x, top_y), Pos2::new(spine_x, bottom_y)],
                Stroke::new(1.0, theme.border_subtle.gamma_multiply(0.6)),
            );
            ui.painter().line_segment(
                [
                    Pos2::new(spine_x, sub_rect.center().y),
                    Pos2::new(spine_x + 5.0, sub_rect.center().y),
                ],
                Stroke::new(1.0, theme.border_subtle.gamma_multiply(0.6)),
            );

            ui.painter().circle_filled(
                Pos2::new(spine_x + 6.0, sub_rect.center().y),
                2.2,
                crate::theme::lerp_color(theme.text_muted, theme.accent, sub_hover_t),
            );

            paint_elided(
                ui,
                Rect::from_min_max(
                    Pos2::new(sub_rect.min.x + 34.0, sub_rect.min.y),
                    Pos2::new(sub_rect.max.x - 4.0, sub_rect.max.y),
                ),
                display_name,
                crate::theme::font(11.5),
                crate::theme::lerp_color(theme.text_secondary, theme.text_primary, sub_hover_t),
            );

            if sub_resp.clicked() {
                request_settings_jump(card);
            }
        }
    });
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
        for name in [
            "setting_item(",
            "setting_item_with_info(",
            "setting_item_toggle(",
        ] {
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
    fn search_reaches_card_headings() {
        let hits = super::index::scored_setting_cards("theme editor");
        assert!(
            hits.iter().any(
                |(_, category, title)| *category == SettingsCategory::Appearance
                    && *title == "Theme Editor"
            ),
            "the Theme Editor card must be searchable: {hits:?}"
        );
        let hits = super::index::scored_setting_cards("theme gallery");
        assert!(
            hits.iter().any(
                |(_, category, title)| *category == SettingsCategory::Appearance
                    && *title == "Theme Gallery"
            ),
            "the Theme Gallery card must be searchable: {hits:?}"
        );
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

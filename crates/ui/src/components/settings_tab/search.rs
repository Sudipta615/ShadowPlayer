//! The settings search box and its search-as-jump result list.

use super::index::{scored_setting_cards, scored_settings, SettingsCategory, SettingsEntry};
use super::jump::request_settings_jump;
use super::state::SettingsTabState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// Search
// ─────────────────────────────────────────────────────────────────────────────

/// The Settings search field. The shared widget keeps it at the same height,
/// width and centred alignment as the library search.
pub(super) fn render_settings_search(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
    crate::components::search_field::search_field(
        ui,
        &mut state.search_query,
        tr!("Search settings — try “subwoofer” or “latency”"),
        theme,
    );
}

#[derive(Clone, Copy)]
enum SearchHit {
    Row(&'static SettingsEntry),
    Section {
        category: SettingsCategory,
        title: &'static str,
    },
}

impl SearchHit {
    fn label(self) -> &'static str {
        match self {
            Self::Row(entry) => entry.label,
            Self::Section { title, .. } => title,
        }
    }
}

/// Search-as-jump: the results are a route to the real row, not a filtered
/// copy of the page. A row's controls only make sense where they live, and a
/// card heading takes the user to the section itself.
pub(super) fn render_settings_search_results(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
    let query = state.search_query.trim().to_string();
    let mut hits: Vec<(i32, SearchHit)> = scored_settings(&query)
        .into_iter()
        .map(|(score, entry)| (score, SearchHit::Row(entry)))
        .collect();
    hits.extend(
        scored_setting_cards(&query)
            .into_iter()
            .map(|(score, category, title)| (score, SearchHit::Section { category, title })),
    );
    hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label().cmp(b.1.label())));

    ui.label(
        egui::RichText::new(if hits.is_empty() {
            tr_args!("No settings or sections match “{query}”", "query" => query)
        } else {
            tr_args!(
                "Results for “{query}”: {count}",
                "query" => query,
                "count" => hits.len()
            )
        })
        .font(crate::theme::font(12.5))
        .strong()
        .color(theme.text_secondary),
    );

    if hits.is_empty() {
        ui.label(
            egui::RichText::new(
                tr!("Search labels, card titles and descriptions. Settings are reached \n                 where they live rather than edited in a list."),
            )
            .font(crate::theme::font(11.5))
            .color(theme.text_dim),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("settings_search_results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xs);

            for (_, hit) in hits {
                let (label, category, location, description) = match hit {
                    SearchHit::Row(entry) => (
                        entry.label,
                        entry.category,
                        entry.card.to_string(),
                        Some(entry.description),
                    ),
                    SearchHit::Section { category, title } => {
                        (title, category, tr!("Jump to this section"), None)
                    }
                };

                let frame = egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_lg))
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_xl as i8,
                        theme.space_md as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_md, 0.0);

                            // Category icon chip
                            let (icon_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                            ui.painter().rect_filled(
                                icon_rect,
                                CornerRadius::same(theme.radius_sm),
                                theme.bg_pill,
                            );
                            category.draw_icon(ui, icon_rect.shrink(5.0), theme.accent_highlight);

                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);

                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(label)
                                            .font(crate::theme::font(13.0))
                                            .strong()
                                            .color(theme.text_primary),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new("Jump ➔")
                                                    .font(crate::theme::font(10.5))
                                                    .color(theme.accent),
                                            );
                                        },
                                    );
                                });

                                // Context Breadcrumb: Category > Card Section
                                let breadcrumb = format!("{}  ›  {}", category.title(), location);
                                ui.label(
                                    egui::RichText::new(breadcrumb)
                                        .font(crate::theme::font(10.8))
                                        .color(theme.text_secondary),
                                );

                                if let Some(description) = description {
                                    if !description.is_empty() {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(description)
                                                    .font(crate::theme::font(10.5))
                                                    .color(theme.text_dim),
                                            )
                                            .truncate(),
                                        );
                                    }
                                }
                            });
                        });
                    });

                let response = frame.response.interact(Sense::click());
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
                if response.hovered() {
                    ui.painter().rect_stroke(
                        frame.response.rect,
                        CornerRadius::same(theme.radius_md),
                        Stroke::new(1.0, theme.accent),
                        egui::StrokeKind::Inside,
                    );
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if response.clicked() {
                    state.active_category = category;
                    request_settings_jump(label);
                    state.search_query.clear();
                    ui.ctx().request_repaint();
                }
            }
        });
}

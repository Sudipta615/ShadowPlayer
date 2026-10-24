//! The settings search box and its search-as-jump result list.

use super::index::search_settings;
use super::jump::request_settings_jump;
use super::state::SettingsTabState;
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Sense, Stroke, Vec2};

// ─────────────────────────────────────────────────────────────────────────────
// Search
// ─────────────────────────────────────────────────────────────────────────────

/// The Settings search field: one line, slimmed down and centered.
pub(super) fn render_settings_search(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
    let avail_w = ui.available_width();
    let search_w = 460.0f32.min(avail_w);
    let pad = ((avail_w - search_w) * 0.5).max(0.0);

    ui.horizontal(|ui| {
        if pad > 0.0 {
            ui.add_space(pad);
        }
        ui.allocate_ui_with_layout(
            Vec2::new(search_w, 36.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                egui::Frame::NONE
                    .fill(theme.bg_input)
                    .stroke(Stroke::new(1.0, theme.border_input))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_md as i8,
                        theme.space_sm as i8,
                    ))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                            let (icon_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                            crate::components::icons::draw_search(ui, icon_rect, theme.text_muted);

                            let has_query = !state.search_query.is_empty();
                            let clear_w = if has_query { 28.0 } else { 0.0 };
                            let input_w = (ui.available_width() - clear_w - 6.0).max(80.0);

                            ui.add(
                                egui::TextEdit::singleline(&mut state.search_query)
                                    .hint_text(
                                        egui::RichText::new(tr!(
                                            "Search settings — try “subwoofer” or “latency”"
                                        ))
                                        .color(theme.text_muted)
                                        .font(crate::theme::font(12.5)),
                                    )
                                    .desired_width(input_w)
                                    .frame(egui::Frame::NONE)
                                    .font(crate::theme::font(13.0))
                                    .text_color(theme.text_primary),
                            );

                            if has_query
                                && ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(tr!("×"))
                                                .font(crate::theme::font(14.0))
                                                .color(theme.text_muted),
                                        )
                                        .frame(false),
                                    )
                                    .on_hover_text(tr!(Msg::S5D7548BE29B19777))
                                    .clicked()
                            {
                                state.search_query.clear();
                            }
                        });
                    });
            },
        );
    });
}

/// Search-as-jump: the results are a route to the real row, not a filtered
/// copy of the page. A row's controls only make sense where they live.
pub(super) fn render_settings_search_results(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
    let query = state.search_query.trim().to_string();
    let results = search_settings(&query);

    ui.label(
        egui::RichText::new(if results.is_empty() {
            format!("No settings match \u{201c}{query}\u{201d}")
        } else {
            format!(
                "{} setting{} for \u{201c}{query}\u{201d}",
                results.len(),
                if results.len() == 1 { "" } else { "s" }
            )
        })
        .font(crate::theme::font(12.5))
        .strong()
        .color(theme.text_secondary),
    );

    if results.is_empty() {
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

            for entry in results {
                let frame = egui::Frame::NONE
                    .fill(theme.bg_card)
                    .stroke(Stroke::new(1.0, theme.border_subtle))
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_xl as i8,
                        theme.space_md as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(entry.label)
                                    .font(crate::theme::font(12.5))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(entry.category.title())
                                            .font(crate::theme::mono_font(10.0))
                                            .color(theme.text_muted),
                                    );
                                },
                            );
                        });
                        ui.label(
                            egui::RichText::new(entry.card)
                                .font(crate::theme::font(11.0))
                                .color(theme.text_secondary),
                        );
                        if !entry.description.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(entry.description)
                                        .font(crate::theme::font(10.5))
                                        .color(theme.text_dim),
                                )
                                .truncate(),
                            );
                        }
                    });

                let response = frame.response.interact(Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, entry.label)
                });
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
                    state.active_category = entry.category;
                    request_settings_jump(entry.label);
                    state.search_query.clear();
                    ui.ctx().request_repaint();
                }
            }
        });
}

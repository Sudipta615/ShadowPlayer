//! The Settings category rail and its compact switcher.
//!
//! Split from `mod.rs` so the façade keeps the tab shell and category
//! dispatch, while every way of choosing a category lives in one place: the
//! wide rail with its sub-headings, the narrow dropdown, and the persona
//! toggle that decides which categories are visible at all.

use super::{
    index, request_settings_jump, CategoryGroup, SettingsCategory, SettingsTabState,
    SettingsViewMode,
};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// Experience persona mode toggle pill (Standard vs Pro Studio)
pub(super) fn render_view_mode_toggle_pill(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
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
pub(super) fn render_settings_nav_compact(
    ui: &mut egui::Ui,
    state: &mut SettingsTabState,
    theme: &Theme,
) {
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
pub(super) fn render_settings_nav(ui: &mut egui::Ui, state: &mut SettingsTabState, theme: &Theme) {
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

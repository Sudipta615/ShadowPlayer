//! Header bar for the Equalizer panel: title badge, status, persona pill,
//! A/B audition toggle, curve undo/redo, and master enable switch.

use super::{controls::custom_toggle_switch, EqPanelActions, EqPanelState, EqTab, EqViewMode};
use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, Sense, Stroke, Vec2};

pub(super) fn render_header(
    ui: &mut egui::Ui,
    state: &mut EqPanelState,
    actions: &mut EqPanelActions,
    theme: &Theme,
) {
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

        // A soft accent badge anchors the title without a heavy rule.
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
        ui.painter().rect_filled(
            icon_rect,
            CornerRadius::same(theme.radius_md),
            crate::theme::with_alpha(theme.accent_warm, 28),
        );
        crate::components::icons::draw_equalizer_sliders(
            ui,
            icon_rect.shrink(9.0),
            theme.accent_warm,
        );

        // The right-hand controls have a known width, so the title column is
        // bounded instead of letting a long status line force the window
        // wider than the user dragged it.
        let right_w = 26.0 + 38.0 + 52.0 + 74.0 + 68.0 + 52.0 + theme.space_sm * 6.0;
        let title_w =
            (ui.available_width() - 32.0 - theme.space_sm - right_w - theme.space_sm).max(70.0);
        ui.allocate_ui_with_layout(
            Vec2::new(title_w, 36.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_max_width(title_w);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(tr!("Equalizer"))
                            .font(crate::theme::font(17.0))
                            .strong()
                            .color(theme.text_primary),
                    )
                    .truncate(),
                );
                let status = if state.eq_enabled {
                    match state.active_tab {
                        EqTab::Graphic => tr!("Graphic EQ"),
                        EqTab::Parametric => tr!("Parametric"),
                        EqTab::Tone => tr!("Tone Controls"),
                        EqTab::Dynamics => tr!("Dynamics"),
                        EqTab::Filters => tr!("Filters"),
                        EqTab::Effects => tr!("Creative Effects"),
                        EqTab::Advanced => tr!("Advanced"),
                    }
                } else {
                    tr!("BYPASSED")
                };
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(status)
                            .font(crate::theme::font(10.5))
                            .color(if state.eq_enabled {
                                theme.text_muted
                            } else {
                                theme.accent_warm
                            }),
                    )
                    .truncate(),
                );
            },
        );

        // Right-aligned actions: master enable + close (✕) + A/B + Undo/Redo + View Mode
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

            let (close_rect, close_resp) =
                ui.allocate_exact_size(Vec2::new(26.0, 26.0), Sense::click());
            if !state.was_open {
                ctx.memory_mut(|m| m.request_focus(close_resp.id));
            }
            close_resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, tr!(Msg::S35EC68DAC50375CB))
            });
            let _ = close_resp
                .clone()
                .on_hover_text(tr!(Msg::S35EC68DAC50375CB));
            let hover_t = ctx.animate_bool_with_time(
                close_resp.id.with("eq_close_hover"),
                close_resp.hovered(),
                theme.motion_time(0.12),
            );
            let cp = ui.painter_at(close_rect);
            let close_bg = crate::theme::lerp_color(
                theme.bg_pill,
                theme.destructive.gamma_multiply(0.25),
                hover_t,
            );
            cp.rect_filled(close_rect, CornerRadius::same(theme.radius_md), close_bg);
            let icon_col =
                crate::theme::lerp_color(theme.text_secondary, theme.destructive, hover_t);
            crate::components::icons::draw_window_close(
                ui,
                close_rect.shrink2(Vec2::splat(6.0)),
                icon_col,
            );
            if close_resp
                .on_hover_text(tr!(Msg::S99DFAE3E6EE98739))
                .clicked()
            {
                state.is_open = false;
            }

            // Labelled master bypass switch.
            let mut eq_active = state.eq_enabled;
            if custom_toggle_switch(ui, &mut eq_active, "Enable Equalizer", theme) {
                state.eq_enabled = eq_active;
                state.graphic_enabled = eq_active && state.eq_mode == config::EqMode::Graphic;
                actions.eq_enabled_changed = Some(eq_active);
            }
            ui.label(
                egui::RichText::new(if eq_active {
                    tr!("ACTIVE")
                } else {
                    tr!("BYPASSED")
                })
                .font(crate::theme::font(10.5))
                .strong()
                .color(if eq_active {
                    theme.accent_highlight
                } else {
                    theme.text_muted
                }),
            );

            // Subtle vertical separator
            let (sep_r, _) = ui.allocate_exact_size(Vec2::new(4.0, 18.0), Sense::hover());
            ui.painter().line_segment(
                [sep_r.center_top(), sep_r.center_bottom()],
                Stroke::new(1.0, theme.border_subtle),
            );

            // View Mode Pill (Quick Tune vs Studio Pro)
            let is_quick = state.view_mode == EqViewMode::QuickTune;
            let view_btn = ui.add(
                egui::Button::new(
                    egui::RichText::new(if is_quick { "Quick Tune" } else { "Studio Pro" })
                        .font(crate::theme::font(10.5))
                        .strong()
                        .color(theme.accent_highlight),
                )
                .fill(theme.accent_highlight.linear_multiply(0.12))
                .stroke(Stroke::new(1.0, theme.accent_highlight.linear_multiply(0.4)))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(0.0, 24.0)),
            );
            if view_btn
                .on_hover_text(
                    "Switch view between acoustic Quick Tune dashboard and full Studio Pro rack",
                )
                .clicked()
            {
                state.view_mode = if is_quick {
                    EqViewMode::StudioPro
                } else {
                    EqViewMode::QuickTune
                };
            }

            // A/B Audition Comparison Button
            let ab_label = if state.is_auditioning_b {
                "Slot B"
            } else {
                "Slot A"
            };
            let ab_btn = ui.add(
                egui::Button::new(
                    egui::RichText::new(format!("A/B: {}", ab_label))
                        .font(crate::theme::font(10.5))
                        .strong()
                        .color(if state.is_auditioning_b {
                            theme.accent_warm
                        } else {
                            theme.text_primary
                        }),
                )
                .fill(if state.is_auditioning_b {
                    theme.accent_warm.linear_multiply(0.18)
                } else {
                    theme.bg_pill
                })
                .stroke(Stroke::new(
                    1.0,
                    if state.is_auditioning_b {
                        theme.accent_warm
                    } else {
                        theme.border_subtle
                    },
                ))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(0.0, 24.0)),
            );
            if ab_btn
                .on_hover_text(
                    "A/B Audition: Instantly toggle between current tweaks (A) and baseline comparison (B)",
                )
                .clicked()
            {
                state.toggle_ab_audition();
                actions.graphic_eq_changed = true;
            }

            // Redo Button
            let can_redo = !state.redo_stack.is_empty();
            let redo_btn = ui.add_enabled(
                can_redo,
                egui::Button::new(
                    egui::RichText::new("↷")
                        .font(crate::theme::font(12.0))
                        .color(if can_redo {
                            theme.text_primary
                        } else {
                            theme.text_dim
                        }),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::new(1.0, theme.border_subtle))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(22.0, 24.0)),
            );
            if redo_btn.on_hover_text("Redo curve tweak").clicked() && state.redo() {
                actions.graphic_eq_changed = true;
            }

            // Undo Button
            let can_undo = !state.undo_stack.is_empty();
            let undo_btn = ui.add_enabled(
                can_undo,
                egui::Button::new(
                    egui::RichText::new("↶")
                        .font(crate::theme::font(12.0))
                        .color(if can_undo {
                            theme.text_primary
                        } else {
                            theme.text_dim
                        }),
                )
                .fill(theme.bg_pill)
                .stroke(Stroke::new(1.0, theme.border_subtle))
                .corner_radius(CornerRadius::same(theme.radius_sm))
                .min_size(Vec2::new(22.0, 24.0)),
            );
            if undo_btn.on_hover_text("Undo curve tweak").clicked() && state.undo() {
                actions.graphic_eq_changed = true;
            }
        });
    });

    state.was_open = state.is_open;
}

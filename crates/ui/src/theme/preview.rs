use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, Pos2, Rect, Sense, Stroke, StrokeKind,
    Vec2,
};

use super::tokens::Theme;

/// Actions emitted by the staged preview sandbox.
#[derive(Default, Clone, Debug)]
pub struct PreviewActions {
    pub apply_theme: Option<String>,
    pub cancel_preview: bool,
    pub open_editor: Option<String>,
}

/// Render a live preview sandbox showing miniature real ShadowPlayer components
/// styled with the candidate theme.
pub fn render_theme_preview(
    ui: &mut egui::Ui,
    theme: &Theme,
    preview_theme: &Theme,
    theme_id: &str,
    theme_name: &str,
    is_staged: bool,
) -> PreviewActions {
    let mut actions = PreviewActions::default();

    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(Margin::same(theme.space_xl as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Live Component Preview:")
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(theme_name)
                            .font(crate::theme::font(13.0))
                            .color(theme.accent_highlight),
                    );
                    if is_staged {
                        let tag_frame = egui::Frame::NONE
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_xs))
                            .inner_margin(Margin::symmetric(
                                theme.space_xs as i8,
                                theme.space_xxs as i8,
                            ));
                        tag_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("PREVIEWING")
                                    .font(crate::theme::mono_font(9.0))
                                    .color(Color32::WHITE),
                            );
                        });
                    }
                });

                ui.add_space(theme.space_sm);

                // Preview Sandbox Board
                egui::Frame::NONE
                    .fill(preview_theme.bg_app)
                    .stroke(Stroke::new(1.0, preview_theme.border_subtle))
                    .corner_radius(CornerRadius::same(preview_theme.radius_md))
                    .inner_margin(Margin::same(preview_theme.space_lg as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        // Top section: Album Art + Sample Track Rows
                        ui.horizontal(|ui| {
                            // 1. Mini Album Artwork
                            let art_size = Vec2::splat(72.0);
                            let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                            let painter = ui.painter_at(art_rect);
                            painter.rect_filled(
                                art_rect,
                                CornerRadius::same(preview_theme.radius_md),
                                preview_theme.artwork_placeholder,
                            );
                            painter.rect_stroke(
                                art_rect,
                                CornerRadius::same(preview_theme.radius_md),
                                Stroke::new(1.0, preview_theme.border_card),
                                StrokeKind::Inside,
                            );
                            let center = art_rect.center();
                            painter.circle_filled(
                                center,
                                20.0,
                                preview_theme.bg_card_active,
                            );
                            painter.circle_stroke(
                                center,
                                20.0,
                                Stroke::new(1.5, preview_theme.accent_highlight),
                            );
                            painter.circle_filled(
                                center,
                                6.0,
                                preview_theme.accent,
                            );

                            ui.add_space(preview_theme.space_md);

                            // 2. Mini Track Rows
                            ui.vertical(|ui| {
                                let tracks = [
                                    ("1", "Solar Resonance", "04:12", false),
                                    ("2", "Oceanic Drift (Playing)", "03:45", true),
                                    ("3", "Luminescence", "05:10", false),
                                ];

                                for (num, title, dur, playing) in tracks {
                                    let row_bg = if playing {
                                        preview_theme.bg_active_row
                                    } else {
                                        preview_theme.bg_card
                                    };
                                    let border_color = if playing {
                                        preview_theme.border_active_row
                                    } else {
                                        preview_theme.border_subtle
                                    };

                                    egui::Frame::NONE
                                        .fill(row_bg)
                                        .stroke(Stroke::new(1.0, border_color))
                                        .corner_radius(CornerRadius::same(preview_theme.radius_sm))
                                        .inner_margin(Margin::symmetric(
                                            preview_theme.space_sm as i8,
                                            preview_theme.space_xs as i8,
                                        ))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(num)
                                                        .font(crate::theme::mono_font(10.5))
                                                        .color(if playing {
                                                            preview_theme.accent_highlight
                                                        } else {
                                                            preview_theme.text_dim
                                                        }),
                                                );
                                                ui.label(
                                                    egui::RichText::new(title)
                                                        .font(crate::theme::font(11.5))
                                                        .strong()
                                                        .color(if playing {
                                                            preview_theme.accent_highlight
                                                        } else {
                                                            preview_theme.text_primary
                                                        }),
                                                );
                                                ui.with_layout(
                                                    Layout::right_to_left(Align::Center),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(dur)
                                                                .font(crate::theme::mono_font(10.5))
                                                                .color(preview_theme.text_muted),
                                                        );
                                                    },
                                                );
                                            });
                                        });
                                }
                            });
                        });

                        ui.add_space(preview_theme.space_md);

                        // Bottom section: Transport bar, seek slider, and mini meters
                        egui::Frame::NONE
                            .fill(preview_theme.bg_card)
                            .stroke(Stroke::new(1.0, preview_theme.border_card))
                            .corner_radius(CornerRadius::same(preview_theme.radius_md))
                            .inner_margin(Margin::symmetric(
                                preview_theme.space_md as i8,
                                preview_theme.space_sm as i8,
                            ))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Mini play button
                                    let btn_frame = egui::Frame::NONE
                                        .fill(preview_theme.accent)
                                        .corner_radius(CornerRadius::same(preview_theme.radius_pill))
                                        .inner_margin(Margin::symmetric(
                                            preview_theme.space_md as i8,
                                            preview_theme.space_xs as i8,
                                        ));
                                    btn_frame.show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new("PLAY")
                                                .font(crate::theme::mono_font(10.0))
                                                .strong()
                                                .color(Color32::WHITE),
                                        );
                                    });

                                    ui.add_space(preview_theme.space_sm);

                                    // Mini progress track
                                    let track_w = (ui.available_width() - 90.0).max(80.0);
                                    let (bar_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(track_w, 14.0),
                                        Sense::hover(),
                                    );
                                    let p = ui.painter_at(bar_rect);
                                    let mid_y = bar_rect.center().y;

                                    // Track gutter
                                    p.rect_filled(
                                        Rect::from_min_max(
                                            Pos2::new(bar_rect.min.x, mid_y - 2.0),
                                            Pos2::new(bar_rect.max.x, mid_y + 2.0),
                                        ),
                                        CornerRadius::same(preview_theme.radius_xs),
                                        preview_theme.slider_track,
                                    );
                                    // Progress fill (42%)
                                    let prog_x = bar_rect.min.x + bar_rect.width() * 0.42;
                                    p.rect_filled(
                                        Rect::from_min_max(
                                            Pos2::new(bar_rect.min.x, mid_y - 2.0),
                                            Pos2::new(prog_x, mid_y + 2.0),
                                        ),
                                        CornerRadius::same(preview_theme.radius_xs),
                                        preview_theme.accent,
                                    );
                                    // Thumb
                                    p.circle_filled(
                                        Pos2::new(prog_x, mid_y),
                                        4.5,
                                        preview_theme.accent_highlight,
                                    );

                                    // Mini VU meter bars
                                    let (meter_rect, _) = ui.allocate_exact_size(
                                        Vec2::new(70.0, 14.0),
                                        Sense::hover(),
                                    );
                                    let mp = ui.painter_at(meter_rect);
                                    mp.rect_filled(
                                        meter_rect,
                                        CornerRadius::same(preview_theme.radius_xs),
                                        preview_theme.meter_bg,
                                    );
                                    let l_val = meter_rect.width() * 0.72;
                                    let r_val = meter_rect.width() * 0.65;
                                    mp.rect_filled(
                                        Rect::from_min_max(
                                            Pos2::new(meter_rect.min.x + 2.0, meter_rect.min.y + 2.0),
                                            Pos2::new(meter_rect.min.x + l_val, meter_rect.min.y + 6.0),
                                        ),
                                        CornerRadius::same(preview_theme.radius_xs),
                                        preview_theme.bit_perfect_green,
                                    );
                                    mp.rect_filled(
                                        Rect::from_min_max(
                                            Pos2::new(meter_rect.min.x + 2.0, meter_rect.min.y + 8.0),
                                            Pos2::new(meter_rect.min.x + r_val, meter_rect.min.y + 12.0),
                                        ),
                                        CornerRadius::same(preview_theme.radius_xs),
                                        preview_theme.accent_highlight,
                                    );
                                });
                            });
                    });

                ui.add_space(theme.space_md);

                // Action Bar: Apply, Cancel, Edit
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("Apply & Save Theme")
                                    .font(crate::theme::font(12.0))
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(theme.accent)
                            .corner_radius(CornerRadius::same(theme.radius_md))
                            .min_size(Vec2::new(140.0, 30.0)),
                        )
                        .clicked()
                    {
                        actions.apply_theme = Some(theme_id.to_string());
                    }

                    if is_staged
                        && ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Revert / Cancel")
                                        .font(crate::theme::font(12.0))
                                        .color(theme.destructive),
                                )
                                .fill(theme.bg_pill)
                                .corner_radius(CornerRadius::same(theme.radius_md))
                                .min_size(Vec2::new(120.0, 30.0)),
                            )
                            .clicked()
                    {
                        actions.cancel_preview = true;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(
                                egui::RichText::new("Customize in Editor")
                                    .font(crate::theme::font(11.5))
                                    .color(theme.info),
                            )
                            .clicked()
                        {
                            actions.open_editor = Some(theme_id.to_string());
                        }
                    });
                });
            });
        });

    actions
}

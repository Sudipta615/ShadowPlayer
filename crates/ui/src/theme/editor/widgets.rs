use eframe::egui::{self, Align, Color32, CornerRadius, Layout, Margin, Stroke, Vec2};

use crate::theme::tokens::{format_hex_color, with_alpha, Theme};

use super::state::ThemeEditorState;

pub(super) fn section_heading(ui: &mut egui::Ui, label: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(11.5))
            .strong()
            .color(theme.accent_highlight),
    );
    ui.add_space(theme.space_xxs);
}

pub(super) fn field_label(ui: &mut egui::Ui, label: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(label)
            .font(crate::theme::font(10.5))
            .color(theme.text_muted),
    );
}

/// A miniature of the real player, painted with the live editor palette.
pub(super) fn render_live_preview(
    ui: &mut egui::Ui,
    state: &ThemeEditorState,
    preview: &Theme,
    host: &Theme,
) {
    let w = 178.0;
    egui::Frame::NONE
        .fill(preview.bg_app)
        .stroke(Stroke::new(1.0, preview.border_card))
        .corner_radius(CornerRadius::same(preview.radius_lg))
        .inner_margin(Margin::same(host.space_sm as i8))
        .show(ui, |ui| {
            ui.set_width(w);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let (art, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
                    ui.painter().rect_filled(
                        art,
                        CornerRadius::same(preview.radius_md),
                        preview.artwork_placeholder,
                    );
                    ui.painter().circle_filled(
                        art.center(),
                        12.0,
                        with_alpha(preview.accent_highlight, 190),
                    );
                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&state.name)
                                    .font(crate::theme::font(11.0))
                                    .strong()
                                    .color(preview.text_primary),
                            )
                            .truncate(),
                        );
                        ui.label(
                            egui::RichText::new("Sample artist")
                                .font(crate::theme::font(9.5))
                                .color(preview.text_muted),
                        );
                    });
                });

                ui.add_space(host.space_xs);

                let (track, _) = ui.allocate_exact_size(Vec2::new(w, 4.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    track,
                    CornerRadius::same(preview.radius_xs),
                    preview.slider_track,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        track.min,
                        Vec2::new(track.width() * 0.42, track.height()),
                    ),
                    CornerRadius::same(preview.radius_xs),
                    preview.accent,
                );

                ui.add_space(host.space_xs);

                ui.horizontal(|ui| {
                    ui.push_id("editor_preview_transport", |ui| {
                        crate::components::play_button::play_button(ui, 30.0, false, preview);
                    });
                    ui.label(
                        egui::RichText::new(state.play_button_shape.label())
                            .font(crate::theme::mono_font(9.0))
                            .color(preview.text_secondary),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for color in [
                            preview.accent,
                            preview.accent_highlight,
                            preview.accent_warm,
                            preview.info,
                        ] {
                            let (dot, _) =
                                ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.0, color);
                        }
                    });
                });
            });
        });
}

pub(super) fn font_role_input(
    ui: &mut egui::Ui,
    value: &mut Option<String>,
    label: &str,
    hint: &str,
    theme: &Theme,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space_xs;
        ui.label(
            egui::RichText::new(label)
                .font(crate::theme::font(10.0))
                .color(theme.text_muted),
        );
        let mut text = value.clone().unwrap_or_default();
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(hint)
                .desired_width(106.0)
                .font(crate::theme::font(11.0)),
        );
        if response.changed() {
            *value = (!text.trim().is_empty()).then(|| text.trim().to_string());
        }
    });
}

pub(super) fn color_picker_row(
    ui: &mut egui::Ui,
    state: &mut ThemeEditorState,
    token: &str,
    label: &str,
    theme: &Theme,
) {
    if let Some(color) = state.colors.get_mut(token) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = theme.space_xs;
            let mut srgba = [color.r(), color.g(), color.b(), color.a()];
            if ui
                .color_edit_button_srgba_unmultiplied(&mut srgba)
                .changed()
            {
                *color = Color32::from_rgba_unmultiplied(srgba[0], srgba[1], srgba[2], srgba[3]);
            }
            // Keep the swatch, its name and its value together. A fixed label
            // column aligns every hex code without pushing it into the next
            // palette column's swatch.
            ui.allocate_ui_with_layout(
                Vec2::new(104.0, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(label)
                                .font(crate::theme::font(10.5))
                                .color(theme.text_secondary),
                        )
                        .truncate(),
                    );
                },
            );
            ui.label(
                egui::RichText::new(format_hex_color(*color))
                    .font(crate::theme::mono_font(9.5))
                    .color(theme.text_muted),
            );
        });
    }
}

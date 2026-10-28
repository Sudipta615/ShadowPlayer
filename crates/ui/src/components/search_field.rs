//! One search field for every search surface in the player.
//!
//! The library's top bar, Settings, the manual, the theme gallery, AutoEq and
//! the playlist editor all use this widget. Keeping a single implementation is
//! what makes the height, corner radius, icon spacing, centred text and clear
//! affordance identical wherever a user types.

use crate::theme::Theme;
use eframe::egui::{self, Align, Color32, Id, Layout, Margin, Sense, Stroke, Vec2};

/// Maximum width of a centred search field. Wider windows do not stretch the
/// field into an unreadable band; narrower ones shrink it to the available
/// width.
pub const SEARCH_MAX_WIDTH: f32 = 520.0;

/// Height of the field, including its frame and padding.
pub const SEARCH_HEIGHT: f32 = 30.0;

/// Draw the standard centred search field. Returns `true` when the query
/// changed (typed, pasted, or cleared).
pub fn search_field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: impl Into<String>,
    theme: &Theme,
) -> bool {
    search_field_with_width(ui, text, hint, theme, SEARCH_MAX_WIDTH)
}

/// Draw a search field with an explicit maximum width. Callers inside dense
/// panels pass [`f32::INFINITY`] to fill the available width.
pub fn search_field_with_width(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: impl Into<String>,
    theme: &Theme,
    max_width: f32,
) -> bool {
    search_field_with_id(ui, text, hint, theme, max_width, None)
}

/// Draw a search field with a stable id, so a keyboard shortcut can focus it.
pub fn search_field_with_id(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: impl Into<String>,
    theme: &Theme,
    max_width: f32,
    id: Option<Id>,
) -> bool {
    let hint = hint.into();
    let available = ui.available_width();
    let width = available.min(max_width).max(120.0);
    let pad = ((available - width) * 0.5).max(0.0);
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        if pad > 0.0 {
            ui.add_space(pad);
        }

        ui.allocate_ui_with_layout(
            Vec2::new(width, SEARCH_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                let frame = egui::Frame::NONE
                    .fill(theme.bg_input)
                    .stroke(Stroke::new(1.0, theme.border_input))
                    .corner_radius(theme.control_radius())
                    .inner_margin(Margin::symmetric(theme.space_md as i8, 3));

                let output = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);

                        let (icon_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(15.0), Sense::hover());
                        crate::components::icons::draw_search(ui, icon_rect, theme.text_muted);

                        let has_query = !text.is_empty();
                        let clear_w = if has_query { 22.0 } else { 0.0 };
                        let input_w = (ui.available_width() - clear_w - 2.0).max(80.0);

                        let mut edit = egui::TextEdit::singleline(text)
                            .hint_text(
                                egui::RichText::new(hint)
                                    .color(theme.text_muted)
                                    .font(crate::theme::font(12.0)),
                            )
                            .text_color(theme.text_primary)
                            .font(crate::theme::font(12.5))
                            .align(egui::Align2::CENTER_CENTER)
                            .desired_width(input_w)
                            .frame(egui::Frame::NONE);
                        if let Some(id) = id {
                            edit = edit.id(id);
                        }
                        let edit_response = ui.add(edit);
                        if edit_response.changed() {
                            changed = true;
                        }

                        if has_query
                            && ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(tr!("×"))
                                            .font(crate::theme::font(14.0))
                                            .color(theme.text_muted),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .frame(false)
                                    .min_size(Vec2::splat(20.0)),
                                )
                                .on_hover_text(tr!("Clear the search"))
                                .clicked()
                        {
                            text.clear();
                            changed = true;
                        }

                        edit_response
                    })
                    .inner
                });

                let response = output.inner;
                let focus_t = ui.ctx().animate_bool_with_time(
                    response.id.with("search_focus_ring"),
                    response.has_focus(),
                    theme.motion_time(0.12),
                );
                if focus_t > 0.0 {
                    ui.painter().rect_stroke(
                        output.response.rect,
                        theme.control_radius(),
                        Stroke::new(
                            1.0 + focus_t,
                            crate::theme::lerp_color(theme.border_input, theme.accent, focus_t),
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
            },
        );
    });

    changed
}

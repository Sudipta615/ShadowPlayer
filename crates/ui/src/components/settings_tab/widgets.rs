//! The shared settings widgets: cards, rows, sliders, toggles, and badges.

use super::jump::apply_setting_jump;
use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

/// Dual-tone horizontal slider matching the design system
pub fn dual_tone_slider(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    width: f32,
    theme: &Theme,
) -> bool {
    // A width of zero means "as wide as the row allows": the settings rows
    // give the slider everything except the value badge beside it, so its
    // right-hand end lands on the card's edge.
    let width = if width <= 0.0 {
        (ui.available_width() - 96.0).max(120.0)
    } else {
        width
    };
    let desired_size = Vec2::new(width, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let mut changed = false;

    let min = *range.start();
    let max = *range.end();
    resp.widget_info(|| {
        egui::WidgetInfo::slider(
            true,
            f64::from(((*value - min) / (max - min)).clamp(0.0, 1.0)),
            format!("Setting slider, {value:.2}"),
        )
    });

    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let raw_val = min + t * (max - min);
            let stepped_val = if step > 0.0 {
                ((raw_val / step).round() * step).clamp(min, max)
            } else {
                raw_val.clamp(min, max)
            };
            if (stepped_val - *value).abs() > 0.0001 {
                *value = stepped_val;
                changed = true;
            }
        }
    }

    // Background track (clearly visible in both dark & light modes)
    let track_rect = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), 4.0));
    ui.painter().rect_filled(
        track_rect,
        CornerRadius::same(theme.radius_xs),
        theme.slider_track,
    );

    // Active single-hue filled track
    let thumb_t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
    let thumb_x = rect.left() + thumb_t * rect.width();
    let filled_w = (thumb_x - track_rect.min.x).max(0.0);
    if filled_w > 0.0 {
        let fill_rect = Rect::from_min_max(
            Pos2::new(track_rect.min.x, track_rect.min.y),
            Pos2::new(track_rect.min.x + filled_w, track_rect.max.y),
        );
        ui.painter().rect_filled(
            fill_rect,
            CornerRadius::same(theme.radius_xs),
            theme.accent_highlight,
        );
    }

    // Thumb knob
    let thumb_pos = Pos2::new(thumb_x, rect.center().y);
    let is_hovered = resp.hovered() || resp.dragged();
    ui.painter().circle_filled(
        thumb_pos,
        if is_hovered { 7.5 } else { 6.0 },
        theme
            .accent_highlight
            .gamma_multiply(if is_hovered { 0.35 } else { 0.18 }),
    );
    ui.painter().circle_filled(thumb_pos, 5.0, Color32::WHITE);
    ui.painter()
        .circle_stroke(thumb_pos, 5.0, Stroke::new(1.2, theme.accent_highlight));

    changed
}

pub fn setting_card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    let card_frame = egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_xl))
        .shadow(egui::epaint::Shadow {
            offset: [0, 4],
            blur: 16,
            spread: 0,
            color: theme.shadow,
        })
        .inner_margin(egui::Margin::symmetric(
            theme.space_5xl as i8,
            theme.space_3xl as i8,
        ));

    let card_response = card_frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xl);
        add_contents(ui);
    });

    let card_rect = card_response.response.rect;
    let hover_t = ui.ctx().animate_bool_with_time(
        card_response.response.id.with("setting_card_hover"),
        ui.rect_contains_pointer(card_rect),
        theme.motion_time(0.15),
    );
    if hover_t > 0.0 {
        ui.painter().rect_stroke(
            card_rect,
            CornerRadius::same(theme.radius_xl),
            Stroke::new(
                1.0,
                crate::theme::with_alpha(theme.accent_highlight, (hover_t * 38.0) as u8),
            ),
            egui::StrokeKind::Inside,
        );
    }
    if theme.frosted {
        crate::theme::glass::paint_glass_sheen(
            ui,
            card_rect,
            CornerRadius::same(theme.radius_xl),
            theme,
        );
    }
}

pub fn card_header(
    ui: &mut egui::Ui,
    icon_fn: impl FnOnce(&egui::Ui, Rect, Color32),
    title: &str,
    subtitle: &str,
    status_badge: Option<(&str, Color32, Color32)>,
    theme: &Theme,
) {
    card_header_with_info(ui, icon_fn, title, subtitle, status_badge, None, theme);
}

pub fn card_header_with_info(
    ui: &mut egui::Ui,
    icon_fn: impl FnOnce(&egui::Ui, Rect, Color32),
    title: &str,
    subtitle: &str,
    status_badge: Option<(&str, Color32, Color32)>,
    info: Option<&crate::components::info_box::InfoBoxContent>,
    theme: &Theme,
) {
    let header_resp = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);

            let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(32.0, 32.0), Sense::hover());
            ui.painter_at(icon_rect).rect_filled(
                icon_rect,
                CornerRadius::same(theme.radius_md),
                theme.bg_pill,
            );
            icon_fn(
                ui,
                icon_rect.shrink2(Vec2::splat(7.0)),
                theme.accent_highlight,
            );

            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(title)
                            .font(crate::theme::font(14.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    if let Some(info_content) = info {
                        let id_str = format!("card_header_info_{}", title);
                        crate::components::info_box::render_info_icon_button(
                            ui,
                            id_str,
                            info_content,
                            theme,
                        );
                    }
                    if let Some((badge_text, bg, fg)) = status_badge {
                        egui::Frame::NONE
                            .fill(bg)
                            .stroke(Stroke::new(1.0, fg.linear_multiply(0.4)))
                            .corner_radius(CornerRadius::same(theme.radius_sm))
                            .inner_margin(egui::Margin::symmetric(
                                theme.space_md as i8,
                                theme.space_xxs as i8,
                            ))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(badge_text)
                                        .font(crate::theme::font(10.5))
                                        .strong()
                                        .color(fg),
                                );
                            });
                    }
                });
                if !subtitle.is_empty() {
                    ui.label(
                        egui::RichText::new(subtitle)
                            .font(crate::theme::font(11.5))
                            .color(theme.text_muted),
                    );
                }
            });
        })
        .response;

    apply_setting_jump(title, &header_resp, header_resp.rect, ui, theme);

    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

pub fn setting_item(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    theme: &Theme,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    setting_item_with_info(ui, label, description, None, theme, add_control);
}

pub fn setting_item_with_info(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    info: Option<&crate::components::info_box::InfoBoxContent>,
    theme: &Theme,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    setting_item_impl(
        ui,
        label,
        description,
        info,
        theme,
        ControlAlignment::Start,
        |ui| {
            add_control(ui);
        },
    );
}

/// A setting row whose single compact control is aligned with the card's
/// right edge.
///
/// Simple switches used to sit immediately after a long description, which
/// left the right half of the row empty and made the control feel attached to
/// the sentence rather than the setting. The label column also gets more room
/// here, because a 38 px switch does not need a wide control column.
pub fn setting_item_toggle(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    theme: &Theme,
    value: &mut bool,
) -> bool {
    setting_item_impl(
        ui,
        label,
        description,
        None,
        theme,
        ControlAlignment::End,
        |ui| toggle_switch(ui, value, theme),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlAlignment {
    Start,
    End,
}

fn setting_item_impl<R>(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    info: Option<&crate::components::info_box::InfoBoxContent>,
    theme: &Theme,
    alignment: ControlAlignment,
    add_control: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(theme.space_xl, 0.0);
        let avail_w = ui.available_width();
        // The label column grows with the window instead of staying at a
        // fixed 240, so a wide window does not leave the control floating in
        // the middle of the card with a gutter after it.
        let (share, max_label_w) = match alignment {
            ControlAlignment::Start => (0.30, 340.0),
            ControlAlignment::End => (0.55, 520.0),
        };
        let label_w = (avail_w * share)
            .clamp(200.0, max_label_w)
            .min((avail_w - 170.0).max(160.0));
        let control_w = (avail_w - label_w - theme.space_xl).max(160.0);

        // Column 1: Label + info icon + description
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 24.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                    ui.label(
                        egui::RichText::new(label)
                            .font(crate::theme::font(13.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    if let Some(info_content) = info {
                        let id_str = format!("setting_info_{}", label);
                        crate::components::info_box::render_info_icon_button(
                            ui,
                            id_str,
                            info_content,
                            theme,
                        );
                    }
                });
                if !description.is_empty() {
                    ui.label(
                        egui::RichText::new(description)
                            .font(crate::theme::font(11.0))
                            .color(theme.text_muted),
                    );
                }
            },
        );

        // Column 2: the controls fill the rest of the row. Sliders are given
        // the full width they can have, so their right-hand end lands on the
        // card's right edge and the row reads as two columns rather than a
        // label with a control abandoned beside it. Compact controls opt into
        // a right-aligned layout instead.
        let controls = ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
            // The value box beside a slider takes about 74px; the slider
            // gets what is left.
            ui.spacing_mut().slider_width = (control_w - 86.0).clamp(120.0, 620.0);
            match alignment {
                ControlAlignment::Start => add_control(ui),
                ControlAlignment::End => {
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        add_control,
                    )
                    .inner
                }
            }
        });
        (controls.response.rect, controls.inner)
    });

    let (controls_rect, value) = row.inner;
    // A search result or command-palette entry asked for this row. The row is
    // only known after it has been laid out, which is exactly when a scroll
    // target can be registered. The highlight spans the controls as well as
    // the label: the horizontal row's own rect stops at the label column.
    let highlight = row.response.rect.union(controls_rect);
    apply_setting_jump(label, &row.response, highlight, ui, theme);

    value
}

pub fn toggle_switch(ui: &mut egui::Ui, value: &mut bool, theme: &Theme) -> bool {
    let size = Vec2::new(38.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, "Toggle setting")
    });
    let mut changed = false;
    if resp.clicked() {
        *value = !*value;
        changed = true;
    }
    let painter = ui.painter_at(rect);
    let bg_color = if *value {
        theme.accent
    } else if resp.hovered() {
        theme.bg_pill.linear_multiply(1.2)
    } else {
        theme.bg_pill
    };
    let border_stroke = if *value {
        Stroke::new(1.0, theme.accent_highlight)
    } else {
        Stroke::new(1.0, theme.border_card)
    };
    painter.rect(
        rect,
        CornerRadius::same(theme.radius_lg),
        bg_color,
        border_stroke,
        egui::StrokeKind::Inside,
    );

    let radius = 7.0;
    let knob_center_x = if *value {
        rect.max.x - radius - 3.0
    } else {
        rect.min.x + radius + 3.0
    };
    let knob_pos = Pos2::new(knob_center_x, rect.center().y);
    let knob_color = if *value {
        Color32::WHITE
    } else {
        theme.text_secondary
    };
    painter.circle_filled(knob_pos, radius, knob_color);

    changed
}

pub fn segment_picker<T: Clone + PartialEq>(
    ui: &mut egui::Ui,
    selected: &mut T,
    options: &[(T, &str)],
    theme: &Theme,
) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        for (val, lbl) in options {
            let is_sel = selected == val;
            let btn = egui::Button::new(
                egui::RichText::new(*lbl)
                    .font(crate::theme::font(11.8))
                    .color(if is_sel {
                        Color32::WHITE
                    } else {
                        theme.text_secondary
                    }),
            )
            .fill(if is_sel { theme.accent } else { theme.bg_pill })
            .stroke(if is_sel {
                Stroke::new(1.0, theme.accent_highlight)
            } else {
                Stroke::new(1.0, theme.border_card)
            })
            .corner_radius(CornerRadius::same(theme.radius_sm))
            .min_size(Vec2::new(0.0, 26.0));

            if ui.add(btn).clicked() && !is_sel {
                *selected = val.clone();
                changed = true;
            }
        }
    });
    changed
}

pub fn pill_btn(label: impl Into<String>, selected: bool, theme: &Theme) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.into())
            .font(crate::theme::font(11.5))
            .color(if selected {
                Color32::WHITE
            } else {
                theme.text_secondary
            }),
    )
    .fill(if selected {
        theme.accent
    } else {
        theme.bg_pill
    })
    .stroke(Stroke::new(1.0, theme.border_card))
    .corner_radius(CornerRadius::same(theme.radius_sm))
    .min_size(Vec2::new(0.0, 24.0))
}

pub(super) fn badge_label_toned(
    ui: &mut egui::Ui,
    text: impl Into<String>,
    tone: crate::components::output_facts::FactTone,
    theme: &Theme,
) {
    let t = text.into();
    let color = match tone {
        crate::components::output_facts::FactTone::Neutral => theme.text_primary,
        crate::components::output_facts::FactTone::Good => theme.bit_perfect_green,
        crate::components::output_facts::FactTone::Warn => theme.warning_yellow,
    };
    egui::Frame::NONE
        .fill(theme.bg_pill)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_sm))
        .inner_margin(egui::Margin::symmetric(
            theme.space_md as i8,
            theme.space_xxs as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t)
                    .font(crate::theme::font(11.5))
                    .strong()
                    .color(color),
            );
        });
}

pub(super) fn badge_label(ui: &mut egui::Ui, text: impl Into<String>, theme: &Theme) {
    let t = text.into();
    egui::Frame::NONE
        .fill(theme.bg_pill)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_sm))
        .inner_margin(egui::Margin::symmetric(
            theme.space_md as i8,
            theme.space_xxs as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t)
                    .font(crate::theme::font(11.5))
                    .strong()
                    .color(theme.text_primary),
            );
        });
}

/// The name of a language choice, in the language it names.
///
/// A language picker shows every language in its own language: a reader who
/// does not know the current one still finds theirs.
pub(super) fn language_label(preference: &str) -> String {
    match preference {
        "" => tr!("Auto (system)"),
        "en" => "English".to_string(),
        "de" => "Deutsch".to_string(),
        other => other.to_string(),
    }
}

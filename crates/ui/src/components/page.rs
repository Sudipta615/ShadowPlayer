//! The shared page shell.
//!
//! Every page in the app is built from the same few parts: a lead line under
//! the topbar's title, cards that hold the content, and a strip of tabs to
//! move between sections. They lived in three places before — Settings had one
//! set, Help another, the queue/lyrics panel a third — and each drifted: two
//! different tab strips, two different card margins, two different heading
//! sizes. This is the one definition, so a page cannot look like it came from
//! a different app.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme::Theme;

/// The height of a tab in a [`tab_strip`].
const TAB_HEIGHT: f32 = 34.0;

/// The inset of a tab strip's container.
const STRIP_INSET: f32 = 5.0;

/// One tab in a [`tab_strip`].
#[derive(Clone, Copy)]
pub struct PageTab<'a> {
    pub label: &'a str,
    /// Drawn to the left of the label. The settings strip uses icons; the
    /// help strip and the queue/lyrics pill do not.
    pub icon: Option<fn(&egui::Ui, Rect, Color32)>,
}

impl<'a> PageTab<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, icon: None }
    }

    pub fn with_icon(label: &'a str, icon: fn(&egui::Ui, Rect, Color32)) -> Self {
        Self {
            label,
            icon: Some(icon),
        }
    }
}

/// The line under the page title: one sentence on what the page is for.
///
/// The title itself lives in the topbar, which is sticky, so a page never
/// grows a second heading of its own.
pub fn page_lead(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(crate::theme::font(12.5))
            .color(theme.text_muted),
    );
    ui.add_space(theme.space_md);
}

/// A card, in the app's one card style.
pub fn page_card(ui: &mut egui::Ui, theme: &Theme, add_contents: impl FnOnce(&mut egui::Ui)) {
    page_card_with_padding(ui, theme, (20, 16), add_contents);
}

/// A card with an explicit inner margin.
pub fn page_card_with_padding(
    ui: &mut egui::Ui,
    theme: &Theme,
    padding: (i8, i8),
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_xl))
        .inner_margin(egui::Margin::symmetric(padding.0, padding.1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xl);
            add_contents(ui);
        });
}

/// A card with a title, an optional subtitle and a rule under them.
pub fn page_card_titled(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    subtitle: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    page_card(ui, theme, |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_xxs);
            ui.label(
                egui::RichText::new(title)
                    .font(crate::theme::font(16.0))
                    .strong()
                    .color(theme.text_primary),
            );
            if !subtitle.is_empty() {
                ui.label(
                    egui::RichText::new(subtitle)
                        .font(crate::theme::font(12.0))
                        .color(theme.text_muted),
                );
            }
        });
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);
        add_contents(ui);
    });
}

/// A small status pill: a dot and a label.
pub fn status_pill(ui: &mut egui::Ui, theme: &Theme, label: &str, dot: Color32, fg: Color32) {
    egui::Frame::NONE
        .fill(theme.bg_pill)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_sm))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);
                let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter_at(dot_rect)
                    .circle_filled(dot_rect.center(), 3.5, dot);
                ui.label(
                    egui::RichText::new(label)
                        .font(crate::theme::font(11.5))
                        .strong()
                        .color(fg),
                );
            });
        });
}

/// A row of tabs in the app's one tab-strip style.
///
/// `fill_width` divides the width evenly between the tabs, which is what a
/// page-level section strip wants; leaving it off sizes each tab to its label,
/// which is what a short strip of two or three wants.
///
/// Returns the index the user picked, if any.
pub fn tab_strip(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    tabs: &[PageTab<'_>],
    selected: usize,
    fill_width: bool,
) -> Option<usize> {
    tab_strip_wrapping(ui, theme, id_salt, tabs, selected, fill_width, false)
}

/// A [`tab_strip`] that wraps onto a second row instead of overflowing.
///
/// The help manual has nine sections, which do not fit across a narrow window;
/// wrapping keeps every tab reachable without a scroll bar inside a scroll bar.
pub fn tab_strip_wrapping(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    tabs: &[PageTab<'_>],
    selected: usize,
    fill_width: bool,
    wrap: bool,
) -> Option<usize> {
    if tabs.is_empty() {
        return None;
    }
    let mut picked = None;

    egui::Frame::NONE
        .fill(theme.bg_card)
        .stroke(Stroke::new(1.0, theme.border_card))
        .corner_radius(CornerRadius::same(theme.radius_lg))
        .inner_margin(egui::Margin::symmetric(
            STRIP_INSET as i8,
            STRIP_INSET as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut body = |ui: &mut egui::Ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

                let gap = 4.0 * (tabs.len() as f32 - 1.0);
                let even_w = ((ui.available_width() - gap) / tabs.len() as f32).max(60.0);

                for (index, tab) in tabs.iter().enumerate() {
                    let is_active = index == selected;

                    // A tab that is sized to its label needs the text
                    // measured first, so the icon and the padding fit.
                    let text_w = ui
                        .painter()
                        .layout_no_wrap(
                            tab.label.to_string(),
                            crate::theme::font(12.0),
                            theme.text_primary,
                        )
                        .rect
                        .width();
                    let tab_w = if fill_width {
                        even_w
                    } else {
                        text_w + if tab.icon.is_some() { 48.0 } else { 26.0 }
                    };

                    let (tab_rect, resp) =
                        ui.allocate_exact_size(Vec2::new(tab_w, TAB_HEIGHT), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::Button, true, is_active, tab.label)
                    });
                    if resp.clicked() && !is_active {
                        picked = Some(index);
                    }

                    // The active tab fades in rather than snapping, so a
                    // switch reads as a change of state and not a repaint.
                    // The hover tint fades in rather than snapping, so a
                    // pointer moving across the strip does not strobe.
                    let hover_t = ui.ctx().animate_bool_with_time(
                        ui.scope_id().with((id_salt, "tab-hover", index)),
                        resp.hovered(),
                        0.12,
                    );

                    let bg = if is_active {
                        theme.accent
                    } else {
                        theme.bg_pill_hover.gamma_multiply(hover_t)
                    };

                    let p = ui.painter_at(tab_rect);
                    p.rect(
                        tab_rect,
                        CornerRadius::same(theme.radius_md),
                        bg,
                        Stroke::new(
                            1.0,
                            if is_active {
                                theme.accent_highlight
                            } else {
                                Color32::TRANSPARENT
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );

                    let fg = if is_active {
                        Color32::WHITE
                    } else {
                        theme.text_secondary
                    };

                    if let Some(icon) = tab.icon {
                        let icon_rect = Rect::from_center_size(
                            Pos2::new(tab_rect.min.x + 16.0, tab_rect.center().y),
                            Vec2::splat(14.0),
                        );
                        icon(ui, icon_rect, fg);
                    }

                    let text_left = if tab.icon.is_some() {
                        tab_rect.min.x + 28.0
                    } else {
                        tab_rect.min.x + 26.0
                    };
                    p.text(
                        Pos2::new(text_left, tab_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        tab.label,
                        crate::theme::font(12.0),
                        fg,
                    );
                }
            };
            if wrap {
                ui.horizontal_wrapped(&mut body);
            } else {
                ui.horizontal(&mut body);
            }
        });

    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_can_be_built_without_an_icon() {
        let tab = PageTab::new("Playback");
        assert_eq!(tab.label, "Playback");
        assert!(tab.icon.is_none());
    }

    #[test]
    fn a_tab_can_carry_an_icon() {
        let tab = PageTab::with_icon("Output", crate::components::icons::draw_disc);
        assert!(tab.icon.is_some());
    }
}

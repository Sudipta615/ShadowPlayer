use crate::theme::Theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Default)]
pub struct WindowControlsResponse {
    pub close_requested: bool,
    pub minimize_requested: bool,
    pub maximize_requested: bool,
    pub theme_toggle_requested: bool,
}

/// The stable id of the search field.
///
/// It is fixed rather than derived from the layout so the app can move the
/// keyboard focus to it from a shortcut (`/` or `Ctrl+F`) without needing the
/// `Ui` that drew it.
pub fn search_input_id() -> egui::Id {
    egui::Id::unique("shadowplayer_search_input")
}

/// Renders the search bar that fills the top of the Center Column,
/// perfectly aligning its left edge with the Now Playing card and Songs table.
pub fn render_search_input(ui: &mut Ui, query: &mut String, theme: &Theme) -> bool {
    let mut changed = false;
    let width = ui.available_width();

    ui.allocate_ui_with_layout(
        Vec2::new(width, 38.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            let frame = egui::Frame::NONE
                .fill(theme.bg_input)
                .stroke(Stroke::new(1.0_f32, theme.border_input))
                .corner_radius(CornerRadius::same(theme.radius_pill))
                .inner_margin(egui::Margin::symmetric(
                    theme.space_2xl as i8,
                    theme.space_md as i8,
                ));

            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(theme.space_lg, 0.0);

                    // Vector Search Icon
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                    crate::components::icons::draw_search(ui, icon_rect, theme.text_muted);

                    let hint = "Search for songs, artists, albums, or playlists...";
                    let text_edit = egui::TextEdit::singleline(query)
                        .id(search_input_id())
                        .hint_text(
                            egui::RichText::new(hint)
                                .color(theme.text_muted)
                                .font(crate::theme::font(13.0)),
                        )
                        .text_color(theme.text_primary)
                        .font(crate::theme::font(13.5))
                        .desired_width((width - 56.0).max(80.0))
                        .frame(egui::Frame::NONE);

                    let edit_response = ui.add(text_edit);
                    if edit_response.changed() {
                        changed = true;
                    }
                });
            });
        },
    );

    changed
}

/// Renders the Theme toggle pill (Sun/Moon) + Window controls (—, □, ✕)
/// aligned at the top of the Right Column.
pub fn render_window_controls(ui: &mut Ui, theme: &Theme) -> WindowControlsResponse {
    let mut resp = WindowControlsResponse::default();
    let width = ui.available_width();

    ui.allocate_ui_with_layout(
        Vec2::new(width, 38.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            // Theme Toggle Capsule (Sun / Moon) - Window controls removed as OS titlebar provides them
            let (toggle_rect, toggle_resp) =
                ui.allocate_exact_size(Vec2::new(56.0, 28.0), Sense::click());
            let painter = ui.painter_at(toggle_rect);

            if toggle_resp.clicked() {
                resp.theme_toggle_requested = true;
            }
            toggle_resp.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if theme.is_light {
                        "Switch to the dark theme"
                    } else {
                        "Switch to the light theme"
                    },
                )
            });
            let _ = toggle_resp.on_hover_text(if theme.is_light {
                "Switch to the dark theme"
            } else {
                "Switch to the light theme"
            });

            // Capsule pill background
            painter.rect_filled(
                toggle_rect,
                CornerRadius::same(theme.radius_xl),
                theme.bg_input,
            );
            painter.rect_stroke(
                toggle_rect,
                CornerRadius::same(theme.radius_xl),
                Stroke::new(1.0_f32, theme.border_subtle),
                StrokeKind::Inside,
            );

            let sun_center = Pos2::new(toggle_rect.min.x + 14.0, toggle_rect.center().y);
            let moon_center = Pos2::new(toggle_rect.max.x - 14.0, toggle_rect.center().y);
            let sun_rect = Rect::from_center_size(sun_center, Vec2::new(14.0, 14.0));
            let moon_rect = Rect::from_center_size(moon_center, Vec2::new(14.0, 14.0));

            if theme.is_light {
                // Active light mode: glowing sun circle on left
                painter.circle_filled(sun_center, 11.0, theme.sun_accent);
                crate::components::icons::draw_sun(ui, sun_rect, Color32::WHITE);
                crate::components::icons::draw_moon(ui, moon_rect, theme.text_muted);
            } else {
                // Sun icon (dim) on left
                crate::components::icons::draw_sun(ui, sun_rect, theme.text_muted);

                // Active dark mode Moon with glowing jade circle on right
                painter.circle_filled(moon_center, 11.0, theme.accent);
                crate::components::icons::draw_moon(ui, moon_rect, Color32::WHITE);
            }
        },
    );

    resp
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopbarAction {
    Queue,
    Theme,
    MiniPlayer,
}

/// A compact top-bar action button shared by the full and narrow layouts.
pub fn render_action_button(
    ui: &mut Ui,
    action: TopbarAction,
    active: bool,
    theme: &Theme,
    tooltip: &str,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let clicked = response.clicked();
    let hovered = response.hovered();

    if active {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme.radius_md),
            theme.bg_card_active,
        );
    } else if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(theme.radius_md),
            theme.bg_card_hover,
        );
    }

    let color = if active {
        theme.accent_highlight
    } else if hovered {
        theme.text_primary
    } else {
        theme.text_muted
    };
    let icon = Rect::from_center_size(rect.center(), Vec2::splat(17.0));
    match action {
        TopbarAction::Queue => crate::components::icons::draw_list_icon(ui, icon, color),
        TopbarAction::Theme => {
            if theme.is_light {
                crate::components::icons::draw_sun(ui, icon, color);
            } else {
                crate::components::icons::draw_moon(ui, icon, color);
            }
        }
        TopbarAction::MiniPlayer => {
            let p = ui.painter_at(rect);
            let outer = Rect::from_center_size(rect.center(), Vec2::new(18.0, 13.0));
            p.rect_stroke(
                outer,
                CornerRadius::same(theme.radius_xs),
                Stroke::new(1.4_f32, color),
                StrokeKind::Inside,
            );
            let inner =
                Rect::from_center_size(outer.center() + Vec2::new(1.0, 1.0), Vec2::new(9.0, 6.0));
            p.rect_filled(inner, CornerRadius::same(theme.radius_xs), color);
        }
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), active, tooltip)
    });
    let _ = response.on_hover_text(tooltip);
    clicked
}

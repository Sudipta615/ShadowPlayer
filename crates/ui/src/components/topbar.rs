use crate::theme::Theme;
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

#[derive(Default)]
pub struct WindowControlsResponse {
    pub close_requested: bool,
    pub minimize_requested: bool,
    pub maximize_requested: bool,
    pub theme_toggle_requested: bool,
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
                .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                .corner_radius(CornerRadius::same(19))
                .inner_margin(egui::Margin::symmetric(14, 8));

            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

                    // Vector Search Icon
                    let (icon_rect, _) =
                        ui.allocate_exact_size(Vec2::new(18.0, 18.0), Sense::hover());
                    crate::components::icons::draw_search(ui, icon_rect, theme.text_muted);

                    let hint = "Search for songs, artists, albums, or playlists...";
                    let text_edit = egui::TextEdit::singleline(query)
                        .hint_text(
                            egui::RichText::new(hint)
                                .color(theme.text_muted)
                                .font(FontId::proportional(13.0)),
                        )
                        .text_color(theme.text_primary)
                        .font(FontId::proportional(13.5))
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
            ui.spacing_mut().item_spacing = Vec2::new(14.0, 0.0);

            // Window close, maximize, minimize
            let (close_rect, close_resp) =
                ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
            let close_col = if close_resp.hovered() {
                theme.destructive
            } else {
                theme.text_secondary
            };
            crate::components::icons::draw_window_close(ui, close_rect, close_col);
            if close_resp.clicked() {
                resp.close_requested = true;
            }

            let (max_rect, max_resp) =
                ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
            let max_col = if max_resp.hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            crate::components::icons::draw_window_max(ui, max_rect, max_col);
            if max_resp.clicked() {
                resp.maximize_requested = true;
            }

            let (min_rect, min_resp) =
                ui.allocate_exact_size(Vec2::new(16.0, 16.0), Sense::click());
            let min_col = if min_resp.hovered() {
                theme.text_primary
            } else {
                theme.text_secondary
            };
            crate::components::icons::draw_window_min(ui, min_rect, min_col);
            if min_resp.clicked() {
                resp.minimize_requested = true;
            }

            ui.add_space(8.0);

            // Theme Toggle Capsule (Sun / Moon)
            let (toggle_rect, toggle_resp) =
                ui.allocate_exact_size(Vec2::new(56.0, 28.0), Sense::click());
            let painter = ui.painter_at(toggle_rect);

            if toggle_resp.clicked() {
                resp.theme_toggle_requested = true;
            }

            // Capsule pill background
            painter.rect_filled(toggle_rect, CornerRadius::same(14), theme.bg_input);
            painter.rect_stroke(
                toggle_rect,
                CornerRadius::same(14),
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

                // Active dark mode Moon with glowing purple circle on right
                painter.circle_filled(moon_center, 11.0, theme.accent_violet);
                crate::components::icons::draw_moon(ui, moon_rect, Color32::WHITE);
            }
        },
    );

    resp
}

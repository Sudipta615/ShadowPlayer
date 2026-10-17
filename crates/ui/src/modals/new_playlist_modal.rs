use eframe::egui::{self, Color32, FontId, Vec2};
use crate::theme::Theme;

/// Render dialogue for creating a new playlist; returns Some(name) if confirmed
pub fn show_new_playlist_modal(
    ctx: &egui::Context,
    is_open: &mut bool,
    playlist_name: &mut String,
    theme: &Theme,
) -> Option<String> {
    if !*is_open {
        return None;
    }

    let mut modal_open = true;
    let mut should_close = false;
    let mut create_name = None;
    let mut local_name = playlist_name.clone();

    egui::Window::new("Create New Playlist")
        .open(&mut modal_open)
        .collapsible(false)
        .resizable(false)
        .default_size(Vec2::new(320.0, 130.0))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
            ui.label(
                egui::RichText::new("Playlist Name:")
                    .font(FontId::proportional(13.0))
                    .color(theme.text_primary),
            );
            let edit = egui::TextEdit::singleline(&mut local_name).desired_width(ui.available_width());
            ui.add(edit);

            ui.horizontal(|ui| {
                let create_btn = egui::Button::new(
                    egui::RichText::new("Create").color(Color32::WHITE),
                )
                .fill(theme.accent_violet);

                if ui.add(create_btn).clicked() && !local_name.trim().is_empty() {
                    create_name = Some(local_name.trim().to_string());
                    should_close = true;
                }
                if ui.button("Cancel").clicked() {
                    should_close = true;
                }
            });
        });

    *playlist_name = local_name;
    *is_open = modal_open && !should_close;
    create_name
}

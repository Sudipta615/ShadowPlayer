use crate::theme::Theme;
use eframe::egui::{self, CornerRadius, FontId, Stroke, Vec2, Window};

pub fn show_help_modal(ctx: &egui::Context, is_open: &mut bool, theme: &Theme) {
    if !*is_open {
        return;
    }

    Window::new("ShadowPlayer Help & Shortcuts")
        .open(is_open)
        .resizable(false)
        .collapsible(false)
        .default_size(Vec2::new(560.0, 560.0))
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 12.0);

            // App Brand Banner
            let banner_frame = egui::Frame::NONE
                .fill(theme.bg_pill)
                .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(14));

            banner_frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    ui.label(
                        egui::RichText::new(concat!("ShadowPlayer v", env!("CARGO_PKG_VERSION")))
                            .font(FontId::proportional(16.0))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Audiophile Local Desktop Music Player • Audio Engine v{}",
                            engine::VERSION
                        ))
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                });
            });

            // Keyboard Shortcuts Section
            ui.label(
                egui::RichText::new("Audiophile Keyboard Shortcuts")
                    .font(FontId::proportional(14.0))
                    .strong()
                    .color(theme.accent_magenta),
            );

            let shortcuts = [
                ("Space / K", "Play / Pause playback"),
                ("J", "Previous track"),
                ("L", "Next track"),
                ("M", "Toggle mute / unmute"),
                ("S", "Toggle shuffle playback"),
                ("R", "Cycle repeat mode (Off / All / One)"),
                ("Left Arrow", "Seek backward 5 seconds"),
                ("Right Arrow", "Seek forward 5 seconds"),
                ("Up Arrow", "Increase volume by 5%"),
                ("Down Arrow", "Decrease volume by 5%"),
                ("Ctrl + B", "Toggle Bit-Perfect Pure Direct mode"),
                ("Ctrl + N", "Open the playlist editor (create or manage playlists)"),
                ("Ctrl + P", "Open Settings & DSP Studio"),
                ("Ctrl + O", "Open Folders & Library Manager"),
                ("Ctrl + E", "Toggle the Equalizer & DSP studio panel"),
                ("Escape", "Close the open dialog (one layer per press), or clear the search"),
            ];

            let table_frame = egui::Frame::NONE
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0_f32, theme.border_card))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(14, 10));

            table_frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
                    for (key, desc) in shortcuts {
                        ui.horizontal(|ui| {
                            let (badge_rect, _) = ui.allocate_exact_size(Vec2::new(110.0, 22.0), egui::Sense::hover());
                            let bp = ui.painter_at(badge_rect);
                            bp.rect_filled(badge_rect, CornerRadius::same(4), theme.bg_pill);
                            bp.rect_stroke(badge_rect, CornerRadius::same(4), Stroke::new(1.0_f32, theme.border_subtle), egui::StrokeKind::Inside);
                            bp.text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                key,
                                FontId::monospace(11.5),
                                theme.accent_purple,
                            );

                            ui.label(
                                egui::RichText::new(desc)
                                    .font(FontId::proportional(12.5))
                                    .color(theme.text_secondary),
                            );
                        });
                    }
                });
            });

            // Supported Codecs Section
            ui.label(
                egui::RichText::new("Supported Audiophile Codecs & Formats")
                    .font(FontId::proportional(14.0))
                    .strong()
                    .color(theme.accent_purple),
            );

            let codecs_info = "Lossless: FLAC, ALAC, WAV (PCM), AIFF, APE, WavPack (WV), TTA, DSD (DSF/DFF)\nLossy: MP3, AAC/M4A, OGG Vorbis, Opus\nFeatures: 64-bit float mixing, Rubato sinc resampler, EBU R128 loudness, Parametric EQ, Bit-perfect direct stream.";

            ui.label(
                egui::RichText::new(codecs_info)
                    .font(FontId::proportional(12.0))
                    .color(theme.text_muted),
            );
        });
}

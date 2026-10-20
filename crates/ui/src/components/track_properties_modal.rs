use crate::theme::Theme;
use domain::models::Track;
use eframe::egui::{self, CornerRadius, Stroke, Vec2, Window};

pub fn show_track_properties_modal(
    ctx: &egui::Context,
    track: Option<&Track>,
    is_open: &mut bool,
    theme: &Theme,
) {
    if !*is_open || track.is_none() {
        return;
    }

    let track = track.unwrap();

    Window::new("Track Properties & Audio Details")
        .open(is_open)
        .resizable(true)
        .collapsible(false)
        .default_size(crate::dialog::fit_to_viewport(ctx, Vec2::new(520.0, 420.0)))
        .min_size(crate::dialog::min_dialog_size())
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

            let card_frame = egui::Frame::NONE
                .fill(theme.bg_card)
                .stroke(Stroke::new(1.0_f32, theme.border_card))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::same(14));

            card_frame.show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);

                    render_property_row(ui, "Title", &track.title, theme);
                    render_property_row(ui, "Artist", &track.artist, theme);
                    render_property_row(
                        ui,
                        "Album",
                        if track.album.is_empty() {
                            "—"
                        } else {
                            &track.album
                        },
                        theme,
                    );
                    render_property_row(ui, "Genre", track.genre.as_deref().unwrap_or("—"), theme);
                    render_property_row(
                        ui,
                        "Year",
                        &track
                            .year
                            .map(|y| y.to_string())
                            .unwrap_or_else(|| "—".into()),
                        theme,
                    );
                    render_property_row(
                        ui,
                        "Track / Disc",
                        &format!(
                            "{}/{}",
                            track.track_number.unwrap_or(0),
                            track.disc_number.unwrap_or(1)
                        ),
                        theme,
                    );
                    render_property_row(
                        ui,
                        "Duration",
                        &format_duration(track.duration_secs as f64),
                        theme,
                    );

                    ui.separator();

                    render_property_row(
                        ui,
                        "Format",
                        &format!(
                            "{} ({})",
                            track.format,
                            if track.is_lossless {
                                "Lossless"
                            } else {
                                "Lossy"
                            }
                        ),
                        theme,
                    );
                    render_property_row(
                        ui,
                        "Sample Rate",
                        &format!("{} Hz", track.sample_rate),
                        theme,
                    );
                    render_property_row(
                        ui,
                        "Channels",
                        &format!(
                            "{} ({})",
                            track.channels,
                            if track.channels == 2 {
                                "Stereo"
                            } else {
                                "Mono / Multi"
                            }
                        ),
                        theme,
                    );
                    if let Some(br) = track.bitrate_kbps {
                        render_property_row(ui, "Bitrate", &format!("{} kbps", br), theme);
                    }
                    if let Some(lufs) = track.r128_loudness_lufs {
                        render_property_row(
                            ui,
                            "R128 Loudness",
                            &format!("{:.1} LUFS", lufs),
                            theme,
                        );
                    }

                    let size_mb = track.file_size_bytes as f64 / (1024.0 * 1024.0);
                    render_property_row(
                        ui,
                        "File Size",
                        &format!("{:.2} MB ({} bytes)", size_mb, track.file_size_bytes),
                        theme,
                    );
                    render_property_row(
                        ui,
                        "Play Count",
                        &format!("{} times", track.play_count),
                        theme,
                    );

                    ui.separator();

                    ui.label(
                        egui::RichText::new("File Path:")
                            .font(crate::theme::font(11.5))
                            .color(theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(track.path.to_string_lossy())
                            .font(crate::theme::mono_font(11.0))
                            .color(theme.text_secondary),
                    );
                });
            });
        });
}

fn render_property_row(ui: &mut egui::Ui, label: &str, value: &str, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [110.0, 18.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
            ),
        );
        ui.label(
            egui::RichText::new(value)
                .font(crate::theme::font(12.5))
                .color(theme.text_primary),
        );
    });
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    let mins = s / 60;
    let secs = s % 60;
    format!("{}:{:02}", mins, secs)
}

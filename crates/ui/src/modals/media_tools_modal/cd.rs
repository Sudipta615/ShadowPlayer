//! The CD Ripper tab of the media-tools modal.
use eframe::egui;
use media_tools::cd::RipFormat;

use super::{row, MediaToolsActions, MediaToolsModalState};
use crate::theme::Theme;

pub(super) fn render_cd(
    ui: &mut egui::Ui,
    state: &mut MediaToolsModalState,
    theme: &Theme,
    actions: &mut MediaToolsActions,
) {
    ui.label(
        egui::RichText::new(
            "Read an audio CD with libcdio's paranoia mode, fetch CDDB metadata when the \
             library is available, and encode each track directly. Ripping never overwrites \
             an existing file.",
        )
        .font(crate::theme::font(11.5))
        .color(theme.text_muted),
    );
    ui.add_space(theme.space_md);

    row(ui, theme, "Drive", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.cd_device)
                .hint_text("/dev/sr0 — blank uses the system default")
                .desired_width(260.0),
        );
        if ui
            .add_enabled(!state.busy, egui::Button::new("Read Disc"))
            .clicked()
        {
            actions.probe_disc = true;
        }
    });

    row(ui, theme, "Format", |ui| {
        egui::ComboBox::from_id_salt("cd_rip_format")
            .selected_text(match state.cd_format {
                RipFormat::Wav => "WAV (uncompressed)",
                RipFormat::Flac16 => "FLAC 16-bit",
                RipFormat::Flac24 => "FLAC 24-bit",
                RipFormat::Opus => "Opus 48 kHz",
            })
            .show_ui(ui, |ui| {
                for (format, label) in [
                    (RipFormat::Wav, "WAV (uncompressed)"),
                    (RipFormat::Flac16, "FLAC 16-bit"),
                    (RipFormat::Flac24, "FLAC 24-bit"),
                    (RipFormat::Opus, "Opus 48 kHz"),
                ] {
                    ui.selectable_value(&mut state.cd_format, format, label);
                }
            });
        ui.checkbox(&mut state.cd_fetch_metadata, "Fetch CDDB metadata");
    });

    row(ui, theme, "Save to", |ui| {
        ui.label(
            egui::RichText::new(state.cd_output_dir.display().to_string())
                .font(crate::theme::mono_font(10.5))
                .color(theme.text_secondary),
        );
        if ui.button("Choose…").clicked() {
            actions.pick_cd_output = true;
        }
    });

    if let Some(error) = &state.cd_error {
        ui.label(
            egui::RichText::new(error)
                .font(crate::theme::font(11.0))
                .color(theme.destructive),
        );
    } else if let Some(message) = &state.cd_message {
        ui.label(
            egui::RichText::new(message)
                .font(crate::theme::font(11.0))
                .color(theme.bit_perfect_green),
        );
    }

    if let Some(disc) = &state.disc {
        if let Some(title) = &disc.metadata.title {
            ui.label(
                egui::RichText::new(format!(
                    "{title}{}",
                    disc.metadata
                        .artist
                        .as_ref()
                        .map(|artist| format!(" — {artist}"))
                        .unwrap_or_default()
                ))
                .font(crate::theme::font(13.0))
                .strong()
                .color(theme.text_primary),
            );
        }
        if !disc.warnings.is_empty() {
            for warning in &disc.warnings {
                ui.label(
                    egui::RichText::new(warning)
                        .font(crate::theme::font(10.5))
                        .color(theme.warning_yellow),
                );
            }
        }
        ui.add_space(theme.space_sm);
        egui::ScrollArea::vertical()
            .id_salt("cd_track_list")
            .max_height(230.0)
            .show(ui, |ui| {
                for track in &disc.tracks {
                    ui.horizontal(|ui| {
                        let mut selected = state.cd_selected.contains(&track.number);
                        if ui.checkbox(&mut selected, "").changed() {
                            if selected {
                                state.cd_selected.insert(track.number);
                            } else {
                                state.cd_selected.remove(&track.number);
                            }
                        }
                        ui.label(
                            egui::RichText::new(format!("{:02}", track.number))
                                .font(crate::theme::mono_font(11.0))
                                .color(theme.text_muted),
                        );
                        ui.label(
                            egui::RichText::new(if track.title.trim().is_empty() {
                                format!("Track {}", track.number)
                            } else {
                                track.title.clone()
                            })
                            .font(crate::theme::font(12.0))
                            .color(theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}:{:02}",
                                    track.duration_secs as u64 / 60,
                                    track.duration_secs as u64 % 60
                                ))
                                .font(crate::theme::font(10.5))
                                .color(theme.text_muted),
                            );
                        });
                    });
                }
            });
        if let Some((track, count, title)) = &state.cd_progress {
            ui.add_space(theme.space_sm);
            let fraction = if *count > 0 {
                (*track as f32 / *count as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width(f32::INFINITY)
                    .text(format!("Track {track} of {count} — {title}")),
            );
        }
        ui.add_space(theme.space_sm);
        ui.horizontal(|ui| {
            let label = if state.cd_selected.is_empty() {
                "Rip Entire Disc".to_string()
            } else {
                format!("Rip {} Selected Track(s)", state.cd_selected.len())
            };
            if ui
                .add_enabled(
                    !state.busy,
                    egui::Button::new(
                        egui::RichText::new(label)
                            .color(egui::Color32::WHITE)
                            .strong(),
                    )
                    .fill(theme.accent),
                )
                .clicked()
            {
                actions.rip_disc = true;
            }
        });
    } else {
        ui.label(
            egui::RichText::new(
                "No disc read yet. Put an audio CD in the drive and choose Read Disc.",
            )
            .font(crate::theme::font(11.0))
            .color(theme.text_dim),
        );
    }
}

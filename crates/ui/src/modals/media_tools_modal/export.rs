//! The Transcode Export tab of the media-tools modal.
use eframe::egui;

use super::{row, ExportChoice, MediaToolsActions, MediaToolsModalState};
use crate::theme::Theme;

pub(super) fn render_export(
    ui: &mut egui::Ui,
    state: &mut MediaToolsModalState,
    theme: &Theme,
    actions: &mut MediaToolsActions,
) {
    ui.label(
        egui::RichText::new(
            "Export local audio to FLAC or Opus. Opus is encoded at 48 kHz; FLAC keeps the \
             source rate and depth. Tags are copied where the container supports them.",
        )
        .font(crate::theme::font(11.5))
        .color(theme.text_muted),
    );
    ui.add_space(theme.space_md);

    row(ui, theme, "Source files", |ui| {
        if ui.button("Add Files…").clicked() {
            actions.pick_export_files = true;
        }
        if !state.export_inputs.is_empty() && ui.button("Clear").clicked() {
            actions.clear_export_files = true;
        }
        ui.label(
            egui::RichText::new(format!("{} file(s)", state.export_inputs.len()))
                .font(crate::theme::font(10.5))
                .color(theme.text_muted),
        );
    });
    if !state.export_inputs.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt("export_file_list")
            .max_height(140.0)
            .show(ui, |ui| {
                for path in &state.export_inputs {
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .font(crate::theme::mono_font(10.0))
                            .color(theme.text_secondary),
                    );
                }
            });
    }

    row(ui, theme, "Format", |ui| {
        egui::ComboBox::from_id_salt("export_format")
            .selected_text(state.export_choice.label())
            .show_ui(ui, |ui| {
                for choice in [
                    ExportChoice::Flac16,
                    ExportChoice::Flac24,
                    ExportChoice::Wav16,
                    ExportChoice::Wav24,
                    ExportChoice::Opus,
                ] {
                    ui.selectable_value(&mut state.export_choice, choice, choice.label());
                }
            });
        ui.checkbox(&mut state.export_copy_tags, "Copy tags");
        ui.checkbox(&mut state.export_overwrite, "Overwrite");
    });

    row(ui, theme, "Save to", |ui| {
        ui.label(
            egui::RichText::new(state.export_output_dir.display().to_string())
                .font(crate::theme::mono_font(10.5))
                .color(theme.text_secondary),
        );
        if ui.button("Choose…").clicked() {
            actions.pick_export_output = true;
        }
    });

    if let Some(error) = &state.export_error {
        ui.label(
            egui::RichText::new(error)
                .font(crate::theme::font(11.0))
                .color(theme.destructive),
        );
    } else if let Some(message) = &state.export_message {
        ui.label(
            egui::RichText::new(message)
                .font(crate::theme::font(11.0))
                .color(theme.bit_perfect_green),
        );
    }

    if let Some((done, total, current)) = &state.export_progress {
        let fraction = if *total > 0 {
            *done as f32 / *total as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_width(f32::INFINITY)
                .text(format!("{done}/{total} — {current}")),
        );
    }

    ui.add_space(theme.space_sm);
    if ui
        .add_enabled(
            !state.busy && !state.export_inputs.is_empty(),
            egui::Button::new(
                egui::RichText::new("Export")
                    .color(egui::Color32::WHITE)
                    .strong(),
            )
            .fill(theme.accent),
        )
        .clicked()
    {
        actions.start_export = true;
    }
}

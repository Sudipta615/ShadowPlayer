//! The Updates tab of the media-tools modal.
use eframe::egui;

use super::{row, MediaToolsActions, MediaToolsModalState};
use crate::theme::Theme;

pub(super) fn render_update(
    ui: &mut egui::Ui,
    state: &mut MediaToolsModalState,
    theme: &Theme,
    actions: &mut MediaToolsActions,
) {
    ui.label(
        egui::RichText::new(
            "Update checks are opt-in. ShadowPlayer contacts GitHub only when you enable \
             them here and press Check. A download is verified against its published SHA-256 \
             and is never installed or run automatically.",
        )
        .font(crate::theme::font(11.5))
        .color(theme.text_muted),
    );
    ui.add_space(theme.space_md);

    let mut enabled = state.update_enabled;
    if ui
        .checkbox(&mut enabled, "Allow update checks against GitHub releases")
        .changed()
    {
        state.update_enabled = enabled;
        actions.update_enabled_changed = Some(enabled);
    }

    row(ui, theme, "Download to", |ui| {
        ui.label(
            egui::RichText::new(state.update_dir.display().to_string())
                .font(crate::theme::mono_font(10.5))
                .color(theme.text_secondary),
        );
        if ui.button("Choose…").clicked() {
            actions.pick_update_dir = true;
        }
    });

    ui.add_space(theme.space_sm);
    if ui
        .add_enabled(
            state.update_enabled && !state.busy,
            egui::Button::new("Check for Updates"),
        )
        .clicked()
    {
        actions.check_update = true;
    }

    if let Some(error) = &state.update_error {
        ui.add_space(theme.space_sm);
        ui.label(
            egui::RichText::new(error)
                .font(crate::theme::font(11.0))
                .color(theme.destructive),
        );
    }
    if let Some(message) = &state.update_message {
        ui.add_space(theme.space_sm);
        ui.label(
            egui::RichText::new(message)
                .font(crate::theme::font(11.0))
                .color(theme.bit_perfect_green),
        );
    }

    if let Some(check) = &state.update_check {
        ui.add_space(theme.space_md);
        ui.label(
            egui::RichText::new(format!(
                "Installed: {} · Latest: {}",
                check.current_version, check.release.version
            ))
            .font(crate::theme::font(12.0))
            .strong()
            .color(theme.text_primary),
        );
        ui.label(
            egui::RichText::new(&check.release.name)
                .font(crate::theme::font(11.5))
                .color(theme.text_secondary),
        );
        if !check.release.notes.trim().is_empty() {
            let summary = media_tools::markdown::summary(&check.release.notes, 700);
            ui.label(
                egui::RichText::new(summary)
                    .font(crate::theme::font(11.0))
                    .color(theme.text_muted),
            );
        }

        match &check.recommended_asset {
            Some(asset) if check.newer_available => {
                ui.add_space(theme.space_sm);
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {:.1} MB",
                        asset.name,
                        asset.size as f64 / (1024.0 * 1024.0)
                    ))
                    .font(crate::theme::mono_font(10.5))
                    .color(theme.text_secondary),
                );
                if ui
                    .add_enabled(
                        state.update_enabled && !state.busy,
                        egui::Button::new(
                            egui::RichText::new("Download and Verify")
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(theme.accent),
                    )
                    .clicked()
                {
                    actions.download_update = true;
                }
                ui.label(
                    egui::RichText::new(
                        "The asset is not installed. After verification, run it yourself when \
                         you are ready.",
                    )
                    .font(crate::theme::font(10.0))
                    .color(theme.text_dim),
                );
            }
            Some(_) => {
                ui.label(
                    egui::RichText::new("This is the newest release for your platform.")
                        .font(crate::theme::font(11.0))
                        .color(theme.bit_perfect_green),
                );
            }
            None => {
                ui.label(
                    egui::RichText::new("This release publishes no build for this platform.")
                        .font(crate::theme::font(11.0))
                        .color(theme.warning_yellow),
                );
            }
        }

        if ui.button("Open Release Page").clicked() {
            actions.open_release_page = true;
        }
    }

    if let Some(download) = &state.update_download {
        ui.add_space(theme.space_md);
        ui.label(
            egui::RichText::new(format!(
                "Verified {} ({:.1} MB)",
                download.version,
                download.bytes as f64 / (1024.0 * 1024.0)
            ))
            .font(crate::theme::font(11.5))
            .strong()
            .color(theme.bit_perfect_green),
        );
        ui.label(
            egui::RichText::new(download.path.display().to_string())
                .font(crate::theme::mono_font(10.0))
                .color(theme.text_secondary),
        );
        ui.label(
            egui::RichText::new(format!("SHA-256 {}", download.sha256))
                .font(crate::theme::mono_font(9.5))
                .color(theme.text_dim),
        );
    }
}

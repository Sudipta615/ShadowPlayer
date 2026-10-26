//! The Tools workshop: CD extraction, batch transcode export, and the opt-in
//! update flow.
//!
//! The modal is deliberately dumb. It renders snapshots that the app's
//! [`crate::services::MediaToolsService`] produces and returns action flags;
//! the app owns the worker, the file pickers, and the settings.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui;
use media_tools::cd::{CdDisc, RipFormat};
use media_tools::transcode::OutputFormat;
use media_tools::update::{UpdateCheck, VerifiedDownload};

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaToolsTab {
    #[default]
    Cd,
    Export,
    Update,
}

/// Which encoder an export uses. A UI-local mirror keeps the choices in the
/// order a person expects rather than the enum's declaration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportChoice {
    Flac16,
    #[default]
    Flac24,
    Wav16,
    Wav24,
    Opus,
}

impl ExportChoice {
    fn format(self) -> OutputFormat {
        match self {
            Self::Flac16 => OutputFormat::Flac { bit_depth: 16 },
            Self::Flac24 => OutputFormat::Flac { bit_depth: 24 },
            Self::Wav16 => OutputFormat::Wav { bit_depth: 16 },
            Self::Wav24 => OutputFormat::Wav { bit_depth: 24 },
            Self::Opus => OutputFormat::Opus,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Flac16 => "FLAC 16-bit",
            Self::Flac24 => "FLAC 24-bit",
            Self::Wav16 => "WAV 16-bit",
            Self::Wav24 => "WAV 24-bit",
            Self::Opus => "Opus 48 kHz",
        }
    }
}

/// The modal's whole state.
pub struct MediaToolsModalState {
    pub is_open: bool,
    pub tab: MediaToolsTab,

    // CD extraction
    pub cd_device: String,
    pub cd_output_dir: PathBuf,
    pub cd_format: RipFormat,
    pub cd_fetch_metadata: bool,
    pub disc: Option<CdDisc>,
    pub cd_selected: BTreeSet<u32>,
    pub cd_progress: Option<(u32, u32, String)>,
    pub cd_message: Option<String>,
    pub cd_error: Option<String>,

    // Batch export
    pub export_inputs: Vec<PathBuf>,
    pub export_output_dir: PathBuf,
    pub export_choice: ExportChoice,
    pub export_copy_tags: bool,
    pub export_overwrite: bool,
    pub export_progress: Option<(usize, usize, String)>,
    pub export_message: Option<String>,
    pub export_error: Option<String>,

    // Update
    pub update_enabled: bool,
    pub update_check: Option<UpdateCheck>,
    pub update_message: Option<String>,
    pub update_error: Option<String>,
    pub update_download: Option<VerifiedDownload>,
    pub update_dir: PathBuf,

    /// Worker snapshot, mirrored from the service each frame.
    pub busy: bool,
    pub activity: String,
}

impl Default for MediaToolsModalState {
    fn default() -> Self {
        let music = dirs::audio_dir()
            .or_else(|| dirs::home_dir().map(|home| home.join("Music")))
            .unwrap_or_else(|| PathBuf::from("."));
        let downloads = dirs::download_dir().unwrap_or_else(|| music.clone());
        Self {
            is_open: false,
            tab: MediaToolsTab::Cd,
            cd_device: String::new(),
            cd_output_dir: music.join("CD Rips"),
            cd_format: RipFormat::Flac24,
            cd_fetch_metadata: true,
            disc: None,
            cd_selected: BTreeSet::new(),
            cd_progress: None,
            cd_message: None,
            cd_error: None,
            export_inputs: Vec::new(),
            export_output_dir: music.join("Exports"),
            export_choice: ExportChoice::default(),
            export_copy_tags: true,
            export_overwrite: false,
            export_progress: None,
            export_message: None,
            export_error: None,
            update_enabled: false,
            update_check: None,
            update_message: None,
            update_error: None,
            update_download: None,
            update_dir: downloads.join("ShadowPlayer Updates"),
            busy: false,
            activity: String::new(),
        }
    }
}

impl MediaToolsModalState {
    pub fn open(&mut self, tab: MediaToolsTab) {
        self.is_open = true;
        self.tab = tab;
    }

    pub fn apply_disc(&mut self, disc: CdDisc) {
        self.cd_selected = disc.tracks.iter().map(|track| track.number).collect();
        self.cd_message = Some(format!(
            "{} audio track{} · {:.0} min",
            disc.tracks.len(),
            if disc.tracks.len() == 1 { "" } else { "s" },
            disc.total_duration_secs() / 60.0
        ));
        self.cd_error = None;
        self.disc = Some(disc);
    }

    pub fn update_format(&self) -> OutputFormat {
        self.export_choice.format()
    }

    pub fn selected_tracks(&self) -> Vec<u32> {
        self.cd_selected.iter().copied().collect()
    }
}

#[derive(Default)]
pub struct MediaToolsActions {
    pub close: bool,
    pub probe_disc: bool,
    pub rip_disc: bool,
    pub cancel: bool,
    pub pick_cd_output: bool,
    pub pick_export_files: bool,
    pub pick_export_output: bool,
    pub clear_export_files: bool,
    pub start_export: bool,
    pub pick_update_dir: bool,
    pub check_update: bool,
    pub download_update: bool,
    pub open_release_page: bool,
    /// The opt-in checkbox changed.
    pub update_enabled_changed: Option<bool>,
}

pub fn show_media_tools_modal(
    ctx: &egui::Context,
    state: &mut MediaToolsModalState,
    theme: &Theme,
) -> MediaToolsActions {
    let mut actions = MediaToolsActions::default();
    if !state.is_open {
        return actions;
    }

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "media_tools_modal",
        "Tools",
        720.0,
        theme,
        |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (MediaToolsTab::Cd, "CD Ripper"),
                    (MediaToolsTab::Export, "Transcode Export"),
                    (MediaToolsTab::Update, "Updates"),
                ] {
                    let selected = state.tab == tab;
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .font(crate::theme::font(12.0))
                                    .color(if selected {
                                        egui::Color32::WHITE
                                    } else {
                                        theme.text_secondary
                                    }),
                            )
                            .fill(if selected {
                                theme.accent
                            } else {
                                theme.bg_pill
                            })
                            .corner_radius(egui::CornerRadius::same(theme.radius_md)),
                        )
                        .clicked()
                    {
                        state.tab = tab;
                    }
                }
            });
            ui.add_space(theme.space_lg);

            if state.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(
                        egui::RichText::new(if state.activity.is_empty() {
                            "Working…"
                        } else {
                            state.activity.as_str()
                        })
                        .font(crate::theme::font(12.0))
                        .color(theme.accent_highlight),
                    );
                    if ui.button("Cancel").clicked() {
                        actions.cancel = true;
                    }
                });
                ui.add_space(theme.space_sm);
            }

            match state.tab {
                MediaToolsTab::Cd => render_cd(ui, state, theme, &mut actions),
                MediaToolsTab::Export => render_export(ui, state, theme, &mut actions),
                MediaToolsTab::Update => render_update(ui, state, theme, &mut actions),
            }
        },
    );

    if closed {
        actions.close = true;
    }
    actions
}

fn render_cd(
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

fn render_export(
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

fn render_update(
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

fn row(ui: &mut egui::Ui, theme: &Theme, label: &str, mut content: impl FnMut(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_width(120.0);
            ui.label(
                egui::RichText::new(label)
                    .font(crate::theme::font(11.5))
                    .strong()
                    .color(theme.text_primary),
            );
        });
        ui.vertical(|ui| content(ui));
    });
    ui.add_space(theme.space_sm);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_disc_selects_every_track() {
        let mut state = MediaToolsModalState::default();
        state.apply_disc(CdDisc {
            device: "/dev/sr0".into(),
            tracks: vec![
                media_tools::cd::CdTrack {
                    number: 1,
                    title: "One".into(),
                    artist: "A".into(),
                    first_sector: 0,
                    last_sector: 100,
                    duration_secs: 1.0,
                },
                media_tools::cd::CdTrack {
                    number: 2,
                    title: "Two".into(),
                    artist: "A".into(),
                    first_sector: 100,
                    last_sector: 200,
                    duration_secs: 1.0,
                },
            ],
            metadata: Default::default(),
            warnings: Vec::new(),
        });
        assert_eq!(state.selected_tracks(), vec![1, 2]);
    }

    #[test]
    fn export_choices_map_to_the_encoder() {
        assert_eq!(
            ExportChoice::Flac16.format(),
            OutputFormat::Flac { bit_depth: 16 }
        );
        assert_eq!(
            ExportChoice::Flac24.format(),
            OutputFormat::Flac { bit_depth: 24 }
        );
        assert_eq!(ExportChoice::Opus.format(), OutputFormat::Opus);
    }
}

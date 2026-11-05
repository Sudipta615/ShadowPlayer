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

mod cd;
mod export;
mod update;

use cd::render_cd;
use export::render_export;
use update::render_update;

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

pub(super) fn row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    mut content: impl FnMut(&mut egui::Ui),
) {
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

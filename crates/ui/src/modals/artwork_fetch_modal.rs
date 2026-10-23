//! Modal dialog for bulk fetching missing album cover artwork.

use crate::services::artwork_fetch_service::ArtworkFetchProgress;
use crate::theme::Theme;
use eframe::egui;

#[derive(Default)]
pub struct ArtworkFetchModalState {
    pub is_open: bool,
    pub missing_count: usize,
    pub progress: ArtworkFetchProgress,
    pub finished: bool,
}

impl ArtworkFetchModalState {
    pub fn open(&mut self, missing_count: usize) {
        self.is_open = true;
        self.missing_count = missing_count;
        self.finished = false;
        self.progress = ArtworkFetchProgress::default();
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }
}

pub struct ArtworkFetchActions {
    pub start: bool,
    pub cancel: bool,
}

pub fn show_artwork_fetch_modal(
    ctx: &egui::Context,
    state: &mut ArtworkFetchModalState,
    theme: &Theme,
) -> ArtworkFetchActions {
    let mut actions = ArtworkFetchActions {
        start: false,
        cancel: false,
    };

    if !state.is_open {
        return actions;
    }

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "artwork_fetch_modal",
        "Fetch Missing Cover Artwork",
        560.0,
        theme,
        |ui| {
            ui.add_space(4.0);

            if state.progress.running {
                // ── Progress view ──────────────────────────────────────────
                ui.label(
                    egui::RichText::new("Querying MusicBrainz Cover Art Archive & mirrors…")
                        .color(theme.text_secondary)
                        .size(13.0),
                );

                ui.add_space(12.0);

                let fraction = if state.progress.total > 0 {
                    state.progress.done as f32 / state.progress.total as f32
                } else {
                    0.0
                };

                let bar = egui::ProgressBar::new(fraction)
                    .show_percentage()
                    .animate(true)
                    .fill(theme.accent);
                ui.add(bar);

                ui.add_space(8.0);

                if !state.progress.current_album.is_empty() {
                    ui.label(
                        egui::RichText::new(format!(
                            "Album: {} – {}",
                            state.progress.current_artist, state.progress.current_album
                        ))
                        .color(theme.text_primary)
                        .strong()
                        .size(12.5),
                    );
                }

                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Downloaded: {}", state.progress.found))
                            .color(theme.bit_perfect_green),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new(format!("Not found: {}", state.progress.failed))
                            .color(theme.text_muted),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Processed: {} / {}",
                            state.progress.done, state.progress.total
                        ))
                        .color(theme.text_secondary),
                    );
                });

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    if ui
                        .button(
                            egui::RichText::new("Cancel")
                                .color(theme.destructive)
                                .size(13.0),
                        )
                        .clicked()
                    {
                        actions.cancel = true;
                    }
                });
            } else if state.finished {
                // ── Finished view ──────────────────────────────────────────
                ui.label(
                    egui::RichText::new("Artwork scan completed!")
                        .color(theme.bit_perfect_green)
                        .strong()
                        .size(15.0),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Successfully downloaded and cached {} album covers ({} were not found).",
                        state.progress.found, state.progress.failed
                    ))
                    .color(theme.text_secondary)
                    .size(13.0),
                );

                ui.add_space(16.0);
                if ui
                    .button(egui::RichText::new("Done").color(theme.text_primary))
                    .clicked()
                {
                    state.close();
                }
            } else {
                // ── Initial confirmation view ──────────────────────────────
                ui.label(
                    egui::RichText::new(
                        "Automatically find and download missing album artwork for your library.",
                    )
                    .color(theme.text_secondary)
                    .size(13.0),
                );

                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new(
                        "• Queries the MusicBrainz Cover Art Archive (CAA) and public album art databases.\n\
                         • High-resolution covers are cached in your local artwork directory.\n\
                         • Never overwrites existing cover art files.",
                    )
                    .color(theme.text_muted)
                    .size(12.0),
                );

                ui.add_space(12.0);

                ui.label(
                    egui::RichText::new(format!(
                        "Tracks missing cover art: {}",
                        state.missing_count
                    ))
                    .color(theme.accent_warm)
                    .strong()
                    .size(13.5),
                );

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    let btn = egui::Button::new(
                        egui::RichText::new("Start Downloading Artwork")
                            .color(theme.text_primary)
                            .strong(),
                    )
                    .fill(theme.accent);

                    if ui.add(btn).clicked() {
                        actions.start = true;
                    }

                    if ui
                        .button(egui::RichText::new("Close").color(theme.text_muted))
                        .clicked()
                    {
                        state.close();
                    }
                });
            }
        },
    );

    if closed {
        if state.progress.running {
            actions.cancel = true;
        }
        state.close();
    }

    actions
}

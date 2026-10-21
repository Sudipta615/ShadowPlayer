//! Importing the playlists other players left behind.
//!
//! Everyone arrives with a library built somewhere else. This finds the
//! playlists Rhythmbox, iTunes and Audacious left on the machine and brings
//! them in, matching the files against the library so only the tracks that
//! are actually here come across.

use crate::theme::Theme;
use domain::models::Track;
use eframe::egui::{self, Color32, CornerRadius, Vec2};
use library::FoundPlaylist;

/// What the import dialog did.
#[derive(Debug, Clone, Default)]
pub struct ImportPlaylistsActions {
    /// The playlists the user chose, by index into the list that was shown.
    pub selected: Vec<usize>,
}

/// The import dialog's state.
#[derive(Default)]
pub struct ImportPlaylistsModalState {
    pub is_open: bool,
    /// The playlists that were found.
    pub found: Vec<FoundPlaylist>,
    /// Which of them are ticked.
    pub ticked: Vec<bool>,
    /// How many of a playlist's files are in the library.
    pub matched: Vec<usize>,
}

/// Show the import dialog.
pub fn show_import_playlists_modal(
    ctx: &egui::Context,
    state: &mut ImportPlaylistsModalState,
    _library_tracks: &[Track],
    theme: &Theme,
) -> ImportPlaylistsActions {
    let mut actions = ImportPlaylistsActions::default();
    if !state.is_open {
        return actions;
    }

    let closed = crate::dialog::show_modal_scrollable(
        ctx,
        "import_playlists",
        "Import from other players",
        540.0,
        theme,
        |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, theme.space_md);

            if state.found.is_empty() {
                ui.add_space(theme.space_lg);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("No playlists found")
                            .font(crate::theme::font(13.5))
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.add_space(theme.space_xs);
                    ui.label(
                        egui::RichText::new(
                            "ShadowPlayer looked for the playlists Rhythmbox, iTunes and \
                             Audacious leave behind. There were none on this machine.",
                        )
                        .font(crate::theme::font(11.5))
                        .color(theme.text_muted),
                    );
                });
                ui.add_space(theme.space_lg);
                return;
            }

            ui.label(
                egui::RichText::new(
                    "These playlists were found on this machine. Only the tracks that \
                     are in your library are brought in.",
                )
                .font(crate::theme::font(12.0))
                .color(theme.text_secondary),
            );
            ui.add_space(theme.space_xs);

            for (i, playlist) in state.found.iter().enumerate() {
                let matched = state.matched.get(i).copied().unwrap_or(0);
                let total = playlist.paths.len();
                let card = egui::Frame::NONE
                    .fill(theme.bg_card_hover)
                    .corner_radius(CornerRadius::same(theme.radius_md))
                    .inner_margin(egui::Margin::symmetric(12, 8));
                card.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

                        let mut ticked = state.ticked.get(i).copied().unwrap_or(false);
                        let resp = ui.checkbox(&mut ticked, "");
                        if resp.changed() {
                            if let Some(slot) = state.ticked.get_mut(i) {
                                *slot = ticked;
                            }
                        }

                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(playlist.name.clone())
                                    .font(crate::theme::font(13.0))
                                    .strong()
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {matched} of {total} tracks in your library",
                                    playlist.source
                                ))
                                .font(crate::theme::font(11.0))
                                .color(summary_color(matched, total, theme)),
                            );
                        });
                    });
                });
            }
        },
    );

    if closed {
        state.is_open = false;
    } else {
        // Everything ticked is imported.
        for (i, ticked) in state.ticked.iter().enumerate() {
            if *ticked {
                actions.selected.push(i);
            }
        }
    }
    actions
}

/// How many of a playlist's files are in the library.
///
/// The paths have to be matched against the library's own paths, which are
/// canonicalised, so a file that moved is still found.
pub fn match_count(playlist: &FoundPlaylist, library_tracks: &[Track]) -> usize {
    let known: std::collections::HashSet<&std::path::Path> =
        library_tracks.iter().map(|t| t.path.as_path()).collect();
    playlist
        .paths
        .iter()
        .filter(|p| known.contains(p.as_path()))
        .count()
}

/// The tracks of a playlist that are in the library.
pub fn matched_tracks(playlist: &FoundPlaylist, library_tracks: &[Track]) -> Vec<Track> {
    // The library's own tracks whose file the playlist points at.
    library_tracks
        .iter()
        .filter(|t| playlist.paths.iter().any(|p| p == &t.path))
        .cloned()
        .collect()
}

/// A colour for the summary line.
pub fn summary_color(matched: usize, total: usize, theme: &Theme) -> Color32 {
    if matched == 0 {
        theme.accent_warm
    } else if matched == total {
        theme.text_dim
    } else {
        theme.text_muted
    }
}

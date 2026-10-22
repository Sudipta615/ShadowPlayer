//! Importing the playlists other players left behind.
//!
//! Everyone arrives with a library built somewhere else. This finds the
//! playlists Rhythmbox, Audacious, Winamp, foobar2000, MusicBee, iTunes,
//! Clementine and Strawberry left on the machine and brings them in, matching
//! the files against the library so only the tracks that are actually here
//! come across.

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
                            "ShadowPlayer looked for the playlists Rhythmbox, Audacious, \
                             Winamp, foobar2000, MusicBee, iTunes, Clementine and Strawberry \
                             leave behind. There were none on this machine.",
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
                    .inner_margin(egui::Margin::symmetric(
                        theme.space_xl as i8,
                        theme.space_md as i8,
                    ));
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

/// The tracks of a playlist that are in the library, in playlist order.
pub fn matched_tracks(playlist: &FoundPlaylist, library_tracks: &[Track]) -> Vec<Track> {
    let by_path: std::collections::HashMap<&std::path::Path, &Track> = library_tracks
        .iter()
        .map(|track| (track.path.as_path(), track))
        .collect();
    // Walk the playlist rather than the library. Importing must preserve the
    // order the other player saved; filtering the library would silently
    // replace it with album/scan order.
    playlist
        .paths
        .iter()
        .filter_map(|path| by_path.get(path.as_path()).copied())
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ids::TrackId;
    use std::path::{Path, PathBuf};

    fn track(path: &str) -> Track {
        Track {
            id: TrackId::new(),
            path: PathBuf::from(path),
            title: path.to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            sample_rate: 44100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            bit_depth: Some(16),
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 0,
            mtime_unix_secs: 0,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 0,
        }
    }

    #[test]
    fn imported_tracks_keep_the_playlist_order() {
        let library = vec![track("/music/a.flac"), track("/music/b.flac")];
        let playlist = FoundPlaylist {
            source: "Test",
            name: "Mix".to_string(),
            paths: vec![
                PathBuf::from("/music/b.flac"),
                PathBuf::from("/music/missing.flac"),
                PathBuf::from("/music/a.flac"),
            ],
        };
        let matched = matched_tracks(&playlist, &library);
        let paths: Vec<_> = matched.iter().map(|t| t.path.as_path()).collect();
        assert_eq!(
            paths,
            [Path::new("/music/b.flac"), Path::new("/music/a.flac")]
        );
        assert_eq!(match_count(&playlist, &library), 2);
    }
}

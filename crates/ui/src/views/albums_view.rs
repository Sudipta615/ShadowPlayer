use database::{AlbumNotesRepo, AlbumRepo, FavoritesRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::albums_view::AlbumsView;
use crate::services::{ArtworkService, BiographyService};
use crate::state::{BiographyEditorState, LibraryState};
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_albums_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &mut ArtworkService,
    biography_service: &mut BiographyService,
    biography_editor: &mut BiographyEditorState,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    album_repo: &AlbumRepo,
    album_notes_repo: &AlbumNotesRepo,
    favorites_repo: &FavoritesRepo,
) {
    let biography = library_state
        .selected_album
        .clone()
        .and_then(|(title, artist)| {
            let key = BiographyService::album_key(&artist, &title);
            biography_service.get("album", &key)
        });

    let alb_action = AlbumsView {
        albums: &library_state.albums,
        selected_album: &mut library_state.selected_album,
        album_tracks: &library_state.album_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        album_covers: &library_state.album_cover_tracks,
        artwork: artwork_service,
        biography: biography.as_ref(),
        biography_editor,
        notes: &mut library_state.album_notes,
        notes_repo: album_notes_repo,
        album_analyzer: Some(&library_state.album_analyzer),
        theme,
    }
    .show(ui);

    if let Some(request) = alb_action.biography_request {
        match request {
            crate::components::biography_pane::BiographyPaneRequest::Save { kind, name, text } => {
                match biography_service.save(&kind, &name, &text) {
                    Ok(()) => biography_editor.close(),
                    Err(e) => biography_editor.fail(e),
                }
            }
            crate::components::biography_pane::BiographyPaneRequest::Delete { kind, name } => {
                match biography_service.save(&kind, &name, "") {
                    Ok(()) => biography_editor.close(),
                    Err(e) => biography_editor.fail(e),
                }
            }
        }
    }

    if let Some((title, artist)) = alb_action.play_album {
        let tracks = album_repo
            .get_tracks_by_album(&title, &artist)
            .unwrap_or_default();
        if let Some(first) = tracks.first().cloned() {
            coordinator.play_track(first, Some(tracks));
        }
    }
    if let Some((title, artist)) = alb_action.queue_album {
        let tracks = album_repo
            .get_tracks_by_album(&title, &artist)
            .unwrap_or_default();
        for t in tracks {
            coordinator.add_to_queue(t);
        }
    }
    if let Some(t_id) = alb_action.play_track {
        if let Some(t) = library_state
            .album_tracks
            .iter()
            .find(|x| x.id == t_id)
            .cloned()
        {
            coordinator.play_track(t, Some(library_state.album_tracks.clone()));
        }
    }
    if let Some(fav_id) = alb_action.toggle_favorite {
        library_state.toggle_favorite(favorites_repo, fav_id);
    }
}

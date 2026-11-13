use database::{AlbumRepo, FavoritesRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::albums_panel::AlbumsView;
use crate::services::ArtworkService;
use crate::state::LibraryState;
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_albums_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &mut ArtworkService,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    album_repo: &AlbumRepo,
    favorites_repo: &FavoritesRepo,
) {
    if let Some((ref title, ref artist)) = library_state.selected_album {
        let is_mismatched = library_state.album_tracks.is_empty()
            || library_state
                .album_tracks
                .first()
                .map(|t| (&t.album, &t.artist) != (title, artist))
                .unwrap_or(true);
        if is_mismatched {
            library_state.album_tracks = album_repo
                .get_tracks_by_album(title, artist)
                .unwrap_or_default();
        }
    } else if !library_state.album_tracks.is_empty() {
        library_state.album_tracks.clear();
    }

    let alb_action = AlbumsView {
        albums: &library_state.albums,
        selected_album: &mut library_state.selected_album,
        album_tracks: &library_state.album_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        album_covers: &library_state.album_cover_tracks,
        artwork: artwork_service,
        collection_view: library_state.collection_view,
        album_analyzer: Some(&library_state.album_analyzer),
        theme,
    }
    .show(ui);

    // If an album was just selected during this frame, immediately load its tracks.
    if let Some((ref title, ref artist)) = library_state.selected_album {
        let is_mismatched = library_state.album_tracks.is_empty()
            || library_state
                .album_tracks
                .first()
                .map(|t| (&t.album, &t.artist) != (title, artist))
                .unwrap_or(true);
        if is_mismatched {
            library_state.album_tracks = album_repo
                .get_tracks_by_album(title, artist)
                .unwrap_or_default();
            ui.ctx().request_repaint();
        }
    }

    if let Some(view) = alb_action.view_changed {
        library_state.collection_view = view;
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

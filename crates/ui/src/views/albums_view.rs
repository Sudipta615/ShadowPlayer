use eframe::egui;
use database::{AlbumRepo, FavoritesRepo};
use domain::models::Track;
use playback::PlaybackCoordinator;

use crate::components::albums_view::AlbumsView;
use crate::services::ArtworkService;
use crate::state::LibraryState;
use crate::theme::Theme;

pub fn render_albums_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &ArtworkService,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    album_repo: &AlbumRepo,
    favorites_repo: &FavoritesRepo,
) {
    let alb_action = AlbumsView {
        albums: &library_state.albums,
        selected_album: &mut library_state.selected_album,
        album_tracks: &library_state.album_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        artwork_map: artwork_service.texture_cache(),
        theme,
    }
    .show(ui);

    if let Some((title, artist)) = alb_action.play_album {
        let tracks = album_repo.get_tracks_by_album(&title, &artist).unwrap_or_default();
        if let Some(first) = tracks.first().cloned() {
            coordinator.play_track(first, Some(tracks));
        }
    }
    if let Some((title, artist)) = alb_action.queue_album {
        let tracks = album_repo.get_tracks_by_album(&title, &artist).unwrap_or_default();
        for t in tracks {
            coordinator.add_to_queue(t);
        }
    }
    if let Some(t_id) = alb_action.play_track {
        if let Some(t) = library_state.album_tracks.iter().find(|x| x.id == t_id).cloned() {
            coordinator.play_track(t, Some(library_state.album_tracks.clone()));
        }
    }
    if let Some(fav_id) = alb_action.toggle_favorite {
        library_state.toggle_favorite(favorites_repo, fav_id);
    }
}

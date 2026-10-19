use database::{FavoritesRepo, GenreRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::genres_view::GenresView;
use crate::state::LibraryState;
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_genres_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    genre_repo: &GenreRepo,
    favorites_repo: &FavoritesRepo,
) {
    let gnr_action = GenresView {
        genres: &library_state.genres,
        selected_genre: &mut library_state.selected_genre,
        genre_tracks: &library_state.genre_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        theme,
    }
    .show(ui);

    if let Some(genre) = gnr_action.play_genre {
        let tracks = genre_repo.get_tracks_by_genre(&genre).unwrap_or_default();
        if let Some(first) = tracks.first().cloned() {
            coordinator.play_track(first, Some(tracks));
        }
    }
    if let Some(genre) = gnr_action.queue_genre {
        let tracks = genre_repo.get_tracks_by_genre(&genre).unwrap_or_default();
        for t in tracks {
            coordinator.add_to_queue(t);
        }
    }
    if let Some(t_id) = gnr_action.play_track {
        if let Some(t) = library_state
            .genre_tracks
            .iter()
            .find(|x| x.id == t_id)
            .cloned()
        {
            coordinator.play_track(t, Some(library_state.genre_tracks.clone()));
        }
    }
    if let Some(fav_id) = gnr_action.toggle_favorite {
        library_state.toggle_favorite(favorites_repo, fav_id);
    }
}

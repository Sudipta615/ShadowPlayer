use database::{FavoritesRepo, GenreRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::genres_panel::GenresView;
use crate::services::ArtworkService;
use crate::state::LibraryState;
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_genres_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &mut ArtworkService,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    genre_repo: &GenreRepo,
    favorites_repo: &FavoritesRepo,
) {
    if let Some(ref genre) = library_state.selected_genre {
        let is_mismatched = library_state.genre_tracks.is_empty()
            || library_state
                .genre_tracks
                .first()
                .map(|t| t.genre.as_deref() != Some(genre.as_str()))
                .unwrap_or(true);
        if is_mismatched {
            library_state.genre_tracks = genre_repo.get_tracks_by_genre(genre).unwrap_or_default();
        }
    } else if !library_state.genre_tracks.is_empty() {
        library_state.genre_tracks.clear();
    }

    let gnr_action = GenresView {
        genres: &library_state.genres,
        selected_genre: &mut library_state.selected_genre,
        genre_tracks: &library_state.genre_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        artwork: artwork_service,
        collection_view: library_state.collection_view,
        theme,
    }
    .show(ui);

    if let Some(view) = gnr_action.view_changed {
        library_state.collection_view = view;
    }

    // If a genre was just selected during this frame, immediately load its tracks.
    if let Some(ref genre) = library_state.selected_genre {
        let is_mismatched = library_state.genre_tracks.is_empty()
            || library_state
                .genre_tracks
                .first()
                .map(|t| t.genre.as_deref() != Some(genre.as_str()))
                .unwrap_or(true);
        if is_mismatched {
            library_state.genre_tracks = genre_repo.get_tracks_by_genre(genre).unwrap_or_default();
            ui.ctx().request_repaint();
        }
    }

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

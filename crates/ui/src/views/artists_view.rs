use database::{ArtistRepo, FavoritesRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::artists_panel::ArtistsView;
use crate::services::ArtworkService;
use crate::state::LibraryState;
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_artists_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &mut ArtworkService,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    artist_repo: &ArtistRepo,
    favorites_repo: &FavoritesRepo,
) {
    if let Some(ref artist) = library_state.selected_artist {
        let is_mismatched = library_state.artist_tracks.is_empty()
            || library_state
                .artist_tracks
                .first()
                .map(|t| &t.artist != artist)
                .unwrap_or(true);
        if is_mismatched {
            library_state.artist_tracks =
                artist_repo.get_tracks_by_artist(artist).unwrap_or_default();
        }
    } else if !library_state.artist_tracks.is_empty() {
        library_state.artist_tracks.clear();
    }

    let art_action = ArtistsView {
        artists: &library_state.artists,
        selected_artist: &mut library_state.selected_artist,
        artist_tracks: &library_state.artist_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        artist_covers: &library_state.artist_cover_tracks,
        artwork: artwork_service,
        collection_view: library_state.collection_view,
        theme,
    }
    .show(ui);

    // If an artist was just selected during this frame, immediately load its tracks.
    if let Some(ref artist) = library_state.selected_artist {
        let is_mismatched = library_state.artist_tracks.is_empty()
            || library_state
                .artist_tracks
                .first()
                .map(|t| &t.artist != artist)
                .unwrap_or(true);
        if is_mismatched {
            library_state.artist_tracks =
                artist_repo.get_tracks_by_artist(artist).unwrap_or_default();
            ui.ctx().request_repaint();
        }
    }

    if let Some(view) = art_action.view_changed {
        library_state.collection_view = view;
    }

    if let Some(artist) = art_action.play_artist {
        let tracks = artist_repo
            .get_tracks_by_artist(&artist)
            .unwrap_or_default();
        if let Some(first) = tracks.first().cloned() {
            coordinator.play_track(first, Some(tracks));
        }
    }
    if let Some(artist) = art_action.queue_artist {
        let tracks = artist_repo
            .get_tracks_by_artist(&artist)
            .unwrap_or_default();
        for t in tracks {
            coordinator.add_to_queue(t);
        }
    }
    if let Some(t_id) = art_action.play_track {
        if let Some(t) = library_state
            .artist_tracks
            .iter()
            .find(|x| x.id == t_id)
            .cloned()
        {
            coordinator.play_track(t, Some(library_state.artist_tracks.clone()));
        }
    }
    if let Some(fav_id) = art_action.toggle_favorite {
        library_state.toggle_favorite(favorites_repo, fav_id);
    }
}

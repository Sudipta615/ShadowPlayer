use database::{ArtistRepo, FavoritesRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::artists_view::ArtistsView;
use crate::state::LibraryState;
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_artists_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    artist_repo: &ArtistRepo,
    favorites_repo: &FavoritesRepo,
) {
    let art_action = ArtistsView {
        artists: &library_state.artists,
        selected_artist: &mut library_state.selected_artist,
        artist_tracks: &library_state.artist_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        theme,
    }
    .show(ui);

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

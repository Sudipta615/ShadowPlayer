use database::{ArtistRepo, FavoritesRepo};
use domain::models::Track;
use eframe::egui;
use playback::PlaybackCoordinator;

use crate::components::artists_panel::ArtistsView;
use crate::services::{ArtworkService, BiographyService};
use crate::state::{BiographyEditorState, LibraryState};
use crate::theme::Theme;

#[allow(clippy::too_many_arguments)]
pub fn render_artists_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    artwork_service: &mut ArtworkService,
    biography_service: &mut BiographyService,
    biography_editor: &mut BiographyEditorState,
    current_track: Option<&Track>,
    is_playing: bool,
    theme: &Theme,
    artist_repo: &ArtistRepo,
    favorites_repo: &FavoritesRepo,
) {
    let biography = library_state
        .selected_artist
        .clone()
        .and_then(|name| biography_service.get("artist", &name));

    let art_action = ArtistsView {
        artists: &library_state.artists,
        selected_artist: &mut library_state.selected_artist,
        artist_tracks: &library_state.artist_tracks,
        active_track_id: current_track.map(|t| t.id),
        is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        artist_covers: &library_state.artist_cover_tracks,
        artwork: artwork_service,
        biography: biography.as_ref(),
        biography_editor,
        theme,
    }
    .show(ui);

    if let Some(request) = art_action.biography_request {
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

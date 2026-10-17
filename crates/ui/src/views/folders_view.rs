use std::path::PathBuf;

use eframe::egui;
use database::{FavoritesRepo, ScanRootsRepo, TrackRepo};
use domain::models::Track;
use playback::PlaybackCoordinator;

use crate::components::folders_view::FoldersView;
use crate::services::ScannerService;
use crate::state::LibraryState;
use crate::theme::Theme;

pub fn render_folders_view(
    ui: &mut egui::Ui,
    library_state: &mut LibraryState,
    scanner_service: &mut ScannerService,
    coordinator: &mut PlaybackCoordinator,
    current_track: Option<&Track>,
    theme: &Theme,
    scan_roots_repo: &ScanRootsRepo,
    track_repo: &TrackRepo,
    favorites_repo: &FavoritesRepo,
    on_start_scan: impl FnOnce(PathBuf),
) {
    let fld_action = FoldersView {
        scan_roots: &library_state.scan_roots,
        selected_folder: &mut library_state.selected_folder,
        folder_tracks: &library_state.folder_tracks,
        new_folder_input: &mut library_state.new_folder_input,
        is_scanning: scanner_service.is_scanning(),
        scan_progress_text: scanner_service.progress_text(),
        active_track_id: current_track.map(|t| t.id),
        favorite_track_ids: &library_state.favorite_ids,
        theme,
    }
    .show(ui);

    if let Some(folder) = fld_action.add_folder {
        on_start_scan(folder);
    }
    if let Some(id) = fld_action.remove_folder {
        let _ = scan_roots_repo.remove(&id);
        library_state.scan_roots = scan_roots_repo.list().unwrap_or_default();
    }
    if let Some(folder) = fld_action.rescan_folder {
        scanner_service.start_scan(folder);
    }
    if let Some(folder) = fld_action.play_folder {
        let tracks = track_repo.get_tracks_by_folder(&folder).unwrap_or_default();
        if let Some(first) = tracks.first().cloned() {
            coordinator.play_track(first, Some(tracks));
        }
    }
    if let Some(folder) = fld_action.queue_folder {
        let tracks = track_repo.get_tracks_by_folder(&folder).unwrap_or_default();
        for t in tracks {
            coordinator.add_to_queue(t);
        }
    }
    if let Some(t_id) = fld_action.play_track {
        if let Some(t) = library_state.folder_tracks.iter().find(|x| x.id == t_id).cloned() {
            coordinator.play_track(t, Some(library_state.folder_tracks.clone()));
        }
    }
    if let Some(fav_id) = fld_action.toggle_favorite {
        library_state.toggle_favorite(favorites_repo, fav_id);
    }
}

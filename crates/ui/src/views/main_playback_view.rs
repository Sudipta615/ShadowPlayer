use audio_session::AudioSession;
use database::{FavoritesRepo, PlaylistRepo, TrackRepo};
use domain::ids::TrackId;
use domain::models::{RepeatModeChoice, Track};
use eframe::egui::{self, CornerRadius, Stroke, Vec2};
use playback::PlaybackCoordinator;

use crate::components::now_playing::{NowPlayingCard, SleepTimerRequest};
use crate::components::sidebar::NavItem;
use crate::components::songs_table::{SongsTable, SongsTableAction};
use crate::services::ArtworkService;
use crate::state::{AppState, LibraryState, SleepTimer, TelemetryState};
use crate::theme::Theme;

#[derive(Default, Debug, Clone)]
pub struct MainViewAction {
    pub switch_nav: Option<NavItem>,
    pub create_playlist: bool,
    pub open_eq_panel: bool,
    /// Some table action changed the visible dataset (sort/favorite/playlist);
    /// rebuild the displayed list for the current tab.
    pub needs_reload: bool,
    /// A track was removed from the library; a full reload is required.
    pub removed_track: bool,
    /// The user asked to edit the rules of a smart playlist.
    pub edit_smart_playlist: Option<domain::ids::PlaylistId>,
    /// The user asked to re-evaluate the smart playlists.
    pub refresh_smart_playlist: bool,
    /// The user asked to organize the current selection.
    pub organize_tracks: Option<Vec<Track>>,
    /// The user asked to auto-tag track(s).
    pub autotag_tracks: Option<Vec<Track>>,
    /// The user asked to fetch missing artwork for track(s).
    pub fetch_artwork_tracks: Option<Vec<Track>>,
    /// Start a local radio station seeded from this track.
    pub start_radio: Option<TrackId>,
    /// Rebuild the current radio station with a new shuffle.
    pub refresh_radio: bool,
    /// A grouped-search chip asked to run a narrower query.
    pub search_query: Option<String>,
    /// The loudness inspector was requested.
    pub open_loudness: bool,
    /// Table preferences changed and should be written to the settings store.
    pub settings_dirty: bool,
}

/// Render standard playback view: Now Playing Card + Foobar2000 Tab Strip + Songs Table
#[allow(clippy::too_many_arguments)]
pub fn render_main_playback_view(
    ui: &mut egui::Ui,
    app_state: &mut AppState,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    session: &AudioSession,
    artwork_service: &mut ArtworkService,
    telemetry: &TelemetryState,
    waveform: Option<&[f32]>,
    current_track: Option<&Track>,
    theme: &Theme,
    track_repo: &TrackRepo,
    playlist_repo: &PlaylistRepo,
    favorites_repo: &FavoritesRepo,
    is_eq_open: bool,
) -> MainViewAction {
    let mut view_action = MainViewAction::default();
    let title = current_track
        .map(|t| t.title.as_str())
        .unwrap_or("No Track Selected");
    let artist = current_track
        .map(|t| t.artist.as_str())
        .unwrap_or("ShadowPlayer");
    let album = current_track.map(|t| t.album.as_str()).unwrap_or("Ready");
    if let Some(t) = current_track {
        artwork_service.ensure_loaded(ui.ctx(), t);
    }
    let art_tex = current_track
        .and_then(|t| artwork_service.get_texture(&t.id))
        .cloned();

    let format_str = current_track.map(|t| {
        let sr_khz = t.sample_rate as f32 / 1000.0;
        if t.is_lossless {
            format!("{} {:.1}kHz", t.format, sr_khz)
        } else if let Some(br) = t.bitrate_kbps {
            format!("{} {}kbps", t.format, br)
        } else {
            t.format.clone()
        }
    });

    let is_cur_fav = current_track
        .map(|t| library_state.favorite_ids.contains(&t.id))
        .unwrap_or(false);

    let sleep_label = app_state.sleep_timer.as_ref().map(|t| t.label());

    let np_events = NowPlayingCard {
        title,
        artist,
        album,
        is_playing: telemetry.is_playing,
        is_favorite: is_cur_fav,
        rating: current_track.map(|t| t.rating).unwrap_or(0),
        is_shuffle: coordinator.is_shuffle(),
        is_repeat: coordinator.repeat_mode() != RepeatModeChoice::Off,
        position_secs: telemetry.position_secs,
        duration_secs: telemetry.duration_secs,
        spectrum_db: &telemetry.spectrum.spectrum_db,
        waveform,
        artwork: art_tex.as_ref(),
        format_str: format_str.as_deref(),
        bit_perfect: telemetry.bit_perfect,
        is_eq_open,
        sleep_timer_label: sleep_label.as_deref(),
        theme,
    }
    .show(ui);

    // Handle Now Playing card interactions
    if np_events.toggle_play {
        coordinator.toggle_play_pause();
    }
    if np_events.next_track {
        coordinator.next();
    }
    if np_events.prev_track {
        coordinator.previous();
    }
    if np_events.toggle_shuffle {
        let new_shuffle = !coordinator.is_shuffle();
        coordinator.set_shuffle(new_shuffle);
    }
    if np_events.toggle_repeat {
        let next_mode = match coordinator.repeat_mode() {
            RepeatModeChoice::Off => RepeatModeChoice::All,
            RepeatModeChoice::All => RepeatModeChoice::One,
            RepeatModeChoice::One => RepeatModeChoice::Off,
        };
        coordinator.set_repeat_mode(next_mode);
    }
    if let Some(seek) = np_events.seek_secs {
        session.seek(seek as f32);
    }
    if np_events.toggle_favorite {
        if let Some(t) = current_track {
            library_state.toggle_favorite(favorites_repo, t.id);
            view_action.needs_reload = true;
        }
    }
    if let Some(req) = np_events.sleep_timer_request {
        // Turning a timer off while it is already fading must put the user's
        // volume back, not leave the output stuck at the faded level.
        if matches!(req, SleepTimerRequest::Off) {
            session.set_volume(if app_state.is_muted {
                0.0
            } else {
                app_state.volume
            });
        }
        app_state.sleep_timer = match req {
            SleepTimerRequest::Off => None,
            SleepTimerRequest::AfterSeconds(secs) => Some(SleepTimer::after_seconds(secs)),
            SleepTimerRequest::AfterSecondsFade(secs) => {
                Some(SleepTimer::after_seconds_fade(secs, 10.0))
            }
            SleepTimerRequest::EndOfTrack => Some(SleepTimer::end_of_track()),
        };
    }
    if np_events.open_properties {
        app_state.track_properties_track = current_track.cloned();
        app_state.track_properties_open = true;
    }
    if np_events.open_bit_perfect {
        app_state.bit_perfect_modal_open = true;
    }
    if np_events.open_loudness {
        view_action.open_loudness = true;
    }
    if np_events.open_eq_panel {
        view_action.open_eq_panel = true;
    }

    ui.add_space(8.0);

    // Grouped search: artists, albums and playlists are first-class results
    // rather than silently discarded while only tracks reach the table.
    if !app_state.search_query.trim().is_empty() {
        let result = &library_state.search_result;
        if !result.artists.is_empty() || !result.albums.is_empty() || !result.playlists.is_empty() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                ui.label(
                    egui::RichText::new(tr!("Matches"))
                        .font(crate::theme::font(11.0))
                        .color(theme.text_muted),
                );
                egui::ScrollArea::horizontal()
                    .id_salt("grouped_search_chips")
                    .max_height(30.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(theme.space_sm, 0.0);
                            let chip = |ui: &mut egui::Ui, label: String| -> bool {
                                ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .font(crate::theme::font(11.0))
                                            .color(theme.text_secondary),
                                    )
                                    .fill(theme.bg_pill)
                                    .stroke(Stroke::new(1.0, theme.border_subtle))
                                    .corner_radius(CornerRadius::same(theme.radius_sm)),
                                )
                                .clicked()
                            };
                            for artist in result.artists.iter().take(4) {
                                if chip(ui, format!("Artist: {artist}")) {
                                    view_action.search_query = Some(artist.clone());
                                }
                            }
                            for album in result.albums.iter().take(4) {
                                if chip(ui, format!("Album: {album}")) {
                                    view_action.search_query = Some(album.clone());
                                }
                            }
                            for playlist in result.playlists.iter().take(4) {
                                if chip(ui, format!("Playlist: {}", playlist.name)) {
                                    view_action.switch_nav = Some(NavItem::Playlist(playlist.id));
                                    view_action.search_query = Some(String::new());
                                }
                            }
                        });
                    });
            });
            ui.add_space(2.0);
        }
    }

    // Dynamic View Header
    match app_state.active_nav {
        NavItem::Favorites => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Favorites"))
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.accent_warm),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "({} tracks)",
                        library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::RecentlyAdded => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Recently Added"))
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.info),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "({} tracks, newest first)",
                        library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::RecentlyPlayed => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Recently Played History"))
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.info),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "({} tracks)",
                        library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::MostPlayed => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tr!("Most Played Tracks"))
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.accent_warm),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "({} tracks)",
                        library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::Radio => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if library_state.radio_label.is_empty() {
                        tr!("Radio")
                    } else {
                        library_state.radio_label.clone()
                    })
                    .font(crate::theme::font(16.0))
                    .strong()
                    .color(theme.accent_warm),
                );
                ui.label(
                    egui::RichText::new(tr_args!(
                        "({count} tracks · local smart shuffle)",
                        "count" => library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr!("New Mix")).clicked() {
                        view_action.refresh_radio = true;
                    }
                });
            });
            ui.add_space(4.0);
        }
        NavItem::SmartPlaylist(sp_id) => {
            let sp = library_state
                .smart_playlists
                .iter()
                .find(|s| s.id == sp_id)
                .cloned();
            let name = sp
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "Smart Playlist".into());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(name)
                        .font(crate::theme::font(16.0))
                        .strong()
                        .color(theme.info),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "({} tracks)",
                        library_state.displayed_tracks.len()
                    ))
                    .font(crate::theme::font(12.0))
                    .color(theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr!(Msg::SFA0039F354707FEA)).clicked() {
                        view_action.edit_smart_playlist = Some(sp_id);
                    }
                    if ui.button(tr!("Refresh")).clicked() {
                        view_action.refresh_smart_playlist = true;
                    }
                });
            });
            ui.add_space(4.0);
        }
        _ => {}
    }

    // Songs Table
    let cur_id = current_track.map(|t| t.id);
    let active_preset = app_state
        .active_column_preset
        .min(app_state.column_presets.len().saturating_sub(1));
    let preset_names: Vec<String> = app_state
        .column_presets
        .iter()
        .map(|preset| preset.name.clone())
        .collect();
    let preset = app_state
        .column_presets
        .get(active_preset)
        .cloned()
        .unwrap_or_default();
    let table_action = SongsTable {
        tracks: &library_state.displayed_tracks,
        active_track_id: cur_id,
        is_playing: telemetry.is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        playlists: &library_state.playlists,
        current_sort: app_state.current_sort,
        sort_order: app_state.current_sort_order,
        is_grid_view: app_state.is_grid_view,
        quick_filter: library_state.quick_filter,
        column_preset: &preset,
        column_preset_names: &preset_names,
        active_preset,
        preset_name_draft: &mut app_state.column_preset_name,
        rating_width: app_state.rating_width,
        density: app_state.row_density,
        selected_track_ids: &mut library_state.selected_track_ids,
        last_clicked_index: &mut library_state.last_clicked_index,
        artwork: artwork_service,
        theme,
        show_header: ui.available_height() > 210.0,
        search_query: &app_state.search_query,
        library_track_count: library_state.all_tracks.len(),
    }
    .show(ui);

    // The empty state's call to action.
    if table_action.open_folders {
        view_action.switch_nav = Some(NavItem::Folders);
    }
    if table_action.clear_search {
        view_action.search_query = Some(String::new());
    }

    if let Some(f) = table_action.quick_filter_changed {
        library_state.quick_filter = f;
        // The displayed list is keyed on the filter, so a refresh rebuilds it.
        view_action.needs_reload = true;
    }

    // Column dragging updates the live layout immediately, but the settings
    // store is only written when the drag ends: a database write per frame
    // would be wasteful and would make the drag stutter.
    if let Some(idx) = table_action.select_column_preset {
        if idx < app_state.column_presets.len() {
            app_state.active_column_preset = idx;
            view_action.settings_dirty = true;
        }
    }
    if let Some(next) = table_action.column_preset_changed.clone() {
        if app_state.active_column_preset < app_state.column_presets.len() {
            app_state.column_presets[app_state.active_column_preset] = next.normalized();
        }
    }
    if let Some(mut saved) = table_action.save_column_preset.clone() {
        let name = app_state.column_preset_name.trim();
        saved.name = if name.is_empty() {
            "Custom".to_string()
        } else {
            name.to_string()
        };
        saved = saved.normalized();
        let existing = app_state
            .column_presets
            .iter()
            .position(|preset| preset.name.eq_ignore_ascii_case(&saved.name));
        match existing {
            Some(idx) => {
                app_state.column_presets[idx] = saved;
                app_state.active_column_preset = idx;
            }
            None => {
                app_state.column_presets.push(saved);
                app_state.active_column_preset = app_state.column_presets.len() - 1;
            }
        }
        app_state.column_preset_name.clear();
        view_action.settings_dirty = true;
    }
    if table_action.delete_column_preset && app_state.column_presets.len() > 1 {
        let idx = app_state
            .active_column_preset
            .min(app_state.column_presets.len() - 1);
        app_state.column_presets.remove(idx);
        app_state.active_column_preset = idx.min(app_state.column_presets.len() - 1);
        view_action.settings_dirty = true;
    }
    if let Some(width) = table_action.rating_width_changed {
        app_state.rating_width = width;
    }
    if table_action.column_drag_finished || table_action.column_preset_edited {
        view_action.settings_dirty = true;
    }
    if table_action.toggle_density {
        app_state.row_density = app_state.row_density.toggled();
        view_action.settings_dirty = true;
    }
    if table_action.reset_columns {
        let idx = app_state
            .active_column_preset
            .min(app_state.column_presets.len().saturating_sub(1));
        if let Some(current) = app_state.column_presets.get(idx) {
            let reset = crate::components::songs_table::ColumnPreset::factory(&current.name)
                .unwrap_or_else(|| {
                    let mut fallback = crate::components::songs_table::ColumnPreset::default();
                    fallback.name = current.name.clone();
                    fallback
                });
            app_state.column_presets[idx] = reset.normalized();
        }
        app_state.rating_width =
            crate::components::songs_table::SongTablePrefs::default().rating_width;
        view_action.settings_dirty = true;
    }

    if let Some(order) = table_action.sort_order_changed {
        app_state.current_sort_order = order;
        view_action.needs_reload = true;
    }
    if table_action.sort_field_changed.is_some()
        || table_action.remove_track.is_some()
        || table_action.toggle_favorite.is_some()
        || table_action.set_rating.is_some()
        || table_action.rate_selected.is_some()
        || table_action.add_to_playlist.is_some()
        || table_action.add_selected_to_playlist.is_some()
        || table_action.favorite_selected
        || table_action.remove_selected
    {
        view_action.needs_reload = true;
    }
    if table_action.remove_track.is_some() || table_action.remove_selected {
        view_action.removed_track = true;
    }
    if table_action.organize_selected {
        // Hand the current selection to the app-level organizer; the modal and
        // the disk work both live there.
        view_action.organize_tracks = Some(library_state.selected_tracks_in_order());
    }
    if let Some(track_id) = table_action.autotag_track {
        if let Some(track) = library_state
            .all_tracks
            .iter()
            .find(|t| t.id == track_id)
            .cloned()
        {
            view_action.autotag_tracks = Some(vec![track]);
        }
    }
    if table_action.autotag_selected {
        view_action.autotag_tracks = Some(library_state.selected_tracks_in_order());
    }
    if table_action.fetch_artwork_selected {
        view_action.fetch_artwork_tracks = Some(library_state.selected_tracks_in_order());
    }
    if let Some(seed) = table_action.start_radio {
        view_action.start_radio = Some(seed);
    }

    handle_songs_table_action(
        table_action,
        app_state,
        library_state,
        coordinator,
        track_repo,
        playlist_repo,
        favorites_repo,
    );

    view_action
}

fn handle_songs_table_action(
    action: SongsTableAction,
    app_state: &mut AppState,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    track_repo: &TrackRepo,
    playlist_repo: &PlaylistRepo,
    favorites_repo: &FavoritesRepo,
) {
    if let Some(play_id) = action.play_track {
        if let Some(track) = library_state
            .displayed_tracks
            .iter()
            .find(|t| t.id == play_id)
            .cloned()
        {
            coordinator.play_track(track, Some(library_state.displayed_tracks.clone()));
        }
    }
    if let Some(next_id) = action.queue_next_track {
        if let Some(track) = library_state
            .displayed_tracks
            .iter()
            .find(|t| t.id == next_id)
            .cloned()
        {
            coordinator.add_to_queue_next(track);
        }
    }
    if let Some(q_id) = action.add_to_queue {
        if let Some(track) = library_state
            .displayed_tracks
            .iter()
            .find(|t| t.id == q_id)
            .cloned()
        {
            coordinator.add_to_queue(track);
        }
    }
    if let Some((track_id, playlist_id)) = action.add_to_playlist {
        let _ = playlist_repo.add_track(&playlist_id, &track_id);
    }
    if let Some(sort) = action.sort_field_changed {
        app_state.current_sort = sort;
    }
    if action.toggle_view_mode {
        app_state.is_grid_view = !app_state.is_grid_view;
    }
    if let Some(folder) = action.open_folder {
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&folder).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&folder).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(&folder).spawn();
    }
    if let Some(prop_id) = action.show_properties {
        if let Some(track) = library_state
            .displayed_tracks
            .iter()
            .find(|t| t.id == prop_id)
            .cloned()
        {
            app_state.track_properties_track = Some(track);
            app_state.track_properties_open = true;
        }
    }
    if let Some(rem_id) = action.remove_track {
        let _ = track_repo.delete(&rem_id);
    }
    if let Some(dsp_id) = action.open_dsp {
        // The per-track DSP editor works on any track, not just the one that
        // is playing, so the track is opened by id.
        app_state.track_dsp_modal.track_id = Some(dsp_id);
        app_state.track_dsp_modal.is_open = true;
    }
    if let Some(edit_id) = action.edit_track {
        let track = library_state
            .displayed_tracks
            .iter()
            .find(|t| t.id == edit_id)
            .cloned()
            .or_else(|| track_repo.get_by_id(&edit_id).ok().flatten());
        if let Some(t) = track {
            app_state.edit_track.open_with(&t);
        }
    }

    // ── Bulk (multi-select) actions ────────────────────────────────────────
    if action.clear_selection {
        library_state.clear_selection();
    }
    if action.play_selected {
        let selected = library_state.selected_tracks_in_order();
        if let Some(first) = selected.first().cloned() {
            coordinator.play_track(first, Some(selected));
        }
    }
    if action.queue_next_selected {
        // Insert in reverse so the original order is preserved in the queue.
        let selected = library_state.selected_tracks_in_order();
        for track in selected.into_iter().rev() {
            coordinator.add_to_queue_next(track);
        }
    }
    if action.add_selected_to_queue {
        let selected = library_state.selected_tracks_in_order();
        for track in selected {
            coordinator.add_to_queue(track);
        }
    }
    if let Some(pl_id) = action.add_selected_to_playlist {
        let selected = library_state.selected_tracks_in_order();
        for track in selected {
            let _ = playlist_repo.add_track(&pl_id, &track.id);
        }
    }
    if action.favorite_selected {
        let selected = library_state.selected_tracks_in_order();
        for track in selected {
            library_state.add_favorite(favorites_repo, track.id);
        }
    }
    if let Some((track_id, rating)) = action.set_rating {
        match track_repo.set_rating(&track_id, rating) {
            Ok(()) => coordinator.update_track_rating(track_id, rating.min(5)),
            Err(e) => log::warn!("Could not save the rating for {track_id}: {e}"),
        }
    }
    if let Some(rating) = action.rate_selected {
        let ids: Vec<TrackId> = library_state
            .selected_tracks_in_order()
            .into_iter()
            .map(|t| t.id)
            .collect();
        match track_repo.rate_many(&ids, rating) {
            Ok(()) => {
                for id in ids {
                    coordinator.update_track_rating(id, rating.min(5));
                }
            }
            Err(e) => log::warn!("Could not save the selected ratings: {e}"),
        }
    }
    if action.remove_selected {
        let ids: Vec<TrackId> = library_state.selected_track_ids.iter().copied().collect();
        for id in ids {
            let _ = track_repo.delete(&id);
        }
        library_state.clear_selection();
    }
}

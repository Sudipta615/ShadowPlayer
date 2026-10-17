use eframe::egui::{self, Color32, FontId, Stroke, Vec2};
use audio_session::AudioSession;
use database::{PlaylistRepo, TrackRepo};
use domain::models::{RepeatModeChoice, Track};
use playback::PlaybackCoordinator;

use crate::components::now_playing::NowPlayingCard;
use crate::components::sidebar::NavItem;
use crate::components::songs_table::{SongsTable, SongsTableAction};
use crate::services::ArtworkService;
use crate::state::{AppState, LibraryState, TelemetryState, VisualizerMode};
use crate::theme::Theme;

#[derive(Default, Debug, Clone)]
pub struct MainViewAction {
    pub switch_nav: Option<NavItem>,
    pub create_playlist: bool,
    pub open_eq_panel: bool,
}

/// Render standard playback view: Now Playing Card + Foobar2000 Tab Strip + Songs Table
pub fn render_main_playback_view(
    ui: &mut egui::Ui,
    app_state: &mut AppState,
    library_state: &mut LibraryState,
    coordinator: &mut PlaybackCoordinator,
    session: &AudioSession,
    artwork_service: &ArtworkService,
    telemetry: &TelemetryState,
    current_track: Option<&Track>,
    theme: &Theme,
    track_repo: &TrackRepo,
    playlist_repo: &PlaylistRepo,
    is_eq_open: bool,
) -> MainViewAction {
    let mut view_action = MainViewAction::default();
    let title = current_track.map(|t| t.title.as_str()).unwrap_or("No Track Selected");
    let artist = current_track.map(|t| t.artist.as_str()).unwrap_or("ShadowPlayer");
    let album = current_track.map(|t| t.album.as_str()).unwrap_or("Ready");
    let art_tex = current_track.and_then(|t| artwork_service.get_texture(&t.id));

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

    let np_events = NowPlayingCard {
        title,
        artist,
        album,
        is_playing: telemetry.is_playing,
        is_favorite: is_cur_fav,
        is_shuffle: coordinator.is_shuffle(),
        is_repeat: coordinator.repeat_mode() != RepeatModeChoice::Off,
        position_secs: telemetry.position_secs,
        duration_secs: telemetry.duration_secs,
        spectrum_db: &telemetry.spectrum.spectrum_db,
        artwork: art_tex,
        format_str: format_str.as_deref(),
        bit_perfect: telemetry.bit_perfect,
        is_eq_open,
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
            app_state.track_properties_track = Some(t.clone());
        }
    }
    if np_events.open_advanced_audio {
        app_state.help_modal_open = false;
    }
    if np_events.open_properties {
        app_state.track_properties_track = current_track.cloned();
        app_state.track_properties_open = true;
    }
    if np_events.open_bit_perfect {
        app_state.bit_perfect_modal_open = true;
    }
    if np_events.open_eq_panel {
        view_action.open_eq_panel = true;
    }

    // Telemetry & Visualizer Studio Dock & Tabs (only shown for non-Home views or when explicitly toggled)
    if app_state.active_nav != NavItem::Home && app_state.visualizer_mode != VisualizerMode::Off {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("AUDIO VISUALIZER")
                    .font(FontId::proportional(11.0))
                    .strong()
                    .color(theme.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                for (mode, lbl) in [
                    (VisualizerMode::Off, "Off"),
                    (VisualizerMode::LufsRadar, "⚡ LUFS"),
                    (VisualizerMode::VuMeters, "📻 Analog VU"),
                    (VisualizerMode::Spectrum, "📊 FFT Spectrum"),
                ] {
                    let is_sel = app_state.visualizer_mode == mode;
                    let bg = if is_sel { theme.accent_violet } else { theme.bg_pill };
                    let fg = if is_sel { Color32::WHITE } else { theme.text_secondary };
                    let btn = egui::Button::new(
                        egui::RichText::new(lbl).font(FontId::proportional(11.0)).color(fg),
                    )
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(6));
                    if ui.add(btn).clicked() {
                        app_state.visualizer_mode = mode;
                    }
                }
            });
        });

        match app_state.visualizer_mode {
            VisualizerMode::Spectrum => {
                crate::widgets::SpectrumVisualizerWidget::new(&telemetry.spectrum.spectrum_db, theme)
                    .height(72.0)
                    .show(ui);
                ui.add_space(4.0);
            }
            VisualizerMode::VuMeters => {
                let tp = telemetry.meters.as_ref().map(|m| m.max_true_peak_dbtp).unwrap_or(0.0);
                crate::widgets::VuMeterWidget::new(
                    telemetry.spectrum.peak_db_l,
                    telemetry.spectrum.peak_db_r,
                    tp,
                    theme,
                )
                .height(78.0)
                .show(ui);
                ui.add_space(4.0);
            }
            VisualizerMode::LufsRadar => {
                crate::widgets::LufsMeterWidget::new(telemetry.meters.as_ref(), theme)
                    .height(72.0)
                    .show(ui);
                ui.add_space(4.0);
            }
            VisualizerMode::Off => {}
        }
    }

    if app_state.active_nav != NavItem::Home {
        // Multi-Tab Playlist Bar for non-Home views
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

            let standard_tabs = [
                (NavItem::Home, "⌂ Library"),
                (NavItem::Favorites, "♥ Favorites"),
                (NavItem::RecentlyPlayed, "🕒 Recent"),
                (NavItem::MostPlayed, "📊 Most Played"),
            ];

            egui::ScrollArea::horizontal()
                .id_salt("playlist_tabs_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                        for (item_nav, label) in standard_tabs {
                            let is_active = app_state.active_nav == item_nav;
                            let text_color = if is_active { theme.accent_magenta } else { theme.text_secondary };
                            let bg = if is_active { theme.bg_card_active } else { theme.bg_pill };
                            let stroke = if is_active {
                                Stroke::new(1.0_f32, theme.accent_magenta)
                            } else {
                                Stroke::new(1.0_f32, theme.border_subtle)
                            };

                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .font(FontId::proportional(12.0))
                                    .color(text_color),
                            )
                            .fill(bg)
                            .stroke(stroke)
                            .corner_radius(egui::CornerRadius::same(6));

                            if ui.add(btn).clicked() && !is_active {
                                view_action.switch_nav = Some(item_nav);
                            }
                        }

                        // User custom playlist tabs
                        for pl in &library_state.playlists {
                            let item_nav = NavItem::Playlist(pl.id);
                            let is_active = app_state.active_nav == item_nav;
                            let text_color = if is_active { theme.accent_magenta } else { theme.text_secondary };
                            let bg = if is_active { theme.bg_card_active } else { theme.bg_pill };
                            let stroke = if is_active {
                                Stroke::new(1.0_f32, theme.accent_magenta)
                            } else {
                                Stroke::new(1.0_f32, theme.border_subtle)
                            };

                            let btn = egui::Button::new(
                                egui::RichText::new(format!("♫ {}", pl.name))
                                    .font(FontId::proportional(12.0))
                                    .color(text_color),
                            )
                            .fill(bg)
                            .stroke(stroke)
                            .corner_radius(egui::CornerRadius::same(6));

                            if ui.add(btn).clicked() && !is_active {
                                view_action.switch_nav = Some(item_nav);
                            }
                        }

                        // Add playlist button
                        let add_btn = egui::Button::new(
                            egui::RichText::new("＋")
                                .font(FontId::proportional(13.0))
                                .color(theme.text_muted),
                        )
                        .fill(theme.bg_pill)
                        .stroke(Stroke::new(1.0_f32, theme.border_subtle))
                        .corner_radius(egui::CornerRadius::same(6));

                        if ui.add(add_btn).on_hover_text("Create New Playlist").clicked() {
                            view_action.create_playlist = true;
                        }
                    });
                });
        });
        ui.add_space(4.0);
    } else {
        ui.add_space(8.0);
    }

    // Dynamic View / Playlist Header
    match app_state.active_nav {
        NavItem::Playlist(pl_id) => {
            let pl_name = library_state
                .playlists
                .iter()
                .find(|p| p.id == pl_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Playlist".into());

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Playlist: {}", pl_name))
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(theme.accent_magenta),
                );
                ui.label(
                    egui::RichText::new(format!("({} tracks)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🗑 Delete Playlist").clicked() {
                        let _ = playlist_repo.delete(&pl_id);
                        view_action.switch_nav = Some(NavItem::Home);
                    }
                });
            });
            ui.add_space(4.0);
        }
        NavItem::HiRes => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("💎 Hi-Res Studio Master Collection")
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(theme.bit_perfect_green),
                );
                ui.label(
                    egui::RichText::new(format!("({} tracks >48kHz / >1000kbps)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::Lossless => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("✦ Pure Lossless Bit-Exact Collection")
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(theme.accent_blue),
                );
                ui.label(
                    egui::RichText::new(format!("({} FLAC / ALAC / WAV / DSD tracks)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::Favorites => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("♥ Favorites")
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(theme.accent_pink),
                );
                ui.label(
                    egui::RichText::new(format!("({} tracks)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::RecentlyPlayed => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("🕒 Recently Played History")
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(Color32::from_rgb(99, 102, 241)),
                );
                ui.label(
                    egui::RichText::new(format!("({} tracks)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        NavItem::MostPlayed => {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("📊 Most Played Tracks")
                        .font(FontId::proportional(16.0))
                        .strong()
                        .color(theme.accent_orange),
                );
                ui.label(
                    egui::RichText::new(format!("({} tracks)", library_state.displayed_tracks.len()))
                        .font(FontId::proportional(12.0))
                        .color(theme.text_muted),
                );
            });
            ui.add_space(4.0);
        }
        _ => {}
    }

    // Songs Table
    let cur_id = current_track.map(|t| t.id);
    let table_action = SongsTable {
        tracks: &library_state.displayed_tracks,
        active_track_id: cur_id,
        is_playing: telemetry.is_playing,
        favorite_track_ids: &library_state.favorite_ids,
        playlists: &library_state.playlists,
        current_sort: app_state.current_sort,
        is_grid_view: app_state.is_grid_view,
        artwork_map: artwork_service.texture_cache(),
        theme,
    }
    .show(ui);

    handle_songs_table_action(
        table_action,
        app_state,
        library_state,
        coordinator,
        track_repo,
        playlist_repo,
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
) {
    if let Some(play_id) = action.play_track {
        if let Some(track) = library_state.displayed_tracks.iter().find(|t| t.id == play_id).cloned() {
            coordinator.play_track(track, Some(library_state.displayed_tracks.clone()));
        }
    }
    if let Some(next_id) = action.queue_next_track {
        if let Some(track) = library_state.displayed_tracks.iter().find(|t| t.id == next_id).cloned() {
            coordinator.add_to_queue_next(track);
        }
    }
    if let Some(q_id) = action.add_to_queue {
        if let Some(track) = library_state.displayed_tracks.iter().find(|t| t.id == q_id).cloned() {
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
        if let Some(track) = library_state.displayed_tracks.iter().find(|t| t.id == prop_id).cloned() {
            app_state.track_properties_track = Some(track);
            app_state.track_properties_open = true;
        }
    }
    if let Some(rem_id) = action.remove_track {
        let _ = track_repo.delete(&rem_id);
    }
}

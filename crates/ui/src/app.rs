use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Margin, Vec2};
use audio_session::AudioSession;
use database::{
    AlbumRepo, ArtistRepo, Database, FavoritesRepo, GenreRepo, HistoryRepo, PlaylistRepo,
    ScanRootsRepo, SettingsRepo, SmartPlaylistRepo, TrackRepo,
};
use playback::PlaybackCoordinator;
use search::SearchEngine;

use crate::components::sidebar::NavItem;
use crate::layout::{render_right_drawer, render_sidebar};
use crate::modals::{
    show_advanced_audio_modal, show_autoeq_modal, show_bit_perfect_modal, show_duplicates_modal,
    show_edit_track_modal, show_eq_panel, show_help_modal, show_playlist_editor,
    show_smart_playlist_editor, show_track_properties_modal, DuplicateGroup, DuplicatesState,
    SmartPlaylistEditorState,
};
use crate::services::{
    ArtworkService, ImportKind, ImportResult, ImportService, LoudnessService, LyricsService,
    MprisCommand, MprisService, MprisSnapshot, ScannerService,
};
use crate::state::{AppState, DspState, LibraryState, SleepTimerMode, TelemetryState};
use crate::theme::Theme;
use crate::views::{
    render_albums_view, render_artists_view, render_folders_view, render_genres_view,
    render_help_view, render_main_playback_view, render_settings_view, HelpState,
};

/// Authoritative orchestrator for ShadowPlayer desktop interface
pub struct ShadowPlayerApp {
    session: Arc<AudioSession>,
    coordinator: PlaybackCoordinator,
    search_engine: SearchEngine,

    track_repo: TrackRepo,
    playlist_repo: PlaylistRepo,
    favorites_repo: FavoritesRepo,
    history_repo: HistoryRepo,
    album_repo: AlbumRepo,
    artist_repo: ArtistRepo,
    genre_repo: GenreRepo,
    scan_roots_repo: ScanRootsRepo,
    settings_repo: SettingsRepo,
    smart_playlist_repo: SmartPlaylistRepo,

    artwork_service: ArtworkService,
    lyrics_service: LyricsService,
    scanner_service: ScannerService,
    import_service: ImportService,
    mpris_service: MprisService,
    loudness_service: LoudnessService,

    app_state: AppState,
    library_state: LibraryState,
    dsp_state: DspState,
    help_state: HelpState,
    smart_playlist_editor: SmartPlaylistEditorState,
    theme: Theme,
    settings_dirty: bool,

    /// Tracks belonging to the playlist currently selected in the manager.
    playlist_editor_tracks: Vec<domain::models::Track>,
    /// Previous frame's current-track id, used by the end-of-track sleep timer.
    prev_track_id: Option<domain::ids::TrackId>,
    /// Whether playback was running on the previous frame (sleep-timer edge detection).
    prev_playing: bool,

    /// Live filesystem watchers, one per scan root. Dropping one stops watching,
    /// so they must be kept alive for the life of the app.
    watchers: Vec<library::LibraryWatcher>,
    /// Roots reported as changed by the watchers, drained on the UI thread.
    watch_rx: crossbeam::channel::Receiver<PathBuf>,
    /// Sender handed to each watcher (kept so watchers can be rebuilt).
    watch_tx: crossbeam::channel::Sender<PathBuf>,
    /// A watched root that changed, pending a debounced rescan.
    watch_pending: Option<(PathBuf, std::time::Instant)>,
    /// Roots still to scan (startup catch-up and watcher-triggered rescans).
    scan_queue: std::collections::VecDeque<PathBuf>,
    /// Context used to wake the idle UI loop when a watched file changes.
    wake_ctx: Option<egui::Context>,
    /// Playlist staged for M3U export, written once the save dialog returns.
    pending_m3u_export: Option<(String, Vec<library::M3uEntry>)>,
    /// Duplicate-track finder state.
    duplicates: DuplicatesState,
}

impl ShadowPlayerApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        session: Arc<AudioSession>,
        db: Database,
    ) -> Self {
        egui_extras::install_image_loaders(&_cc.egui_ctx);

        let track_repo = TrackRepo::new(db.clone());
        let playlist_repo = PlaylistRepo::new(db.clone());
        let favorites_repo = FavoritesRepo::new(db.clone());
        let history_repo = HistoryRepo::new(db.clone());
        let album_repo = AlbumRepo::new(db.clone());
        let artist_repo = ArtistRepo::new(db.clone());
        let genre_repo = GenreRepo::new(db.clone());
        let scan_roots_repo = ScanRootsRepo::new(db.clone());
        let settings_repo = SettingsRepo::new(db.clone());
        let smart_playlist_repo = SmartPlaylistRepo::new(db.clone());
        let search_engine = SearchEngine::new(db.clone());
        let coordinator = PlaybackCoordinator::new(session.clone(), db.clone());

        let artwork_service = ArtworkService::new();
        let lyrics_service = LyricsService::new();
        let loudness_service = LoudnessService::new(db.clone());
        let scanner_service = ScannerService::new(db, artwork_service.manager());
        let import_service = ImportService::new();
        let mpris_service = MprisService::start();
        let (watch_tx, watch_rx) = crossbeam::channel::unbounded::<PathBuf>();

        let mut app = Self {
            session,
            coordinator,
            search_engine,

            track_repo,
            playlist_repo,
            favorites_repo,
            history_repo,
            album_repo,
            artist_repo,
            genre_repo,
            scan_roots_repo,
            settings_repo,
            smart_playlist_repo,

            artwork_service,
            lyrics_service,
            scanner_service,
            import_service,
            mpris_service,
            loudness_service,

            app_state: AppState::default(),
            library_state: LibraryState::default(),
            dsp_state: DspState::default(),
            help_state: HelpState::default(),
            smart_playlist_editor: SmartPlaylistEditorState::default(),
            theme: Theme::default(),
            settings_dirty: false,

            playlist_editor_tracks: Vec::new(),
            prev_track_id: None,
            prev_playing: false,

            watchers: Vec::new(),
            watch_rx,
            watch_tx,
            watch_pending: None,
            scan_queue: std::collections::VecDeque::new(),
            wake_ctx: Some(_cc.egui_ctx.clone()),
            pending_m3u_export: None,
            duplicates: DuplicatesState::default(),
        };

        // Load all persistent settings (normal, theme, EQ, DSP)
        app.load_persisted_settings();

        app.reload_library();

        // Keep the library fresh: rebuild watchers for the saved roots and run a
        // catch-up scan so files added while the app was closed are picked up.
        app.rebuild_watchers();
        app.schedule_startup_scan();

        app
    }

    /// (Re)create one recursive filesystem watcher per scan root. A changed file
    /// only signals the UI thread; the actual rescan is debounced and serialised
    /// with the scanner so a slow disk never gets hammered.
    fn rebuild_watchers(&mut self) {
        self.watchers.clear();
        let roots: Vec<PathBuf> = self
            .library_state
            .scan_roots
            .iter()
            .filter(|r| r.watch_enabled)
            .map(|r| r.path.clone())
            .collect();
        for root in roots {
            let tx = self.watch_tx.clone();
            let wake = self.wake_ctx.clone();
            let signal_root = root.clone();
            match library::LibraryWatcher::watch(&root, move |_paths| {
                // Coalesce: the drain side debounces anyway.
                let _ = tx.send(signal_root.clone());
                if let Some(ctx) = &wake {
                    ctx.request_repaint();
                }
            }) {
                Ok(w) => self.watchers.push(w),
                Err(e) => log::warn!("Could not watch {}: {e}", root.display()),
            }
        }
    }

    /// Queue every known scan root once, so a launch after some downtime
    /// reconciles files that appeared while the app was not running.
    fn schedule_startup_scan(&mut self) {
        let roots: Vec<PathBuf> = self.library_state.scan_roots.iter().map(|r| r.path.clone()).collect();
        for root in roots {
            if !self.scan_queue.contains(&root) {
                self.scan_queue.push_back(root);
            }
        }
        self.pump_scan_queue();
    }

    /// Start the next queued scan if the scanner is idle.
    fn pump_scan_queue(&mut self) {
        if self.scanner_service.is_scanning() {
            return;
        }
        if let Some(next) = self.scan_queue.pop_front() {
            self.scanner_service.start_scan(next);
        }
    }

    /// Drain watcher signals and, after a short quiet period, enqueue a rescan.
    fn poll_watchers(&mut self) {
        while let Ok(root) = self.watch_rx.try_recv() {
            self.watch_pending = Some((root, std::time::Instant::now()));
        }

        const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(2500);
        if let Some((root, at)) = self.watch_pending.clone() {
            if at.elapsed() < DEBOUNCE {
                return;
            }
            // Never race the loudness scanner: with tag write-back enabled it
            // modifies files, which would otherwise retrigger the watcher.
            if self.loudness_service.is_running() {
                return;
            }
            self.watch_pending = None;
            if !self.scan_queue.contains(&root) {
                self.scan_queue.push_back(root);
            }
            self.pump_scan_queue();
        }
    }

    pub fn load_persisted_settings(&mut self) {
        // 1. AppSettings (Theme, Volume, Mute, Sort, Shuffle, Repeat, Bit-Perfect)
        if let Ok(app_settings) = self.settings_repo.load() {
            self.theme = if app_settings.theme_dark {
                Theme::dark()
            } else {
                Theme::light()
            };
            self.app_state.volume = app_settings.volume;
            self.app_state.is_muted = app_settings.is_muted;
            self.app_state.current_sort = match app_settings.sort_field {
                domain::models::SortField::Title => crate::components::songs_table::SortField::Title,
                domain::models::SortField::Artist => crate::components::songs_table::SortField::Artist,
                domain::models::SortField::Album => crate::components::songs_table::SortField::Album,
                domain::models::SortField::Duration => crate::components::songs_table::SortField::Duration,
                domain::models::SortField::DateAdded => crate::components::songs_table::SortField::DateAdded,
                _ => crate::components::songs_table::SortField::Default,
            };
            self.app_state.is_grid_view = app_settings.view_mode == domain::models::ViewMode::Grid;
            self.coordinator.set_shuffle(app_settings.shuffle);
            self.coordinator.set_repeat_mode(app_settings.repeat_mode);
            self.dsp_state.settings_tab.bit_perfect = app_settings.bit_perfect_enabled;

            let eff_vol = if app_settings.is_muted { 0.0 } else { app_settings.volume };
            self.session.set_volume(eff_vol);
        }

        // 2. Persistent Eq Settings (EQ layout, bands, gains, preamp, tone controls, custom presets, window position)
        if let Ok(Some(json)) = self.settings_repo.get_value("eq_settings") {
            if let Ok(saved_eq) = serde_json::from_str::<crate::components::eq_panel::PersistentEqState>(&json) {
                saved_eq.apply_to(&mut self.dsp_state.eq_panel);
            }
        }

        // 3. Persistent Audio / DSP Settings (Speed, pitch, crossfade, spatial acoustics, dac config)
        if let Ok(Some(json)) = self.settings_repo.get_value("dsp_settings") {
            if let Ok(saved_dsp) = serde_json::from_str::<crate::components::settings_tab::PersistentSettingsTabState>(&json) {
                saved_dsp.apply_to(&mut self.dsp_state.settings_tab);
            }
        }

        // 4. Directories of individually imported song files (shown in Settings)
        if let Ok(Some(json)) = self.settings_repo.get_value("imported_file_dirs") {
            if let Ok(dirs) = serde_json::from_str::<Vec<PathBuf>>(&json) {
                self.app_state.imported_dirs = dirs;
            }
        }

        // Sync restored state to audio session
        self.dsp_state.sync_all_to_session(&self.session);
    }

    pub fn save_persisted_settings(&self) {
        let domain_sort = match self.app_state.current_sort {
            crate::components::songs_table::SortField::Title => domain::models::SortField::Title,
            crate::components::songs_table::SortField::Artist => domain::models::SortField::Artist,
            crate::components::songs_table::SortField::Album => domain::models::SortField::Album,
            crate::components::songs_table::SortField::Duration => domain::models::SortField::Duration,
            crate::components::songs_table::SortField::DateAdded => domain::models::SortField::DateAdded,
            crate::components::songs_table::SortField::Default => domain::models::SortField::Title,
        };

        // 1. Save AppSettings
        let app_settings = domain::models::AppSettings {
            theme_dark: !self.theme.is_light,
            volume: self.app_state.volume,
            is_muted: self.app_state.is_muted,
            sort_field: domain_sort,
            sort_order: domain::models::SortOrder::Ascending,
            view_mode: if self.app_state.is_grid_view {
                domain::models::ViewMode::Grid
            } else {
                domain::models::ViewMode::List
            },
            audio_profile: domain::models::AudioProfileChoice::Normal,
            repeat_mode: self.coordinator.repeat_mode(),
            shuffle: self.coordinator.is_shuffle(),
            selected_device: if self.dsp_state.settings_tab.output_device.is_empty() {
                None
            } else {
                Some(self.dsp_state.settings_tab.output_device.clone())
            },
            bit_perfect_enabled: self.dsp_state.settings_tab.bit_perfect,
            window_width: 1280.0,
            window_height: 800.0,
        };
        let _ = self.settings_repo.save(&app_settings);

        // 2. Save EQ Settings
        let eq_persisted = crate::components::eq_panel::PersistentEqState::from(&self.dsp_state.eq_panel);
        if let Ok(json) = serde_json::to_string(&eq_persisted) {
            let _ = self.settings_repo.set_value("eq_settings", &json);
        }

        // 3. Save DSP & Settings Tab State
        let dsp_persisted = crate::components::settings_tab::PersistentSettingsTabState::from(&self.dsp_state.settings_tab);
        if let Ok(json) = serde_json::to_string(&dsp_persisted) {
            let _ = self.settings_repo.set_value("dsp_settings", &json);
        }

        // 4. Save imported-file directory list
        if let Ok(json) = serde_json::to_string(&self.app_state.imported_dirs) {
            let _ = self.settings_repo.set_value("imported_file_dirs", &json);
        }
    }

    /// Runs on the UI thread when the native picker returns.
    fn handle_import_result(&mut self, result: ImportResult) {
        match result {
            ImportResult::Cancelled => {}
            ImportResult::Folder(path) => {
                // Folder imports become monitored roots (visible in Settings and
                // the Folders tab) and trigger a background scan.
                self.start_scan(path);
            }
            ImportResult::Files(paths) => {
                let mut imported = 0usize;
                let mut dirs: Vec<PathBuf> = Vec::new();

                for path in paths {
                    let is_cue = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("cue"))
                        .unwrap_or(false);

                    if is_cue {
                        if let Ok(cue_tracks) = metadata::extract_cue_metadata(&path) {
                            for meta in &cue_tracks {
                                if self.track_repo.upsert(&meta.track).is_ok() {
                                    imported += 1;
                                }
                            }
                        }
                        continue;
                    }

                    if self.import_file(&path).is_some() {
                        imported += 1;
                        if let Some(parent) = path.parent() {
                            dirs.push(parent.to_path_buf());
                        }
                    }
                }

                if imported > 0 {
                    for dir in dirs {
                        if !self.app_state.imported_dirs.contains(&dir) {
                            self.app_state.imported_dirs.insert(0, dir);
                        }
                    }
                    self.app_state.imported_dirs.truncate(24);
                    self.settings_dirty = true;
                    self.reload_library();
                    log::info!("Imported {} track(s) into the library", imported);
                }
            }
            ImportResult::Playlist(path) => {
                self.import_m3u_playlist(&path);
            }
            ImportResult::SavePath(path) => {
                self.write_pending_m3u_export(&path);
            }
        }
    }

    /// Import one audio file into the library, resolving stable identity so a
    /// moved-but-already-known file keeps its id, play counts and favourites.
    /// Returns the track id on success.
    fn import_file(&mut self, path: &std::path::Path) -> Option<domain::ids::TrackId> {
        if !library::scanner::is_supported_audio_path(path) {
            log::debug!("Skipping unsupported import: {:?}", path);
            return None;
        }

        match metadata::extract_metadata(path) {
            Ok(mut meta) => {
                // Imported files bypass the scanner, so resolve their identity
                // here too.
                let already_indexed = self.track_repo.get_by_path(path).ok().flatten().is_some();
                let path_derived_id = meta.track.id;
                if let Some(stable_id) = library::resolve_identity(
                    &self.track_repo,
                    path,
                    path_derived_id,
                    already_indexed,
                    &mut meta.track.content_hash,
                ) {
                    meta.track.id = stable_id;
                }
                if let Some((bytes, _mime)) = meta.embedded_artwork.as_ref() {
                    let _ = self
                        .artwork_service
                        .manager()
                        .save_thumbnail(&meta.track.id, bytes);
                }
                let id = meta.track.id;
                match self.track_repo.upsert(&meta.track) {
                    Ok(()) => Some(id),
                    Err(e) => {
                        log::warn!("Could not index {:?}: {e}", path);
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Could not read metadata for {:?}: {}", path, e);
                None
            }
        }
    }

    /// Open an M3U playlist: every referenced file is matched against the
    /// library, importing any that are missing, and the result becomes a new
    /// playlist (named after the file) in playlist order.
    fn import_m3u_playlist(&mut self, path: &std::path::Path) {
        let entries = match library::parse_m3u_file(path) {
            Ok(entries) => entries,
            Err(e) => {
                log::warn!("Could not read playlist {}: {e}", path.display());
                return;
            }
        };
        if entries.is_empty() {
            log::info!("Playlist {} had no entries", path.display());
            return;
        }

        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Imported Playlist".to_string());

        let playlist = match self.playlist_repo.create(&name, Some("Imported from M3U")) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Could not create playlist '{name}': {e}");
                return;
            }
        };

        let mut added = 0usize;
        for entry in &entries {
            let track_id = match self.track_repo.get_by_path(&entry.path) {
                Ok(Some(t)) => Some(t.id),
                // Not in the library yet — index it now (this is what makes
                // "open a playlist from a friend" work for files you already
                // own but have not scanned).
                _ => self.import_file(&entry.path),
            };
            if let Some(id) = track_id {
                if self.playlist_repo.add_track(&playlist.id, &id).is_ok() {
                    added += 1;
                }
            }
        }

        log::info!(
            "Imported playlist '{name}': {added}/{} entries",
            entries.len()
        );
        self.reload_library();
    }

    /// Write the queued M3U export to the path the user chose in the save dialog.
    fn write_pending_m3u_export(&mut self, path: &std::path::Path) {
        let Some((_name, entries)) = self.pending_m3u_export.take() else {
            return;
        };
        // Prefer relative paths when the playlist lands on the same volume as
        // the music, so it survives being copied alongside the library.
        let text = library::write_m3u(&entries, path.parent());
        match std::fs::write(path, text) {
            Ok(()) => log::info!("Exported {} track(s) to {}", entries.len(), path.display()),
            Err(e) => log::warn!("Could not write playlist {}: {e}", path.display()),
        }
    }

    pub fn reload_library(&mut self) {
        self.library_state.reload(
            &self.track_repo,
            &self.favorites_repo,
            &self.playlist_repo,
            &self.smart_playlist_repo,
            &self.album_repo,
            &self.artist_repo,
            &self.genre_repo,
            &self.scan_roots_repo,
            &self.history_repo,
            &self.search_engine,
            self.app_state.active_nav,
            &self.app_state.search_query,
            self.app_state.current_sort,
        );
    }

    /// Rebuild only the visible track list for the current tab/search/sort.
    /// Much cheaper than [`reload_library`] because it does not re-query the
    /// whole 10k-track table or the collection metadata.
    pub fn refresh_view(&mut self) {
        // Settings/Help have no track table; skip the pointless 10k clone.
        if matches!(self.app_state.active_nav, NavItem::Settings | NavItem::Help) {
            return;
        }
        self.library_state.update_displayed_tracks(
            &self.search_engine,
            &self.history_repo,
            &self.playlist_repo,
            &self.smart_playlist_repo,
            &self.album_repo,
            &self.artist_repo,
            &self.genre_repo,
            &self.track_repo,
            self.app_state.active_nav,
            &self.app_state.search_query,
            self.app_state.current_sort,
        );
    }

    /// Scan the library for byte-identical duplicates and load the finder.
    fn scan_duplicates(&mut self) {
        let groups = match self.track_repo.find_duplicates() {
            Ok(groups) => groups,
            Err(e) => {
                log::warn!("Could not scan for duplicates: {e}");
                return;
            }
        };
        let groups: Vec<DuplicateGroup> = groups
            .into_iter()
            .map(|tracks| DuplicateGroup {
                // The library has known the lowest-id copy longest, so keep it.
                keep: tracks.first().map(|t| t.id),
                tracks,
            })
            .collect();
        log::info!("Duplicate scan: {} group(s)", groups.len());
        self.duplicates.set_groups(groups);
        self.duplicates.is_open = true;
    }

    /// Stage an M3U export and ask the user where to save it.
    pub fn export_playlist_as_m3u(&mut self, name: String, tracks: Vec<domain::models::Track>) {
        let entries: Vec<library::M3uEntry> = tracks
            .iter()
            .map(|t| library::M3uEntry {
                path: t.path.clone(),
                duration_secs: Some(t.duration_secs.max(0.0).round() as u64),
                artist: Some(t.artist.clone()).filter(|a| !a.trim().is_empty()),
                title: Some(t.title.clone()).filter(|t| !t.trim().is_empty()),
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        self.pending_m3u_export = Some((name.clone(), entries));
        self.import_service.request_save_m3u(name);
    }

    pub fn start_scan(&mut self, path: PathBuf) {
        let _ = self.scan_roots_repo.add(&path);
        self.library_state.scan_roots = self.scan_roots_repo.list().unwrap_or_default();
        // Watch the new root from now on, and scan it once the scanner is free.
        self.rebuild_watchers();
        if !self.scan_queue.contains(&path) {
            self.scan_queue.push_back(path);
        }
        self.pump_scan_queue();
    }

    /// Bridge for the MPRIS2 desktop service: drain transport requests coming
    /// from media keys / the shell and publish the current player state.
    fn handle_mpris(&mut self, ctx: &egui::Context) {
        if !self.mpris_service.is_active() {
            return;
        }
        self.mpris_service.attach_context(ctx);

        while let Some(cmd) = self.mpris_service.try_recv() {
            match cmd {
                MprisCommand::Play => self.session.play(),
                MprisCommand::Pause => self.session.pause(),
                MprisCommand::PlayPause => self.coordinator.toggle_play_pause(),
                MprisCommand::Stop => {
                    self.session.pause();
                    self.session.seek(0.0);
                }
                MprisCommand::Next => self.coordinator.next(),
                MprisCommand::Previous => self.coordinator.previous(),
                MprisCommand::SeekRelative(delta) => {
                    let cur = self.session.position_secs_compensated();
                    let dur = self
                        .coordinator
                        .current_track()
                        .map(|t| t.duration_secs)
                        .unwrap_or(f32::MAX);
                    self.session.seek((cur + delta as f32).clamp(0.0, dur));
                }
                MprisCommand::SetPosition(pos) => {
                    self.session.seek(pos.max(0.0) as f32);
                }
                MprisCommand::SetVolume(v) => {
                    self.app_state.volume = v.clamp(0.0, 1.0);
                    self.app_state.is_muted = false;
                    self.session.set_volume(self.app_state.volume);
                    self.settings_dirty = true;
                }
                MprisCommand::Raise => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                MprisCommand::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        let track = self.coordinator.current_track().cloned();
        let art_url = track.as_ref().and_then(|t| {
            let p = self
                .artwork_service
                .manager()
                .get_cached_thumbnail_path(&t.id);
            if p.exists() {
                Some(format!("file://{}", p.display()))
            } else {
                None
            }
        });
        self.mpris_service.publish(MprisSnapshot {
            loaded: track.is_some(),
            playing: self.session.is_playing(),
            title: track.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
            artist: track.as_ref().map(|t| t.artist.clone()).unwrap_or_default(),
            album: track.as_ref().map(|t| t.album.clone()).unwrap_or_default(),
            length_secs: track.as_ref().map(|t| t.duration_secs as f64).unwrap_or(0.0),
            position_secs: self.session.position_secs_compensated() as f64,
            volume: self.app_state.volume,
            art_url,
        });
    }

    /// Advance the sleep timer. In `Duration` mode this counts down and pauses
    /// playback when it reaches zero; in `EndOfTrack` mode it pauses when the
    /// current track changes (or playback stops on its own).
    fn update_sleep_timer(&mut self, ctx: &egui::Context) {
        let current_id = self.coordinator.current_track().map(|t| t.id);

        let Some(mut timer) = self.app_state.sleep_timer.clone() else {
            self.prev_track_id = current_id;
            self.prev_playing = self.session.is_playing();
            return;
        };

        match timer.mode {
            SleepTimerMode::Duration => {
                let dt = ctx.input(|i| i.stable_dt).min(0.5) as f64;
                // Only count down while something is actually playing.
                if self.session.is_playing() {
                    timer.remaining -= dt;
                }
                if timer.remaining <= 0.0 {
                    self.session.pause();
                    self.app_state.sleep_timer = None;
                    log::info!("Sleep timer elapsed — playback paused");
                } else {
                    self.app_state.sleep_timer = Some(timer);
                }
            }
            SleepTimerMode::EndOfTrack => {
                // We deliberately initialise on the first frame after enabling.
                let track_changed =
                    self.prev_track_id.is_some() && current_id != self.prev_track_id;
                if track_changed {
                    self.session.pause();
                    self.app_state.sleep_timer = None;
                    log::info!("Sleep timer reached end of track — playback paused");
                } else if self.prev_playing && !self.session.is_playing() {
                    // Playback ended by itself; timer has done its job.
                    self.app_state.sleep_timer = None;
                }
            }
        }

        self.prev_track_id = current_id;
        self.prev_playing = self.session.is_playing();
    }

    /// Opens the native OS picker on a worker thread for files or a folder.
    pub fn request_import(&mut self, kind: ImportKind) {
        match kind {
            ImportKind::Files => self.import_service.request_files(),
            ImportKind::Folder => self.import_service.request_folder(),
            // SaveM3u is driven by `export_playlist_as_m3u`, never from here.
            ImportKind::Playlist => self.import_service.request_playlist(),
            ImportKind::SaveM3u => {}
        }
    }
}

impl eframe::App for ShadowPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.theme.apply(&ctx);

        // 1. Poll Coordinator & Session events
        self.coordinator.poll_events();

        // 2. Poll Library Scanner progress
        if self.scanner_service.poll() {
            self.reload_library();
            // A finished scan may unblock the next queued root.
            self.pump_scan_queue();
        }

        // 2a. Folders that changed on disk (debounced) feed the scan queue.
        self.poll_watchers();

        // 2b. Poll the native import picker (runs on a worker thread)
        if let Some(result) = self.import_service.poll() {
            self.handle_import_result(result);
        }

        // 2c. Sleep timer (countdown or end-of-track).
        self.update_sleep_timer(&ctx);

        // 2d. Desktop integration: apply media-key transport, publish state.
        self.handle_mpris(&ctx);

        // 2e. Library-wide loudness scan progress.
        if self.loudness_service.poll() {
            self.reload_library();
        }

        // 3. Reactive Repaints (capped at 60fps during playback/scan; 0% idle when paused)
        let is_playing = self.session.is_playing();
        if is_playing || self.scanner_service.is_scanning() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        } else if self.loudness_service.is_running() {
            // Progress bar only needs a couple of updates per second.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        } else if self.import_service.is_active() {
            // Keep polling briskly while the OS picker is open, cheaply when idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.app_state.sleep_timer.is_some() {
            // Tick the timer display roughly once per second while idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else if self.watch_pending.is_some() || !self.scan_queue.is_empty() {
            // A watched folder changed: keep ticking until the debounce elapses
            // and the queued rescan starts.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // 4. Automated Evaluation Screenshot Hook (SHADOWPLAYER_EVAL only)
        self.app_state.frame_count += 1;
        if std::env::var("SHADOWPLAYER_EVAL").is_ok() {
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SETTINGS").is_ok() {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Settings;
                if let Ok(cat_str) = std::env::var("SHADOWPLAYER_EVAL_SETTINGS_CAT") {
                    match cat_str.as_str() {
                        "spatial" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::Spatial,
                        "playback" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::Playback,
                        "output" | "hardware" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::Output,
                        "library" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::Library,
                        "mixbus" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::MixBus,
                        "tools" => self.dsp_state.settings_tab.active_category = crate::components::settings_tab::SettingsCategory::Tools,
                        _ => {},
                    }
                }
                if std::env::var("SHADOWPLAYER_EVAL_BASS").is_ok() {
                    {
                        let st = &mut self.dsp_state.settings_tab;
                        st.bass_management.enabled = true;
                        st.lfe.enabled = true;
                        st.lfe.crossover_hz = Some(120.0);
                    }
                    let bass = self.dsp_state.settings_tab.bass_management.clone();
                    let lfe = self.dsp_state.settings_tab.lfe.clone();
                    let a = crate::components::settings_tab::SettingsTabActions {
                        bass_management_changed: Some(bass),
                        lfe_changed: Some(lfe),
                        ..Default::default()
                    };
                    self.dsp_state.handle_settings_actions(&self.session, &a);
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SELECT").is_ok() {
                let n: usize = std::env::var("SHADOWPLAYER_EVAL_SELECT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(3);
                for t in self.library_state.displayed_tracks.iter().take(n) {
                    self.library_state.selected_track_ids.insert(t.id);
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SMART").is_ok() {
                self.smart_playlist_editor.open_new();
            }
            if self.app_state.frame_count == 1
                && std::env::var("SHADOWPLAYER_EVAL_DUPLICATES").is_ok()
            {
                self.scan_duplicates();
            }
            // Generic navigation / modal selectors used to capture screenshots
            // of individual views.
            if self.app_state.frame_count == 1 {
                use crate::components::sidebar::NavItem as Nav;
                if let Ok(nav) = std::env::var("SHADOWPLAYER_EVAL_NAV") {
                    self.app_state.active_nav = match nav.as_str() {
                        "albums" => Nav::Albums,
                        "artists" => Nav::Artists,
                        "genres" => Nav::Genres,
                        "folders" => Nav::Folders,
                        "favorites" => Nav::Favorites,
                        "recent" => Nav::RecentlyPlayed,
                        "most" => Nav::MostPlayed,
                        "settings" => Nav::Settings,
                        "help" => Nav::Help,
                        _ => Nav::Home,
                    };
                }
                match std::env::var("SHADOWPLAYER_EVAL_MODAL").as_deref() {
                    Ok("bitperfect") => self.app_state.bit_perfect_modal_open = true,
                    Ok("autoeq") => self.dsp_state.autoeq.is_open = true,
                    Ok("playlist") => {
                        let first = self.library_state.playlists.first().map(|p| p.id);
                        self.app_state.playlist_editor.open(first);
                    }
                    _ => {}
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_HELP").is_ok() {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Help;
                if let Ok(tab_str) = std::env::var("SHADOWPLAYER_EVAL_HELP_TAB") {
                    match tab_str.as_str() {
                        "signal" => self.help_state.active_tab = crate::views::help_view::HelpTab::SignalPath,
                        "eq" => self.help_state.active_tab = crate::views::help_view::HelpTab::EqualizerAcoustics,
                        "bitperfect" => self.help_state.active_tab = crate::views::help_view::HelpTab::BitPerfectDac,
                        "spatial" => self.help_state.active_tab = crate::views::help_view::HelpTab::SpatialBinaural,
                        "playback" => self.help_state.active_tab = crate::views::help_view::HelpTab::PlaybackDsp,
                        "mixbus" => self.help_state.active_tab = crate::views::help_view::HelpTab::MixBusStudio,
                        "shortcuts" => self.help_state.active_tab = crate::views::help_view::HelpTab::KeyboardShortcuts,
                        "glossary" => self.help_state.active_tab = crate::views::help_view::HelpTab::Glossary,
                        _ => {},
                    }
                }
                if let Ok(search_str) = std::env::var("SHADOWPLAYER_EVAL_HELP_SEARCH") {
                    self.help_state.search_query = search_str;
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_EQ").is_ok() {
                self.dsp_state.eq_panel.is_open = true;
                if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_ok() {
                    self.dsp_state.eq_panel.graphic_enabled = false;
                    self.dsp_state.eq_panel.eq_enabled = false;
                    self.dsp_state.eq_panel.selected_preset = crate::components::eq_panel::EqPreset::Flat;
                    for g in self.dsp_state.eq_panel.graphic_gains.iter_mut() {
                        *g = 0.0;
                    }
                    self.dsp_state.eq_panel.graphic_preamp_db = 0.0;
                    self.dsp_state.eq_panel.bass_shelf_db = 0.0;
                    self.dsp_state.eq_panel.treble_shelf_db = 0.0;
                    self.dsp_state.eq_panel.stereo_width = 1.0;
                    self.dsp_state.eq_panel.balance = 0.0;
                    self.dsp_state.eq_panel.auto_headroom = true;
                } else if std::env::var("SHADOWPLAYER_EVAL_EQ_DISABLED").is_ok() {
                    self.dsp_state.eq_panel.graphic_enabled = false;
                    self.dsp_state.eq_panel.eq_enabled = false;
                } else {
                    self.dsp_state.eq_panel.graphic_enabled = true;
                    self.dsp_state.eq_panel.eq_enabled = true;
                }
                if std::env::var("SHADOWPLAYER_EVAL_TONE").is_ok() {
                    self.dsp_state.eq_panel.active_tab = crate::components::eq_panel::EqTab::Tone;
                    if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_err() {
                        self.dsp_state.eq_panel.bass_shelf_db = 4.0;
                        self.dsp_state.eq_panel.treble_shelf_db = -1.5;
                        self.dsp_state.eq_panel.stereo_width = 1.35;
                        self.dsp_state.eq_panel.balance = 0.20;
                        self.dsp_state.eq_panel.graphic_preamp_db = 1.5;
                    }
                } else if std::env::var("SHADOWPLAYER_EVAL_ADVANCED").is_ok() {
                    self.dsp_state.eq_panel.active_tab = crate::components::eq_panel::EqTab::Advanced;
                    self.dsp_state.eq_panel.extended_range = false;
                    self.dsp_state.eq_panel.auto_headroom = true;
                    self.dsp_state.eq_panel.midside_eq = false;
                    self.dsp_state.eq_panel.graphic_preamp_db = 0.0;
                } else if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_err() {
                    self.dsp_state.eq_panel.selected_preset = crate::components::eq_panel::EqPreset::Pop;
                    if let Some(g) = crate::components::eq_panel::EqPreset::Pop.gains() {
                        for i in 0..10 {
                            self.dsp_state.eq_panel.graphic_gains[i] = g[i];
                        }
                    }
                }
            }
            if let Ok(info_key) = std::env::var("SHADOWPLAYER_EVAL_INFO_KEY") {
                let active_key = egui::Id::NULL.with("shadowplayer_active_info_id");
                let target_id = egui::Id::NULL.with(info_key);
                ctx.data_mut(|d| d.insert_temp(active_key, target_id));
            }
            if self.app_state.frame_count < 30 { ctx.request_repaint(); }
            if self.app_state.frame_count == 24 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |image: std::sync::Arc<egui::ColorImage>| {
                        let _ = image::save_buffer(
                            "eval_screenshot.png",
                            image.as_raw(),
                            image.width() as u32,
                            image.height() as u32,
                            image::ExtendedColorType::Rgba8,
                        );
                        std::process::exit(0);
                    },
                )));
            }
            if self.app_state.frame_count > 45 { std::process::exit(0); }
        }

        // 5. Drag & Drop File / Folder Ingestion
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                let path = file.path();
                if path.is_dir() {
                    self.start_scan(path.to_path_buf());
                } else if path.is_file() {
                    let is_cue = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("cue")).unwrap_or(false);
                    if is_cue {
                        if let Ok(cue_tracks) = metadata::extract_cue_metadata(path) {
                            for meta in &cue_tracks {
                                let _ = self.track_repo.upsert(&meta.track);
                            }
                            if let Some(first) = cue_tracks.first().map(|m| m.track.clone()) {
                                let all_t = cue_tracks.into_iter().map(|m| m.track).collect();
                                self.coordinator.play_track(first, Some(all_t));
                            }
                            self.reload_library();
                        }
                    } else if let Ok(meta) = metadata::extract_metadata(path) {
                        let _ = self.track_repo.upsert(&meta.track);
                        self.coordinator.play_track(meta.track.clone(), None);
                        self.reload_library();
                    }
                }
            }
        });

        // 6. Global Keyboard Shortcuts
        ctx.input(|i| {
            if !i.focused {
                if i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::K) {
                    self.coordinator.toggle_play_pause();
                }
                if i.key_pressed(egui::Key::J) {
                    self.coordinator.previous();
                }
                if i.key_pressed(egui::Key::L) {
                    self.coordinator.next();
                }
                if i.key_pressed(egui::Key::M) {
                    self.app_state.is_muted = !self.app_state.is_muted;
                    let target_vol = if self.app_state.is_muted { 0.0 } else { self.app_state.volume };
                    self.session.set_volume(target_vol);
                    self.settings_dirty = true;
                }
                if i.key_pressed(egui::Key::S) {
                    let new_shuffle = !self.coordinator.is_shuffle();
                    self.coordinator.set_shuffle(new_shuffle);
                    self.settings_dirty = true;
                }
                if i.key_pressed(egui::Key::R) {
                    let next_mode = match self.coordinator.repeat_mode() {
                        domain::models::RepeatModeChoice::Off => domain::models::RepeatModeChoice::All,
                        domain::models::RepeatModeChoice::All => domain::models::RepeatModeChoice::One,
                        domain::models::RepeatModeChoice::One => domain::models::RepeatModeChoice::Off,
                    };
                    self.coordinator.set_repeat_mode(next_mode);
                    self.settings_dirty = true;
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    let cur = self.session.position_secs_compensated();
                    self.session.seek((cur - 5.0).max(0.0));
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    let cur = self.session.position_secs_compensated();
                    let dur = self.session.duration_secs().max(1.0);
                    self.session.seek((cur + 5.0).min(dur));
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    self.app_state.volume = (self.app_state.volume + 0.05).min(1.0);
                    self.app_state.is_muted = false;
                    self.session.set_volume(self.app_state.volume);
                    self.settings_dirty = true;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    self.app_state.volume = (self.app_state.volume - 0.05).max(0.0);
                    self.app_state.is_muted = false;
                    self.session.set_volume(self.app_state.volume);
                    self.settings_dirty = true;
                }
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::N) {
                self.app_state.new_playlist_modal_open = true;
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::B) {
                let cur_bp = self.session.bit_perfect();
                if !cur_bp {
                    // Activate Pure Direct
                    self.session.set_volume(1.0);
                    self.session.set_preamp(0.0);
                    self.session.set_speed(1.0);
                    self.session.set_eq_enabled(false);
                    self.session.set_graphic_eq_enabled(false);
                    self.session.set_limiter_enabled(false);
                    self.session.set_compressor_enabled(false);
                    self.session.set_spatial_enabled(false);
                    self.session.set_stereo_enhancer_enabled(false);
                    self.session.set_bit_perfect(true);
                    self.dsp_state.settings_tab.bit_perfect = true;
                } else {
                    self.session.set_bit_perfect(false);
                    self.dsp_state.settings_tab.bit_perfect = false;
                }
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::P) {
                self.app_state.active_nav = NavItem::Settings;
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::O) {
                self.app_state.active_nav = NavItem::Folders;
            }
            if (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::E) {
                self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
            }
            if i.key_pressed(egui::Key::Escape) {
                if !self.app_state.search_query.is_empty() {
                    self.app_state.search_query.clear();
                    self.refresh_view();
                }
                self.app_state.new_playlist_modal_open = false;
                self.app_state.help_modal_open = false;
                self.app_state.track_properties_open = false;
                self.dsp_state.eq_panel.is_open = false;
            }
        });

        // 7. Polled Authoritative Telemetry
        let current_track = self.coordinator.current_track().cloned();
        let dur_secs = if self.session.duration_secs() > 0.0 {
            self.session.duration_secs() as f64
        } else if let Some(ref t) = current_track {
            t.duration_secs as f64
        } else {
            0.0
        };

        let telemetry = TelemetryState {
            position_secs: self.session.position_secs_compensated() as f64,
            duration_secs: dur_secs,
            is_playing,
            bit_perfect: self.session.bit_perfect(),
            bit_perfect_cause: self.session.bit_perfect_cause().map(|c| format!("{:?}", c)),
            spectrum: self.session.analyzer_snapshot(),
            meters: Some(self.session.meters_snapshot()),
        };

        // Cache lyrics for the current track. Artwork for visible rows is loaded
        // lazily by the views themselves, so no bulk preloading happens here.
        if let Some(ref t) = current_track {
            self.lyrics_service.ensure_loaded(t);
            self.artwork_service.ensure_loaded(&ctx, t);
        }

        let (active_lyrics, plain_lyrics) = current_track
            .as_ref()
            .map(|t| self.lyrics_service.get_lyrics(&t.id))
            .unwrap_or_default();
        let active_lyric_idx = LyricsService::active_index(&active_lyrics, telemetry.position_secs as f32);

        // 8. Render Left Sidebar
        let mut nav_to_select = None;
        let mut create_playlist_req = false;
        let mut create_smart_playlist_req = false;
        render_sidebar(
            ui,
            self.app_state.active_nav,
            &self.library_state.playlists,
            &self.library_state.smart_playlists,
            &self.theme,
            |nav| nav_to_select = Some(nav),
            || create_playlist_req = true,
            || create_smart_playlist_req = true,
        );

        if let Some(nav) = nav_to_select {
            self.app_state.active_nav = nav;
            self.library_state.selected_album = None;
            self.library_state.selected_artist = None;
            self.library_state.selected_genre = None;
            self.library_state.selected_folder = None;
            if nav != NavItem::Settings && nav != NavItem::Help {
                self.refresh_view();
            }
        }
        if create_playlist_req {
            self.app_state.new_playlist_modal_open = true;
        }
        if create_smart_playlist_req {
            self.smart_playlist_editor.open_new();
        }

        // 9. Render Central Panel (Main Views + Right Drawer)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.bg_app).inner_margin(Margin { left: 10, right: 18, top: 14, bottom: 14 }))
            .show(ui, |ui| {
                let avail_w = ui.available_width();
                let avail_h = ui.available_height();
                let is_help_view = self.app_state.active_nav == NavItem::Help;
                let show_right_panel = !is_help_view && avail_w > 420.0;
                let right_w = if show_right_panel { 285.0 } else { 0.0 };
                let center_w = if show_right_panel { (avail_w - right_w - 16.0).max(300.0) } else { avail_w };

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(16.0, 0.0);

                    // A. Center View Column
                    ui.allocate_ui_with_layout(
                        Vec2::new(center_w, avail_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                            // Top Search Bar (Only shown for library views; hidden in Help and Settings)
                            if !is_help_view && self.app_state.active_nav != NavItem::Settings {
                                let mut search_buf = self.app_state.search_query.clone();
                                if crate::components::topbar::render_search_input(ui, &mut search_buf, &self.theme) {
                                    self.app_state.search_query = search_buf;
                                    self.refresh_view();
                                }
                            }

                            // View Switcher
                            match self.app_state.active_nav {
                                NavItem::Albums => {
                                    render_albums_view(
                                        ui,
                                        &mut self.library_state,
                                        &mut self.coordinator,
                                        &self.artwork_service,
                                        current_track.as_ref(),
                                        is_playing,
                                        &self.theme,
                                        &self.album_repo,
                                        &self.favorites_repo,
                                    );
                                }
                                NavItem::Artists => {
                                    render_artists_view(
                                        ui,
                                        &mut self.library_state,
                                        &mut self.coordinator,
                                        current_track.as_ref(),
                                        is_playing,
                                        &self.theme,
                                        &self.artist_repo,
                                        &self.favorites_repo,
                                    );
                                }
                                NavItem::Genres => {
                                    render_genres_view(
                                        ui,
                                        &mut self.library_state,
                                        &mut self.coordinator,
                                        current_track.as_ref(),
                                        is_playing,
                                        &self.theme,
                                        &self.genre_repo,
                                        &self.favorites_repo,
                                    );
                                }
                                NavItem::Folders => {
                                    let mut scan_to_start = None;
                                    let mut import_req = None;
                                    let import_active = self.import_service.is_active();
                                    render_folders_view(
                                        ui,
                                        &mut self.library_state,
                                        &mut self.scanner_service,
                                        &mut self.coordinator,
                                        current_track.as_ref(),
                                        &self.theme,
                                        &self.scan_roots_repo,
                                        &self.track_repo,
                                        &self.favorites_repo,
                                        import_active,
                                        |path| scan_to_start = Some(path),
                                        |kind| import_req = Some(kind),
                                    );
                                    if let Some(path) = scan_to_start {
                                        self.start_scan(path);
                                    }
                                    if let Some(kind) = import_req {
                                        self.request_import(kind);
                                    }
                                }
                                NavItem::Settings => {
                                    let prev_light = self.theme.is_light;
                                    let mut scan_to_start = None;
                                    let mut import_req = None;
                                    let is_scanning = self.scanner_service.is_scanning();
                                    let import_active = self.import_service.is_active();
                                    let sv_actions = render_settings_view(
                                        ui,
                                        &mut self.dsp_state,
                                        &self.session,
                                        &mut self.theme,
                                        &self.scan_roots_repo,
                                        &self.track_repo,
                                        is_scanning,
                                        import_active,
                                        &self.app_state.imported_dirs,
                                        self.loudness_service.progress(),
                                        |path| scan_to_start = Some(path),
                                        |kind| import_req = Some(kind),
                                    );
                                    if self.theme.is_light != prev_light {
                                        self.settings_dirty = true;
                                    }
                                    // Settings view can mutate settings_tab directly
                                    self.settings_dirty = true;
                                    if let Some(path) = scan_to_start {
                                        self.start_scan(path);
                                    }
                                    if let Some(kind) = import_req {
                                        self.request_import(kind);
                                    }
                                    if sv_actions.start_loudness_scan {
                                        let write_tags =
                                            self.dsp_state.settings_tab.loudness_write_tags;
                                        self.loudness_service.start(write_tags);
                                    }
                                    if sv_actions.cancel_loudness_scan {
                                        self.loudness_service.cancel();
                                    }
                                    if sv_actions.find_duplicates {
                                        self.scan_duplicates();
                                    }
                                }
                                NavItem::Help => {
                                    render_help_view(
                                        ui,
                                        &mut self.help_state,
                                        &self.theme,
                                        &mut self.app_state,
                                        &mut self.dsp_state,
                                    );
                                }
                                _ => {
                                    let view_action = render_main_playback_view(
                                        ui,
                                        &mut self.app_state,
                                        &mut self.library_state,
                                        &mut self.coordinator,
                                        &self.session,
                                        &mut self.artwork_service,
                                        &telemetry,
                                        current_track.as_ref(),
                                        &self.theme,
                                        &self.track_repo,
                                        &self.playlist_repo,
                                        &self.favorites_repo,
                                        self.dsp_state.eq_panel.is_open,
                                    );
                                    if let Some(nav) = view_action.switch_nav {
                                        self.app_state.active_nav = nav;
                                        self.library_state.selected_album = None;
                                        self.library_state.selected_artist = None;
                                        self.library_state.selected_genre = None;
                                        self.library_state.selected_folder = None;
                                        self.refresh_view();
                                    }
                                    if view_action.create_playlist {
                                        self.app_state.new_playlist_modal_open = true;
                                    }
                                    if view_action.open_eq_panel {
                                        self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
                                        self.settings_dirty = true;
                                    }
                                    if view_action.needs_reload {
                                        self.refresh_view();
                                    }
                                    if view_action.removed_track {
                                        self.reload_library();
                                    }
                                    if let Some(sp_id) = view_action.edit_smart_playlist {
                                        if let Some(sp) = self
                                            .library_state
                                            .smart_playlists
                                            .iter()
                                            .find(|s| s.id == sp_id)
                                            .cloned()
                                        {
                                            self.smart_playlist_editor.open_existing(&sp);
                                        }
                                    }
                                }
                            }
                        },
                    );

                    // B. Right Column Drawer
                    if show_right_panel {
                        let prev_light = self.theme.is_light;
                        let prev_vol = self.app_state.volume;
                        let prev_muted = self.app_state.is_muted;
                        render_right_drawer(
                            ui,
                            &ctx,
                            right_w,
                            avail_h,
                            &mut self.app_state,
                            &mut self.dsp_state,
                            &mut self.coordinator,
                            &self.session,
                            &mut self.artwork_service,
                            &active_lyrics,
                            plain_lyrics.as_deref(),
                            active_lyric_idx,
                            &mut self.theme,
                        );
                        if self.theme.is_light != prev_light
                            || (self.app_state.volume - prev_vol).abs() > 0.001
                            || self.app_state.is_muted != prev_muted
                        {
                            self.settings_dirty = true;
                        }
                    }
                });
            });

        // 10. Modals Execution
        let audio_actions = show_advanced_audio_modal(
            &ctx,
            &mut self.dsp_state.advanced_audio,
            telemetry.bit_perfect,
            telemetry.bit_perfect_cause.as_deref(),
            &self.theme,
        );
        self.dsp_state.handle_audio_modal_actions(&self.session, audio_actions);

        let eq_actions = show_eq_panel(&ctx, &mut self.dsp_state.eq_panel, &self.theme);
        self.dsp_state.handle_eq_panel_actions(&self.session, eq_actions);
        if self.dsp_state.eq_panel.is_open {
            self.settings_dirty = true;
        }

        show_autoeq_modal(
            &ctx,
            &mut self.dsp_state.autoeq,
            &mut self.dsp_state.eq_panel,
            &self.session,
            &self.theme,
        );

        show_bit_perfect_modal(
            &ctx,
            &mut self.app_state.bit_perfect_modal_open,
            current_track.as_ref(),
            &self.session,
            &mut self.dsp_state,
            &self.theme,
        );

        show_help_modal(&ctx, &mut self.app_state.help_modal_open, &self.theme);

        show_track_properties_modal(
            &ctx,
            self.app_state.track_properties_track.as_ref(),
            &mut self.app_state.track_properties_open,
            &self.theme,
        );

        // Smart playlist editor: rule-based, self-updating playlists. Re-evaluate
        // the draft only when it changed, so typing never re-queries the library
        // more than once per frame.
        if self.smart_playlist_editor.is_open && self.smart_playlist_editor.draft_dirty {
            let draft = self.smart_playlist_editor.draft.clone();
            self.library_state
                .evaluate_smart(&self.smart_playlist_repo, &draft);
            self.smart_playlist_editor.draft_dirty = false;
        }
        let sp_actions = show_smart_playlist_editor(
            &ctx,
            &mut self.smart_playlist_editor,
            &self.library_state.smart_playlists,
            &self.library_state.smart_playlist_tracks,
            &self.theme,
        );
        let mut sp_changed = false;
        if let Some(sp) = sp_actions.save {
            match self.smart_playlist_repo.save(&sp) {
                Ok(saved) => {
                    self.app_state.active_nav = NavItem::SmartPlaylist(saved.id);
                    self.smart_playlist_editor.is_new = false;
                    self.smart_playlist_editor.draft = saved;
                    sp_changed = true;
                }
                Err(e) => log::warn!("Could not save smart playlist: {}", e),
            }
        }
        if let Some(id) = sp_actions.delete {
            let _ = self.smart_playlist_repo.delete(&id);
            if self.app_state.active_nav == NavItem::SmartPlaylist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            if !self.smart_playlist_editor.is_new && self.smart_playlist_editor.draft.id == id {
                self.smart_playlist_editor.is_open = false;
            }
            sp_changed = true;
        }
        if let Some(id) = sp_actions.open {
            if let Some(sp) = self
                .library_state
                .smart_playlists
                .iter()
                .find(|s| s.id == id)
                .cloned()
            {
                self.smart_playlist_editor.open_existing(&sp);
            }
        }
        if sp_changed {
            self.reload_library();
        }

        // Playlist manager: create / rename / delete / add songs.
        if self.app_state.new_playlist_modal_open {
            self.app_state.new_playlist_modal_open = false;
            self.app_state.playlist_editor.open(None);
        }
        let pl_actions = show_playlist_editor(
            &ctx,
            &mut self.app_state.playlist_editor,
            &self.library_state.playlists,
            &self.library_state.all_tracks,
            &self.playlist_editor_tracks,
            &self.theme,
        );
        let mut pl_changed = false;
        if let Some(name) = pl_actions.create {
            match self.playlist_repo.create(&name, None) {
                Ok(_) => pl_changed = true,
                Err(e) => log::warn!("Could not create playlist: {}", e),
            }
        }
        if let Some((id, name)) = pl_actions.rename {
            let _ = self.playlist_repo.rename(&id, &name);
            pl_changed = true;
        }
        if let Some(id) = pl_actions.delete {
            let _ = self.playlist_repo.delete(&id);
            if self.app_state.playlist_editor.selected == Some(id) {
                self.app_state.playlist_editor.selected = None;
            }
            self.playlist_editor_tracks.clear();
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.add_track {
            let _ = self.playlist_repo.add_track(&pl, &tr);
            self.playlist_editor_tracks = self.playlist_repo.get_tracks(&pl).unwrap_or_default();
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.remove_track {
            let _ = self.playlist_repo.remove_track(&pl, &tr);
            self.playlist_editor_tracks = self.playlist_repo.get_tracks(&pl).unwrap_or_default();
            pl_changed = true;
        }
        if let Some(id) = pl_actions.select_changed {
            self.playlist_editor_tracks = self.playlist_repo.get_tracks(&id).unwrap_or_default();
        }
        if pl_actions.export_m3u {
            if let Some(id) = self.app_state.playlist_editor.selected {
                let name = self
                    .library_state
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "playlist".to_string());
                let tracks = self.playlist_editor_tracks.clone();
                self.export_playlist_as_m3u(name, tracks);
            }
        }
        if pl_changed {
            self.reload_library();
        }

        // Duplicate-track finder.
        let dupe_actions = show_duplicates_modal(&ctx, &mut self.duplicates, &self.theme);
        if dupe_actions.rescan {
            self.scan_duplicates();
        }
        if dupe_actions.remove_selected {
            let delete_from_disk = self.duplicates.delete_from_disk;
            let ids: Vec<domain::ids::TrackId> =
                self.duplicates.selected.iter().copied().collect();
            let mut removed = 0usize;
            for id in ids {
                // Deleting the file first means a failed unlink leaves the
                // library row intact (rather than orphaning a file on disk).
                if delete_from_disk {
                    match self.track_repo.get_by_id(&id) {
                        Ok(Some(t)) if t.path.exists() => {
                            if let Err(e) = std::fs::remove_file(&t.path) {
                                log::warn!("Could not delete {}: {e}", t.path.display());
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                if self.track_repo.delete(&id).is_ok() {
                    removed += 1;
                }
            }
            log::info!("Removed {removed} duplicate track(s)");
            self.reload_library();
            // Refresh so the list reflects what is left.
            self.scan_duplicates();
        }

        // Edit Song Info dialog.
        let edit_actions = show_edit_track_modal(&ctx, &mut self.app_state.edit_track, &self.theme);
        if let Some(track) = edit_actions.save {
            if let Err(e) = self.track_repo.update_metadata(&track) {
                log::warn!("Could not save track metadata: {}", e);
            }
            self.reload_library();
        }

        // 11. Event-driven & periodic settings persistence across sessions
        if std::env::var("SHADOWPLAYER_EVAL").is_err() && (self.settings_dirty || (self.app_state.frame_count % 120 == 0)) {
            self.save_persisted_settings();
            self.settings_dirty = false;
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if std::env::var("SHADOWPLAYER_EVAL").is_err() {
            self.save_persisted_settings();
        }
    }
}

use std::path::PathBuf;
use std::sync::Arc;

use audio_session::AudioSession;
use database::{
    AlbumNotesRepo, AlbumRepo, ArtistRepo, Database, FavoritesRepo, GenreRepo, HistoryRepo,
    PlaylistRepo, PodcastRepo, ScanRootsRepo, SettingsRepo, SmartPlaylistRepo, TrackRepo,
};
use domain::models::Track;
use eframe::egui::{self, Margin, Pos2, Rect, Sense, Vec2};
use playback::PlaybackCoordinator;
use search::SearchEngine;

use crate::components::clap_rack_panel::show_clap_rack_panel;
use crate::components::command_palette::{PaletteAction, PaletteEntry};
use crate::components::mini_player::MiniPlayer;
use crate::components::sidebar::NavItem;
use crate::components::topbar::{render_action_button, TopbarAction};
use crate::layout::{render_right_drawer, render_sidebar};
use crate::modals::{
    show_artwork_fetch_modal, show_autoeq_modal, show_autotag_modal, show_bit_perfect_modal,
    show_duplicates_modal, show_edit_track_modal, show_eq_panel, show_media_tools_modal,
    show_organize_modal, show_playlist_editor, show_room_measurement_modal,
    show_smart_playlist_editor, show_track_properties_modal, ArtworkFetchModalState,
    AutotagModalState, DuplicateGroup, DuplicatesState, MediaToolsTab, OrganizeState,
    RoomMeasurementState, SmartPlaylistEditorState,
};
use crate::services::{
    ArtworkFetchService, ArtworkService, AutotagService, BiographyService, ControlService,
    CrossfadePreview, ImportKind, ImportResult, ImportService, LoudnessService, LyricsService,
    MediaToolEvent, MediaToolsService, MprisCommand, MprisService, MprisSnapshot, PodcastEvent,
    PodcastService, ScannerService, WaveformService,
};
use crate::state::{
    AppState, DspState, LibraryState, PodcastState, SleepTimerMode, TelemetryState,
};
use crate::theme::Theme;
use crate::theme::ThemeManager;
use crate::views::{
    render_albums_view, render_artists_view, render_folders_view, render_genres_view,
    render_help_view, render_main_playback_view, render_settings_view, HelpState,
};

mod actions;
mod autotag;
mod clap_eval;
mod command_palette;
mod desktop;
mod editors;
mod eval;
mod history;
mod imports;
mod library_actions;
mod modals;
mod render;
mod service_poll;
mod settings_io;
mod shortcuts;

#[cfg(feature = "eval-tools")]
use clap_eval::ClapEval;
use library_actions::load_playlist_tracks;
use settings_io::eval_holds_persistence;
/// Authoritative orchestrator for ShadowPlayer desktop interface
pub struct ShadowPlayerApp {
    session: Arc<AudioSession>,
    coordinator: PlaybackCoordinator,
    search_engine: SearchEngine,

    track_repo: TrackRepo,
    track_dsp_repo: database::TrackDspRepo,
    /// The track whose DSP profile was last applied, so the profile is
    /// applied once per track rather than once per frame.
    dsp_profile_track: Option<domain::ids::TrackId>,
    /// The chain as it stood when the current track started with no profile.
    /// The user may adjust the chain while listening to author that track's
    /// override; this is the base the override is diffed against, so those
    /// adjustments do not silently become the global chain.
    dsp_track_base: Option<crate::state::ChainPreset>,
    playlist_repo: PlaylistRepo,
    favorites_repo: FavoritesRepo,
    history_repo: HistoryRepo,
    scrobble_repo: database::ScrobbleRepo,
    album_repo: AlbumRepo,
    album_notes_repo: AlbumNotesRepo,
    artist_repo: ArtistRepo,
    genre_repo: GenreRepo,
    scan_roots_repo: ScanRootsRepo,
    settings_repo: SettingsRepo,
    smart_playlist_repo: SmartPlaylistRepo,

    artwork_service: ArtworkService,
    lyrics_service: LyricsService,
    waveform_service: WaveformService,
    crossfade_preview: CrossfadePreview,
    biography_service: BiographyService,
    podcast_service: PodcastService,
    podcast_repo: PodcastRepo,
    podcast_state: PodcastState,
    scanner_service: ScannerService,
    import_service: ImportService,
    mpris_service: MprisService,
    /// Loopback control for `shadowplayer-remote` and second-launch
    /// activation. `None` in a multi-instance process, which owns no
    /// transport.
    control_service: Option<ControlService>,
    loudness_service: LoudnessService,
    /// Persistent data directory, used for non-database settings that must be
    /// readable before the database is opened.
    data_dir: PathBuf,
    /// Mirrors the Settings toggle; changes are written to the pre-launch
    /// marker immediately.
    multi_instance_allowed: bool,
    /// The saved language preference. Empty means "follow the environment".
    locale: String,

    app_state: AppState,
    library_state: LibraryState,
    dsp_state: DspState,
    help_state: HelpState,
    smart_playlist_editor: SmartPlaylistEditorState,
    theme: Theme,
    /// The theme variant currently installed in the egui context. `None` means
    /// no style has been applied yet; the app only rebuilds egui's style when
    /// this differs from `theme.is_light`.
    applied_theme: Option<Theme>,
    settings_dirty: bool,

    pub theme_manager: ThemeManager,
    pub autotag_service: AutotagService,
    pub autotag_modal: AutotagModalState,
    pub artwork_fetch_service: ArtworkFetchService,
    pub artwork_fetch_modal: ArtworkFetchModalState,
    /// Room-correction measurement wizard.
    pub room_measurement: RoomMeasurementState,
    /// CD ripping, transcode export, and opt-in update jobs.
    pub media_tools_service: MediaToolsService,
    pub media_tools: crate::modals::MediaToolsModalState,

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
    /// Whether the next completed scan deserves a notification (user- or
    /// watcher-initiated) or should stay silent (startup catch-up).
    scan_toast_wanted: bool,
    /// Context used to wake the idle UI loop when a watched file changes.
    wake_ctx: Option<egui::Context>,
    /// Playlist staged for export, written once the save dialog returns.
    pending_playlist_export: Option<(String, Vec<library::M3uEntry>, library::PlaylistFormat)>,
    /// Duplicate-track finder state.
    duplicates: DuplicatesState,
    /// Batch organize / rename state.
    organize: OrganizeState,
    /// Watcher events are ignored until this instant. Our own file moves would
    /// otherwise look like external changes and start a needless rescan.
    watch_suppress_until: Option<std::time::Instant>,
    /// Frames left in which to ask the engine for the CLAP rack view at
    /// startup. A plugin restored from the engine's own config only becomes
    /// visible once the graph has been rebuilt, which happens on the engine's
    /// first ticks; asking for a few frames means the answer does not depend on
    /// which frame that was.
    clap_refresh_frames: u32,
    /// `SHADOWPLAYER_EVAL_CLAP`'s script, when that hook is in use.
    #[cfg(feature = "eval-tools")]
    clap_eval: Option<ClapEval>,
    /// Transient user-facing notifications.
    toasts: crate::components::toasts::ToastState,
    /// Last observed window size in logical points. Written every frame and
    /// persisted, so the next launch opens where the user left it instead of
    /// at the hard-coded default.
    window_size: (f32, f32),
    /// Size to restore when leaving the mini player. Mini mode never persists
    /// its 320x110 geometry as the normal window size.
    normal_window_size: (f32, f32),
    /// Whether the viewport commands for mini mode have been sent.
    mini_applied: bool,
}

/// How many frames the app keeps asking for the CLAP rack view after startup.
const CLAP_STARTUP_REFRESHES: u32 = 5;

impl ShadowPlayerApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        session: Arc<AudioSession>,
        db: Database,
        control_service: Option<ControlService>,
        data_dir: PathBuf,
    ) -> Self {
        egui_extras::install_image_loaders(&_cc.egui_ctx);
        crate::theme::install_fonts(&_cc.egui_ctx);

        let track_repo = TrackRepo::new(db.clone());
        let track_dsp_repo = database::TrackDspRepo::new(db.clone());
        let playlist_repo = PlaylistRepo::new(db.clone());
        let favorites_repo = FavoritesRepo::new(db.clone());
        let history_repo = HistoryRepo::new(db.clone());
        let scrobble_repo = database::ScrobbleRepo::new(db.clone());
        let album_repo = AlbumRepo::new(db.clone());
        let album_notes_repo = AlbumNotesRepo::new(db.clone());
        let artist_repo = ArtistRepo::new(db.clone());
        let genre_repo = GenreRepo::new(db.clone());
        let scan_roots_repo = ScanRootsRepo::new(db.clone());
        let settings_repo = SettingsRepo::new(db.clone());
        let smart_playlist_repo = SmartPlaylistRepo::new(db.clone());
        let search_engine = SearchEngine::new(db.clone());
        let coordinator = PlaybackCoordinator::new(session.clone(), db.clone());

        let artwork_service = ArtworkService::new();
        let lyrics_service = LyricsService::new();
        let waveform_service = WaveformService::new(db.clone());
        let biography_service = BiographyService::new(db.clone());
        let podcast_repo = PodcastRepo::new(db.clone());
        let podcast_service = PodcastService::new(PodcastRepo::new(db.clone()), &data_dir);
        let podcast_state = PodcastState::default();
        let loudness_service = LoudnessService::new(db.clone());
        let scanner_service = ScannerService::new(db, artwork_service.manager());
        let import_service = ImportService::new();
        let mpris_service = MprisService::start();
        let (watch_tx, watch_rx) = crossbeam::channel::unbounded::<PathBuf>();

        let mut theme_manager = ThemeManager::new(&data_dir);
        let _ = theme_manager.write_example_theme();
        theme_manager.load_all();

        let autotag_service = AutotagService::new();
        let autotag_modal = AutotagModalState::default();
        let artwork_fetch_service = ArtworkFetchService::new();
        let artwork_fetch_modal = ArtworkFetchModalState::default();

        let mut app = Self {
            session,
            coordinator,
            search_engine,

            track_repo,
            track_dsp_repo,
            dsp_profile_track: None,
            dsp_track_base: None,
            playlist_repo,
            favorites_repo,
            history_repo,
            scrobble_repo,
            album_repo,
            album_notes_repo,
            artist_repo,
            genre_repo,
            scan_roots_repo,
            settings_repo,
            smart_playlist_repo,

            artwork_service,
            lyrics_service,
            waveform_service,
            crossfade_preview: CrossfadePreview::new(),
            biography_service,
            podcast_service,
            podcast_repo,
            podcast_state,
            scanner_service,
            import_service,
            mpris_service,
            control_service,
            loudness_service,
            data_dir,
            multi_instance_allowed: false,
            locale: String::new(),

            app_state: AppState::default(),
            library_state: LibraryState::default(),
            dsp_state: DspState::default(),
            help_state: HelpState::default(),
            smart_playlist_editor: SmartPlaylistEditorState::default(),
            theme: Theme::default(),
            applied_theme: None,
            settings_dirty: false,

            theme_manager,
            autotag_service,
            autotag_modal,
            artwork_fetch_service,
            artwork_fetch_modal,
            room_measurement: RoomMeasurementState::default(),
            media_tools_service: MediaToolsService::new(),
            media_tools: crate::modals::MediaToolsModalState::default(),

            playlist_editor_tracks: Vec::new(),
            prev_track_id: None,
            prev_playing: false,

            watchers: Vec::new(),
            watch_rx,
            watch_tx,
            watch_pending: None,
            scan_queue: std::collections::VecDeque::new(),
            scan_toast_wanted: false,
            wake_ctx: Some(_cc.egui_ctx.clone()),
            pending_playlist_export: None,
            duplicates: DuplicatesState::default(),
            organize: OrganizeState::default(),
            watch_suppress_until: None,
            clap_refresh_frames: 0,
            #[cfg(feature = "eval-tools")]
            clap_eval: None,
            toasts: crate::components::toasts::ToastState::default(),
            window_size: (1280.0, 800.0),
            normal_window_size: (1280.0, 800.0),
            mini_applied: false,
        };

        // Load all persistent settings (normal, theme, EQ, DSP)
        app.load_persisted_settings();
        app.biography_service
            .set_source_dir(&app.dsp_state.settings_tab.biography_source_dir);
        app.crossfade_preview.set_preview(
            app.dsp_state.settings_tab.crossfade_curve,
            app.dsp_state.settings_tab.crossfade_duration_secs,
        );
        app.podcast_state.reload_feeds(&app.podcast_repo);

        app.reload_library();

        // Keep the library fresh: rebuild watchers for the saved roots and run a
        // catch-up scan so files added while the app was closed are picked up.
        app.rebuild_watchers();
        app.schedule_startup_scan();

        app
    }
}

impl eframe::App for ShadowPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.prepare_frame(&ctx, frame);
        let is_playing = self.poll_services(&ctx);
        self.eval_hook(&ctx);
        self.handle_dropped_files(&ctx);
        // 6. Global Keyboard Shortcuts
        self.handle_global_shortcuts(&ctx);

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
            // First play decodes the file once and caches the peaks in the
            // library database; later plays are a cache hit.
            self.waveform_service.ensure_loaded(t);
        }

        let (active_lyrics, plain_lyrics) = current_track
            .as_ref()
            .map(|t| self.lyrics_service.get_lyrics(&t.id))
            .unwrap_or_default();
        let active_lyric_idx =
            LyricsService::active_index(&active_lyrics, telemetry.position_secs as f32);
        let lyrics_source = current_track
            .as_ref()
            .and_then(|t| self.lyrics_service.source(&t.id));

        let playback = render::PlaybackView {
            current_track: current_track.as_ref(),
            telemetry: &telemetry,
            is_playing,
            active_lyrics: &active_lyrics,
            plain_lyrics: plain_lyrics.as_deref(),
            lyrics_source,
            active_lyric_idx,
        };

        if self.render_now_playing_full(ui, &ctx, &playback) {
            self.persist_settings_if_dirty();
            return;
        }
        if self.render_mini_player(ui, &playback) {
            self.persist_settings_if_dirty();
            return;
        }

        let window_w = ctx.content_rect().width();
        self.render_main_layout(ui, &ctx, window_w, &playback);

        self.render_modals(&ctx, playback.current_track);
        self.persist_settings_if_dirty();
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if !eval_holds_persistence() {
            self.save_persisted_settings();
        }
    }
}

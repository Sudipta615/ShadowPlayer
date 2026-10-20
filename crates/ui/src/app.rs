use std::path::PathBuf;
use std::sync::Arc;

use audio_session::AudioSession;
use database::{
    AlbumRepo, ArtistRepo, Database, FavoritesRepo, GenreRepo, HistoryRepo, PlaylistRepo,
    ScanRootsRepo, SettingsRepo, SmartPlaylistRepo, TrackRepo,
};
use domain::models::Track;
use eframe::egui::{self, Color32, Margin, Pos2, Rect, Sense, Vec2};
use playback::PlaybackCoordinator;
use search::SearchEngine;

use crate::components::clap_rack_panel::show_clap_rack_panel;
use crate::components::command_palette::{PaletteAction, PaletteEntry};
use crate::components::mini_player::MiniPlayer;
use crate::components::sidebar::NavItem;
use crate::components::topbar::{render_action_button, TopbarAction};
use crate::layout::{render_right_drawer, render_sidebar};
use crate::modals::{
    show_autoeq_modal, show_bit_perfect_modal, show_duplicates_modal, show_edit_track_modal,
    show_eq_panel, show_organize_modal, show_playlist_editor, show_smart_playlist_editor,
    show_track_properties_modal, DuplicateGroup, DuplicatesState, OrganizeState,
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

/// Fixture titles for the synthetic library rows.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_TITLES: [&str; 6] = [
    "Weightless",
    "An Ending (Ascent)",
    "Nightcall",
    "Teardrop",
    "Xtal",
    "Halcyon Drift",
];
/// Fixture artists.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_ARTISTS: [&str; 3] = ["Marconi Union", "Neon Harbour", "Aurelia Vox"];
/// Fixture albums.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_ALBUMS: [&str; 2] = ["Ambient Transmissions", "Low Orbit"];
/// Fixture sample rates, so the Hi-Res tag has something to mark.
#[cfg(feature = "eval-tools")]
const EVAL_SAMPLE_RATES: [u32; 4] = [44100, 48000, 96000, 192000];

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
    /// The theme variant currently installed in the egui context. `None` means
    /// no style has been applied yet; the app only rebuilds egui's style when
    /// this differs from `theme.is_light`.
    applied_theme: Option<bool>,
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
    /// Whether the next completed scan deserves a notification (user- or
    /// watcher-initiated) or should stay silent (startup catch-up).
    scan_toast_wanted: bool,
    /// Context used to wake the idle UI loop when a watched file changes.
    wake_ctx: Option<egui::Context>,
    /// Playlist staged for M3U export, written once the save dialog returns.
    pending_m3u_export: Option<(String, Vec<library::M3uEntry>)>,
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

/// The `SHADOWPLAYER_EVAL_CLAP` script's progress.
///
/// The engine answers on its own thread, so the script cannot be a straight
/// line: each step asserts on what the previous one asked for and then asks for
/// the next thing. Publishing the steps rather than sleeping keeps the eval
/// honest about what the UI actually observes. Development only: see the
/// `eval-tools` feature.
#[cfg(feature = "eval-tools")]
struct ClapEval {
    mode: String,
    library: String,
    step: u8,
    /// The state blob the preset captured, for the log.
    saved_state: Option<Vec<u8>>,
    /// Whether the diagnostic dump has been printed yet.
    logged: bool,
}

impl ShadowPlayerApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        session: Arc<AudioSession>,
        db: Database,
    ) -> Self {
        egui_extras::install_image_loaders(&_cc.egui_ctx);
        crate::theme::install_fonts(&_cc.egui_ctx);

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
            applied_theme: None,
            settings_dirty: false,

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
            pending_m3u_export: None,
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
        let roots: Vec<PathBuf> = self
            .library_state
            .scan_roots
            .iter()
            .map(|r| r.path.clone())
            .collect();
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
        // A batch organize moves files itself. Those changes are already
        // reflected in the library database, so re-scanning would be pure
        // duplicate work — and the events trail the move by a moment, so a
        // window is needed rather than a single drain.
        if let Some(until) = self.watch_suppress_until {
            if std::time::Instant::now() < until {
                while self.watch_rx.try_recv().is_ok() {}
                self.watch_pending = None;
                return;
            }
            self.watch_suppress_until = None;
        }

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
            self.scan_toast_wanted = true;
            self.pump_scan_queue();
        }
    }

    /// Close whatever dialog is in front, innermost first.
    ///
    /// Escape used to close only four of the thirteen overlays (search, help,
    /// new playlist, track properties, EQ) and left the rest — bit-perfect,
    /// AutoEq, the playlist and smart-playlist editors, duplicates, organize,
    /// the plugin rack — open with no keyboard way out. One press closes one
    /// layer, so Escape never tears down the whole UI at once.
    fn dismiss_foreground_overlay(&mut self) {
        // Editors and dialogs that were opened deliberately, newest concept
        // first.
        if self.dsp_state.autoeq.is_open {
            self.dsp_state.autoeq.is_open = false;
        } else if self.app_state.bit_perfect_modal_open {
            self.app_state.bit_perfect_modal_open = false;
        } else if self.duplicates.is_open {
            self.duplicates.is_open = false;
        } else if self.organize.is_open {
            self.organize.is_open = false;
        } else if self.app_state.edit_track.is_open {
            self.app_state.edit_track.is_open = false;
        } else if self.smart_playlist_editor.is_open {
            self.smart_playlist_editor.is_open = false;
        } else if self.app_state.playlist_editor.is_open {
            self.app_state.playlist_editor.is_open = false;
        } else if self.app_state.track_properties_open {
            self.app_state.track_properties_open = false;
        } else if self.app_state.open_playlist_editor {
            self.app_state.open_playlist_editor = false;
        } else if self.dsp_state.clap_rack.is_open {
            self.dsp_state.clap_rack.is_open = false;
        } else if self.dsp_state.eq_panel.is_open {
            self.dsp_state.eq_panel.is_open = false;
        } else if self.app_state.right_overlay_open {
            self.app_state.right_overlay_open = false;
        } else if self.app_state.command_palette_open {
            self.app_state.command_palette_open = false;
        } else if !self.app_state.search_query.is_empty() {
            self.app_state.search_query.clear();
            self.refresh_view();
        }
    }

    /// Handle the app-wide keyboard shortcuts.
    ///
    /// Two subtleties shape this function, and both were wrong before:
    ///
    /// * `InputState::focused` is the *native window* focus — whether the OS is
    ///   delivering key presses to this window — not "a widget has focus".
    ///   Guarding on `!i.focused` therefore ran these bindings only while the
    ///   window was in the background, where the OS delivers no key events at
    ///   all. Space/K, J, L, M, S, R and the four arrows could never fire.
    /// * A focused text field owns the keyboard, and egui routes Space/Enter
    ///   to whatever widget holds focus. Both have to be left alone, or typing
    ///   "moon" into the search box would mute the player.
    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        let text_focused = ctx.text_edit_focused();
        let widget_focused = ctx.memory(|m| m.focused()).is_some();

        ctx.input(|i| {
            let ctrl = i.modifiers.command || i.modifiers.ctrl;

            // Ctrl/Cmd chords are explicit app commands, and none of them is a
            // key a text field wants, so they stay live even while a field has
            // focus.
            if ctrl {
                if i.key_pressed(egui::Key::N) {
                    self.app_state.open_playlist_editor = true;
                }
                if i.key_pressed(egui::Key::B) {
                    self.toggle_bit_perfect();
                }
                if i.key_pressed(egui::Key::P) {
                    self.app_state.active_nav = NavItem::Settings;
                }
                if i.key_pressed(egui::Key::O) {
                    self.app_state.active_nav = NavItem::Folders;
                }
                if i.key_pressed(egui::Key::E) {
                    self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
                }
                if i.key_pressed(egui::Key::F) {
                    self.request_search_focus();
                }
                if i.key_pressed(egui::Key::Space) {
                    self.app_state.command_palette_open = true;
                }
            }

            if i.key_pressed(egui::Key::Escape) {
                // The info popup is the topmost transient surface, so it takes
                // the press first: one Escape must not close it *and* the
                // layer behind it.
                if crate::components::info_box::dismiss_open_info_box(ctx) {
                    return;
                }
                self.dismiss_foreground_overlay();
            }

            // The plain transport keys. Ctrl/Cmd is excluded so `Ctrl+M` and
            // friends stay available to the OS, and Shift/Alt are excluded so
            // they keep their usual text-selection and window-management jobs.
            if ctrl || i.modifiers.shift || i.modifiers.alt || text_focused {
                return;
            }

            // Space also activates a focused widget (egui synthesises a click
            // for it), so leave it alone when something else holds focus.
            if !widget_focused && (i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::K)) {
                self.coordinator.toggle_play_pause();
            }
            if i.key_pressed(egui::Key::J) {
                self.coordinator.previous();
            }
            if i.key_pressed(egui::Key::Slash) {
                self.request_search_focus();
            }
            if i.key_pressed(egui::Key::L) {
                self.coordinator.next();
            }
            if i.key_pressed(egui::Key::M) {
                self.app_state.is_muted = !self.app_state.is_muted;
                let target_vol = if self.app_state.is_muted {
                    0.0
                } else {
                    self.app_state.volume
                };
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
        });
    }

    /// Flip the bit-perfect "Pure Direct" mode, bypassing the whole DSP chain.
    ///
    /// Enabling it also neutralises every stage that would alter the signal, so
    /// the toggle is the same in both directions.
    fn toggle_bit_perfect(&mut self) {
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

    /// Everything the command palette can reach.
    ///
    /// The static entries are cheap and always present. Tracks are only
    /// matched when there is a query, and only the best few are turned into
    /// entries, so a ten-thousand-track library does not allocate ten
    /// thousand rows on every keystroke.
    fn command_palette_entries(&self) -> Vec<PaletteEntry> {
        use crate::components::command_palette::fuzzy_score;

        let mut entries: Vec<PaletteEntry> = Vec::new();

        // ── Navigation ─────────────────────────────────────────────────────
        let nav: [(&str, NavItem, &str); 9] = [
            ("Go to", NavItem::Home, "Home"),
            ("Go to", NavItem::Albums, "Albums"),
            ("Go to", NavItem::Artists, "Artists"),
            ("Go to", NavItem::Genres, "Genres"),
            ("Go to", NavItem::Folders, "Folders & library manager"),
            ("Go to", NavItem::Favorites, "Favorites"),
            ("Go to", NavItem::RecentlyPlayed, "Recently played"),
            ("Go to", NavItem::MostPlayed, "Most played"),
            ("Go to", NavItem::RecentlyAdded, "Recently added"),
        ];
        for (category, item, title) in nav {
            entries.push(PaletteEntry::new(
                category,
                title,
                "",
                PaletteAction::Nav(item),
            ));
        }
        entries.push(PaletteEntry::new(
            "Go to",
            "Settings",
            "",
            PaletteAction::Nav(NavItem::Settings),
        ));
        entries.push(PaletteEntry::new(
            "Go to",
            "Help & manual",
            "",
            PaletteAction::Nav(NavItem::Help),
        ));

        // ── Playlists ─────────────────────────────────────────────────────
        for pl in &self.library_state.playlists {
            entries.push(PaletteEntry::new(
                "Playlist",
                pl.name.clone(),
                format!("{} tracks", pl.track_count),
                PaletteAction::Nav(NavItem::Playlist(pl.id)),
            ));
        }
        for sp in &self.library_state.smart_playlists {
            entries.push(PaletteEntry::new(
                "Smart playlist",
                sp.name.clone(),
                "rule-based",
                PaletteAction::Nav(NavItem::SmartPlaylist(sp.id)),
            ));
        }

        // ── Toggles and panels ─────────────────────────────────────────────
        entries.push(
            PaletteEntry::new(
                "Toggle",
                "Light / dark theme",
                "",
                PaletteAction::ToggleTheme,
            )
            .with_shortcut("Ctrl+Space"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Shuffle", "", PaletteAction::ToggleShuffle)
                .with_shortcut("S"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Repeat mode", "", PaletteAction::ToggleRepeat)
                .with_shortcut("R"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Mute", "", PaletteAction::ToggleMute).with_shortcut("M"),
        );
        entries.push(
            PaletteEntry::new(
                "Toggle",
                "Bit-perfect direct stream",
                "",
                PaletteAction::ToggleBitPerfect,
            )
            .with_shortcut("Ctrl+B"),
        );
        entries.push(
            PaletteEntry::new(
                "Panel",
                "DSP studio",
                "equalizer, dynamics and filters",
                PaletteAction::OpenEq,
            )
            .with_shortcut("Ctrl+E"),
        );
        entries.push(
            PaletteEntry::new(
                "Panel",
                "Playlist studio",
                "create and curate playlists",
                PaletteAction::OpenPlaylistEditor,
            )
            .with_shortcut("Ctrl+N"),
        );
        entries.push(PaletteEntry::new(
            "Panel",
            "AutoEQ calibration",
            "headphone and IEM correction",
            PaletteAction::OpenAutoEq,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Bit-perfect inspector",
            "what the signal path is doing",
            PaletteAction::OpenBitPerfect,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Loudness inspector",
            "how loud this track is",
            PaletteAction::OpenLoudness,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Duplicate tracks",
            "find identical audio",
            PaletteAction::OpenDuplicates,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Organize files",
            "batch rename and move",
            PaletteAction::OpenOrganizer,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Import songs",
            "choose files",
            PaletteAction::ImportFiles,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Import a folder",
            "scan a music folder",
            PaletteAction::ImportFolder,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Search the library",
            "",
            PaletteAction::FocusSearch,
        ));

        // ── Tracks ─────────────────────────────────────────────────────────
        // Scored against the raw titles first, so only the best few are ever
        // turned into rows.
        let query = self.app_state.command_palette.query.trim();
        if !query.is_empty() {
            let mut scored: Vec<(i32, &Track)> = self
                .library_state
                .all_tracks
                .iter()
                .filter_map(|tr| fuzzy_score(&tr.title, query).map(|s| (s, tr)))
                .collect();
            scored.sort_by_key(|a| std::cmp::Reverse(a.0));
            for (_, tr) in scored.into_iter().take(24) {
                let hint = if tr.artist.is_empty() {
                    tr.album.clone()
                } else {
                    format!("{} — {}", tr.artist, tr.album)
                };
                entries.push(PaletteEntry::new(
                    "Track",
                    tr.title.clone(),
                    hint,
                    PaletteAction::PlayTrack(tr.id),
                ));
            }
        }

        entries
    }

    /// Act on a command palette choice.
    fn run_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::Nav(item) => {
                self.app_state.active_nav = item;
                self.library_state.selected_album = None;
                self.library_state.selected_artist = None;
                self.library_state.selected_genre = None;
                self.library_state.selected_folder = None;
                self.refresh_view();
            }
            PaletteAction::ToggleTheme => {
                self.theme.toggle();
                self.settings_dirty = true;
            }
            PaletteAction::ToggleShuffle => {
                let new_shuffle = !self.coordinator.is_shuffle();
                self.coordinator.set_shuffle(new_shuffle);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleRepeat => {
                let next_mode = match self.coordinator.repeat_mode() {
                    domain::models::RepeatModeChoice::Off => domain::models::RepeatModeChoice::All,
                    domain::models::RepeatModeChoice::All => domain::models::RepeatModeChoice::One,
                    domain::models::RepeatModeChoice::One => domain::models::RepeatModeChoice::Off,
                };
                self.coordinator.set_repeat_mode(next_mode);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleMute => {
                self.app_state.is_muted = !self.app_state.is_muted;
                let target_vol = if self.app_state.is_muted {
                    0.0
                } else {
                    self.app_state.volume
                };
                self.session.set_volume(target_vol);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleBitPerfect => {
                self.toggle_bit_perfect();
                self.settings_dirty = true;
            }
            PaletteAction::OpenEq => {
                self.dsp_state.eq_panel.is_open = true;
            }
            PaletteAction::OpenPlaylistEditor => {
                self.app_state.open_playlist_editor = true;
            }
            PaletteAction::OpenAutoEq => {
                self.dsp_state.autoeq.is_open = true;
            }
            PaletteAction::OpenBitPerfect => {
                self.app_state.bit_perfect_modal_open = true;
            }
            PaletteAction::OpenLoudness => {
                self.open_loudness_inspector();
            }
            PaletteAction::OpenDuplicates => {
                self.scan_duplicates();
            }
            PaletteAction::OpenOrganizer => {
                self.open_organize(
                    self.library_state.displayed_tracks.clone(),
                    "the current view".to_string(),
                );
            }
            PaletteAction::OpenFolders => {
                self.app_state.active_nav = NavItem::Folders;
                self.refresh_view();
            }
            PaletteAction::ImportFiles => {
                self.request_import(crate::services::ImportKind::Files);
            }
            PaletteAction::ImportFolder => {
                self.request_import(crate::services::ImportKind::Folder);
            }
            PaletteAction::FocusSearch => {
                self.request_search_focus();
            }
            PaletteAction::ClosePalette => {
                self.app_state.command_palette_open = false;
            }
            PaletteAction::PlayTrack(id) => {
                if let Some(track) = self
                    .library_state
                    .all_tracks
                    .iter()
                    .find(|t| t.id == id)
                    .cloned()
                {
                    self.coordinator.play_track(track, None);
                }
            }
        }
    }

    /// Open the loudness inspector for the current track.
    ///
    /// The album's other tracks are captured here rather than per frame, so
    /// the inspector can show how the track sits inside its album without
    /// scanning the library on every redraw.
    fn open_loudness_inspector(&mut self) {
        let Some(current) = self.coordinator.current_track().cloned() else {
            return;
        };
        let album = current.album.clone();
        let artist = current.artist.clone();
        self.app_state.loudness_modal.album_tracks = if album.is_empty() {
            Vec::new()
        } else {
            self.library_state
                .all_tracks
                .iter()
                .filter(|t| t.album == album && t.artist == artist)
                .cloned()
                .collect()
        };
        self.app_state.loudness_modal.is_open = true;
    }

    /// Move the keyboard focus to the search field.
    ///
    /// The Settings and Help views have no search box, so the shortcut returns
    /// to the library first and focuses the field on the next frame.
    fn request_search_focus(&mut self) {
        if matches!(self.app_state.active_nav, NavItem::Help | NavItem::Settings) {
            self.app_state.active_nav = NavItem::Home;
        }
        self.app_state.focus_search = true;
    }

    pub fn load_persisted_settings(&mut self) {
        if let Ok(app_settings) = self.settings_repo.load() {
            self.theme = if app_settings.theme_dark {
                Theme::dark()
            } else {
                Theme::light()
            };
            self.app_state.volume = app_settings.volume;
            self.app_state.is_muted = app_settings.is_muted;
            self.app_state.current_sort = app_settings.sort_field;
            self.app_state.current_sort_order = app_settings.sort_order;
            self.app_state.is_grid_view = app_settings.view_mode == domain::models::ViewMode::Grid;
            self.window_size = (
                app_settings.window_width.max(640.0),
                app_settings.window_height.max(360.0),
            );
            self.coordinator.set_shuffle(app_settings.shuffle);
            self.coordinator.set_repeat_mode(app_settings.repeat_mode);
            self.dsp_state.settings_tab.bit_perfect = app_settings.bit_perfect_enabled;
            self.dsp_state.settings_tab.audio_profile = app_settings.audio_profile;
            if let Some(device) = &app_settings.selected_device {
                self.dsp_state.settings_tab.output_device = device.clone();
            }

            let eff_vol = if app_settings.is_muted {
                0.0
            } else {
                app_settings.volume
            };
            self.session.set_volume(eff_vol);
        }

        // 2. Persistent Eq Settings (EQ layout, bands, gains, preamp, tone controls, custom presets, window position)
        if let Ok(Some(json)) = self.settings_repo.get_value("eq_settings") {
            if let Ok(saved_eq) =
                serde_json::from_str::<crate::components::eq_panel::PersistentEqState>(&json)
            {
                saved_eq.apply_to(&mut self.dsp_state.eq_panel);
            }
        }

        // 3. Persistent Audio / DSP Settings (Speed, pitch, crossfade, spatial acoustics, dac config)
        if let Ok(Some(json)) = self.settings_repo.get_value("dsp_settings") {
            match serde_json::from_str::<crate::components::settings_tab::SettingsTabState>(&json) {
                Ok(saved) => {
                    // Presets and the live CLAP summary are skipped by serde:
                    // they keep their own storage key / are refreshed each
                    // frame.
                    let clap = std::mem::take(&mut self.dsp_state.settings_tab.clap);
                    self.dsp_state.settings_tab = saved;
                    self.dsp_state.settings_tab.clap = clap;
                }
                Err(e) => {
                    log::warn!("Could not read the saved audio settings, using defaults: {e}");
                }
            }
        }

        // 4. Directories of individually imported song files (shown in Settings)
        if let Ok(Some(json)) = self.settings_repo.get_value("imported_file_dirs") {
            if let Ok(dirs) = serde_json::from_str::<Vec<PathBuf>>(&json) {
                self.app_state.imported_dirs = dirs;
            }
        }

        // 5. Whole-chain DSP presets. Stored separately from the live EQ/DSP
        // state: a preset outlives the settings it was captured from.
        if let Ok(Some(json)) = self.settings_repo.get_value("chain_presets") {
            match serde_json::from_str::<Vec<crate::state::ChainPreset>>(&json) {
                Ok(presets) => self.dsp_state.settings_tab.chain_presets = presets,
                Err(e) => log::warn!("Ignoring unreadable chain presets: {}", e),
            }
        }

        // 6. Batch-organizer preferences (last pattern and destination).
        if let Ok(Some(json)) = self.settings_repo.get_value("organize_prefs") {
            match serde_json::from_str::<crate::modals::OrganizePrefs>(&json) {
                Ok(prefs) => {
                    self.organize.pattern = prefs.pattern;
                    self.organize.destination = prefs.destination;
                }
                Err(e) => log::warn!("Ignoring unreadable organizer preferences: {e}"),
            }
        }

        // 7. CLAP plugin rack presets. The plugin itself is restored by the
        // engine (it owns the insert); what lives here is the named snapshots
        // the user saved, which the engine knows nothing about.
        if let Ok(Some(json)) = self.settings_repo.get_value("clap_rack") {
            match serde_json::from_str::<crate::state::clap_rack::ClapRackPrefs>(&json) {
                Ok(prefs) => {
                    self.dsp_state.clap_rack.presets = prefs.presets;
                    self.dsp_state.clap_rack.last_library = prefs.last_library;
                }
                Err(e) => log::warn!("Ignoring unreadable CLAP rack presets: {e}"),
            }
        }

        // Sync restored state to audio session
        self.dsp_state.sync_all_to_session(&self.session);

        // The engine rebuilds the DSP graph on its first tick, so a plugin
        // restored from the persisted engine config only becomes visible after
        // that. Ask for the rack view for a few frames rather than once, so the
        // answer does not depend on which frame the graph happened to be built
        // in.
        self.clap_refresh_frames = CLAP_STARTUP_REFRESHES;
    }

    pub fn save_persisted_settings(&self) {
        // 1. Save AppSettings
        let app_settings = domain::models::AppSettings {
            theme_dark: !self.theme.is_light,
            volume: self.app_state.volume,
            is_muted: self.app_state.is_muted,
            sort_field: self.app_state.current_sort,
            sort_order: self.app_state.current_sort_order,
            view_mode: if self.app_state.is_grid_view {
                domain::models::ViewMode::Grid
            } else {
                domain::models::ViewMode::List
            },
            audio_profile: self.dsp_state.settings_tab.audio_profile,
            repeat_mode: self.coordinator.repeat_mode(),
            shuffle: self.coordinator.is_shuffle(),
            selected_device: if self.dsp_state.settings_tab.output_device.is_empty() {
                None
            } else {
                Some(self.dsp_state.settings_tab.output_device.clone())
            },
            bit_perfect_enabled: self.dsp_state.settings_tab.bit_perfect,
            // Read from the live window, not a fixed pair of numbers: the value
            // written here is what the next launch restores.
            window_width: if self.app_state.mini_player {
                self.normal_window_size.0
            } else {
                self.window_size.0
            },
            window_height: if self.app_state.mini_player {
                self.normal_window_size.1
            } else {
                self.window_size.1
            },
        };
        if let Err(e) = self.settings_repo.save(&app_settings) {
            log::warn!("Could not persist app settings: {e}");
        }

        // 2. Save EQ Settings
        let eq_persisted =
            crate::components::eq_panel::PersistentEqState::from(&self.dsp_state.eq_panel);
        if let Ok(json) = serde_json::to_string(&eq_persisted) {
            if let Err(e) = self.settings_repo.set_value("eq_settings", &json) {
                log::warn!("Could not save the EQ settings: {e}");
            }
        }

        // 3. Save DSP & Settings Tab State
        match serde_json::to_string(&self.dsp_state.settings_tab) {
            Ok(json) => {
                if let Err(e) = self.settings_repo.set_value("dsp_settings", &json) {
                    log::warn!("Could not save the audio settings: {e}");
                }
            }
            Err(e) => log::warn!("Could not serialize the audio settings: {e}"),
        }

        // 4. Save imported-file directory list
        if let Ok(json) = serde_json::to_string(&self.app_state.imported_dirs) {
            if let Err(e) = self.settings_repo.set_value("imported_file_dirs", &json) {
                log::warn!("Could not save the imported-file list: {e}");
            }
        }

        // 5. Save whole-chain DSP presets
        if let Ok(json) = serde_json::to_string(&self.dsp_state.settings_tab.chain_presets) {
            if let Err(e) = self.settings_repo.set_value("chain_presets", &json) {
                log::warn!("Could not save the chain presets: {e}");
            }
        }

        // 6. Save batch-organizer preferences
        let organize_prefs = crate::modals::OrganizePrefs {
            pattern: self.organize.pattern.clone(),
            destination: self.organize.destination.clone(),
        };
        if let Ok(json) = serde_json::to_string(&organize_prefs) {
            if let Err(e) = self.settings_repo.set_value("organize_prefs", &json) {
                log::warn!("Could not save the organizer preferences: {e}");
            }
        }

        // 7. Save CLAP rack presets
        let clap_prefs = crate::state::clap_rack::ClapRackPrefs {
            presets: self.dsp_state.clap_rack.presets.clone(),
            last_library: self.dsp_state.clap_rack.last_library.clone(),
        };
        if let Ok(json) = serde_json::to_string(&clap_prefs) {
            if let Err(e) = self.settings_repo.set_value("clap_rack", &json) {
                log::warn!("Could not save the CLAP rack presets: {e}");
            }
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
                    if imported > 0 {
                        self.toasts
                            .success(format!("Imported {imported} track(s) into the library"));
                    } else {
                        self.toasts
                            .warning("Nothing was imported — those files are not supported audio");
                    }
                }
            }
            ImportResult::Playlist(path) => {
                self.import_m3u_playlist(&path);
            }
            ImportResult::SavePath(path) => {
                self.write_pending_m3u_export(&path);
            }
            ImportResult::OrganizeDestination(path) => {
                self.organize.set_destination(Some(path));
            }
            ImportResult::ClapLibrary(path) => {
                self.open_clap_library(path);
            }
        }
    }

    /// Probe a chosen CLAP library and, when it holds a single plugin, load it.
    ///
    /// The probe has to happen even in the single-plugin case: until the file
    /// has been read there is no id to load by, and a library that holds
    /// several plugins needs the user to pick one from the rack.
    fn open_clap_library(&mut self, path: std::path::PathBuf) {
        let display = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        self.dsp_state.clap_rack.probe_path = Some(path.to_string_lossy().to_string());
        self.dsp_state.clap_rack.last_library = path.to_string_lossy().to_string();
        self.dsp_state.clap_rack.pending_autoload_done = false;
        self.dsp_state.clap_rack.is_open = true;
        self.dsp_state
            .clap_rack
            .announce(format!("Reading {display}…"));
        self.session.probe_clap_library(&path);
    }

    /// Load the plugin a probe turned up, once that probe has answered.
    ///
    /// A library holding exactly one plugin needs no decision from the user, so
    /// it loads on its own; several leave the choice to the rack, which lists
    /// them.
    fn autoload_probed_clap_plugin(&mut self) {
        let Some(expected) = self.dsp_state.clap_rack.probe_path.clone() else {
            return;
        };
        let view = self.session.clap_view();
        let Some(library) = &view.library else {
            return;
        };
        if library.path != expected || view.rack.is_loaded() {
            return;
        }
        // Already handled this probe; wait for the user to choose.
        if self.dsp_state.clap_rack.pending_autoload_done {
            return;
        }
        if let Some(error) = &library.error {
            self.dsp_state
                .clap_rack
                .announce(format!("Could not read {expected}: {error}"));
            self.dsp_state.clap_rack.pending_autoload_done = true;
            return;
        }
        match library.plugins.len() {
            0 => {
                self.dsp_state.clap_rack.notice =
                    Some(format!("{expected} contains no CLAP plugins"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
            1 => {
                let id = library.plugins[0].id.clone();
                let name = library.plugins[0].name.clone();
                self.session.load_clap_plugin(&expected, Some(&id));
                self.dsp_state
                    .clap_rack
                    .announce(format!("Loading {name}…"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
            count => {
                self.dsp_state.clap_rack.notice =
                    Some(format!("{count} plugins found — choose one below"));
                self.dsp_state.clap_rack.pending_autoload_done = true;
            }
        }
    }

    /// Drive `SHADOWPLAYER_EVAL_CLAP` one step per frame.
    ///
    /// `=load` exercises probe, load, enumeration and parameter plumbing;
    /// `=preset` also captures a named preset and recalls it. Either way it goes
    /// through the same code the UI uses, so what it proves is what a user gets.
    ///
    /// The library defaults to the in-repo fixture plugin, which is the only
    /// CLAP plugin this machine has. `SHADOWPLAYER_EVAL_CLAP_LIB` names another.
    #[cfg(feature = "eval-tools")]
    fn drive_clap_eval(&mut self) {
        if self.clap_eval.is_none() {
            let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_CLAP") else {
                return;
            };
            let library = std::env::var("SHADOWPLAYER_EVAL_CLAP_LIB").unwrap_or_else(|_| {
                format!(
                    "{}/../../target/debug/libclap_fixture_plugin.so",
                    env!("CARGO_MANIFEST_DIR")
                )
            });
            log::info!("CLAP EVAL: mode {mode}, library {library}");
            self.clap_eval = Some(ClapEval {
                mode,
                library,
                step: 0,
                saved_state: None,
                logged: false,
            });
        }

        let (mode, step, library) = {
            let eval = self.clap_eval.as_ref().expect("set just above");
            (eval.mode.clone(), eval.step, eval.library.clone())
        };
        let view = self.session.clap_view();

        match step {
            0 => {
                // A settings screenshot wants the card in its page, not a window
                // over it.
                self.dsp_state.clap_rack.is_open =
                    std::env::var("SHADOWPLAYER_EVAL_CLAP_NO_WINDOW").is_err();
                self.dsp_state.clap_rack.probe_path = Some(library.clone());
                // The app asks for a few refreshes at startup so a plugin
                // restored from config becomes visible. A refresh republishes
                // the plugin's own values, which would race this script's
                // parameter step, so the eval stops it.
                self.clap_refresh_frames = 0;
                self.session.probe_clap_library(&library);
                log::info!("CLAP EVAL [0] probing {library}");
                self.clap_eval_advance(1);
            }
            1 => {
                let Some(probe) = view.library.as_ref() else {
                    return;
                };
                if probe.path != library {
                    return;
                }
                assert!(
                    probe.error.is_none(),
                    "probe of {library} failed: {:?}",
                    probe.error
                );
                let listed: Vec<String> = probe
                    .plugins
                    .iter()
                    .map(|p| format!("{} [{}]", p.name, p.id))
                    .collect();
                log::info!(
                    "CLAP EVAL [1] {} plugin(s): {}",
                    probe.plugins.len(),
                    listed.join(", ")
                );
                assert_eq!(
                    probe.plugins.len(),
                    2,
                    "the fixture library holds two plugins"
                );
                assert!(
                    probe
                        .plugins
                        .iter()
                        .any(|p| p.id == "com.shadowplayer.fixture.gain"),
                    "the gain fixture is listed"
                );
                assert!(
                    probe
                        .plugins
                        .iter()
                        .any(|p| p.id == "com.shadowplayer.fixture.trim"),
                    "the trim fixture is listed"
                );
                self.dsp_state.clap_rack.notice = Some(format!("Found {}", listed.join(", ")));
                self.session
                    .load_clap_plugin(&library, Some("com.shadowplayer.fixture.gain"));
                self.clap_eval_advance(2);
            }
            2 => {
                if !view.rack.is_loaded() {
                    return;
                }
                let plugin = view.rack.plugin.as_ref().expect("loaded implies a plugin");
                assert_eq!(plugin.id, "com.shadowplayer.fixture.gain");
                assert_eq!(plugin.name, "Fixture Gain");
                assert_eq!(plugin.vendor, "ShadowPlayer");
                log::info!(
                    "CLAP EVAL [2] loaded {} v{} by {}",
                    plugin.name,
                    plugin.version,
                    plugin.vendor
                );
                for param in &view.rack.params {
                    log::info!(
                        "CLAP EVAL [2]   #{} {}/{} = {} ({:?}) in {}..{} stepped={} bypass={}",
                        param.id,
                        param.module,
                        param.name,
                        param.value,
                        param.value_text,
                        param.min,
                        param.max,
                        param.is_stepped,
                        param.is_bypass
                    );
                }
                assert_eq!(
                    view.rack.params.len(),
                    4,
                    "the fixture Gain declares four parameters"
                );
                let by_name = |name: &str| view.rack.params.iter().find(|p| p.name == name);

                // An enumeration renders as its label, not its number: the
                // plugin's `value_to_text` is what a native editor would show.
                let mode_param = by_name("Mode").expect("a Mode parameter");
                assert_eq!(mode_param.value_text, "Clean");
                assert!(mode_param.is_stepped);
                assert_eq!(mode_param.min, 0.0);
                assert_eq!(mode_param.max, 2.0);

                let bypass = by_name("Bypass").expect("a Bypass parameter");
                assert!(bypass.is_bypass, "the fixture flags its bypass parameter");
                assert_eq!(bypass.value_text, "Off");
                assert!(view.rack.has_state, "the fixture implements state");

                let gain = by_name("Gain").expect("a Gain parameter");
                assert_eq!(gain.min, -24.0);
                assert_eq!(gain.max, 24.0);
                assert!((gain.value).abs() < 1e-9, "the default is unity");
                assert_eq!(gain.value_text, "0.00");

                self.session.set_clap_plugin_param(gain.id, 6.0);
                log::info!("CLAP EVAL [2] set Gain to +6.00 dB");
                self.clap_eval_advance(3);
            }
            3 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                log::info!("CLAP EVAL [3] host reports Gain = {}", gain.value);

                if mode != "preset" {
                    log::info!("CLAP EVAL OK: probe, load, enumerate and drive a parameter");
                    self.clap_eval_advance(9);
                    return;
                }

                {
                    let rack = &mut self.dsp_state.clap_rack;
                    rack.preset_name = "Eval 6dB".to_string();
                    rack.preset_note = "captured by the eval hook".to_string();
                }
                self.dsp_state.handle_clap_rack_actions(
                    &self.session,
                    crate::state::clap_rack::ClapRackActions {
                        save_preset: true,
                        ..Default::default()
                    },
                );
                log::info!("CLAP EVAL [3] asked for a preset save");
                self.clap_eval_advance(4);
            }
            4 => {
                // The save completes on the frame the engine's state blob
                // arrives; until then there is nothing to assert.
                if self.dsp_state.clap_rack.pending_save.is_some() {
                    return;
                }
                let preset = self
                    .dsp_state
                    .clap_rack
                    .presets
                    .first()
                    .cloned()
                    .expect("the preset was saved");
                let bytes = preset
                    .state_bytes()
                    .expect("the plugin returned a state blob");
                assert!(!bytes.is_empty(), "an empty blob is not state");
                log::info!(
                    "CLAP EVAL [4] preset {:?} captured {} parameter values and {} state bytes",
                    preset.name,
                    preset.params.len(),
                    bytes.len()
                );
                assert_eq!(preset.params.len(), 4, "every parameter is captured");
                assert_eq!(preset.plugin_id, "com.shadowplayer.fixture.gain");
                if let Some(eval) = self.clap_eval.as_mut() {
                    eval.saved_state = Some(bytes);
                }

                // Move it, so the recall has something to undo.
                self.session.set_clap_plugin_param(0, -12.0);
                log::info!("CLAP EVAL [4] moved Gain to -12.00 dB");
                self.clap_eval_advance(5);
            }
            5 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value + 12.0).abs() > 1e-9 {
                    return;
                }
                log::info!("CLAP EVAL [5] host reports Gain = {}", gain.value);
                self.dsp_state.handle_clap_rack_actions(
                    &self.session,
                    crate::state::clap_rack::ClapRackActions {
                        apply_preset: Some(0),
                        ..Default::default()
                    },
                );
                log::info!("CLAP EVAL [5] recalled the preset");
                self.clap_eval_advance(6);
            }
            6 => {
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                assert!(
                    view.rack.error.is_none(),
                    "the recall reported an error: {:?}",
                    view.rack.error
                );
                log::info!(
                    "CLAP EVAL [6] recall restored Gain = {} ({:?})",
                    gain.value,
                    gain.value_text
                );

                // Re-read from the plugin rather than trusting the host's own
                // cache, so the next step proves the plugin accepted the blob.
                self.session.refresh_clap_plugin();
                self.clap_eval_advance(7);
            }
            7 => {
                if !self.clap_eval.as_ref().is_some_and(|e| e.logged) {
                    let dump: Vec<(String, f64, String)> = view
                        .rack
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.value, p.value_text.clone()))
                        .collect();
                    log::info!(
                        "CLAP EVAL [7] plugin reports, after re-reading: plugin={:?} params={dump:?} error={:?}",
                        view.rack.plugin.as_ref().map(|p| p.name.clone()),
                        view.rack.error
                    );
                    if let Some(eval) = self.clap_eval.as_mut() {
                        eval.logged = true;
                    }
                }
                let Some(gain) = view.rack.params.iter().find(|p| p.name == "Gain") else {
                    return;
                };
                if (gain.value - 6.0).abs() > 1e-9 {
                    return;
                }
                let state_len = self
                    .clap_eval
                    .as_ref()
                    .and_then(|e| e.saved_state.as_ref())
                    .map(|b| b.len())
                    .unwrap_or(0);
                log::info!(
                    "CLAP EVAL OK: probe, load, enumerate, drive, capture ({state_len} bytes) and recall"
                );
                self.dsp_state.clap_rack.notice = Some(format!(
                    "Eval: recalled “Eval 6dB” — {state_len} bytes of state, Gain back at {} ({:?})",
                    gain.value, gain.value_text
                ));
                self.clap_eval_advance(9);
            }
            _ => {}
        }
    }

    /// Move the eval script along. A separate method so the steps above read as
    /// a sequence rather than as borrow-juggling.
    #[cfg(feature = "eval-tools")]
    fn clap_eval_advance(&mut self, step: u8) {
        if let Some(eval) = self.clap_eval.as_mut() {
            eval.step = step;
        }
    }

    /// Import one audio file into the library, resolving stable identity so a
    /// moved-but-already-known file keeps its id, play counts and favorites.
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
        if added == 0 {
            self.toasts
                .warning(format!("'{name}' matched no files in the library"));
        } else {
            self.toasts.success(format!(
                "Opened playlist '{name}' — {added} of {} tracks",
                entries.len()
            ));
        }
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
            Ok(()) => {
                log::info!("Exported {} track(s) to {}", entries.len(), path.display());
                self.toasts.success(format!(
                    "Exported {} track(s) to {}",
                    entries.len(),
                    path.display()
                ));
            }
            Err(e) => {
                log::warn!("Could not write playlist {}: {e}", path.display());
                self.toasts
                    .warning(format!("Could not write playlist: {e}"));
            }
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
            self.app_state.current_sort_order,
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
            self.app_state.current_sort_order,
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

    /// Open the batch organizer for a set of tracks.
    fn open_organize(&mut self, tracks: Vec<domain::models::Track>, scope_label: String) {
        if tracks.is_empty() {
            self.toasts.info("Nothing to organize".to_string());
            return;
        }
        let label = format!(
            "{} · {} track{}",
            scope_label,
            tracks.len(),
            if tracks.len() == 1 { "" } else { "s" }
        );
        self.organize.open(tracks, label);
    }

    /// Move the files in the reviewed plan and relink the library.
    ///
    /// The plan is rebuilt from the board immediately before applying, so what
    /// moves is exactly what is on screen — not what was on screen a keystroke
    /// ago — and any destination that appeared meanwhile is caught as a
    /// conflict rather than silently clobbered.
    fn apply_organize_plan(&mut self) {
        self.organize.plan = library::organize::build_plan(
            &self.organize.tracks,
            &self.organize.pattern,
            self.organize.destination.as_deref(),
        );
        self.organize.plan_dirty = false;

        let plan = self.organize.plan.clone();
        let outcome = library::organize::apply_plan(&plan);

        // Relink each moved row in place: the id, play counts, playlists and
        // artwork all survive a rename.
        let mut relinked = 0usize;
        for (id, new_path, content_hash) in &outcome.moved {
            match self
                .track_repo
                .relink_path(id, new_path, content_hash.as_deref())
            {
                Ok(true) => relinked += 1,
                Ok(false) => log::warn!(
                    "Organizer moved {} but found no library row",
                    new_path.display()
                ),
                Err(e) => log::warn!("Could not relink {}: {e}", new_path.display()),
            }
        }

        // The queue holds resolved paths, so repoint it too.
        let moved: Vec<(domain::ids::TrackId, PathBuf)> = outcome
            .moved
            .iter()
            .map(|(id, path, _)| (*id, path.clone()))
            .collect();
        self.coordinator.remap_paths(&moved);

        // The modal's scope still describes the pre-move world. Point it at the
        // new locations and rebuild, so the preview afterwards says "already in
        // place" instead of claiming the files have vanished.
        for track in self.organize.tracks.iter_mut() {
            if let Some((_, new_path, hash)) =
                outcome.moved.iter().find(|(id, _, _)| *id == track.id)
            {
                track.path = new_path.clone();
                track.content_hash = hash.clone();
            }
        }
        self.organize.force_rebuild();

        // Our own moves must not look like external edits.
        self.watch_suppress_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5));

        let failed = outcome.failures.len();
        if failed > 0 {
            let detail = outcome
                .failures
                .iter()
                .take(3)
                .map(|(path, reason)| format!("{}: {reason}", path.display()))
                .collect::<Vec<_>>()
                .join("; ");
            log::warn!("Organizer: {relinked} moved, {failed} failed — {detail}");
        } else {
            log::info!("Organizer: moved {relinked} file(s)");
        }

        self.organize.result = Some(if failed == 0 {
            format!("Moved {relinked} file(s).")
        } else {
            format!("Moved {relinked}, {failed} left in place — see the log.")
        });
        self.organize.applied = true;

        if relinked > 0 {
            self.toasts.success(format!(
                "Organized {relinked} file(s){}",
                if failed > 0 {
                    format!(" ({failed} skipped)")
                } else {
                    String::new()
                }
            ));
        }
        self.reload_library();
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
        if let Err(e) = self.scan_roots_repo.add(&path) {
            log::warn!("Could not add the scan root {}: {e}", path.display());
        }
        self.library_state.scan_roots = match self.scan_roots_repo.list() {
            Ok(roots) => roots,
            Err(e) => {
                log::warn!("Could not list the scan roots: {e}");
                Vec::new()
            }
        };
        // Watch the new root from now on, and scan it once the scanner is free.
        self.rebuild_watchers();
        if !self.scan_queue.contains(&path) {
            self.scan_queue.push_back(path);
        }
        self.scan_toast_wanted = true;
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
            length_secs: track
                .as_ref()
                .map(|t| t.duration_secs as f64)
                .unwrap_or(0.0),
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

    /// Top-bar queue/theme/mini actions. Returns nothing; each button updates
    /// the state it owns. Kept in one place so Settings and Help get the same
    /// controls as library views.
    fn render_topbar_actions(&mut self, ui: &mut egui::Ui, window_w: f32) {
        ui.spacing_mut().item_spacing.x = 6.0;
        let wide = window_w >= 1100.0;
        let queue_active = if wide {
            self.app_state.right_panel_visible
        } else {
            self.app_state.right_overlay_open
        };
        if render_action_button(
            ui,
            TopbarAction::Queue,
            queue_active,
            &self.theme,
            "Queue & lyrics",
        ) {
            if wide {
                self.app_state.right_panel_visible = !self.app_state.right_panel_visible;
            } else {
                self.app_state.right_overlay_open = !self.app_state.right_overlay_open;
            }
        }
        // The right drawer already carries the sun/moon pill when it is a
        // column. Only duplicate it in the top bar for the narrow layout,
        // where the drawer is an overlay.
        if !wide
            && render_action_button(
                ui,
                TopbarAction::Theme,
                false,
                &self.theme,
                if self.theme.is_light {
                    "Switch to dark theme"
                } else {
                    "Switch to light theme"
                },
            )
        {
            self.theme.toggle();
            self.settings_dirty = true;
        }
        if render_action_button(
            ui,
            TopbarAction::MiniPlayer,
            self.app_state.mini_player,
            &self.theme,
            "Mini player",
        ) {
            self.set_mini_player(!self.app_state.mini_player);
        }
    }

    /// Switch between the normal library shell and the compact mini player.
    /// The viewport resize itself is applied in the frame loop so it can be
    /// undone cleanly when the user restores the normal window.
    fn set_mini_player(&mut self, enabled: bool) {
        self.app_state.mini_player = enabled;
    }

    /// Opens the native OS picker on a worker thread for files or a folder.
    pub fn request_import(&mut self, kind: ImportKind) {
        match kind {
            ImportKind::Files => self.import_service.request_files(),
            ImportKind::Folder => self.import_service.request_folder(),
            // SaveM3u is driven by `export_playlist_as_m3u`, and the organize
            // destination by the organizer, never from here.
            ImportKind::Playlist => self.import_service.request_playlist(),
            ImportKind::SaveM3u => {}
            ImportKind::OrganizeDestination => {}
            ImportKind::ClapLibrary => self.import_service.request_clap_library(),
        }
    }
}

impl eframe::App for ShadowPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Windows/macOS create their SMTC/Now Playing objects from the HWND
        // (or NSWindow) on the first frame; Linux's MPRIS backend is already
        // running from `new()`.
        self.mpris_service.attach_window(frame);
        if self.applied_theme != Some(self.theme.is_light) {
            self.theme.apply(&ctx);
            self.applied_theme = Some(self.theme.is_light);
        }

        // Remember the live window size: this is what the next launch restores.
        let size = ctx.content_rect().size();
        if size.x > 0.0 && size.y > 0.0 {
            self.window_size = (size.x, size.y);
        }

        // Mini-player viewport transition. The normal geometry is kept aside so
        // restoring never lands at the mini player's 320x110 floor.
        if self.app_state.mini_player && !self.mini_applied {
            self.normal_window_size =
                (self.window_size.0.max(640.0), self.window_size.1.max(360.0));
            self.mini_applied = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(320.0, 110.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(320.0, 110.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
        } else if !self.app_state.mini_player && self.mini_applied {
            self.mini_applied = false;
            let (w, h) = self.normal_window_size;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(640.0, 360.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                w.max(640.0),
                h.max(360.0),
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
            self.settings_dirty = true;
        }

        // 1. Poll Coordinator & Session events
        self.coordinator.poll_events();
        for notice in self.coordinator.take_engine_notices() {
            self.toasts.warning(notice);
        }

        // 2. Poll Library Scanner progress
        if self.scanner_service.poll() {
            self.reload_library();
            // A finished scan may unblock the next queued root.
            self.pump_scan_queue();
            // Only announce scans the user (or a watched folder) asked for;
            // the quiet startup catch-up should not pop a toast on every launch.
            if self.scan_toast_wanted {
                self.scan_toast_wanted = false;
                let msg = self.scanner_service.progress_text().to_string();
                self.toasts.info(msg);
            }
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
            let summary = self.loudness_service.progress().current.clone();
            if !summary.is_empty() {
                self.toasts.info(summary);
            }
        }

        // 2f. Embedded lyric tags are read on a worker thread; pick up the
        // result as soon as it lands so the lyrics panel fills in.
        if self.lyrics_service.poll() {
            ctx.request_repaint();
        }

        // 2g. Chain-preset feedback. Saving or applying a profile also marks
        // the settings dirty so it persists now rather than at the next
        // periodic save.
        if let Some(msg) = self.dsp_state.settings_tab.chain_preset_notice.take() {
            self.toasts.success(msg);
            self.settings_dirty = true;
        }

        // 2h. The Settings > Library card asks for the batch organizer. Raised
        // here because neither settings surface owns app-level modals.
        if std::mem::take(&mut self.dsp_state.settings_tab.request_organize) {
            let tracks = self.library_state.all_tracks.clone();
            self.open_organize(tracks, "Whole library".to_string());
        }
        if std::mem::take(&mut self.dsp_state.settings_tab.request_clap_library) {
            self.request_import(ImportKind::ClapLibrary);
        }

        // 2i. CLAP plugin rack. Three things happen every frame:
        // - a preset save waiting on the plugin's state is completed or timed
        //   out (the engine answers off the frame loop),
        // - the settings card gets the plugin's current description,
        // - a library probe that has answered is acted on: one plugin in the
        //   file loads itself, several wait for the user to choose.
        self.dsp_state.poll_clap_state_capture(&self.session);
        self.dsp_state.refresh_clap_summary(&self.session);
        self.autoload_probed_clap_plugin();
        if self.clap_refresh_frames > 0 {
            self.clap_refresh_frames -= 1;
            self.session.refresh_clap_plugin();
            ctx.request_repaint();
        }
        if let Some(msg) = self.dsp_state.clap_rack.pending_toast.take() {
            self.toasts.info(msg);
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
        } else if !self.toasts.is_empty() {
            // Keep the fade-out animating while a notification is on screen.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.lyrics_service.is_loading() {
            // Waiting on an embedded-lyrics read: tick until it lands, even
            // while paused.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        // 3b. Toast notifications ride above everything else.
        self.toasts.prune();
        self.toasts.show(&ctx, &self.theme);

        // 4. Automated Evaluation Screenshot Hook.
        //
        // Development only: the whole block (and the CLAP eval script it
        // drives) is compiled out unless the `eval-tools` feature is on, so a
        // shipped binary has no `process::exit` reachable from an environment
        // variable.
        self.app_state.frame_count += 1;
        #[cfg(feature = "eval-tools")]
        if std::env::var("SHADOWPLAYER_EVAL").is_ok() {
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_MINI").is_ok() {
                self.app_state.mini_player = true;
            }
            if self.app_state.frame_count == 1
                && std::env::var("SHADOWPLAYER_EVAL_SETTINGS").is_ok()
            {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Settings;
                if let Ok(cat_str) = std::env::var("SHADOWPLAYER_EVAL_SETTINGS_CAT") {
                    match cat_str.as_str() {
                        "spatial" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Spatial
                        }
                        "playback" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Playback
                        }
                        "output" | "hardware" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Output
                        }
                        "library" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Library
                        }
                        "mixbus" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::MixBus
                        }
                        "tools" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Tools
                        }
                        _ => {}
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

            // Chain presets: drive the real save/apply path and verify the whole
            // chain survives the round trip. `SHADOWPLAYER_EVAL_CHAIN=seed`
            // saves profiles and leaves them on screen; `=roundtrip` also wipes
            // the chain, recalls a profile and asserts every field came back.
            if self.app_state.frame_count == 1 {
                if let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_CHAIN") {
                    {
                        let eq = &mut self.dsp_state.eq_panel;
                        eq.eq_enabled = true;
                        eq.graphic_enabled = true;
                        eq.preamp_db = -7.5;
                        eq.bass_shelf_db = 4.0;
                        eq.limiter_enabled = true;
                        eq.crossfeed_enabled = true;
                        eq.crossfeed_profile = config::CrossfeedProfile::Jmeier;
                        eq.bands[2].freq = 315.0;
                        eq.bands[2].gain_db = -5.5;
                        eq.comp_threshold[0] = -28.0;
                        let st = &mut self.dsp_state.settings_tab;
                        st.bass_management.enabled = true;
                        st.bass_management.crossover_hz = 85.0;
                        st.lfe.enabled = true;
                        st.lfe.gain_db = -3.0;
                        st.channel_mix.enabled = true;
                    }

                    for (name, note) in [
                        ("Headphones", "HD650 + crossfeed"),
                        ("Speakers", "2.1 living room"),
                    ] {
                        self.dsp_state.settings_tab.chain_preset_draft_note = note.to_string();
                        let a = crate::components::settings_tab::SettingsTabActions {
                            chain_preset_save: Some(name.to_string()),
                            ..Default::default()
                        };
                        self.dsp_state.handle_settings_actions(&self.session, &a);
                    }

                    if mode == "roundtrip" {
                        // Flatten the chain, then recall the first profile.
                        {
                            let eq = &mut self.dsp_state.eq_panel;
                            eq.eq_enabled = false;
                            eq.graphic_enabled = false;
                            eq.preamp_db = 0.0;
                            eq.bass_shelf_db = 0.0;
                            eq.limiter_enabled = false;
                            eq.crossfeed_enabled = false;
                            eq.bands[2].gain_db = 0.0;
                            eq.comp_threshold[0] = 0.0;
                            let st = &mut self.dsp_state.settings_tab;
                            st.bass_management.enabled = false;
                            st.lfe.enabled = false;
                            st.channel_mix.enabled = false;
                        }
                        let a = crate::components::settings_tab::SettingsTabActions {
                            chain_preset_apply: Some(0),
                            ..Default::default()
                        };
                        self.dsp_state.handle_settings_actions(&self.session, &a);

                        let eq = &self.dsp_state.eq_panel;
                        let st = &self.dsp_state.settings_tab;
                        assert!(eq.eq_enabled, "eq_enabled lost");
                        assert!((eq.preamp_db + 7.5).abs() < 1e-6, "preamp lost");
                        assert!((eq.bass_shelf_db - 4.0).abs() < 1e-6, "bass shelf lost");
                        assert!(eq.limiter_enabled, "limiter lost");
                        assert!(eq.crossfeed_enabled, "crossfeed lost");
                        assert_eq!(eq.crossfeed_profile, config::CrossfeedProfile::Jmeier);
                        assert!((eq.bands[2].gain_db + 5.5).abs() < 1e-6, "EQ band lost");
                        assert!(
                            (eq.comp_threshold[0] + 28.0).abs() < 1e-6,
                            "compressor lost"
                        );
                        assert!(st.bass_management.enabled, "bass management lost");
                        assert!((st.bass_management.crossover_hz - 85.0).abs() < 1e-6);
                        assert!(st.lfe.enabled, "LFE lost");
                        assert!(st.channel_mix.enabled, "channel mix lost");
                        log::info!(
                            "CHAIN PRESET ROUND TRIP OK ({} saved): {}",
                            st.chain_presets.len(),
                            st.chain_presets[0].summary()
                        );
                    }
                }
            }

            // Batch organizer: drive the real preview -> apply -> relink path
            // with real files, so the app-level code (DB relink, queue repoint,
            // watcher suppression) is exercised, not just the library crate.
            // `preview` leaves the modal on screen; `apply` also moves and
            // verifies. Rows are created fresh so their ids are real UUIDs, the
            // same shape the scanner produces.
            if self.app_state.frame_count == 1 {
                if let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_ORGANIZE") {
                    let root = std::path::PathBuf::from("/tmp/opencode/sp_organize");
                    let incoming = root.join("incoming");
                    let library_dir = root.join("library");
                    // Start clean so a re-run sees the same empty destinations.
                    let _ = std::fs::remove_dir_all(&root);
                    std::fs::create_dir_all(&incoming).unwrap();
                    std::fs::create_dir_all(&library_dir).unwrap();

                    let seed = incoming.join("_seed.mp3");
                    std::fs::write(&seed, b"organizer fixture").unwrap();

                    const SPECS: [(&str, &str, &str, u32); 3] = [
                        ("Midnight Drive", "Nova Pulse", "Neon Horizon", 1),
                        ("Gravity Well", "Orbital Drift", "Aphelion", 3),
                        ("Cathedral of Rain", "Aurelia Vox", "Hymnal", 2),
                    ];
                    let mut scope = Vec::new();
                    for (i, (title, artist, album, track_no)) in SPECS.iter().enumerate() {
                        let copy = incoming.join(format!("src{i}.mp3"));
                        std::fs::copy(&seed, &copy).unwrap();
                        let track = domain::models::Track {
                            id: domain::ids::TrackId::new(),
                            path: copy,
                            title: title.to_string(),
                            artist: artist.to_string(),
                            album: album.to_string(),
                            genre: Some("Ambient".to_string()),
                            year: Some(1997),
                            track_number: Some(*track_no),
                            disc_number: None,
                            duration_secs: 200.0,
                            sample_rate: 44100,
                            channels: 2,
                            format: "MP3".to_string(),
                            bitrate_kbps: Some(128),
                            is_lossless: false,
                            replaygain_track_db: None,
                            replaygain_track_peak: None,
                            replaygain_album_db: None,
                            replaygain_album_peak: None,
                            r128_loudness_lufs: None,
                            file_size_bytes: 18,
                            mtime_unix_secs: 1,
                            mtime_nanos: 0,
                            content_hash: Some(format!("eval-hash-{i}")),
                            is_favorite: false,
                            play_count: 0,
                            last_played_at: None,
                            added_at: 0,
                        };
                        self.track_repo.upsert(&track).unwrap();
                        scope.push(track);
                    }
                    std::fs::remove_file(&seed).unwrap();
                    self.reload_library();

                    // A play count must survive the move, so bump one.
                    self.track_repo.increment_play_count(&scope[0].id).unwrap();

                    // Occupy one destination so the plan has a real conflict.
                    let conflict = library_dir.join("Orbital Drift/Aphelion/03 - Gravity Well.mp3");
                    std::fs::create_dir_all(conflict.parent().unwrap()).unwrap();
                    std::fs::write(&conflict, b"existing").unwrap();

                    // Queue one so the path remap is exercised too.
                    self.coordinator.add_to_queue(scope[0].clone());

                    let rows_before = self.track_repo.count().unwrap();
                    self.open_organize(scope.clone(), format!("{mode} fixture"));
                    self.organize.set_destination(Some(library_dir.clone()));
                    self.organize.pattern = "{artist}/{album}/{track} - {title}".to_string();
                    self.organize.plan_dirty = true;
                    self.organize.rebuild_if_due();

                    log::info!(
                        "ORGANIZE PREVIEW: {} to move, {} conflict(s), {} unchanged",
                        self.organize.plan.pending_count(),
                        self.organize.plan.conflict_count(),
                        self.organize.plan.unchanged_count()
                    );

                    if mode == "apply" {
                        self.apply_organize_plan();

                        let mut moved = 0usize;
                        for track in &scope {
                            let row = self
                                .track_repo
                                .get_by_id(&track.id)
                                .unwrap()
                                .expect("row must survive a move");
                            assert_eq!(row.id, track.id, "identity must be preserved");
                            if row.path == track.path {
                                log::info!("  left in place: {}", track.title);
                                continue;
                            }
                            moved += 1;
                            assert!(
                                row.path.exists(),
                                "relinked path must exist: {}",
                                row.path.display()
                            );
                            assert!(!track.path.exists(), "old path should be gone");
                            assert!(row.path.starts_with(&library_dir));
                            log::info!("  moved: {} -> {}", track.title, row.path.display());
                        }
                        assert_eq!(moved, 2, "two move, the occupied destination is skipped");

                        let played = self
                            .track_repo
                            .get_by_id(&scope[0].id)
                            .unwrap()
                            .unwrap()
                            .play_count;
                        assert_eq!(played, 1, "play count must survive a move");

                        let queued = self
                            .coordinator
                            .queue()
                            .iter()
                            .find(|t| t.id == scope[0].id)
                            .expect("track is queued");
                        assert!(
                            queued.path.starts_with(&library_dir),
                            "queued path must be repointed: {}",
                            queued.path.display()
                        );

                        assert_eq!(
                            self.track_repo.count().unwrap(),
                            rows_before,
                            "a move must not duplicate rows"
                        );
                        log::info!(
                            "ORGANIZE APPLY OK: {moved} moved, {rows_before} rows, play count + queue path preserved"
                        );
                    }
                }
            }

            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SELECT").is_ok()
            {
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
                        "added" => Nav::RecentlyAdded,
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
                if let Ok(idx) = std::env::var("SHADOWPLAYER_EVAL_LYRICS") {
                    let n: usize = idx.parse().unwrap_or(0);
                    if let Some(track) = self.library_state.displayed_tracks.get(n).cloned() {
                        self.coordinator.play_track(track, None);
                    }
                    self.app_state.right_tab = crate::components::right_panel::RightTab::Lyrics;
                }
                if std::env::var("SHADOWPLAYER_EVAL_TOAST").is_ok() {
                    self.toasts
                        .success("Opened playlist 'Late Night Drive' — 12 of 14 tracks");
                    self.toasts.info("Scan complete: 812 tracks (2410ms)");
                }
                if let Ok(f) = std::env::var("SHADOWPLAYER_EVAL_FILTER") {
                    use crate::components::songs_table::QuickFilter as QF;
                    self.library_state.quick_filter = match f.as_str() {
                        "lossless" => QF::Lossless,
                        "hires" => QF::HiRes,
                        "favorites" => QF::Favorites,
                        "played" => QF::Played,
                        _ => QF::All,
                    };
                    self.refresh_view();
                }
            }
            // Synthetic library rows, so a screenshot run can show a
            // populated library: the song table with its quality tags, the
            // populated state of every view, and the loudness inspector.
            if self.app_state.frame_count == 1 {
                if let Ok(raw) = std::env::var("SHADOWPLAYER_EVAL_TRACKS") {
                    let count: usize = raw.parse().unwrap_or(0);
                    for i in 0..count {
                        let track = domain::models::Track {
                            id: domain::ids::TrackId::new(),
                            path: std::path::PathBuf::from(format!("/eval/track_{i}.flac")),
                            title: EVAL_TRACK_TITLES[i % EVAL_TRACK_TITLES.len()].to_string(),
                            artist: EVAL_TRACK_ARTISTS[i % EVAL_TRACK_ARTISTS.len()].to_string(),
                            album: EVAL_TRACK_ALBUMS[i % EVAL_TRACK_ALBUMS.len()].to_string(),
                            genre: Some("Ambient".to_string()),
                            year: Some(2019 + (i % 6) as u32),
                            track_number: Some((i % 6) as u32 + 1),
                            disc_number: None,
                            duration_secs: 180.0 + (i as f32) * 23.5,
                            sample_rate: EVAL_SAMPLE_RATES[i % EVAL_SAMPLE_RATES.len()],
                            channels: 2,
                            format: if i % 4 == 3 { "MP3" } else { "FLAC" }.to_string(),
                            bitrate_kbps: Some(if i % 4 == 3 { 320 } else { 1000 }),
                            is_lossless: i % 4 != 3,
                            replaygain_track_db: Some(-6.0 - (i as f32) * 0.4),
                            replaygain_track_peak: Some(0.91),
                            replaygain_album_db: Some(-5.2),
                            replaygain_album_peak: Some(0.88),
                            r128_loudness_lufs: Some(-16.0 - (i as f32) * 0.7),
                            file_size_bytes: 48_000_000,
                            mtime_unix_secs: 1_600_000_000,
                            mtime_nanos: 0,
                            content_hash: None,
                            is_favorite: i % 3 == 0,
                            play_count: (i as u32) * 7,
                            last_played_at: Some(1_700_000_000),
                            added_at: 1_600_000_000 + (i as i64) * 86_400,
                        };
                        let _ = self.track_repo.upsert(&track);
                    }
                    self.reload_library();
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_EDIT").is_ok() {
                if let Some(track) = self.library_state.all_tracks.first().cloned() {
                    self.app_state.edit_track.open_with(&track);
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_PALETTE").is_ok()
            {
                self.app_state.command_palette_open = true;
                if let Ok(q) = std::env::var("SHADOWPLAYER_EVAL_PALETTE_QUERY") {
                    self.app_state.command_palette.query = q;
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_LIGHT").is_ok() {
                self.theme = crate::theme::Theme::light();
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_HELP").is_ok() {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Help;
                if let Ok(tab_str) = std::env::var("SHADOWPLAYER_EVAL_HELP_TAB") {
                    match tab_str.as_str() {
                        "signal" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::SignalPath
                        }
                        "eq" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::EqualizerAcoustics
                        }
                        "bitperfect" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::BitPerfectDac
                        }
                        "spatial" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::SpatialBinaural
                        }
                        "playback" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::PlaybackDsp
                        }
                        "mixbus" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::MixBusStudio
                        }
                        "shortcuts" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::KeyboardShortcuts
                        }
                        "glossary" => {
                            self.help_state.active_tab = crate::views::help_view::HelpTab::Glossary
                        }
                        _ => {}
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
                    self.dsp_state.eq_panel.selected_preset =
                        crate::components::eq_panel::EqPreset::Flat;
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
                    self.dsp_state.eq_panel.active_tab =
                        crate::components::eq_panel::EqTab::Advanced;
                    self.dsp_state.eq_panel.extended_range = false;
                    self.dsp_state.eq_panel.auto_headroom = true;
                    self.dsp_state.eq_panel.midside_eq = false;
                    self.dsp_state.eq_panel.graphic_preamp_db = 0.0;
                } else if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_err() {
                    self.dsp_state.eq_panel.selected_preset =
                        crate::components::eq_panel::EqPreset::Pop;
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
            self.drive_clap_eval();
            if self.app_state.frame_count < 30 {
                ctx.request_repaint();
            }
            if self.app_state.frame_count == 24 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(
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
                    ),
                ));
            }
            if self.app_state.frame_count > 45 {
                std::process::exit(0);
            }
        }

        // 5. Drag & Drop File / Folder Ingestion
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                let path = file.path();
                if path.is_dir() {
                    self.start_scan(path.to_path_buf());
                } else if path.is_file() {
                    let is_cue = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("cue"))
                        .unwrap_or(false);
                    if is_cue {
                        if let Ok(cue_tracks) = metadata::extract_cue_metadata(path) {
                            for meta in &cue_tracks {
                                if let Err(e) = self.track_repo.upsert(&meta.track) {
                                    log::warn!(
                                        "Could not import {}: {e}",
                                        meta.track.path.display()
                                    );
                                }
                            }
                            if let Some(first) = cue_tracks.first().map(|m| m.track.clone()) {
                                let all_t = cue_tracks.into_iter().map(|m| m.track).collect();
                                self.coordinator.play_track(first, Some(all_t));
                            }
                            self.reload_library();
                        }
                    } else if let Ok(meta) = metadata::extract_metadata(path) {
                        if let Err(e) = self.track_repo.upsert(&meta.track) {
                            log::warn!("Could not import {}: {e}", meta.track.path.display());
                        }
                        self.coordinator.play_track(meta.track.clone(), None);
                        self.reload_library();
                    }
                }
            }
        });

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

        // Mini player: a dedicated 320x110 shell instead of squeezing the
        // library chrome into a window that cannot hold it.
        if self.app_state.mini_player {
            let art = current_track
                .as_ref()
                .and_then(|t| self.artwork_service.get_texture(&t.id))
                .cloned();
            let mini_events = MiniPlayer {
                title: current_track
                    .as_ref()
                    .map(|t| t.title.as_str())
                    .unwrap_or("Nothing playing"),
                artist: current_track
                    .as_ref()
                    .map(|t| t.artist.as_str())
                    .unwrap_or("ShadowPlayer"),
                artwork: art.as_ref(),
                is_playing,
                position_secs: telemetry.position_secs,
                duration_secs: telemetry.duration_secs,
                theme: &self.theme,
            }
            .show(ui);

            if mini_events.toggle_play {
                self.coordinator.toggle_play_pause();
            }
            if mini_events.previous {
                self.coordinator.previous();
            }
            if mini_events.next {
                self.coordinator.next();
            }
            if let Some(seek) = mini_events.seek_secs {
                self.session.seek(seek as f32);
            }
            if mini_events.restore {
                self.set_mini_player(false);
            }

            if !eval_holds_persistence() && self.settings_dirty {
                self.save_persisted_settings();
                self.settings_dirty = false;
            }
            return;
        }

        // 8. Render Left Sidebar
        let window_w = ctx.content_rect().width();
        let sidebar_rail = window_w < 720.0;
        let sidebar_w = if sidebar_rail {
            60.0
        } else if window_w < 1100.0 {
            // Wide enough for "Help & Feedback" without clipping, narrow
            // enough to leave the table usable on a 900px tiled window.
            200.0
        } else {
            208.0
        };
        let mut nav_to_select = None;
        let mut create_playlist_req = false;
        let mut create_smart_playlist_req = false;
        let mut delete_playlist_id = None;
        let mut delete_smart_playlist_id = None;
        let mut edit_smart_playlist_id = None;
        render_sidebar(
            ui,
            self.app_state.active_nav,
            &self.library_state.playlists,
            &self.library_state.smart_playlists,
            &self.theme,
            sidebar_rail,
            sidebar_w,
            |nav| nav_to_select = Some(nav),
            || create_playlist_req = true,
            || create_smart_playlist_req = true,
            |id| delete_playlist_id = Some(id),
            |id| delete_smart_playlist_id = Some(id),
            |id| edit_smart_playlist_id = Some(id),
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
            self.app_state.open_playlist_editor = true;
        }
        if create_smart_playlist_req {
            self.smart_playlist_editor.open_new();
        }
        if let Some(id) = delete_playlist_id {
            if let Err(e) = self.playlist_repo.delete(&id) {
                log::warn!("Could not delete playlist: {e}");
            }
            if self.app_state.active_nav == NavItem::Playlist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            self.reload_library();
        }
        if let Some(id) = delete_smart_playlist_id {
            if let Err(e) = self.smart_playlist_repo.delete(&id) {
                log::warn!("Could not delete smart playlist: {e}");
            }
            if self.app_state.active_nav == NavItem::SmartPlaylist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            self.reload_library();
        }
        if let Some(id) = edit_smart_playlist_id {
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

        // 9. Render Central Panel (Main Views + Right Drawer)
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(self.theme.bg_app)
                    .inner_margin(Margin {
                        left: 10,
                        right: 18,
                        top: 14,
                        bottom: 14,
                    }),
            )
            .show(ui, |root_ui| {
                let full_rect = root_ui.available_rect_before_wrap();
                let avail_w = full_rect.width();
                let avail_h = full_rect.height();
                let is_help_view = self.app_state.active_nav == NavItem::Help;
                let show_right_panel = !is_help_view
                    && self.app_state.active_nav != NavItem::Settings
                    && window_w >= 1100.0
                    && self.app_state.right_panel_visible;
                let right_w = if show_right_panel {
                    if window_w >= 1280.0 {
                        300.0
                    } else {
                        272.0
                    }
                } else {
                    0.0
                };
                let spacing = if show_right_panel { 16.0 } else { 0.0 };
                let center_w = (avail_w - right_w - spacing).max(0.0);

                let center_rect = Rect::from_min_size(full_rect.min, Vec2::new(center_w, avail_h));
                let right_rect = Rect::from_min_size(
                    Pos2::new(full_rect.max.x - right_w, full_rect.min.y),
                    Vec2::new(right_w, avail_h),
                );

                // A. Center View Column
                let mut center_ui = root_ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(center_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                center_ui.set_clip_rect(center_rect);
                center_ui.set_width(center_w);
                center_ui.set_max_width(center_w);
                let ui = &mut center_ui;
                ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_lg);

                // Top bar: search plus queue/theme/mini actions. On
                // Settings and Help the search is replaced by the
                // page title, but the actions stay reachable.
                if !is_help_view && self.app_state.active_nav != NavItem::Settings {
                    let mut search_buf = self.app_state.search_query.clone();
                    let mut search_changed = false;
                    ui.horizontal(|ui| {
                        let actions_w = 34.0 * 3.0 + 12.0;
                        let search_w = (ui.available_width() - actions_w - 8.0).max(160.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(search_w, 38.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                search_changed = crate::components::topbar::render_search_input(
                                    ui,
                                    &mut search_buf,
                                    &self.theme,
                                );
                            },
                        );
                        self.render_topbar_actions(ui, window_w);
                    });
                    if search_changed {
                        self.app_state.search_query = search_buf;
                        self.refresh_view();
                    }
                    // The `/` and `Ctrl+F` shortcuts park the caret in the
                    // search field. The focus has to be requested after the
                    // field has been laid out, otherwise egui has no widget
                    // with that id to focus yet.
                    if self.app_state.focus_search {
                        self.app_state.focus_search = false;
                        ctx.memory_mut(|m| {
                            m.request_focus(crate::components::topbar::search_input_id())
                        });
                    }
                } else {
                    let title = if is_help_view {
                        "Help & Feedback"
                    } else {
                        "Settings"
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .font(crate::theme::font(18.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.render_topbar_actions(ui, window_w)
                        });
                    });
                }

                // View Switcher
                match self.app_state.active_nav {
                    NavItem::Albums => {
                        render_albums_view(
                            ui,
                            &mut self.library_state,
                            &mut self.coordinator,
                            &mut self.artwork_service,
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
                            &mut self.artwork_service,
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
                            let write_tags = self.dsp_state.settings_tab.loudness_write_tags;
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
                        let mut view_action = render_main_playback_view(
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
                        if let Some(q) = view_action.search_query.take() {
                            self.app_state.search_query = q;
                            self.refresh_view();
                        }
                        if let Some(nav) = view_action.switch_nav {
                            self.app_state.active_nav = nav;
                            self.library_state.selected_album = None;
                            self.library_state.selected_artist = None;
                            self.library_state.selected_genre = None;
                            self.library_state.selected_folder = None;
                            self.refresh_view();
                        }
                        if view_action.create_playlist {
                            self.app_state.open_playlist_editor = true;
                        }
                        if view_action.open_eq_panel {
                            self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
                            self.settings_dirty = true;
                        }
                        if view_action.open_loudness {
                            self.open_loudness_inspector();
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
                        if let Some(tracks) = view_action.organize_tracks {
                            self.open_organize(tracks, "Selection".to_string());
                        }
                    }
                }
                // B. Right Column Drawer
                if show_right_panel {
                    let mut right_ui = root_ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(right_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    right_ui.set_clip_rect(right_rect);
                    let prev_light = self.theme.is_light;
                    let prev_vol = self.app_state.volume;
                    let prev_muted = self.app_state.is_muted;
                    render_right_drawer(
                        &mut right_ui,
                        &ctx,
                        right_w,
                        avail_h,
                        &mut self.app_state,
                        &mut self.coordinator,
                        &self.session,
                        &mut self.artwork_service,
                        &active_lyrics,
                        plain_lyrics.as_deref(),
                        lyrics_source,
                        active_lyric_idx,
                        &mut self.theme,
                        true,
                    );
                    if self.theme.is_light != prev_light
                        || (self.app_state.volume - prev_vol).abs() > 0.001
                        || self.app_state.is_muted != prev_muted
                    {
                        self.settings_dirty = true;
                    }
                }

                // Narrow layout: the same drawer becomes a temporary
                // overlay anchored below the top bar, leaving the queue button
                // reachable. Escape closes it as well.
                if !is_help_view
                    && self.app_state.active_nav != NavItem::Settings
                    && window_w < 1100.0
                    && self.app_state.right_overlay_open
                {
                    let overlay_w = (avail_w - 16.0).clamp(240.0, 320.0);
                    let top = full_rect.min.y + 50.0;
                    let overlay_rect = Rect::from_min_max(
                        Pos2::new(full_rect.max.x - overlay_w, top),
                        Pos2::new(full_rect.max.x, full_rect.max.y),
                    );
                    // The overlay is a floating card with transparent margins
                    // around it. Paint the column background first so table
                    // content cannot bleed through below the card.
                    root_ui
                        .painter()
                        .rect_filled(overlay_rect, 0.0, self.theme.bg_app);
                    // A soft scrim makes the drawer read as foreground without
                    // hiding the top-bar controls.
                    root_ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(full_rect.min.x, top),
                            Pos2::new(overlay_rect.min.x, full_rect.max.y),
                        ),
                        0.0,
                        Color32::from_black_alpha(if self.theme.is_light { 24 } else { 80 }),
                    );
                    let mut overlay_ui = root_ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(overlay_rect)
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    overlay_ui.set_clip_rect(overlay_rect);
                    let prev_light = self.theme.is_light;
                    let prev_vol = self.app_state.volume;
                    let prev_muted = self.app_state.is_muted;
                    render_right_drawer(
                        &mut overlay_ui,
                        &ctx,
                        overlay_w,
                        overlay_rect.height(),
                        &mut self.app_state,
                        &mut self.coordinator,
                        &self.session,
                        &mut self.artwork_service,
                        &active_lyrics,
                        plain_lyrics.as_deref(),
                        lyrics_source,
                        active_lyric_idx,
                        &mut self.theme,
                        false,
                    );
                    if self.theme.is_light != prev_light
                        || (self.app_state.volume - prev_vol).abs() > 0.001
                        || self.app_state.is_muted != prev_muted
                    {
                        self.settings_dirty = true;
                    }
                }

                root_ui.allocate_rect(full_rect, Sense::hover());
            });

        // 10. Modals Execution
        let eq_actions = show_eq_panel(&ctx, &mut self.dsp_state.eq_panel, &self.theme);
        self.dsp_state
            .handle_eq_panel_actions(&self.session, eq_actions);
        if self.dsp_state.eq_panel.is_open {
            self.settings_dirty = true;
        }

        // The plugin rack. The view is a snapshot the engine publishes, so the
        // window never touches the audio thread.
        let clap_view = self.session.clap_view();
        let clap_actions =
            show_clap_rack_panel(&ctx, &mut self.dsp_state.clap_rack, &clap_view, &self.theme);
        if clap_actions.choose_library {
            self.request_import(ImportKind::ClapLibrary);
        }
        self.dsp_state
            .handle_clap_rack_actions(&self.session, clap_actions);
        if self.dsp_state.clap_rack.is_open {
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
            if let Err(e) = self.smart_playlist_repo.delete(&id) {
                log::warn!("Could not delete the smart playlist: {e}");
            }
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
        if self.app_state.open_playlist_editor {
            self.app_state.open_playlist_editor = false;
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
                Ok(new_pl) => {
                    self.app_state.playlist_editor.selected = Some(new_pl.id);
                    self.playlist_editor_tracks.clear();
                    pl_changed = true;
                }
                Err(e) => log::warn!("Could not create playlist: {}", e),
            }
        }
        if let Some((id, name)) = pl_actions.rename {
            if let Err(e) = self.playlist_repo.rename(&id, &name) {
                log::warn!("Could not rename the playlist: {e}");
            }
            pl_changed = true;
        }
        if let Some(id) = pl_actions.delete {
            if let Err(e) = self.playlist_repo.delete(&id) {
                log::warn!("Could not delete the playlist: {e}");
            }
            if self.app_state.playlist_editor.selected == Some(id) {
                self.app_state.playlist_editor.selected = None;
            }
            self.playlist_editor_tracks.clear();
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.add_track {
            if let Err(e) = self.playlist_repo.add_track(&pl, &tr) {
                log::warn!("Could not add the track to the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.remove_track {
            if let Err(e) = self.playlist_repo.remove_track(&pl, &tr) {
                log::warn!("Could not remove the track from the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some((pl, from, to)) = pl_actions.move_track {
            if let Err(e) = self.playlist_repo.move_track(&pl, from, to) {
                log::warn!("Could not reorder the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some(id) = pl_actions.select_changed {
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &id);
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
            let ids: Vec<domain::ids::TrackId> = self.duplicates.selected.iter().copied().collect();
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
            if removed > 0 {
                let verb = if delete_from_disk {
                    "deleted"
                } else {
                    "removed"
                };
                self.toasts
                    .success(format!("{removed} duplicate track(s) {verb}"));
            }
            self.reload_library();
            // Refresh so the list reflects what is left.
            self.scan_duplicates();
        }

        // Batch organizer.
        let organize_actions = show_organize_modal(&ctx, &mut self.organize, &self.theme);
        if organize_actions.reset_pattern {
            self.organize.reset_pattern();
        }
        if organize_actions.clear_destination {
            self.organize.set_destination(None);
        }
        if organize_actions.pick_destination {
            self.import_service.request_organize_destination();
        }
        if organize_actions.apply {
            self.apply_organize_plan();
        }

        // Edit Song Info dialog.
        let edit_actions = show_edit_track_modal(&ctx, &mut self.app_state.edit_track, &self.theme);
        if let Some(track) = edit_actions.save {
            if let Err(e) = self.track_repo.update_metadata(&track) {
                log::warn!("Could not save track metadata: {}", e);
            }
            // The file is the source of truth: an edit that only lives in the
            // database is lost the moment the file moves or the library is
            // rebuilt from disk, so the tags are written back too.
            if edit_actions.write_to_file {
                let tags = engine::decode::EditorialTags {
                    title: Some(track.title.clone()),
                    artist: Some(track.artist.clone()),
                    album: Some(track.album.clone()),
                    genre: track.genre.clone(),
                    year: track.year,
                    track_number: track.track_number,
                    disc_number: track.disc_number,
                };
                match engine::decode::write_editorial_tags(&track.path, &tags) {
                    Ok(()) => self.toasts.success(format!(
                        "Updated the tags in {}",
                        track
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| track.path.display().to_string())
                    )),
                    Err(e) => self.toasts.warning(format!(
                        "Saved in the library, but the file's tags could not be written: {e}"
                    )),
                }
            }
            self.reload_library();
        }

        // The loudness inspector.
        crate::modals::loudness_modal::show_loudness_modal(
            &ctx,
            &mut self.app_state.loudness_modal,
            self.coordinator.current_track(),
            &self.theme,
        );

        // The command palette. It is a modal, so it traps the keyboard and
        // the background stays inert while it is open.
        if self.app_state.command_palette_open {
            let entries = self.command_palette_entries();
            if let Some(action) = crate::components::command_palette::show_command_palette(
                &ctx,
                &mut self.app_state.command_palette,
                &entries,
                &self.theme,
            ) {
                self.app_state.command_palette_open = false;
                self.app_state.command_palette.query.clear();
                self.app_state.command_palette.selected = 0;
                self.run_palette_action(action);
            }
        }

        // 11. Event-driven settings persistence across sessions. Window size,
        // theme and transport changes mark the state dirty; writing on every
        // 120th frame made an otherwise idle app wake the disk twice a second.
        if !eval_holds_persistence() && self.settings_dirty {
            self.save_persisted_settings();
            self.settings_dirty = false;
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        if !eval_holds_persistence() {
            self.save_persisted_settings();
        }
    }
}

/// `true` while the eval harness is driving the UI: it must not write the real
/// settings file from a screenshot run. In a normal build this is a constant
/// `false` and the environment variable is never read.
fn eval_holds_persistence() -> bool {
    #[cfg(feature = "eval-tools")]
    {
        std::env::var("SHADOWPLAYER_EVAL").is_ok()
    }
    #[cfg(not(feature = "eval-tools"))]
    {
        false
    }
}

/// Load a playlist's tracks for the editor.
///
/// `unwrap_or_default()` made a failed read look like an empty playlist, which
/// reads as "this playlist has no songs" rather than "the database refused".
fn load_playlist_tracks(
    repo: &PlaylistRepo,
    id: &domain::ids::PlaylistId,
) -> Vec<domain::models::Track> {
    match repo.get_tracks(id) {
        Ok(tracks) => tracks,
        Err(e) => {
            log::warn!("Could not load the playlist's tracks: {e}");
            Vec::new()
        }
    }
}

use crate::components::right_panel::RightTab;
use crate::components::sidebar::NavItem;
use crate::modals::edit_track_modal::EditTrackState;
use crate::modals::playlist_editor_modal::PlaylistEditorState;
use crate::state::sleep_timer::SleepTimer;
use domain::models::{SortField, SortOrder, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisualizerMode {
    #[default]
    Spectrum,
    VuMeters,
    LufsRadar,
    Off,
}

/// Central UI application state for navigation, controls, and modal visibility
pub struct AppState {
    pub active_nav: NavItem,
    pub search_query: String,
    pub current_sort: SortField,
    /// Ascending or descending, for the current sort field.
    pub current_sort_order: SortOrder,
    pub is_grid_view: bool,
    /// The song table's flexible column shares, persisted between launches.
    pub column_widths: crate::components::songs_table::ColumnWidths,
    /// Width of the fixed rating column, persisted between launches.
    pub rating_width: f32,
    /// Comfortable or compact song-table rows, persisted between launches.
    pub row_density: crate::components::songs_table::RowDensity,
    /// Named songs-table column layouts (Default, Casual, Audiophile, custom).
    pub column_presets: Vec<crate::components::songs_table::ColumnPreset>,
    /// Index of the active layout in `column_presets`.
    pub active_column_preset: usize,
    /// Name typed into the preset editor, not persisted.
    pub column_preset_name: String,
    pub right_tab: RightTab,
    pub visualizer_mode: VisualizerMode,
    /// Wide layout: show the queue/lyrics drawer as a column.
    pub right_panel_visible: bool,
    /// Narrow layout: show the queue/lyrics drawer as a temporary overlay.
    pub right_overlay_open: bool,
    /// Set by the `/` and `Ctrl+F` shortcuts: move the keyboard focus to the
    /// search field on the next frame.
    pub focus_search: bool,
    /// The command palette is open.
    pub command_palette_open: bool,
    /// The loudness inspector.
    pub loudness_modal: crate::modals::loudness_modal::LoudnessModalState,
    /// The library statistics.
    pub library_stats_modal: crate::modals::library_stats_modal::LibraryStatsModalState,
    /// The local listening history: charts, streaks and export.
    pub listening_modal: crate::modals::listening_modal::ListeningModalState,
    /// The per-track DSP editor.
    pub track_dsp_modal: crate::modals::track_dsp_modal::TrackDspModalState,
    /// The user's own chain, saved the first time a track's own profile takes
    /// over, so the next track without one can have it back.
    pub dsp_global_snapshot: Option<crate::state::ChainPreset>,
    /// The scrobble log.
    pub scrobble_log_open: bool,
    /// The import-from-other-players dialog.
    pub import_playlists_modal: crate::modals::import_playlists_modal::ImportPlaylistsModalState,
    /// The command palette's own state: its query and the highlighted row.
    pub command_palette: crate::components::command_palette::CommandPaletteState,
    /// Single-window compact player mode.
    pub mini_player: bool,
    /// The full-screen Now Playing mode: the window gives itself over to the
    /// artwork, the visualiser and the lyrics.
    pub now_playing_full: bool,
    /// Keep the mini player above other windows (best-effort on Wayland).
    pub mini_always_on_top: bool,

    pub volume: f32,
    pub is_muted: bool,
    pub prev_volume: f32,
    pub frame_count: u32,

    // Modal dialogue states
    /// A menu item asked for the playlist editor (create/manage playlists).
    pub open_playlist_editor: bool,
    pub track_properties_track: Option<Track>,
    pub track_properties_open: bool,
    pub bit_perfect_modal_open: bool,

    /// Directories that individual songs were imported from (newest first),
    /// surfaced under the import buttons in Settings.
    pub imported_dirs: Vec<std::path::PathBuf>,

    /// Playlist manager floating window state.
    pub playlist_editor: PlaylistEditorState,
    /// Edit Song Info dialog state.
    pub edit_track: EditTrackState,
    /// Active sleep timer, if any.
    pub sleep_timer: Option<SleepTimer>,
    /// Inline lyrics editor draft for the right-hand Lyrics tab.
    pub lyrics_editor: crate::state::LyricsEditorState,
    /// Inline biography draft for the Artist/Album detail pane.
    pub biography_editor: crate::state::BiographyEditorState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_nav: NavItem::Home,
            search_query: String::new(),
            current_sort: SortField::TrackNumber,
            current_sort_order: SortOrder::Ascending,
            is_grid_view: false,
            column_widths: Default::default(),
            rating_width: crate::components::songs_table::SongTablePrefs::default().rating_width,
            row_density: Default::default(),
            column_presets: crate::components::songs_table::ColumnPreset::builtins(),
            active_column_preset: 0,
            column_preset_name: String::new(),
            right_tab: RightTab::Queue,
            visualizer_mode: VisualizerMode::Spectrum,
            right_panel_visible: true,
            right_overlay_open: false,
            focus_search: false,
            command_palette_open: false,
            loudness_modal: Default::default(),
            library_stats_modal: Default::default(),
            listening_modal: Default::default(),
            track_dsp_modal: Default::default(),
            dsp_global_snapshot: None,
            scrobble_log_open: false,
            import_playlists_modal: Default::default(),
            command_palette: Default::default(),
            mini_player: false,
            now_playing_full: false,
            mini_always_on_top: true,

            volume: 0.60,
            is_muted: false,
            prev_volume: 0.60,
            frame_count: 0,

            open_playlist_editor: false,
            track_properties_track: None,
            track_properties_open: false,
            bit_perfect_modal_open: false,

            imported_dirs: Vec::new(),

            playlist_editor: PlaylistEditorState::default(),
            edit_track: EditTrackState::default(),
            sleep_timer: None,
            lyrics_editor: Default::default(),
            biography_editor: Default::default(),
        }
    }
}

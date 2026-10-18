use domain::models::Track;
use crate::components::right_panel::RightTab;
use crate::components::sidebar::NavItem;
use crate::components::songs_table::SortField;
use crate::modals::edit_track_modal::EditTrackState;
use crate::modals::playlist_editor_modal::PlaylistEditorState;
use crate::state::sleep_timer::SleepTimer;

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
    pub is_grid_view: bool,
    pub right_tab: RightTab,
    pub visualizer_mode: VisualizerMode,

    pub volume: f32,
    pub is_muted: bool,
    pub prev_volume: f32,
    pub frame_count: u32,

    // Modal dialogue states
    pub new_playlist_modal_open: bool,
    pub new_playlist_name: String,
    pub track_properties_track: Option<Track>,
    pub track_properties_open: bool,
    pub help_modal_open: bool,
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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_nav: NavItem::Home,
            search_query: String::new(),
            current_sort: SortField::Default,
            is_grid_view: false,
            right_tab: RightTab::Queue,
            visualizer_mode: VisualizerMode::Spectrum,

            volume: 0.60,
            is_muted: false,
            prev_volume: 0.60,
            frame_count: 0,

            new_playlist_modal_open: false,
            new_playlist_name: String::new(),
            track_properties_track: None,
            track_properties_open: false,
            help_modal_open: false,
            bit_perfect_modal_open: false,

            imported_dirs: Vec::new(),

            playlist_editor: PlaylistEditorState::default(),
            edit_track: EditTrackState::default(),
            sleep_timer: None,
        }
    }
}

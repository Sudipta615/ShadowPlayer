pub mod albums_view;
pub mod artists_view;
pub mod folders_view;
pub mod genres_view;
pub mod help_view;
pub mod main_playback_view;
pub mod settings_view;

pub use albums_view::render_albums_view;
pub use artists_view::render_artists_view;
pub use folders_view::render_folders_view;
pub use genres_view::render_genres_view;
pub use help_view::{render_help_view, HelpState, HelpTab};
pub use main_playback_view::render_main_playback_view;
pub use settings_view::{render_settings_view, SettingsViewActions};

pub use crate::components::settings_tab::RecommendationSettingsContext;

use crate::ids::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_secs: f32,
    pub sample_rate: u32,
    pub channels: usize,
    pub format: String,
    pub bitrate_kbps: Option<u32>,
    pub is_lossless: bool,
    pub replaygain_track_db: Option<f32>,
    pub replaygain_track_peak: Option<f32>,
    pub r128_loudness_lufs: Option<f32>,
    pub file_size_bytes: u64,
    pub mtime_unix_secs: i64,
    /// Cheap content fingerprint (size + head/tail hash). Used to recognise a
    /// file that was moved or renamed so its identity survives. `None` when the
    /// fingerprint could not be computed (e.g. CUE-sheet virtual tracks).
    pub content_hash: Option<String>,
    pub is_favorite: bool,
    pub play_count: u32,
    pub last_played_at: Option<i64>,
    pub added_at: i64,
}

impl Track {
    pub fn duration_formatted(&self) -> String {
        format_duration(self.duration_secs)
    }
}

pub fn format_duration(secs: f32) -> String {
    if secs <= 0.0 || !secs.is_finite() {
        return "0:00".to_string();
    }
    let total_secs = secs as u64;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let hours = minutes / 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes % 60, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Album {
    pub id: AlbumId,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    pub track_count: u32,
    pub artwork_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub id: ArtistId,
    pub name: String,
    pub track_count: u32,
    pub album_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Genre {
    pub name: String,
    pub track_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: PlaylistId,
    pub name: String,
    pub description: Option<String>,
    pub track_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub playlist_id: PlaylistId,
    pub track_id: TrackId,
    pub position: usize,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanRoot {
    pub id: ScanRootId,
    pub path: PathBuf,
    pub watch_enabled: bool,
    pub last_scanned_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncedLyricLine {
    pub start_time_secs: f32,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortField {
    #[default]
    Title,
    Artist,
    Album,
    Duration,
    TrackNumber,
    DateAdded,
    /// Favourites first. There is no "unfavourite" ranking, so this is a
    /// one-sided key rather than a text or numeric column.
    Favorite,
}

impl std::fmt::Display for SortField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Title => write!(f, "Title"),
            Self::Artist => write!(f, "Artist"),
            Self::Album => write!(f, "Album"),
            Self::Duration => write!(f, "Duration"),
            Self::TrackNumber => write!(f, "Track Number"),
            Self::DateAdded => write!(f, "Date Added"),
            Self::Favorite => write!(f, "Favourites"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioProfileChoice {
    #[default]
    Normal,
    Fidelity,
    LegacyLowPower,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RepeatModeChoice {
    #[default]
    Off,
    All,
    One,
}

/// User preferences.
///
/// `#[serde(default)]` is load-bearing: settings are stored as a JSON blob, so
/// without it a payload written by a build with fewer (or renamed) fields
/// fails to deserialize and the whole configuration silently reverts to
/// defaults on the next launch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme_dark: bool,
    pub volume: f32,
    pub is_muted: bool,
    pub sort_field: SortField,
    pub sort_order: SortOrder,
    pub view_mode: ViewMode,
    pub audio_profile: AudioProfileChoice,
    pub repeat_mode: RepeatModeChoice,
    pub shuffle: bool,
    pub selected_device: Option<String>,
    pub bit_perfect_enabled: bool,
    pub window_width: f32,
    pub window_height: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_dark: true,
            volume: 0.75,
            is_muted: false,
            sort_field: SortField::Title,
            sort_order: SortOrder::Ascending,
            view_mode: ViewMode::List,
            audio_profile: AudioProfileChoice::Normal,
            repeat_mode: RepeatModeChoice::Off,
            shuffle: false,
            selected_device: None,
            bit_perfect_enabled: false,
            window_width: 1280.0,
            window_height: 800.0,
        }
    }
}

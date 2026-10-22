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
    /// Source PCM precision in bits, where the container reports it. `None`
    /// for lossy streams and for containers that do not expose a bit depth.
    #[serde(default)]
    pub bit_depth: Option<u32>,
    pub is_lossless: bool,
    pub replaygain_track_db: Option<f32>,
    pub replaygain_track_peak: Option<f32>,
    /// The album's ReplayGain gain, in dB.
    ///
    /// The engine can read this from the file's own `REPLAYGAIN_ALBUM` tag,
    /// but the library indexes it too, so the app can show it and so an album
    /// with no tag still gets a computed value.
    pub replaygain_album_db: Option<f32>,
    /// The album's ReplayGain peak (linear amplitude).
    pub replaygain_album_peak: Option<f32>,
    pub r128_loudness_lufs: Option<f32>,
    pub file_size_bytes: u64,
    pub mtime_unix_secs: i64,
    /// Nanoseconds past `mtime_unix_secs`.
    ///
    /// Seconds are too coarse to tell an unchanged file from one that was
    /// rewritten within the same second as the last scan, which the scanner
    /// treated as "unchanged" and skipped. `0` means "unknown" (a row from
    /// before this column existed).
    pub mtime_nanos: u32,
    /// Cheap content fingerprint (size + head/tail hash). Used to recognise a
    /// file that was moved or renamed so its identity survives. `None` when the
    /// fingerprint could not be computed (e.g. CUE-sheet virtual tracks).
    pub content_hash: Option<String>,
    /// The filesystem's own identity for this file — see
    /// [`crate::file_identity`].
    ///
    /// This is what lets a file that was *renamed* — including a rename that
    /// only changed the case, which a case-insensitive filesystem (Windows,
    /// macOS by default) treats as the same file — keep one row instead of
    /// becoming two tracks. `None` when the platform cannot answer, or for a
    /// virtual track (a CUE sheet's entries point at slices of one file).
    #[serde(default)]
    pub file_id: Option<String>,
    pub is_favorite: bool,
    /// The listener's 0–5 star rating. `0` means unrated, not zero stars.
    ///
    /// Ratings live in the library database rather than in the file's tags:
    /// they belong to the listener, and scoring a track must not rewrite the
    /// user's music. This is the model foobar2000 and AIMP use.
    #[serde(default)]
    pub rating: u8,
    /// Whether a sidecar or embedded lyric was found when the track was
    /// indexed. Kept in the database so "Has Lyrics" is a cheap smart rule.
    #[serde(default)]
    pub has_lyrics: bool,
    pub play_count: u32,
    /// How many times this track was skipped before it was heard.
    pub skip_count: u32,
    pub last_played_at: Option<i64>,
    pub added_at: i64,
}

impl Track {
    pub fn duration_formatted(&self) -> String {
        format_duration(self.duration_secs)
    }

    /// Whether the track is high-resolution: above CD quality in either
    /// sample rate or bitrate.
    ///
    /// This is the single definition of "Hi-Res" in the app — the per-track
    /// tag, the quick-filter chip and the library filters all read it, so
    /// they can never disagree about which tracks qualify.
    pub fn is_hires(&self) -> bool {
        self.sample_rate > 48_000 || self.bitrate_kbps.unwrap_or(0) > 1000
    }

    /// A short label for the track's quality tier, or `None` for a plain
    /// lossy CD-quality file.
    ///
    /// A high-resolution *and* lossless file is reported as Hi-Res, because
    /// that is the more interesting fact about it.
    pub fn quality_tag(&self) -> Option<&'static str> {
        if self.is_hires() {
            Some("HI-RES")
        } else if self.is_lossless {
            Some("LOSSLESS")
        } else {
            None
        }
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
    /// Favorites first. There is no "unfavorite" ranking, so this is a
    /// one-sided key rather than a text or numeric column.
    Favorite,
    /// The listener's star rating, 0–5. Unlike Favorites this is a normal
    /// numeric key, so descending puts the five-star tracks first.
    Rating,
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
            Self::Favorite => write!(f, "Favorites"),
            Self::Rating => write!(f, "Rating"),
        }
    }
}

/// A track's own DSP settings, applied when that track plays.
///
/// This is the per-track DSP that every serious local player has: a track
/// that needs a corrective curve, or a different preamp because it was
/// mastered louder, gets its own settings rather than changing the whole
/// library.
///
/// [`Self::overrides`] is a JSON object holding **only** the chain fields that
/// differ from the user's global chain: a diff, not a snapshot. Everything the
/// track does not name is inherited, so changing the global compressor next
/// month also changes this track's compressor — while its own EQ curve stays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TrackDspProfile {
    /// The graphic EQ's per-band gains, in dB.
    ///
    /// Kept for profiles written before whole-chain overrides existed. New
    /// profiles store the graphic EQ inside [`Self::overrides`].
    #[serde(default)]
    pub graphic_gains: Vec<f32>,
    /// Whether the graphic EQ is on for this track.
    #[serde(default)]
    pub graphic_enabled: bool,
    /// The graphic EQ's preamp, in dB.
    #[serde(default)]
    pub graphic_preamp_db: f32,
    /// The chain fields that differ from the global chain, as nested JSON.
    ///
    /// It is the UI's serialised chain preset with only the differing leaves
    /// present. The domain crate stores it opaquely so it does not have to
    /// depend on the audio configuration types the chain is made of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<serde_json::Value>,
    /// A per-track ReplayGain correction, in dB, applied on top of the chain's
    /// preamp whenever this track plays. `None` inherits the library/profile
    /// behaviour; the value is stored with the profile so it travels with the
    /// rest of the track's overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaygain_override_db: Option<f32>,
}

impl TrackDspProfile {
    /// Whether the profile would change anything.
    pub fn is_neutral(&self) -> bool {
        let legacy_neutral =
            !self.graphic_enabled && self.graphic_gains.iter().all(|g| g.abs() < 0.01);
        legacy_neutral
            && self.overrides.as_ref().is_none_or(|v| match v {
                serde_json::Value::Object(map) => map.is_empty(),
                _ => false,
            })
            && self.replaygain_override_db.is_none_or(|db| db.abs() < 0.01)
    }
}

/// One play in the permanent listening history.
///
/// The artist, title and album are copied here at write time. That is the
/// point of a scrobble: it belongs to the listener, not to the file, so
/// deleting a track from the library must not erase the times it was heard.
/// `track_id` is therefore nullable (`ON DELETE SET NULL`) and only used to
/// recognise a track that is still in the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scrobble {
    pub track_id: Option<TrackId>,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub played_at: i64,
    pub duration_secs: f32,
    pub listened_secs: f32,
}

/// One row of a listening chart: a name, an optional second line, and a count.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScrobbleCount {
    pub name: String,
    /// The artist, for an album or a track; empty for an artist.
    pub detail: String,
    pub count: usize,
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
    /// Opt in to concurrent windows. The default is one owner, where a second
    /// launch activates the first. Enabling this skips the single-instance
    /// lock on the next launch (the database is WAL-mode, so it is safe).
    pub allow_multiple_instances: bool,
    pub window_width: f32,
    pub window_height: f32,
    /// The interface language.
    ///
    /// Empty means "ask the environment": the POSIX locale variables decide,
    /// and English is the fallback. A non-empty value is a language code
    /// (`"en"`, `"de"`) or a locale (`"de_DE.UTF-8"`); the part before the
    /// region is what matters.
    pub locale: String,
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
            allow_multiple_instances: false,
            window_width: 1280.0,
            window_height: 800.0,
            locale: String::new(),
        }
    }
}

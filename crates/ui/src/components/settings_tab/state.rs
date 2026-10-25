//! The settings panel's persisted state, the action sink, and the borrowed
//! context the view supplies.

use super::index::SettingsCategory;
use config::{
    AudioBackend, BassManagementConfig, ChannelMixConfig, CrossfadeCurve, FallbackPolicy,
    LfeConfig, LoudnessMode, PrecisionMode, ResamplerQuality, SampleRatePolicy, SpatialQuality,
    SpeedMode, TransitionMode, VolumeMode,
};
use domain::ids::ScanRootId;
use domain::models::ScanRoot;
use engine::output::devices::OutputDeviceInfo;
use engine::AudioSource;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

/// Everything the settings panel needs to remember between launches.
///
/// Serde lives **on this struct** rather than on a hand-written mirror. The
/// mirror (`PersistentSettingsTabState`) listed 34 fields while the live struct
/// had more, so the two drifted: spatial screen placement, the mix-bus scratch
/// values and the capture fields silently reset on every launch, and nothing
/// caught the omission at compile time. Deriving here means a new field is
/// persisted unless it is explicitly skipped.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsTabState {
    pub active_category: SettingsCategory,

    /// The Settings search box. Scratch state: a query is not a preference.
    #[serde(skip)]
    pub search_query: String,

    // Playback
    pub speed: f32,
    pub pitch_semitones: f32,
    pub speed_mode: SpeedMode,
    pub transition_mode: TransitionMode,
    pub crossfade_duration_secs: f32,
    pub crossfade_curve: CrossfadeCurve,
    pub loudness_mode: LoudnessMode,

    // Output & Device
    pub output_backend: AudioBackend,
    pub output_device: String,
    pub sample_rate_policy: SampleRatePolicy,
    pub volume_mode: VolumeMode,
    pub bit_perfect: bool,
    /// Opt in to concurrent windows. Read before the database opens on the
    /// next launch, via the marker file the app keeps in step with this field.
    pub allow_multiple_instances: bool,
    pub dither_enabled: bool,
    pub resampler_quality: ResamplerQuality,
    pub fallback_policy: FallbackPolicy,
    /// Engine processing profile, applied to the session on change.
    ///
    /// Persisted with `AppSettings` rather than here: the engine needs it at
    /// startup, before the UI (and this struct) exist.
    #[serde(skip)]
    pub audio_profile: domain::models::AudioProfileChoice,

    // Internal DSP precision (f32 vs f64 core). `None` until the user chooses,
    // so the engine's own persisted setting is never clobbered at startup.
    pub precision_mode: Option<PrecisionMode>,

    // Multichannel: subwoofer integration and up/downmix.
    pub bass_management: BassManagementConfig,
    pub lfe: LfeConfig,
    pub channel_mix: ChannelMixConfig,

    // Whole-chain named presets. Stored under their own key: they are a library
    // the user curates, not a setting, and a preset outlives the state it was
    // captured from.
    #[serde(skip)]
    pub chain_presets: Vec<crate::state::chain_preset::ChainPreset>,
    /// Scratch text for the preset name being typed.
    #[serde(skip)]
    pub chain_preset_draft_name: String,
    #[serde(skip)]
    pub chain_preset_draft_note: String,
    /// Index of the preset whose rename box is open.
    #[serde(skip)]
    pub chain_preset_renaming: Option<usize>,
    /// Scratch text for the inline rename box.
    #[serde(skip)]
    pub chain_preset_rename_buffer: String,
    /// Pending user-facing message about the last preset save/apply. Consumed
    /// by the app and shown as a toast.
    #[serde(skip)]
    pub chain_preset_notice: Option<String>,

    // CLAP plugin insert. The card only describes the plugin; everything that
    // changes it happens in the rack window.
    /// Live summary of the hosted plugin, refreshed by the app each frame.
    #[serde(skip)]
    pub clap: crate::state::clap_rack::ClapCardSummary,

    /// When the smart playlists are re-evaluated.
    ///
    /// A smart playlist is a saved query over the library, so the only
    /// question is when the query runs again.
    pub smart_refresh: domain::smart::SmartRefreshSettings,

    // Library maintenance
    /// Set by the Settings > Library "Organize Files" button; the app raises the
    /// batch organizer and clears it.
    #[serde(skip)]
    pub request_organize: bool,
    /// Set by the CLAP card when the user asks for a plugin file; the app owns
    /// the native picker, so the card can only ask. A flag rather than an action
    /// because both settings surfaces can raise it and only one of them returns
    /// actions.
    #[serde(skip)]
    pub request_clap_library: bool,

    // Spatial
    /// Master bypass for the whole spatial stage. Without this, the HRTF/room
    /// controls wrote into a stage that still reported itself disabled.
    pub spatial_enabled: bool,
    pub spatial_quality: SpatialQuality,
    pub hrtf_profile: String,
    pub listener_smoothing_ms: f32,
    pub listener_max_rate: f32,
    /// Head orientation in degrees, independent of the virtual centre.
    pub listener_yaw_deg: f32,
    pub room_enabled: bool,
    pub room_width: f32,
    pub room_depth: f32,
    pub room_height: f32,
    pub room_absorption: f32,
    pub room_rt60_ms: f32,
    pub room_wet: f32,
    pub air_absorption_enabled: bool,
    pub voice_budget_enabled: bool,
    pub voice_capacity: usize,
    /// Azimuth of the virtual centre, driven by the soundstage radar.
    pub screen_center_az: f32,
    pub screen_half_w: f32,
    pub screen_elevation: f32,
    pub screen_gain: f32,

    // Mix Bus
    #[serde(skip)]
    pub add_track_path: String,
    pub track_slot: u8,
    pub track_gain: f32,
    pub track_pan: f32,
    pub track_muted: bool,
    pub track_active: bool,
    pub aux_enabled: bool,
    pub aux_return_gain: f32,
    pub duck_source_slot: u8,
    pub duck_threshold_db: f32,
    pub duck_depth_db: f32,
    pub duck_attack_ms: f32,
    pub duck_release_ms: f32,

    // Capture & Tools
    pub capture_path: String,
    pub capture_device: String,
    /// Opt in to GitHub release checks. Off by default; no check runs until
    /// the user enables it and presses Check.
    pub update_check_enabled: bool,
    pub stereo_enhancer_enabled: bool,
    pub loudness_tag_path: String,
    /// When set, the library ReplayGain scan also writes the measured values
    /// into each file's tags. Off by default: the app never modifies files
    /// without explicit consent.
    pub loudness_write_tags: bool,

    // Library input
    #[serde(skip)]
    pub new_scan_path: String,
    /// Folder of `<Artist>.txt` / `<Album>.md` biography files. Empty means
    /// only user-written biographies are shown.
    pub biography_source_dir: String,

    // Themes & Appearance state
    #[serde(skip)]
    pub theme_gallery: crate::theme::ThemeGalleryState,
    #[serde(skip)]
    pub theme_editor: crate::theme::ThemeEditorState,

    // Legacy collapsible section flags preserved for compatibility
    pub section_playback: bool,
    pub section_output: bool,
    pub section_spatial: bool,
    pub section_mixbus: bool,
    pub section_capture: bool,
    pub section_advanced: bool,
}

impl Default for SettingsTabState {
    fn default() -> Self {
        Self {
            active_category: SettingsCategory::Playback,
            search_query: String::new(),

            speed: 1.0,
            pitch_semitones: 0.0,
            speed_mode: SpeedMode::Varispeed,
            transition_mode: TransitionMode::Gapless,
            crossfade_duration_secs: 3.0,
            crossfade_curve: CrossfadeCurve::ConstantPower,
            loudness_mode: LoudnessMode::Off,

            output_backend: AudioBackend::default(),
            output_device: String::new(),
            sample_rate_policy: SampleRatePolicy::FollowTrack,
            volume_mode: VolumeMode::SoftwareOnly,
            bit_perfect: false,
            allow_multiple_instances: false,
            dither_enabled: true,
            resampler_quality: ResamplerQuality::HighQuality,
            fallback_policy: FallbackPolicy::Allow,
            audio_profile: domain::models::AudioProfileChoice::default(),
            precision_mode: None,
            bass_management: BassManagementConfig::default(),
            lfe: LfeConfig::default(),
            channel_mix: ChannelMixConfig::default(),

            chain_presets: Vec::new(),
            chain_preset_draft_name: String::new(),
            chain_preset_draft_note: String::new(),
            chain_preset_renaming: None,
            chain_preset_rename_buffer: String::new(),
            chain_preset_notice: None,
            clap: crate::state::clap_rack::ClapCardSummary::default(),
            smart_refresh: domain::smart::SmartRefreshSettings::default(),
            request_organize: false,
            request_clap_library: false,

            spatial_enabled: false,
            spatial_quality: SpatialQuality::High,
            hrtf_profile: String::new(),
            listener_smoothing_ms: 50.0,
            listener_max_rate: 0.0,
            listener_yaw_deg: 0.0,
            room_enabled: false,
            room_width: 5.0,
            room_depth: 4.0,
            room_height: 2.5,
            room_absorption: 0.3,
            room_rt60_ms: 400.0,
            room_wet: 0.15,
            air_absorption_enabled: false,
            voice_budget_enabled: false,
            voice_capacity: 32,
            screen_center_az: 0.0,
            screen_half_w: 30.0,
            screen_elevation: 0.0,
            screen_gain: 1.0,

            add_track_path: String::new(),
            track_slot: 2,
            track_gain: 1.0,
            track_pan: 0.0,
            track_muted: false,
            track_active: true,
            aux_enabled: false,
            aux_return_gain: 1.0,
            duck_source_slot: 0,
            duck_threshold_db: -18.0,
            duck_depth_db: 12.0,
            duck_attack_ms: 10.0,
            duck_release_ms: 200.0,

            capture_path: String::new(),
            capture_device: String::new(),
            update_check_enabled: false,
            stereo_enhancer_enabled: false,
            loudness_tag_path: String::new(),
            loudness_write_tags: false,

            new_scan_path: String::new(),
            biography_source_dir: String::new(),

            theme_gallery: crate::theme::ThemeGalleryState::default(),
            theme_editor: crate::theme::ThemeEditorState::default(),

            section_playback: true,
            section_output: false,
            section_spatial: false,
            section_mixbus: false,
            section_capture: false,
            section_advanced: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Actions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SettingsTabActions {
    // Playback
    pub speed_changed: Option<f32>,
    pub pitch_changed: Option<f32>,
    pub speed_mode_changed: Option<SpeedMode>,
    pub transition_mode_changed: Option<TransitionMode>,
    pub crossfade_config_changed: bool,
    pub crossfade_curve_changed: Option<CrossfadeCurve>,
    /// Play or stop the three-second crossfade audition.
    pub toggle_crossfade_preview: bool,
    pub loudness_mode_changed: Option<LoudnessMode>,

    // Output
    pub backend_changed: Option<AudioBackend>,
    pub device_changed: Option<String>,
    pub sample_rate_policy_changed: Option<SampleRatePolicy>,
    pub volume_mode_changed: Option<VolumeMode>,
    pub bit_perfect_changed: Option<bool>,
    pub dither_changed: Option<bool>,
    pub resampler_quality_changed: Option<ResamplerQuality>,
    pub fallback_policy_changed: Option<FallbackPolicy>,
    /// The engine processing profile changed (Consumer / Fidelity / Low power).
    pub audio_profile_changed: Option<domain::models::AudioProfileChoice>,
    pub precision_mode_changed: Option<PrecisionMode>,
    pub bass_management_changed: Option<BassManagementConfig>,
    pub lfe_changed: Option<LfeConfig>,
    pub channel_mix_changed: Option<ChannelMixConfig>,
    pub open_asio_panel: bool,
    pub clear_output_profile: bool,

    // Spatial
    /// The whole spatial stage was enabled or bypassed.
    pub spatial_enabled_changed: Option<bool>,
    pub spatial_quality_changed: Option<SpatialQuality>,
    pub hrtf_profile_changed: Option<String>,
    pub listener_tracking_changed: bool,
    pub spatial_room_changed: bool,
    pub spatial_screen_changed: bool,
    /// The listener's head orientation changed.
    pub spatial_listener_changed: bool,
    /// Air absorption was switched on or off.
    pub air_absorption_changed: Option<bool>,
    pub voice_budget_changed: bool,

    // Mix Bus
    pub add_track: Option<AudioSource>,
    pub remove_track: Option<u8>,
    pub track_gain_changed: Option<(u8, f32)>,
    pub track_pan_changed: Option<(u8, f32)>,
    pub track_mute_changed: Option<(u8, bool)>,
    pub track_active_changed: Option<(u8, bool)>,
    pub aux_changed: bool,
    pub duck_tracks: bool,

    // Chain presets (whole-chain named profiles)
    /// Save the current chain under this name (replacing a same-named preset).
    pub chain_preset_save: Option<String>,
    /// Apply the preset at this index.
    pub chain_preset_apply: Option<usize>,
    /// Delete the preset at this index.
    pub chain_preset_delete: Option<usize>,
    /// Rename the preset at this index to the given name.
    pub chain_preset_rename: Option<(usize, String)>,

    // CLAP plugin insert
    /// Open the plugin rack window.
    pub clap_open_rack: bool,
    /// Open the native picker for a `.clap` library file.
    pub clap_choose_library: bool,
    /// Unload the hosted plugin.
    pub clap_unload: bool,

    // Capture
    pub start_capture: bool,
    pub stop_capture: bool,

    // Tools: the workshop, CD, transcode, and opt-in updates
    pub open_room_measurement: bool,
    pub open_cd_ripper: bool,
    pub open_transcode_export: bool,
    pub open_update_check: bool,

    // Advanced
    pub stereo_enhancer_changed: Option<bool>,
    pub write_loudness_tags: Option<String>,
    pub recover_stream: bool,
    /// User pressed "Scan Library" — analyse every track lacking loudness data.
    pub scan_loudness_library: bool,
    /// User pressed "Cancel" during a loudness scan.
    pub cancel_loudness_scan: bool,

    // Library & Appearance
    pub trigger_scan: Option<PathBuf>,
    pub remove_scan_root: Option<ScanRootId>,
    pub toggle_theme: bool,
    /// The interface language changed. `None` means "follow the environment".
    pub locale_changed: Option<String>,
    /// User pressed "Import Songs…" — open the native file picker.
    pub import_files: bool,
    /// User pressed "Import Folder…" — open the native folder picker.
    pub import_folder: bool,
    /// User pressed "Open M3U…" — open the native playlist picker.
    pub import_playlist: bool,
    /// User pressed "Find Duplicates" — open the duplicate finder.
    pub find_duplicates: bool,
    /// User pressed "Fetch Missing Artwork".
    pub open_artwork_fetch: bool,

    // Themes & Gallery actions
    pub preview_theme: Option<String>,
    pub apply_theme: Option<String>,
    pub cancel_theme_preview: bool,
    pub reload_themes: bool,
    pub import_theme: bool,
    pub open_custom_themes_folder: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Extra Context for rich view
// ─────────────────────────────────────────────────────────────────────────────

/// The crossfade audition's live state, supplied by the app that owns the
/// preview output stream.
#[derive(Default, Clone, Copy)]
pub struct CrossfadePreviewContext<'a> {
    pub playing: bool,
    /// Downsampled envelope of the rendered demo, 0.0–1.0.
    pub peaks: &'a [f32],
    /// How far through the three-second loop the sound is, 0.0–1.0.
    pub progress: f32,
    pub error: Option<&'a str>,
}

#[derive(Default)]
pub struct SettingsExtraContext<'a> {
    /// Live preview state for the Track Transitions card.
    pub crossfade_preview: CrossfadePreviewContext<'a>,
    pub scan_roots: &'a [ScanRoot],
    pub track_count: usize,
    pub is_scanning: bool,
    pub is_dark_theme: bool,
    pub theme_manager: Option<&'a crate::theme::ThemeManager>,
    /// A native import picker is currently open (disables the import buttons).
    pub import_active: bool,
    /// Directories that individual songs were imported from, newest first.
    pub imported_dirs: &'a [PathBuf],
    /// Library-wide loudness scan is in progress.
    pub loudness_running: bool,
    /// Tracks analysed so far in the current/last loudness scan.
    pub loudness_done: usize,
    /// Total tracks queued in the current loudness scan.
    pub loudness_total: usize,
    /// Current/last loudness scan status line.
    pub loudness_current: &'a str,
    /// Whether any loudness scan has finished (successfully or not).
    pub loudness_has_run: bool,
    /// Round-trip output latency reported by the backend, in milliseconds.
    pub latency_ms: f32,
    /// Backend the settings asked for.
    pub requested_backend: Option<AudioBackend>,
    /// Backend the engine actually opened. Differs from
    /// [`Self::requested_backend`] when the native path was unavailable and the
    /// engine fell back — which the user needs to see, because the fallback is
    /// shared-mode.
    pub actual_backend: Option<AudioBackend>,
    /// The full negotiated output snapshot. Shared with the Bit-Perfect modal
    /// so both surfaces render exactly the same facts.
    pub output_info: Option<&'a engine::output::OutputInfo>,
    /// The output devices the *selected* backend can open.
    ///
    /// Each backend enumerates its own devices, so a device chosen from this
    /// list is the device the engine will actually open. A list built from
    /// cpal would be wrong for every native backend: cpal's WASAPI backend
    /// opens shared-mode streams, and the ASIO drivers are not in cpal's list
    /// at all.
    pub output_devices: &'a [OutputDeviceInfo],
    /// Mean DSP graph-rebuild cost in milliseconds.
    pub graph_build_ms: f64,
    /// Number of DSP graph rebuilds since startup.
    pub graph_builds: u64,
    /// The saved language preference.
    ///
    /// Empty means "follow the environment". The picker shows what is
    /// stored, not what is active, so the user can see that the choice is
    /// "the system decides" even when the system happens to speak German.
    pub locale: &'a str,
}

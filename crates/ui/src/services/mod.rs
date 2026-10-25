pub mod artwork_fetch_service;
pub mod artwork_service;
pub mod autotag_service;
pub mod biography_service;
pub mod control_service;
pub mod crossfade_preview;
pub mod import_service;
pub mod loudness_service;
pub mod lyrics_service;
pub mod media_tools_service;
pub mod mpris_service;
pub mod podcast_service;
pub mod preset_io_service;
pub mod scanner_service;
pub mod waveform_service;

pub use artwork_fetch_service::{ArtworkFetchEvent, ArtworkFetchProgress, ArtworkFetchService};
pub use artwork_service::ArtworkService;
pub use autotag_service::{
    AutotagMatch, AutotagOutcome, AutotagProgress, AutotagResult, AutotagService,
};
pub use biography_service::BiographyService;
pub use control_service::{
    activate_existing_instance, multi_instance_enabled, read_endpoint, send_control_command,
    set_multi_instance_enabled, ControlEndpoint, ControlRequest, ControlResponse, ControlService,
};
pub use crossfade_preview::CrossfadePreview;
pub use import_service::{ImportKind, ImportResult, ImportService};
pub use loudness_service::{LoudnessProgress, LoudnessService};
pub use lyrics_service::{LyricsService, LyricsSource};
pub use media_tools_service::{MediaToolEvent, MediaToolsService};
pub use mpris_service::{MprisCommand, MprisService, MprisSnapshot};
pub use podcast_service::{PodcastEvent, PodcastService};
pub use preset_io_service::{PresetIoEvent, PresetIoService};
pub use scanner_service::ScannerService;
pub use waveform_service::WaveformService;

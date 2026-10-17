use crate::ids::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LibraryScanEvent {
    Started { root: std::path::PathBuf },
    Progress { scanned: usize, added: usize, updated: usize },
    Completed { total_tracks: usize, elapsed_ms: u64 },
    Failed { error: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlayerEvent {
    TrackChanged { track_id: Option<TrackId> },
    StateChanged { is_playing: bool, is_paused: bool },
    PositionUpdated { position_secs: f32, duration_secs: f32 },
    VolumeChanged { volume: f32, is_muted: bool },
    QueueUpdated { length: usize },
    Error { message: String },
}

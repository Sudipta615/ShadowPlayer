use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LibraryScanEvent {
    Started { root: std::path::PathBuf },
    Progress { scanned: usize, added: usize, updated: usize },
    Completed { total_tracks: usize, elapsed_ms: u64 },
    Failed { error: String },
}

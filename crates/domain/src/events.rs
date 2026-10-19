use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LibraryScanEvent {
    Started { root: std::path::PathBuf },
    Progress { scanned: usize, added: usize, updated: usize },
    Completed {
        total_tracks: usize,
        elapsed_ms: u64,
        /// Files the walk saw but could not index (unreadable metadata, or a
        /// decoder that refused them). They are neither added nor removed, so
        /// without this count they simply vanished.
        skipped: usize,
    },
    Failed { error: String },
}

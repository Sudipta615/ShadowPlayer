//! Stable track identity across moves and renames.
//!
//! Track ids are derived from the file path, so a move used to create a brand
//! new track and orphan its play counts, favourites and playlist membership.
//! [`resolve_identity`] fixes that: before storing a file at a path we have not
//! seen before, it fingerprints the content and — if an existing track holds the
//! same content and its old file is gone — moves that track's row to the new
//! path, keeping its id.

use database::TrackRepo;
use domain::ids::TrackId;
use log::{info, warn};
use std::path::Path;

use crate::fingerprint::Fingerprint;

/// Fingerprint the file at `path` for a caller that already knows the row at
/// that path exists but may predate fingerprints. Best-effort.
pub fn backfill_fingerprint(repo: &TrackRepo, path: &Path, id: &TrackId) {
    if let Some(fp) = Fingerprint::compute(path) {
        let _ = repo.set_content_hash(id, Some(fp.as_str()));
    }
}

/// Work out the identity to store a file under.
///
/// * `path` — the file being indexed.
/// * `path_derived_id` — the id the caller computed from `path`.
/// * `already_indexed` — whether a row already exists for this exact path.
/// * `content_hash` — filled in with the file's fingerprint when computable.
///
/// Returns `Some(id)` when the track must be stored under a *different*
/// (pre-existing) id because the file was recognised as moved. Returns `None`
/// when the caller's own id is correct.
///
/// Relinking only happens when the old file no longer exists on disk, so a
/// duplicated copy of a track is still indexed as a separate track.
pub fn resolve_identity(
    repo: &TrackRepo,
    path: &Path,
    path_derived_id: TrackId,
    already_indexed: bool,
    content_hash: &mut Option<String>,
) -> Option<TrackId> {
    let fp = Fingerprint::compute(path)?;
    let hash = fp.into_string();
    *content_hash = Some(hash.clone());

    if already_indexed {
        return None;
    }

    // An ambiguous fingerprint is never used, so we cannot steal the wrong row.
    let (old_id, old_path) = repo.find_by_content_hash(&hash).ok().flatten()?;
    if old_path.as_path() == path || old_path.exists() {
        return None;
    }

    match repo.relink_path(&old_id, path, Some(&hash)) {
        Ok(true) => {
            info!(
                "Relinked moved track {} -> {}",
                old_path.display(),
                path.display()
            );
            Some(old_id)
        }
        Ok(false) => None,
        Err(e) => {
            warn!(
                "Could not relink {} -> {}: {}",
                old_path.display(),
                path.display(),
                e
            );
            // Fall through: the caller stores the file under its own id.
            let _ = path_derived_id;
            None
        }
    }
}

//! Small helpers for loading optional system libraries.
//!
//! The candidates are ordered from the most specific ABI name to the plain
//! developer name. Loaders themselves remain alive for as long as the symbol
//! table that points into them, which is why every wrapper owns its
//! [`libloading::Library`].

use crate::MediaError;

/// Try each candidate in order.
pub(crate) fn open(
    what: &'static str,
    library: &'static str,
    candidates: &[&str],
) -> Result<libloading::Library, MediaError> {
    let mut details = Vec::new();
    for candidate in candidates {
        match unsafe { libloading::Library::new(candidate) } {
            Ok(lib) => return Ok(lib),
            Err(error) => details.push(format!("{candidate}: {error}")),
        }
    }
    Err(MediaError::LibraryMissing {
        what,
        library,
        detail: details.join("; "),
    })
}

/// The names a system library may have on this platform.
pub(crate) fn linux_macos_candidates(stem: &str, soname: &str) -> Vec<String> {
    if cfg!(target_os = "macos") {
        vec![
            format!("lib{stem}.{soname}.dylib"),
            format!("lib{stem}.dylib"),
        ]
    } else {
        vec![format!("lib{stem}.so.{soname}"), format!("lib{stem}.so")]
    }
}

/// `true` when a missing shared library means the feature is unavailable —
/// not that the caller supplied invalid media.
pub fn is_missing_library(error: &MediaError) -> bool {
    matches!(error, MediaError::LibraryMissing { .. })
}

//! Opt-in update checks and hash-verified release downloads.
//!
//! This module never installs anything and never runs a downloaded file. Its
//! job is narrower: ask GitHub for the latest release of
//! [`DEFAULT_REPOSITORY`], choose a build for this platform, download it beside
//! the user's chosen destination, and refuse to hand back a path unless a
//! SHA-256 checksum published with the release matches the bytes on disk.
//!
//! The network is touched only when the caller constructs the client with
//! updates enabled and explicitly calls [`UpdateClient::check`]. There is no
//! background polling and no download without a checksum.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod client;
mod compare;
mod parse;

pub use client::*;
pub use compare::*;
pub use parse::*;

/// The project whose GitHub releases are checked.
pub const DEFAULT_REPOSITORY: &str = "Sudipta615/ShadowPlayer";

/// A release asset published on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A release as the update UI needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseInfo {
    /// Version without the leading `v`, for comparisons and display.
    pub version: String,
    /// The original tag, for linking back to the release page.
    pub tag: String,
    pub name: String,
    pub notes: String,
    pub page_url: String,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

/// The result of an update check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub current_version: String,
    pub release: ReleaseInfo,
    pub newer_available: bool,
    /// The asset that would be downloaded for this platform, if any.
    pub recommended_asset: Option<ReleaseAsset>,
}

/// A verified download. The file has not been installed or executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedDownload {
    pub path: PathBuf,
    pub version: String,
    pub asset: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Why a download was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadRefusal {
    MissingChecksum,
    ChecksumMismatch { expected: String, actual: String },
}

impl std::fmt::Display for DownloadRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingChecksum => write!(f, "no SHA-256 checksum was published for the asset"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "SHA-256 mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaError;
    use sha2::{Digest, Sha256};

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
            size: 10,
        }
    }

    #[test]
    fn disabled_checks_never_touch_the_network() {
        let client = UpdateClient::new(false);
        assert!(matches!(
            client.check("0.14.0"),
            Err(MediaError::UpdateDisabled)
        ));
    }

    #[test]
    fn a_release_response_becomes_a_version_and_asset_list() {
        let json = r#"{
            "tag_name": "v0.15.0",
            "name": "ShadowPlayer 0.15",
            "body": "Fixes",
            "html_url": "https://github.com/Sudipta615/ShadowPlayer/releases/tag/v0.15.0",
            "published_at": "2026-01-02T03:04:05Z",
            "assets": [
                {"name": "shadowplayer-x86_64-unknown-linux-gnu.tar.gz",
                 "browser_download_url": "https://example.invalid/linux.tar.gz",
                 "size": 1234}
            ]
        }"#;
        let release = parse_release(json).unwrap();
        assert_eq!(release.version, "0.15.0");
        assert_eq!(release.tag, "v0.15.0");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].size, 1234);
        assert_eq!(
            release.assets[0].url,
            "https://example.invalid/linux.tar.gz"
        );
    }

    #[test]
    fn the_wrong_architecture_is_never_selected() {
        let assets = vec![
            asset("shadowplayer-aarch64-apple-darwin.tar.gz"),
            asset("shadowplayer-x86_64-apple-darwin.tar.gz"),
            asset("shadowplayer-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let chosen = compatible_asset(&assets, "macos", "aarch64").unwrap();
        assert_eq!(chosen.name, "shadowplayer-aarch64-apple-darwin.tar.gz");
        let chosen = compatible_asset(&assets, "macos", "x86_64").unwrap();
        assert_eq!(chosen.name, "shadowplayer-x86_64-apple-darwin.tar.gz");
    }

    #[test]
    fn a_platform_without_a_build_gets_no_recommendation() {
        let assets = vec![asset("shadowplayer-x86_64-unknown-linux-gnu.tar.gz")];
        assert!(compatible_asset(&assets, "windows", "x86_64").is_none());
        assert!(compatible_asset(&assets, "macos", "aarch64").is_none());
    }

    #[test]
    fn an_appimage_is_preferred_to_a_tarball() {
        let assets = vec![
            asset("shadowplayer-x86_64-unknown-linux-gnu.tar.gz"),
            asset("shadowplayer-x86_64.AppImage"),
        ];
        let chosen = compatible_asset(&assets, "linux", "x86_64").unwrap();
        assert_eq!(chosen.name, "shadowplayer-x86_64.AppImage");
    }

    #[test]
    fn checksums_are_found_in_sidecars_and_listings() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_checksum(hash, "shadowplayer.tar.gz").as_deref(),
            Some(hash)
        );
        assert_eq!(
            parse_checksum(
                &format!("{hash}  shadowplayer.tar.gz\n"),
                "shadowplayer.tar.gz"
            )
            .as_deref(),
            Some(hash)
        );
        assert_eq!(
            parse_checksum(
                &format!("{hash} *shadowplayer.tar.gz\n"),
                "shadowplayer.tar.gz"
            )
            .as_deref(),
            Some(hash)
        );
        assert_eq!(
            parse_checksum(
                &format!("SHA256 (shadowplayer.tar.gz) = {hash}\n"),
                "shadowplayer.tar.gz"
            )
            .as_deref(),
            Some(hash)
        );
    }

    #[test]
    fn a_checksum_for_another_file_is_not_reused() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let text = format!("{hash}  another-file.tar.gz\n");
        assert!(parse_checksum(&text, "shadowplayer.tar.gz").is_none());
        assert!(parse_checksum("not a checksum", "shadowplayer.tar.gz").is_none());
    }

    #[test]
    fn sidecar_names_come_before_aggregate_listings() {
        let candidates = checksum_asset_candidates("shadowplayer.tar.gz");
        assert_eq!(candidates[0], "shadowplayer.tar.gz.sha256");
        assert!(candidates.iter().any(|name| name == "SHA256SUMS"));
    }

    #[test]
    fn version_comparison_understands_tags_and_prereleases() {
        assert!(version_is_newer("v0.15.0", "0.14.0"));
        assert!(version_is_newer("0.14.1", "0.14.0"));
        assert!(!version_is_newer("0.14.0", "0.14.0"));
        assert!(!version_is_newer("0.13.9", "0.14.0"));
        assert!(version_is_newer("0.14.0", "0.14.0-beta.1"));
        assert!(!version_is_newer("0.14.0-beta.1", "0.14.0"));
    }

    #[test]
    fn verification_rejects_a_modified_file() {
        let dir = std::env::temp_dir().join(format!(
            "sp_update_verify_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("release.bin");
        std::fs::write(&path, b"the release bytes").unwrap();
        let good = "3fba6b0c2d9ce8e6b6f1c2a5ab1f7e8d5a3f14d5c9e7b2a8d3c4e5f60718293a";
        // The literal was not computed from the payload: the test forces a
        // rejection instead of hoping the file is corrupt.
        let error = verify_file(&path, good).unwrap_err();
        assert!(matches!(error, MediaError::ChecksumMismatch { .. }));

        // The same check, using the digest of the actual bytes, succeeds.
        let digest = hex(&Sha256::digest(b"the release bytes"));
        verify_file(&path, &digest).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}

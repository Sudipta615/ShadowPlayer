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

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::MediaError;

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

/// A client for `https://api.github.com/repos/{repository}/releases/latest`.
///
/// `enabled` is not a cached preference; it is the caller's explicit consent
/// for this operation. A disabled client fails with
/// [`MediaError::UpdateDisabled`] before any socket is opened.
pub struct UpdateClient {
    repository: String,
    enabled: bool,
    agent: ureq::Agent,
}

impl UpdateClient {
    pub fn new(enabled: bool) -> Self {
        Self {
            repository: DEFAULT_REPOSITORY.to_string(),
            enabled,
            agent: build_agent(),
        }
    }

    /// A client over another repository, for tests and forks.
    pub fn with_repository(enabled: bool, repository: impl Into<String>) -> Self {
        Self {
            repository: repository.into(),
            enabled,
            agent: build_agent(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Fetch the latest release. Disabled clients refuse without networking.
    pub fn check(&self, current_version: &str) -> Result<UpdateCheck, MediaError> {
        if !self.enabled {
            return Err(MediaError::UpdateDisabled);
        }
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repository
        );
        let body = self.get_text(&url)?;
        let release = parse_release(&body)?;
        let recommended_asset = compatible_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
        .cloned();
        Ok(UpdateCheck {
            current_version: current_version.to_string(),
            newer_available: version_is_newer(&release.version, current_version),
            release,
            recommended_asset,
        })
    }

    /// Download `asset` from `release` into `directory`, verifying its
    /// published SHA-256 before it is given a final name.
    ///
    /// The checksum is mandatory. If a release has no checksum sidecar or
    /// checksum listing, this returns [`MediaError::MissingChecksum`] without
    /// downloading the asset. A mismatch removes the partial file.
    pub fn download_verified(
        &self,
        release: &ReleaseInfo,
        asset: &ReleaseAsset,
        directory: impl AsRef<Path>,
        overwrite: bool,
    ) -> Result<VerifiedDownload, MediaError> {
        if !self.enabled {
            return Err(MediaError::UpdateDisabled);
        }
        let expected = self.expected_checksum(release, &asset.name)?;
        let final_path = safe_destination(directory.as_ref(), &asset.name)?;
        if final_path.exists() && !overwrite {
            return Err(MediaError::InvalidInput(format!(
                "{} already exists; choose another folder or allow overwriting",
                final_path.display()
            )));
        }
        let bytes = self.download_to(&asset.url, &final_path, &expected)?;
        Ok(VerifiedDownload {
            path: final_path,
            version: release.version.clone(),
            asset: asset.name.clone(),
            sha256: expected,
            bytes,
        })
    }

    /// Look up and fetch the published checksum for `asset_name`.
    fn expected_checksum(
        &self,
        release: &ReleaseInfo,
        asset_name: &str,
    ) -> Result<String, MediaError> {
        let candidates = checksum_asset_candidates(asset_name);
        let mut last_error = None;
        for candidate in candidates {
            let Some(checksum_asset) = release
                .assets
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(&candidate))
            else {
                continue;
            };
            match self.get_text(&checksum_asset.url) {
                Ok(text) => {
                    if let Some(expected) = parse_checksum(&text, asset_name) {
                        return Ok(expected);
                    }
                    last_error = Some(format!(
                        "{} did not contain a checksum for {asset_name}",
                        checksum_asset.name
                    ));
                }
                Err(error) => last_error = Some(error.to_string()),
            }
        }
        match last_error {
            Some(detail) => Err(MediaError::MissingChecksum {
                version: format!("{} ({detail})", release.version),
            }),
            None => Err(MediaError::MissingChecksum {
                version: release.version.clone(),
            }),
        }
    }

    fn get_text(&self, url: &str) -> Result<String, MediaError> {
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| MediaError::Network(format!("GET {url}: {error}")))?;
        response
            .into_string()
            .map_err(|error| MediaError::Network(format!("reading {url}: {error}")))
    }

    /// Stream the response into a temporary sibling, hashing as it is written,
    /// and only rename it after the digest matches.
    fn download_to(
        &self,
        url: &str,
        destination: &Path,
        expected: &str,
    ) -> Result<u64, MediaError> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = temporary_sibling(destination);
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| MediaError::Network(format!("GET {url}: {error}")))?;
        let mut reader = response.into_reader();
        let mut hasher = Sha256::new();
        let mut bytes = 0u64;
        {
            let mut file = std::fs::File::create(&temp)?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = match reader.read(&mut buffer) {
                    Ok(read) => read,
                    Err(error) => {
                        drop(file);
                        let _ = std::fs::remove_file(&temp);
                        return Err(MediaError::Network(format!("downloading {url}: {error}")));
                    }
                };
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                if let Err(error) = file.write_all(&buffer[..read]) {
                    drop(file);
                    let _ = std::fs::remove_file(&temp);
                    return Err(MediaError::Io(error));
                }
                bytes += read as u64;
            }
            file.sync_all()?;
        }

        let actual = hex(&hasher.finalize());
        if !constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
            let _ = std::fs::remove_file(&temp);
            return Err(MediaError::ChecksumMismatch {
                path: destination.to_path_buf(),
                expected: expected.to_string(),
                actual,
            });
        }
        std::fs::rename(&temp, destination).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            MediaError::Io(error)
        })?;
        Ok(bytes)
    }
}

fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(concat!(
            "ShadowPlayer/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Sudipta615/ShadowPlayer)"
        ))
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(60))
        .build()
}

/// Parse a GitHub "latest release" response.
pub fn parse_release(json: &str) -> Result<ReleaseInfo, MediaError> {
    #[derive(Deserialize)]
    struct RawRelease {
        tag_name: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        html_url: String,
        #[serde(default)]
        published_at: Option<String>,
        #[serde(default)]
        assets: Vec<RawAsset>,
    }
    #[derive(Deserialize)]
    struct RawAsset {
        name: String,
        #[serde(default)]
        browser_download_url: String,
        #[serde(default)]
        url: String,
        #[serde(default)]
        size: u64,
    }

    let raw: RawRelease = serde_json::from_str(json)
        .map_err(|error| MediaError::Network(format!("unexpected release response: {error}")))?;
    let tag = raw.tag_name.trim().to_string();
    if tag.is_empty() {
        return Err(MediaError::Network(
            "the release response has no tag name".to_string(),
        ));
    }
    Ok(ReleaseInfo {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        tag,
        name: raw.name,
        notes: raw.body,
        page_url: raw.html_url,
        published_at: raw.published_at,
        assets: raw
            .assets
            .into_iter()
            .map(|asset| ReleaseAsset {
                name: asset.name,
                url: if asset.browser_download_url.is_empty() {
                    asset.url
                } else {
                    asset.browser_download_url
                },
                size: asset.size,
            })
            .collect(),
    })
}

/// Choose the best asset for `os` and `arch`, or nothing when the release
/// publishes no build for this platform.
///
/// Architecture markers win over packaging preferences: a release that ships
/// both x86_64 and aarch64 builds must never offer the wrong one. Assets with
/// checksum or signature suffixes are never candidates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchMatch {
    Exact,
    Neutral,
    Wrong,
}

pub fn compatible_asset<'a>(
    assets: &'a [ReleaseAsset],
    os: &str,
    arch: &str,
) -> Option<&'a ReleaseAsset> {
    let os = os.to_ascii_lowercase();
    let arch = normalize_arch(arch);
    let mut best: Option<(u32, &ReleaseAsset)> = None;
    for asset in assets {
        let name = asset.name.to_ascii_lowercase();
        if is_checksum_name(&name) || name.ends_with(".sig") || name.ends_with(".asc") {
            continue;
        }
        let Some(os_score) = name_os_score(&name, &os) else {
            continue;
        };
        let arch_match = name_arch_match(&name, arch);
        if arch_match == ArchMatch::Wrong {
            continue;
        }
        let extension_score = extension_score(&name, &os);
        let score = os_score
            + extension_score
            + if arch_match == ArchMatch::Exact {
                100
            } else {
                0
            };
        if score == 0 {
            continue;
        }
        if best
            .as_ref()
            .map(|(best_score, best_asset)| {
                score > *best_score || (score == *best_score && asset.name < best_asset.name)
            })
            .unwrap_or(true)
        {
            best = Some((score, asset));
        }
    }
    best.map(|(_, asset)| asset)
}

fn normalize_arch(arch: &str) -> &str {
    match arch.to_ascii_lowercase().as_str() {
        "x86_64" | "x86-64" | "amd64" | "x64" => "x86_64",
        "aarch64" | "arm64" | "armv8" => "aarch64",
        "x86" | "i686" | "i386" => "x86",
        _ => arch,
    }
}

fn name_os_score(name: &str, os: &str) -> Option<u32> {
    match os {
        "linux" => {
            if name.contains("linux") {
                Some(30)
            } else if name.ends_with(".appimage") || name.contains("appimage") {
                // An AppImage's name does not always carry "linux"; its
                // extension is platform evidence enough.
                Some(40)
            } else if name.ends_with(".deb") || name.ends_with(".rpm") {
                Some(15)
            } else {
                None
            }
        }
        "macos" | "darwin" => {
            if name.contains("macos") || name.contains("darwin") || name.contains("osx") {
                Some(30)
            } else if name.ends_with(".dmg") {
                Some(25)
            } else {
                None
            }
        }
        "windows" => {
            if name.contains("windows") || name.contains("win64") || name.contains("win32") {
                Some(30)
            } else if name.ends_with(".msi") || name.ends_with(".exe") {
                Some(25)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn name_arch_match(name: &str, arch: &str) -> ArchMatch {
    let has = |needle: &str| name.contains(needle);
    let x86 = has("x86_64") || has("x86-64") || has("amd64") || has("x64");
    let arm = has("aarch64") || has("arm64") || has("armv8");
    match arch {
        "x86_64" => {
            if arm && !x86 {
                ArchMatch::Wrong
            } else if x86 {
                ArchMatch::Exact
            } else {
                ArchMatch::Neutral
            }
        }
        "aarch64" => {
            if x86 && !arm {
                ArchMatch::Wrong
            } else if arm {
                ArchMatch::Exact
            } else {
                ArchMatch::Neutral
            }
        }
        _ => ArchMatch::Neutral,
    }
}

fn extension_score(name: &str, os: &str) -> u32 {
    match os {
        "linux" => {
            if name.ends_with(".appimage") {
                45
            } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                35
            } else if name.ends_with(".tar.xz") {
                34
            } else if name.ends_with(".deb") || name.ends_with(".rpm") {
                20
            } else {
                1
            }
        }
        "macos" | "darwin" => {
            if name.ends_with(".dmg") {
                40
            } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
                30
            } else {
                1
            }
        }
        "windows" => {
            if name.ends_with(".msi") {
                40
            } else if name.ends_with(".exe") {
                35
            } else if name.ends_with(".zip") {
                30
            } else {
                1
            }
        }
        _ => 0,
    }
}

fn is_checksum_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".sha256")
        || lower.ends_with(".sha256sum")
        || lower.ends_with(".sha256.txt")
        || lower.contains("checksum")
        || lower == "sha256sums"
        || lower == "sha256sums.txt"
}

/// Filenames that may hold the checksum for `asset_name`, most specific first.
pub fn checksum_asset_candidates(asset_name: &str) -> Vec<String> {
    vec![
        format!("{asset_name}.sha256"),
        format!("{asset_name}.sha256.txt"),
        format!("{asset_name}.sha256sum"),
        format!("{asset_name}.SHA256"),
        "SHA256SUMS".to_string(),
        "sha256sums.txt".to_string(),
        "checksums.txt".to_string(),
        "checksums.sha256".to_string(),
    ]
}

/// Find `asset_name`'s SHA-256 in a checksum file.
///
/// Accepts the three formats releases tend to use:
///
/// ```text
/// <hex>
/// <hex>  <file>
/// <hex> *<file>
/// SHA256 (<file>) = <hex>
/// ```
pub fn parse_checksum(text: &str, asset_name: &str) -> Option<String> {
    let wanted = asset_basename(asset_name);
    let mut bare = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, hash)) = parse_bsd_line(line) {
            if asset_basename(name).eq_ignore_ascii_case(wanted) {
                return Some(hash);
            }
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else { continue };
        if !is_sha256_hex(hash) {
            continue;
        }
        let hash = hash.to_ascii_lowercase();
        let file = parts
            .next()
            .map(|name| name.trim_start_matches('*'))
            .unwrap_or("");
        if file.is_empty() {
            bare = Some(hash);
        } else if asset_basename(file).eq_ignore_ascii_case(wanted) {
            return Some(hash);
        }
    }
    bare
}

fn parse_bsd_line(line: &str) -> Option<(&str, String)> {
    let rest = line.strip_prefix("SHA256 (")?;
    let (name, rest) = rest.split_once(')')?;
    let hash = rest.trim().strip_prefix('=')?.trim();
    is_sha256_hex(hash).then(|| (name.trim(), hash.to_ascii_lowercase()))
}

fn asset_basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Verify a file's SHA-256 against an expected digest.
pub fn verify_file(path: impl AsRef<Path>, expected: &str) -> Result<(), MediaError> {
    let path = path.as_ref();
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex(&hasher.finalize());
    if constant_time_eq(expected.to_ascii_lowercase().as_bytes(), actual.as_bytes()) {
        Ok(())
    } else {
        Err(MediaError::ChecksumMismatch {
            path: path.to_path_buf(),
            expected: expected.to_ascii_lowercase(),
            actual,
        })
    }
}

fn safe_destination(directory: &Path, asset_name: &str) -> Result<PathBuf, MediaError> {
    let name = asset_basename(asset_name).trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(MediaError::InvalidInput(
            "the release asset has no usable file name".to_string(),
        ));
    }
    Ok(directory.join(name))
}

/// Compare dotted versions. A leading `v`, build metadata and prerelease
/// suffixes are accepted; a stable release sorts after its prereleases.
pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    let (candidate_parts, candidate_pre) = split_version(candidate);
    let (current_parts, current_pre) = split_version(current);
    let length = candidate_parts.len().max(current_parts.len());
    for index in 0..length {
        let left = candidate_parts.get(index).copied().unwrap_or(0);
        let right = current_parts.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    match (candidate_pre.is_empty(), current_pre.is_empty()) {
        (false, true) => false,
        (true, false) => true,
        _ => candidate_pre > current_pre,
    }
}

fn split_version(version: &str) -> (Vec<u64>, String) {
    let trimmed = version.trim().trim_start_matches(['v', 'V']);
    let without_build = trimmed.split('+').next().unwrap_or(trimmed);
    let (numbers, prerelease) = without_build
        .split_once('-')
        .map(|(numbers, pre)| (numbers, pre.to_string()))
        .unwrap_or((without_build, String::new()));
    let parts = numbers
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect();
    (parts, prerelease)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

fn temporary_sibling(destination: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    destination.with_file_name(format!(".{name}.{}-{stamp}.part", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

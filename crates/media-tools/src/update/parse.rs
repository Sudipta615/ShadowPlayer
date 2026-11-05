// Release parsing, asset selection, checksum and file verification.
use super::compare::{constant_time_eq, hex};
use super::{ReleaseAsset, ReleaseInfo};
use crate::MediaError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

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

pub(crate) fn safe_destination(directory: &Path, asset_name: &str) -> Result<PathBuf, MediaError> {
    let name = asset_basename(asset_name).trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(MediaError::InvalidInput(
            "the release asset has no usable file name".to_string(),
        ));
    }
    Ok(directory.join(name))
}

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::compare::{constant_time_eq, hex, temporary_sibling};
use super::{
    checksum_asset_candidates, compatible_asset, parse_checksum, parse_release, safe_destination,
    version_is_newer, ReleaseAsset, ReleaseInfo, UpdateCheck, VerifiedDownload, DEFAULT_REPOSITORY,
};
use crate::MediaError;

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

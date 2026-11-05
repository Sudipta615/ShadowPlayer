// Version comparison — no I/O, dedicated per AGENTS 4.3.
use std::path::{Path, PathBuf};

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

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

pub(crate) fn temporary_sibling(destination: &Path) -> PathBuf {
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

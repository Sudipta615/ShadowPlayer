//! The versions of the workspace must move together.
//!
//! The player is eleven crates that are released as one thing, and the engine
//! is four more that are released in step with them. Bumping one manifest and
//! forgetting another produces a build whose version badge lies, and a
//! CHANGELOG that describes a release which does not exist.
//!
//! The bump is a chore that happens four times over the life of this plan, so
//! it is worth a test rather than a habit. This one fails the build when the
//! versions drift, when the CHANGELOG and the manifests disagree, or when a
//! README quotes a version that is no longer true.

use std::path::{Path, PathBuf};

/// The workspace root, from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the app crate lives two directories below the workspace root")
        .to_path_buf()
}

/// The player crates, which share one version.
const PLAYER_CRATES: &[&str] = &[
    "domain",
    "audio-session",
    "database",
    "metadata",
    "lyrics",
    "search",
    "library",
    "playback",
    "recommendation",
    "ui",
    "media-tools",
    "app",
    "clap-fixture-plugin",
];

/// The engine crates, which share one version among themselves.
///
/// The paths are relative to the engine's own workspace root.
const ENGINE_CRATES: &[&str] = &[".", "crates/config", "crates/plugin-abi"];

/// The `version = "…"` line of a manifest.
fn manifest_version(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    text.lines()
        .find(|line| line.trim_start().starts_with("version = "))
        .map(|line| {
            line.trim()
                .trim_start_matches("version = ")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .unwrap_or_else(|| panic!("{} has no version", path.display()))
}

#[test]
fn the_player_crates_share_one_version() {
    let root = repo_root();
    let versions: Vec<(String, String)> = PLAYER_CRATES
        .iter()
        .map(|name| {
            let manifest = root.join("crates").join(name).join("Cargo.toml");
            (name.to_string(), manifest_version(&manifest))
        })
        .collect();

    let first = &versions[0].1;
    for (name, version) in &versions {
        assert_eq!(
            version, first,
            "the player crates are released as one thing, so they must share a \
             version; {name} is {version} and {} is {first}",
            versions[0].0
        );
    }
}

#[test]
fn the_engine_crates_share_one_version() {
    let root = repo_root();
    let engine = root.join("crates/engine");
    let versions: Vec<(String, String)> = ENGINE_CRATES
        .iter()
        .map(|name| {
            let manifest = engine.join(name).join("Cargo.toml");
            (name.to_string(), manifest_version(&manifest))
        })
        .collect();
    let first = &versions[0].1;
    for (name, version) in &versions {
        assert_eq!(
            version, first,
            "the engine and its sub-crates move in lockstep; {name} is {version} \
             and {} is {first}",
            versions[0].0
        );
    }
}

#[test]
fn the_opus_decoder_keeps_its_own_version_line() {
    // `opus-decoder` is a fork of an upstream crate and is versioned
    // independently, on purpose. The lockstep policy does not apply to it.
    let manifest = repo_root().join("crates/engine/crates/opus-decoder/Cargo.toml");
    let version = manifest_version(&manifest);
    assert!(
        version.starts_with("0.1."),
        "the opus decoder stays on its own 0.1.x line, found {version}"
    );
}

#[test]
fn the_changelog_names_the_current_version() {
    let root = repo_root();
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
        .expect("CHANGELOG.md should be readable");
    let app_version = manifest_version(&root.join("crates/app/Cargo.toml"));

    // The newest released version heading in the changelog.
    let newest = changelog
        .lines()
        .filter_map(|line| {
            line.strip_prefix("## [")
                .and_then(|rest| rest.split(']').next())
                .filter(|v| !v.contains("Unreleased"))
                .map(|v| v.to_string())
        })
        .next()
        .unwrap_or_else(|| panic!("CHANGELOG.md has no released version heading"));

    assert_eq!(
        newest, app_version,
        "the newest CHANGELOG entry ({newest}) must be the version the \
         manifests carry ({app_version})"
    );

    // And the engine's version must be named in that same entry, because the
    // two are released together and the startup log prints both.
    let engine_version = manifest_version(&root.join("crates/engine/Cargo.toml"));
    let section = changelog
        .split(&format!("## [{newest}]"))
        .nth(1)
        .expect("the newest section exists");
    let section = section
        .split("\n## [")
        .next()
        .expect("the newest section is bounded");
    assert!(
        section.contains(&engine_version),
        "the {newest} changelog entry must name the engine version \
         ({engine_version}), because the two are released together"
    );
}

#[test]
fn the_engine_readme_quotes_the_current_engine_version() {
    let root = repo_root();
    let readme = std::fs::read_to_string(root.join("crates/engine/README.md"))
        .expect("the engine README should be readable");
    let engine_version = manifest_version(&root.join("crates/engine/Cargo.toml"));

    assert!(
        readme.contains(&engine_version),
        "the engine README's version badge must show {engine_version}"
    );
    // And it must not still be showing the version it had before.
    assert!(
        !readme.contains("version-1.3.1-blue.svg"),
        "the engine README still quotes the old version badge"
    );
}

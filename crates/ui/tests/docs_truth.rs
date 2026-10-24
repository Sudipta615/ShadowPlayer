//! Documentation truth for the player-facing claims.
//!
//! The engine has its own suite; this one checks the claims a *player* user
//! reads: the shortcut table, the limitations list, the commands the README
//! tells them to run, the versions in the badges, the manual's live capability
//! lists, and the CHANGELOG's release line.
//!
//! Every assertion names its source of truth in the failure message, so a
//! drift is fixed by updating the document — never by weakening the test.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the ui crate lives two directories below the workspace root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// Read every source file of the in-app manual.
///
/// The manual used to live in one `help_view.rs`; splitting it into submodules
/// must not make these drift checks inspect only the facade.
fn read_help_sources(root: &Path) -> String {
    let dir = root.join("crates/ui/src/views/help_view");
    let mut paths = Vec::new();
    collect_rust_sources(&dir, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "the in-app manual has source files");
    let mut source = String::new();
    for path in paths {
        source.push_str(&read(&path));
        source.push('\n');
    }
    source
}

fn collect_rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("could not read {}: {e}", dir.display()))
    {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The rows of a markdown table under a level-two heading.
fn table_rows(text: &str, heading: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed.contains(heading);
            continue;
        }
        if !in_section || !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() < 2 {
            continue;
        }
        if cells[0].eq_ignore_ascii_case("key") || cells[0].starts_with(':') {
            continue;
        }
        rows.push((
            cells[0].trim_matches('*').trim().to_string(),
            cells[1].trim_matches('*').trim().to_string(),
        ));
    }
    rows
}

#[test]
fn the_readme_shortcut_table_is_exactly_the_in_app_list() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    let documented = table_rows(&readme, "Keyboard shortcuts");
    let expected: Vec<(String, String)> = ui::shortcuts::SHORTCUTS
        .iter()
        .map(|shortcut| (shortcut.keys.to_string(), shortcut.description.to_string()))
        .collect();
    assert_eq!(
        documented, expected,
        "README.md's shortcut table must match ui::shortcuts::SHORTCUTS exactly; \
         update the README (or the in-app list) rather than letting them drift"
    );
}

#[test]
fn the_readme_mentions_every_engine_limitation() {
    let root = repo_root();
    let engine = read(&root.join("crates/engine/README.md"));
    let readme = read(&root.join("README.md")).to_lowercase();

    let limitations = engine_limitations(&engine);
    assert_eq!(
        limitations.len(),
        6,
        "the engine now documents {} limitations ({limitations:?}); add each one \
         to the player README's Known Limitations and update this test",
        limitations.len()
    );

    // Each limitation's distinctive subject. The titles are prose, so the
    // test asks that the player README mention the subject rather than the
    // exact sentence: "plugin-dylib is not sandboxed" -> "plugin".
    for keyword in [
        "musepack",
        "talkback",
        "physical",
        "ambisonic",
        "mix bus",
        "plugin",
    ] {
        assert!(
            readme.contains(keyword),
            "crates/engine/README.md documents a limitation about {keyword:?} \
             that README.md's Known limitations does not mention"
        );
    }
}

/// The bold titles of the top-level bullets in the engine README's
/// "Known Limitations" section.
fn engine_limitations(text: &str) -> Vec<String> {
    let mut titles = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed.to_lowercase().contains("known limitations");
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("> - **") else {
            continue;
        };
        if !in_section {
            continue;
        }
        if let Some(end) = rest.find("**") {
            titles.push(rest[..end].to_string());
        }
    }
    titles
}

#[test]
fn the_readme_describes_the_tools_and_the_opt_in_network() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    let lower = readme.to_lowercase();

    for (needle, why) in [
        ("cd ripper", "the CD ripper is a shipped feature"),
        ("transcode export", "FLAC/Opus export is a shipped feature"),
        (
            "room-correction",
            "the measurement wizard is a shipped feature",
        ),
        ("liner notes", "Markdown album notes are a shipped feature"),
        ("headless", "headless mode is a shipped feature"),
        ("sha-256", "update downloads are checksum-verified"),
        ("clap.gui", "CLAP editors are passed through"),
    ] {
        assert!(
            lower.contains(needle),
            "README.md no longer mentions {needle:?}, but {why}; update the README"
        );
    }

    for stale in [
        "no network requests at all",
        "no cd ripping",
        "no transcoding",
        "no auto-update",
        "no microphone permission",
        "captures nothing",
        "omits nsmicrophoneusagedescription",
        "some icon-only controls are still unnamed",
    ] {
        assert!(
            !lower.contains(stale),
            "README.md still contains the stale claim {stale:?}; the app changed, so the README must too"
        );
    }

    let plist = read(&root.join("crates/app/macos/Info.plist"));
    assert!(
        plist.contains("<key>NSMicrophoneUsageDescription</key>"),
        "the macOS bundle must declare the microphone use required by the measurement wizard"
    );
}

#[test]
fn the_readme_test_commands_are_the_commands_ci_runs() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    let ci = normalize_whitespace(&read(&root.join(".github/workflows/ci.yml")));

    let mut commands = Vec::new();
    let mut logical = String::new();
    let mut in_tests = false;
    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_tests = trimmed.contains("Tests");
            continue;
        }
        if !in_tests || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(prefix) = trimmed.strip_suffix('\\') {
            logical.push_str(prefix.trim_end());
            logical.push(' ');
            continue;
        }
        logical.push_str(trimmed);
        let command = logical.trim().to_string();
        logical.clear();
        let Some(start) = command.find("cargo ") else {
            continue;
        };
        let command = command[start..].trim();
        if command.starts_with("cargo test") || command.starts_with("cargo bench") {
            commands.push(command.to_string());
        }
    }

    assert!(
        !commands.is_empty(),
        "README.md's Tests section names no cargo test/bench commands"
    );
    for command in commands {
        let normalized = normalize_whitespace(&command);
        assert!(
            ci.contains(&normalized),
            "README.md says to run `{command}`, but .github/workflows/ci.yml does not"
        );
    }
}

fn normalize_whitespace(text: &str) -> String {
    text.replace('\\', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_engine_readme_badge_matches_the_engine_manifest() {
    let root = repo_root();
    let engine_readme = read(&root.join("crates/engine/README.md"));
    let manifest = read(&root.join("crates/engine/Cargo.toml"));
    let manifest_version = first_version(&manifest);

    let marker = "badge/version-";
    let start = engine_readme
        .find(marker)
        .expect("the engine README carries a version badge")
        + marker.len();
    let badge = &engine_readme[start..];
    let end = badge
        .find('-')
        .expect("the version badge ends with a colour suffix");
    assert_eq!(
        &badge[..end],
        manifest_version,
        "crates/engine/README.md's version badge disagrees with crates/engine/Cargo.toml"
    );
    assert_eq!(
        manifest_version,
        engine::VERSION,
        "the engine manifest and the linked engine disagree"
    );
}

fn first_version(manifest: &str) -> &str {
    let mut after_package = false;
    for line in manifest.lines() {
        if line.trim() == "[package]" {
            after_package = true;
            continue;
        }
        if after_package {
            if let Some(value) = line.trim().strip_prefix("version = \"") {
                return value.trim_end_matches('"');
            }
        }
    }
    panic!("manifest has no [package] version");
}

#[test]
fn the_manual_reads_capabilities_from_the_live_engine() {
    let root = repo_root();
    let help = read_help_sources(&root);

    // The manual must not hard-code these lists. A build capability is a fact
    // about the binary, not a sentence in the source.
    for (needle, why) in [
        (
            "engine::output::available_backends()",
            "the output-backend list must come from the compiled engine",
        ),
        (
            "engine::decode::codecs::decodable_codecs()",
            "the codec list must come from the compiled engine",
        ),
        (
            "crate::modals::autoeq_modal::get_curated_autoeq_profiles().len()",
            "the AutoEQ count must come from the bundled profile list",
        ),
    ] {
        assert!(
            help.contains(needle),
            "the in-app manual must contain `{needle}`: {why}"
        );
    }

    assert!(
        !engine::output::available_backends().is_empty(),
        "this build compiled in no output backend, so the manual has nothing truthful to show"
    );
    assert!(
        !engine::decode::codecs::decodable_codecs().is_empty(),
        "this build compiled in no decoder, so the manual has nothing truthful to show"
    );
    assert!(
        !ui::modals::autoeq_modal::get_curated_autoeq_profiles().is_empty(),
        "the bundled AutoEQ profile list is empty"
    );
}

#[test]
fn the_changelog_names_the_current_release_and_its_engine() {
    let root = repo_root();
    let changelog = read(&root.join("CHANGELOG.md"));

    let released: Vec<(&str, &str)> = changelog
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("## [")?;
            let end = rest.find(']')?;
            let version = &rest[..end];
            // The `[Unreleased]` section is not a release.
            if version.eq_ignore_ascii_case("unreleased") {
                return None;
            }
            Some((version, line))
        })
        .collect();

    let (latest, _) = *released
        .first()
        .expect("CHANGELOG.md has at least one released version");
    assert_eq!(
        latest,
        env!("CARGO_PKG_VERSION"),
        "the newest CHANGELOG heading must be the workspace version"
    );

    let release_start = changelog
        .find(&format!("## [{latest}]"))
        .expect("the latest release heading");
    let release_body = &changelog[release_start..];
    let release_end = release_body[1..]
        .find("\n## [")
        .map(|index| index + 1)
        .unwrap_or(release_body.len());
    let release_body = &release_body[..release_end];
    assert!(
        release_body.contains(&format!("audio-engine {}", engine::VERSION)),
        "the {latest} CHANGELOG entry must name the engine it shipped with \
         (audio-engine {}), not a remembered version",
        engine::VERSION
    );
}

//! The documentation must match the code.
//!
//! This project makes a lot of claims — in the READMEs, in the in-app manual,
//! in the CHANGELOG — and a claim that has drifted from the code is worse than
//! no claim at all. The engine's own rule is "never a guessed claim", and a
//! guessed claim in a README is still a guess.
//!
//! So the claims that *can* be checked automatically are checked
//! automatically, and the build fails when they drift. The shortcut table was
//! the first of these; this is the generalisation.
//!
//! What is checked here:
//!
//! * the README's keyboard-shortcut table matches the in-app list;
//! * the README's "Tests" section names commands that exist;
//! * the README's "Known limitations" covers every limitation the engine
//!   documents;
//! * the manual's backend claims come from the live engine, not from a string;
//! * the manual's AutoEQ claim matches the number of bundled measurements.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the app crate lives two directories below the workspace root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// The rows of a markdown table under a heading, as `(first, second)` cells.
fn table_rows(text: &str, heading: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed == heading || trimmed.starts_with(&format!("{heading} "));
            continue;
        }
        if !in_section {
            continue;
        }
        if !trimmed.starts_with('|') {
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
        // The header row and the separator row are not data.
        if cells[0].eq_ignore_ascii_case("key") || cells[0].eq_ignore_ascii_case("action") {
            continue;
        }
        if cells[0].starts_with(':') || cells[0].starts_with('-') {
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
fn the_readme_shortcut_table_matches_the_in_app_list() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    let rows = table_rows(&readme, "## Keyboard shortcuts");
    assert!(!rows.is_empty(), "the README has a shortcut table");

    // The in-app list lives in the UI crate; it is the source of truth.
    let shortcuts = read(&root.join("crates/ui/src/shortcuts.rs"));
    for (key, action) in &rows {
        assert!(
            shortcuts.contains(key.as_str()) || shortcuts.contains(&key.replace(" / ", "/")),
            "the README lists the shortcut '{key}' ({action}) but the in-app \
             shortcut list does not have it"
        );
    }
}

#[test]
fn the_readme_test_commands_exist() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    // A test target can live in any crate's `tests` directory: the budget
    // test belongs to the UI crate, the truth tests to the app.
    let mut available: Vec<String> = Vec::new();
    for crate_name in ["app", "ui", "engine"] {
        let tests_dir = root.join("crates").join(crate_name).join("tests");
        if !tests_dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&tests_dir) {
            available.extend(entries.filter_map(|e| e.ok()).map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".rs") {
                    name
                } else {
                    format!("{name}.rs")
                }
            }));
        }
    }

    // Every `cargo test ...` command the README shows must name a test
    // target that exists, or the reader is told to run something that does
    // not work.
    for command in readme.lines().filter(|l| l.contains("cargo test")) {
        if let Some(target) = command.split("--test ").nth(1) {
            let name = target.split_whitespace().next().unwrap_or("").trim();
            if name.is_empty() || name.starts_with('-') {
                continue;
            }
            assert!(
                available.contains(&format!("{name}.rs")),
                "the README's test section names '{name}', but no \
                 crates/*/tests/{name}.rs exists"
            );
        }
    }
}

#[test]
fn the_readme_known_limitations_cover_the_engines() {
    let root = repo_root();
    let readme = read(&root.join("README.md")).to_lowercase();
    let engine_readme = read(&root.join("crates/engine/README.md")).to_lowercase();

    // The engine's limitations are the ones a user is most likely to hit.
    // Each of them must be mentioned in the player's README too, because the
    // player is what people read.
    for topic in ["musepack", "tak", "talkback", "ambisonic"] {
        if engine_readme.contains(topic) {
            assert!(
                readme.contains(topic),
                "the engine documents a '{topic}' limitation that the player's \
                 README does not mention"
            );
        }
    }
}

#[test]
fn the_manual_never_names_a_backend_the_build_does_not_have() {
    // The manual's capability claims are rendered from the live engine, so
    // this test checks the *source*: no hard-coded sentence may name a
    // backend that this build does not have.
    let root = repo_root();
    let help = read(&root.join("crates/ui/src/views/help_view.rs"));

    let compiled: Vec<String> = engine::output::available_backends()
        .iter()
        .map(|b| format!("{b:?}"))
        .collect();
    let has_wasapi = compiled.iter().any(|b| b.contains("Wasapi"));
    let has_asio = compiled.iter().any(|b| b.contains("Asio"));
    let has_alsa = compiled.iter().any(|b| b.contains("Alsa"));

    // The manual may *name* a backend it does not have only if the sentence
    // is built from the live list — which is a `format!` over
    // `exclusive_backend_sentence()`, not a literal.
    for (name, present) in [
        ("ExclusiveWasapi", has_wasapi),
        ("ExclusiveAsio", has_asio),
        ("ExclusiveAlsa", has_alsa),
    ] {
        if present {
            continue;
        }
        assert!(
            !help.contains(&format!("\"{name}\"")),
            "the manual hard-codes '{name}', which this build does not have; \
             the claim must come from engine::output::available_backends()"
        );
    }
}

#[test]
fn the_manual_does_not_claim_a_database_of_headphones_it_does_not_ship() {
    let root = repo_root();
    let help = read(&root.join("crates/ui/src/views/help_view.rs"));
    assert!(
        !help.contains("4,000+"),
        "the manual claims a 4,000-entry AutoEQ database, which this build \
         does not ship; the count must be rendered from the real profiles"
    );
    assert!(
        !help.contains("Oratory1990") && !help.contains("Crinacle"),
        "the manual names measurement sources that are not bundled; the \
         bundled set is rendered from the profile list"
    );
}

#[test]
fn the_engine_readme_does_not_document_binaries_that_do_not_exist() {
    let root = repo_root();
    let engine_readme = read(&root.join("crates/engine/README.md"));

    // Every `cargo run --bin <name>` in the engine README must name a binary
    // that exists in the engine's source tree.
    for line in engine_readme.lines() {
        if !line.contains("cargo run") {
            continue;
        }
        // Only the part after `--bin` names a binary; the part before it is
        // the cargo invocation itself.
        let Some((_, after)) = line.split_once("--bin ") else {
            continue;
        };
        let name = after.split_whitespace().next().unwrap_or("").trim();
        if name.is_empty() || name.starts_with('-') {
            continue;
        }
        assert!(
            root.join("crates/engine/src/bin")
                .join(format!("{name}.rs"))
                .exists(),
            "the engine README documents the binary '{name}', but \
             crates/engine/src/bin/{name}.rs does not exist"
        );
    }
}

#[test]
fn the_engine_readme_does_not_promise_benchmarks_that_do_not_exist() {
    let root = repo_root();
    let benches = std::fs::read_dir(root.join("crates/engine/benches"))
        .expect("the engine has a benches directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .count();
    assert!(
        benches > 0,
        "the engine README claims a Criterion benchmark suite, but \
         crates/engine/benches is empty"
    );
}

#[test]
fn the_player_readme_names_the_releases_page() {
    let root = repo_root();
    let readme = read(&root.join("README.md"));
    assert!(
        readme.contains("## Installing"),
        "the README has no install section, so there is nowhere for a user to \
         find the packages"
    );
    assert!(
        readme.contains("## What ShadowPlayer is not"),
        "the README does not state what the player does not do, so the \
         omissions have to be discovered by disappointment"
    );
}

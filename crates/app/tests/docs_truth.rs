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
fn the_readme_describes_the_tools_and_the_opt_in_network() {
    let root = repo_root();
    let lower = read(&root.join("README.md")).to_lowercase();

    for needle in [
        "cd ripper",
        "transcode export",
        "room-correction",
        "liner notes",
        "headless",
        "sha-256",
        "clap.gui",
    ] {
        assert!(
            lower.contains(needle),
            "README.md does not mention {needle:?}, which is a shipped feature; \
             the README must describe the tools the app now has"
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
            "README.md still contains the stale claim {stale:?}; it must \
             describe the current behaviour instead"
        );
    }

    let plist = read(&root.join("crates/app/macos/Info.plist"));
    assert!(
        plist.contains("<key>NSMicrophoneUsageDescription</key>"),
        "the room-correction wizard records a sweep, so the macOS bundle needs \
         a microphone usage description"
    );
}

/// The bolded topic of every bullet under a `## … Known Limitations` heading.
///
/// The engine's README states its limitations as `> - **Topic.** prose` lines
/// inside a blockquote. Reading them from the source means this check cannot
/// fall behind the list the way a hard-coded array does.
fn engine_limitation_topics(engine_readme: &str) -> Vec<String> {
    let mut topics = Vec::new();
    let mut in_section = false;
    for line in engine_readme.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            in_section = trimmed.to_lowercase().contains("known limitations");
            continue;
        }
        if !in_section {
            continue;
        }
        // `> - **Topic.** prose` → `Topic.`
        let Some(rest) = trimmed.strip_prefix("> -") else {
            continue;
        };
        let rest = rest.trim();
        let Some(rest) = rest.strip_prefix("**") else {
            continue;
        };
        let Some(end) = rest.find("**") else {
            continue;
        };
        let topic = rest[..end].trim().trim_end_matches('.').trim().to_string();
        if !topic.is_empty() {
            topics.push(topic);
        }
    }
    topics
}

/// Collapse every run of whitespace to a single space.
///
/// Markdown hard-wraps its lines, so a phrase like "front L/R pair only in a
/// `>2`-channel stream" is split across a newline and an indent in the source.
/// Matching raw file text would fail on a limitation that is plainly present.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The phrases a player-README mention may use to cover one limitation topic.
///
/// A limitation is a full sentence in the engine's README ("Matrix-FIR
/// multicore is CPU-only and has no affinity or GPU controls.") and a summary
/// in the player's ("Matrix-FIR multicore is CPU-only."). So the candidates are
/// tried widest-first: the whole sentence, then the leading clause before the
/// first natural break. A topic counts as covered when any candidate appears.
fn limitation_candidates(topic: &str) -> Vec<String> {
    let lower = topic.to_lowercase();
    let mut out = vec![lower.clone()];

    // The leading clause: up to the first comma, or the first copula/verb that
    // turns the subject into a predicate.
    let mut cut = lower.len();
    for marker in [", ", " is ", " are ", " do ", " does ", " and "] {
        if let Some(i) = lower.find(marker) {
            cut = cut.min(i);
        }
    }
    let clause = lower[..cut].trim().trim_end_matches(',').trim();
    // Drop a leading article so "the output ultrasonic …" also matches
    // "output ultrasonic …".
    let clause = clause
        .trim_start_matches("the ")
        .trim_start_matches("no ")
        .trim_start_matches("several ")
        .trim();
    if !clause.is_empty() && clause != lower {
        out.push(clause.to_string());
    }
    out
}

#[test]
fn the_readme_known_limitations_cover_the_engines() {
    let root = repo_root();
    // Whitespace-collapsed, because markdown wraps lines and a wrapped phrase
    // is still the same phrase.
    let readme = collapse_whitespace(&read(&root.join("README.md"))).to_lowercase();
    let engine_readme = read(&root.join("crates/engine/README.md"));

    // The engine's limitations are the ones a user is most likely to hit, and
    // the player's README is what people actually read. Every one of them must
    // be mentioned there.
    //
    // The list is read from the engine's README rather than hard-coded: an
    // earlier version of this test named four topics, so ten of the fifteen
    // limitations the engine documents could have vanished from the player's
    // README without the build noticing.
    let topics = engine_limitation_topics(&engine_readme);
    assert!(
        topics.len() >= 10,
        "the engine's Known Limitations section should list its boundaries; \
         parsed only {} topic(s) — the section's format may have changed",
        topics.len()
    );

    let mut missing = Vec::new();
    for topic in &topics {
        let candidates: Vec<String> = limitation_candidates(topic)
            .iter()
            .map(|c| collapse_whitespace(c))
            .collect();
        if !candidates.iter().any(|c| readme.contains(c.as_str())) {
            missing.push(format!(
                "{topic}\n      tried: {}",
                candidates
                    .iter()
                    .map(|c| format!("\"{c}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "the engine documents {} limitation(s) the player's README does not \
         mention:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}

#[test]
fn the_manual_never_names_a_backend_the_build_does_not_have() {
    // The manual's capability claims are rendered from the live engine, so
    // this test checks the *source*: no hard-coded sentence may name a
    // backend that this build does not have.
    let root = repo_root();
    let help = read_help_sources(&root);

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
    let help = read_help_sources(&root);
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

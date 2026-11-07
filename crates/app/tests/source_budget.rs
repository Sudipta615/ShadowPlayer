//! The god-file guard from `AGENTS.md` §4.1.
//!
//! The contract allows a file to grow past ~800 lines only for a cohesive
//! reason, and puts a hard stop at ~1000. That was a rule people had to
//! remember, which is how three settings and theme files reached 918, 905 and
//! 876 lines. This test makes the upper bound mechanical: no hand-written
//! source file in the player workspace may exceed [`MAX_LINES`].
//!
//! Test sources are excluded. A table of cases is not a god object, and
//! `database/src/tests/library.rs` is deliberately a long list of repository
//! round-trips. The engine is a separate workspace with its own contract.

use std::path::{Path, PathBuf};

/// Hard cap for a hand-written player source file.
const MAX_LINES: usize = 1000;

/// Crates in the player workspace (the engine has its own `AGENTS.md`).
const PLAYER_CRATES: &[&str] = &[
    "app",
    "audio-session",
    "clap-fixture-plugin",
    "database",
    "domain",
    "library",
    "lyrics",
    "media-tools",
    "metadata",
    "playback",
    "recommendation",
    "search",
    "ui",
];

fn workspace_root() -> PathBuf {
    // `crates/app` → the repository root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/app has a parent twice")
        .to_path_buf()
}

fn collect_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Tests are their own kind of file; see the module docs.
            if path.file_name().and_then(|n| n.to_str()) == Some("tests") {
                continue;
            }
            collect_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && path.file_name().and_then(|n| n.to_str()) != Some("tests.rs")
        {
            out.push(path);
        }
    }
}

#[test]
fn no_hand_written_source_file_exceeds_the_god_file_cap() {
    let root = workspace_root();
    let mut files = Vec::new();
    for krate in PLAYER_CRATES {
        collect_sources(&root.join("crates").join(krate).join("src"), &mut files);
    }
    assert!(!files.is_empty(), "the scan found no sources at {root:?}");

    let mut offenders = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let lines = text.lines().count();
        if lines > MAX_LINES {
            let shown = file.strip_prefix(&root).unwrap_or(file);
            offenders.push(format!("{}: {lines} lines", shown.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "files over the {MAX_LINES}-line cap (AGENTS.md §4.1):\n  {}",
        offenders.join("\n  ")
    );
}

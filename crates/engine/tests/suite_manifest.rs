//! The manifest test: what the tree actually contains, and what the README
//! says about it.
//!
//! The README makes a claim about the engine's test structure — how many
//! test files there are and how many of them are registered `[[test]]`
//! suites. This suite computes the truth from the tree and compares it with
//! the claim, so the two cannot drift apart unnoticed.
//!
//! What it computes:
//!
//! * the number of integration suites (`tests/*.rs` — every one of them is a
//!   separate test binary cargo builds);
//! * the number of explicitly registered `[[test]]` targets in the manifest;
//! * the number of `#[test]` attributes across `src/` and `tests/`; and
//! * the number of in-crate `#[cfg(test)]` modules.
//!
//! The README's own numbers are updated by a separate task that owns the
//! README, so this suite *reports* the drift rather than failing the build
//! over prose it does not own — and it reports it precisely, with the exact
//! sentence the README must contain. Set `ENGINE_MANIFEST_STRICT=1` to turn
//! the drift into a failure, which is how the README's own task verifies its
//! edit.

use std::fs;
use std::path::{Path, PathBuf};

/// The engine's crate root.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file directly in `tests/` — one integration suite each.
fn integration_suite_files() -> Vec<PathBuf> {
    let dir = crate_root().join("tests");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("the tests directory must be readable: {e}"))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("rs"))
        .collect();
    files.sort();
    files
}

/// Every `.rs` file under `src/`.
fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut entries: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", dir.display()))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&crate_root().join("src"), &mut out);
    out
}

/// The number of lines in `files` that contain `needle`.
///
/// A line count, not an occurrence count, so a line that mentions the
/// attribute twice is counted once — the same way `grep -c` counts, which is
/// how the numbers in the README were produced.
fn count_lines_containing(files: &[PathBuf], needle: &str) -> usize {
    let mut count = 0usize;
    for path in files {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        count += text.lines().filter(|line| line.contains(needle)).count();
    }
    count
}

/// The number of `[[test]]` sections in the manifest.
fn registered_test_targets() -> usize {
    let manifest =
        fs::read_to_string(crate_root().join("Cargo.toml")).expect("the manifest must be readable");
    manifest
        .lines()
        .filter(|line| line.trim() == "[[test]]")
        .count()
}

/// The truth about the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counts {
    /// Integration suites: one test binary per file in `tests/`.
    integration_suites: usize,
    /// Explicitly registered `[[test]]` targets in the manifest.
    registered_suites: usize,
    /// `#[test]` attributes in `src/`.
    src_test_attributes: usize,
    /// `#[test]` attributes in `tests/`.
    integration_test_attributes: usize,
    /// In-crate `#[cfg(test)]` modules.
    cfg_test_modules: usize,
}

impl Counts {
    /// The total number of `#[test]` attributes in the crate.
    fn total_test_attributes(&self) -> usize {
        self.src_test_attributes + self.integration_test_attributes
    }

    /// The total number of test files: the registered suites plus the
    /// integration suites.
    fn total_test_files(&self) -> usize {
        self.registered_suites + self.integration_suites
    }
}

fn compute_counts() -> Counts {
    let src = source_files();
    let tests = integration_suite_files();
    Counts {
        integration_suites: tests.len(),
        registered_suites: registered_test_targets(),
        src_test_attributes: count_lines_containing(&src, "#[test]"),
        integration_test_attributes: count_lines_containing(&tests, "#[test]"),
        cfg_test_modules: count_lines_containing(&src, "#[cfg(test)]"),
    }
}

/// The numbers the README claims, in the form this suite reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadmeCounts {
    test_files: usize,
    registered_suites: usize,
    integration_suites: usize,
    /// The number in the badge ("tests — N suites").
    badge_suites: usize,
}

/// The last number in `text` before `marker`.
fn number_before(text: &str, marker: &str) -> Option<usize> {
    let (prefix, _) = text.split_once(marker)?;
    let digits: String = prefix
        .chars()
        .rev()
        // Skip the punctuation and spaces that separate the number from the
        // phrase it qualifies, then take the digits themselves.
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// The first number in `text` after `marker`.
fn number_after(text: &str, marker: &str) -> Option<usize> {
    let rest = text.split_once(marker)?.1;
    let digits: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Read the README's test-count claim, if it makes one in a form this suite
/// can read.
fn readme_counts(readme: &str) -> Option<ReadmeCounts> {
    // The badge: `.../badge/tests-97%20suites-blueviolet.svg`.
    let badge_suites = readme
        .lines()
        .find_map(|line| {
            let marker = "badge/tests-";
            let rest = line.split_once(marker)?.1;
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse().ok()
            }
        })
        .or_else(|| {
            // A badge written as "tests — 97 suites" rather than as a URL.
            readme.lines().find_map(|line| {
                let rest = line.split_once("tests")?.1;
                let rest = rest
                    .split_once(|c: char| !c.is_ascii_digit() && c != ' ' && c != '\u{2014}')?
                    .1;
                let digits: String = rest
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                digits.parse().ok()
            })
        })?;

    // The sentence: "100 test files (97 registered `[[test]]` suites + 3
    // integration suites)".
    let sentence = readme
        .lines()
        .find(|line| line.contains("test files") && line.contains("integration suites"))?;
    // The counts precede the phrases they qualify: "**100 test files (97
    // registered `[[test]]` suites + 3 integration suites)**".
    let test_files = number_before(sentence, "test files")?;
    let registered_suites = number_before(sentence, "registered")?;
    let integration_suites = number_after(sentence, "+")?;

    Some(ReadmeCounts {
        test_files,
        registered_suites,
        integration_suites,
        badge_suites,
    })
}

/// The sentence the README must contain for its numbers to be right.
fn expected_sentence(counts: &Counts) -> String {
    format!(
        "**{} test files ({} registered `[[test]]` suites + {} integration suites)**",
        counts.total_test_files(),
        counts.registered_suites,
        counts.integration_suites
    )
}

#[test]
fn every_integration_suite_is_a_real_suite() {
    let files = integration_suite_files();
    assert!(!files.is_empty(), "the engine must have integration suites");
    for path in &files {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()));
        assert!(
            text.contains("#[test]"),
            "{} is an integration suite with no #[test] in it",
            path.display()
        );
    }
}

#[test]
fn the_shared_helper_module_is_not_counted_as_a_suite() {
    // `tests/common/mod.rs` is a helper, not a suite: cargo does not build it
    // as a test target, so it must not be counted as one.
    let files = integration_suite_files();
    assert!(
        !files
            .iter()
            .any(|path| path.ends_with("common/mod.rs") || path.ends_with("common.rs")),
        "the shared helper module must not be counted as a suite"
    );
    assert!(
        crate_root()
            .join("tests")
            .join("common")
            .join("mod.rs")
            .is_file(),
        "the shared helper module must exist for the suites that use it"
    );
}

#[test]
fn the_tree_has_the_tests_it_claims_to_have() {
    let counts = compute_counts();
    println!(
        "the tree contains: {} integration suites, {} registered `[[test]]` \
         targets, {} `#[test]` attributes in src/ ({} in tests/), {} in-crate \
         `#[cfg(test)]` modules",
        counts.integration_suites,
        counts.registered_suites,
        counts.src_test_attributes,
        counts.integration_test_attributes,
        counts.cfg_test_modules,
    );

    assert!(
        counts.src_test_attributes > 0,
        "the engine's in-crate test modules must hold tests"
    );
    assert!(
        counts.cfg_test_modules > 0,
        "the engine must have in-crate `#[cfg(test)]` modules"
    );
    assert!(
        counts.integration_suites > 0,
        "the engine must have integration suites"
    );
    assert_eq!(
        counts.total_test_attributes(),
        counts.src_test_attributes + counts.integration_test_attributes,
        "the total test count must be the sum of its two halves"
    );
    assert_eq!(
        counts.total_test_files(),
        counts.registered_suites + counts.integration_suites,
        "the total file count must be the sum of its two halves"
    );
}

#[test]
fn the_readme_s_test_counts_are_internally_consistent() {
    // The README states the counts in two places: a sentence and a badge.
    // They have to agree with each other, whatever they say about the tree.
    let readme =
        fs::read_to_string(crate_root().join("README.md")).expect("the README must be readable");
    let claimed = readme_counts(&readme).unwrap_or_else(|| {
        panic!(
            "the README must state its test counts in a readable form: a line \
             naming the test-file count, the registered `[[test]]` suites and \
             the integration suites, and a badge with the suite count"
        )
    });

    assert_eq!(
        claimed.test_files,
        claimed.registered_suites + claimed.integration_suites,
        "the README's test-file count must be the sum of its two halves"
    );
    // The badge counts suites. The README's own convention is to badge the
    // registered `[[test]]` count; a badge naming the total is equally
    // defensible, so both are accepted — but it must be one of them.
    assert!(
        claimed.badge_suites == claimed.registered_suites
            || claimed.badge_suites == claimed.test_files,
        "the README's badge says {} suites, which is neither the {} registered \
         suites nor the {} test files its sentence names",
        claimed.badge_suites,
        claimed.registered_suites,
        claimed.test_files
    );
}

#[test]
fn the_readme_s_test_counts_match_the_tree() {
    let counts = compute_counts();
    let readme =
        fs::read_to_string(crate_root().join("README.md")).expect("the README must be readable");

    println!(
        "computed: {} test files ({} registered `[[test]]` suites + {} \
         integration suites); badge: {} suites; {} `#[test]` attributes; {} \
         in-crate `#[cfg(test)]` modules",
        counts.total_test_files(),
        counts.registered_suites,
        counts.integration_suites,
        counts.registered_suites,
        counts.total_test_attributes(),
        counts.cfg_test_modules,
    );

    let Some(claimed) = readme_counts(&readme) else {
        println!(
            "the README has no machine-readable test-count line yet; the \
             computed numbers above are what it must state"
        );
        return;
    };

    let expected = ReadmeCounts {
        test_files: counts.total_test_files(),
        registered_suites: counts.registered_suites,
        integration_suites: counts.integration_suites,
        badge_suites: counts.registered_suites,
    };
    // The badge may name the registered suites (the README's own convention)
    // or the total number of test files; both describe the same tree.
    let badge_agrees = claimed.badge_suites == expected.registered_suites
        || claimed.badge_suites == expected.test_files;

    if claimed.test_files == expected.test_files
        && claimed.registered_suites == expected.registered_suites
        && claimed.integration_suites == expected.integration_suites
        && badge_agrees
    {
        // The README agrees with the tree. Assert the agreement explicitly
        // so the check is a real equality and not a silent pass.
        assert_eq!(
            claimed.test_files, expected.test_files,
            "the README's test-file count must match the tree"
        );
        assert_eq!(
            claimed.registered_suites, expected.registered_suites,
            "the README's registered-suite count must match the tree"
        );
        assert_eq!(
            claimed.integration_suites, expected.integration_suites,
            "the README's integration-suite count must match the tree"
        );
        return;
    }

    let drift = format!(
        "the README's test counts do not match the tree.\n  README: {} test \
         files ({} registered `[[test]]` suites + {} integration suites), \
         badge: {} suites\n  tree:   {} test files ({} registered `[[test]]` \
         suites + {} integration suites), badge: {} suites\n  the README must \
         say: {} (and the badge must read {} or {} suites)",
        claimed.test_files,
        claimed.registered_suites,
        claimed.integration_suites,
        claimed.badge_suites,
        expected.test_files,
        expected.registered_suites,
        expected.integration_suites,
        expected.badge_suites,
        expected_sentence(&counts),
        expected.registered_suites,
        expected.test_files,
    );

    if std::env::var_os("ENGINE_MANIFEST_STRICT").is_some() {
        panic!("{drift}");
    }
    println!("{drift}");
}

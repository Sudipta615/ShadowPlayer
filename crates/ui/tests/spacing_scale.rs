//! Radii and padding come from the theme's scales, never from a literal.
//!
//! A skin can only restyle what the theme describes. A one-off
//! `CornerRadius::same(1)` or `Margin::symmetric(12, 7)` is a layout decision
//! baked into a view, and it will not move when the scale does.

use std::fs;
use std::path::{Path, PathBuf};

/// Calls whose arguments must be theme lookups rather than numbers.
const CALLS: &[&str] = &[
    "CornerRadius::same(",
    "CornerRadius::nw(",
    "CornerRadius::symmetric(",
    "Margin::same(",
    "Margin::symmetric(",
    "Margin::new(",
];

#[test]
fn every_radius_and_margin_comes_from_the_theme_scale() {
    let root = repo_root();
    let mut offenders = Vec::new();

    for path in rust_sources(&root.join("crates/ui/src")) {
        if path.file_name().is_some_and(|name| name == "theme.rs") {
            continue;
        }
        let source = read(&path);
        let source = without_tests(&source);
        for (index, line) in source.lines().enumerate() {
            let code = code_before_comment(line);
            for call in CALLS {
                if let Some(argument) = numeric_argument(code, call) {
                    offenders.push(format!(
                        "{}:{}: {call}{argument}) — use a theme.radius_* / theme.space_* token",
                        path.display(),
                        index + 1,
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "layout constants must come from the theme scales:\n{}",
        offenders.join("\n")
    );
}

/// If `code` contains `call` followed by a literal number, return the
/// argument text. A theme lookup (`theme.space_md as i8`) is not numeric.
fn numeric_argument<'a>(code: &'a str, call: &str) -> Option<&'a str> {
    let start = code.find(call)? + call.len();
    let rest = &code[start..];
    let end = rest.find(')')?;
    let argument = rest[..end].trim();
    if argument.is_empty() {
        return None;
    }
    let is_numeric = argument
        .chars()
        .all(|ch| ch.is_ascii_digit() || ch.is_ascii_whitespace() || ch == '.' || ch == ',');
    is_numeric.then_some(argument)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the ui crate lives two directories below the workspace root")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

fn rust_sources(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("could not read directory {}: {e}", path.display()))
        {
            let entry = entry.expect("directory entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Test modules are at the end of their file; fixture values there are not
/// painted by the app.
fn without_tests(source: &str) -> &str {
    match source.find("#[cfg(test)]") {
        Some(index) => &source[..index],
        None => source,
    }
}

fn code_before_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or("")
}

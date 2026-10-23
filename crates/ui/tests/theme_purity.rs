//! Colour can only enter the UI through [`ui::theme::Theme`].
//!
//! This is the preparation a future skin needs: if one view is allowed to
//! write `Color32::from_rgb(...)`, a skin cannot repaint it. The two
//! exceptions below are palette-independent primitives, not colours chosen by
//! a theme — document them in `docs/THEMING.md` whenever this list changes.

use std::fs;
use std::path::{Path, PathBuf};

/// Allowed outside `theme.rs`: white is the foreground drawn *on top of* an
/// accent fill in both themes, and transparent means "no fill". Neither is a
/// theme colour, and a skin must not change them.
const ALLOWED: &[&str] = &["Color32::WHITE", "Color32::TRANSPARENT"];

/// Literal constructors that always mean a hard-coded palette.
const FORBIDDEN: &[&str] = &[
    "Color32::from_rgb",
    "Color32::from_rgba",
    "Color32::from_rgba_premultiplied",
    "Color32::from_rgba_unmultiplied",
    "Color32::from_white",
    "Color32::from_black_alpha",
    "Color32::from_white_alpha",
    "Color32::from_gray",
    "Color32::from_gray_u8",
    "Color32::BLACK",
];

#[test]
fn no_colour_literal_exists_outside_the_theme() {
    let root = repo_root();
    let mut offenders = Vec::new();

    let theme_dir = root.join("crates/ui/src/theme");
    for path in rust_sources(&root.join("crates/ui/src")) {
        if path.starts_with(&theme_dir) || path.file_name().is_some_and(|name| name == "theme.rs") {
            continue;
        }
        let source = read(&path);
        let source = without_tests(&source);
        for (index, line) in source.lines().enumerate() {
            let code = code_before_comment(line);
            for forbidden in FORBIDDEN {
                if code.contains(forbidden) {
                    offenders.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "colours must come from ui::theme::Theme; these literals bypass a skin:\n{}",
        offenders.join("\n")
    );

    // Keep the allowlist honest and documented.
    let theming = read(&root.join("docs/THEMING.md"));
    for allowed in ALLOWED {
        assert!(
            theming.contains(allowed),
            "docs/THEMING.md must explain why {allowed} is allowed outside theme.rs"
        );
    }
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

/// Test modules are allowed to build fixture colours: they are not painted by
/// the app. Every test module in this crate is at the end of its file.
fn without_tests(source: &str) -> &str {
    match source.find("#[cfg(test)]") {
        Some(index) => &source[..index],
        None => source,
    }
}

/// Ignore anything after `//`, so a comment that names a forbidden constructor
/// does not fail the test.
fn code_before_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or("")
}

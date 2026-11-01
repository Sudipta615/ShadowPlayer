//! Regression test for the i18n build script's source scanner.
//!
//! The scanner reads Rust source as bytes. A Windows checkout that rewrote LF
//! as CRLF once made the `\` line continuation inside a `tr!` literal look
//! like an unknown escape, so the build failed only on Windows. The scanner
//! normalises line endings before parsing; this test feeds it both forms to
//! keep that true.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

const ENGLISH: &str = "Add a music folder and ShadowPlayer will scan it.";

/// A `tr!` call site whose literal is split with a `\` line continuation —
/// the shape that broke on CRLF checkouts. Written with LF and converted by
/// the caller.
const LF_SOURCE: &str = r#"fn empty_state() {
    let _ = tr!(
        "Add a music folder and ShadowPlayer \
         will scan it."
    );
}
"#;

fn scan_with_newline(newline: &str) {
    let key = build_script::message_key(ENGLISH);
    let mut catalogue = BTreeMap::new();
    catalogue.insert(key.clone(), ENGLISH.to_string());

    let source = LF_SOURCE.replace('\n', newline);
    let mut used = BTreeSet::new();
    build_script::scan_source(Path::new("fixture.rs"), &source, &catalogue, &mut used);

    assert_eq!(used.len(), 1, "the scanner must record exactly one key");
    assert!(used.contains(&key));
}

#[test]
fn lf_source_with_line_continuation_is_scanned() {
    scan_with_newline("\n");
}

#[test]
fn crlf_source_with_line_continuation_is_scanned() {
    scan_with_newline("\r\n");
}

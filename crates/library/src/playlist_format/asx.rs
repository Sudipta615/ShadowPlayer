use super::{
    all_tags, attr, decode_xml_entities, escape_xml, first_attr, path_for_playlist, tag_contents,
};
use super::{resolve, M3uEntry, ParsedPlaylist};
use std::path::Path;

pub(crate) fn parse_asx(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let title = tag_contents(text, "title")
        .into_iter()
        .next()
        .map(decode_xml_entities)
        .filter(|s| !s.trim().is_empty());

    let mut entries = Vec::new();
    for entry in tag_contents(text, "entry") {
        let Some(path) = first_attr(entry, "ref", "href") else {
            continue;
        };
        let path = resolve(&path, base_dir);
        let entry_title = tag_contents(entry, "title")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        entries.push(M3uEntry {
            path,
            duration_secs: None,
            artist: None,
            title: entry_title,
        });
    }

    // Some ASX files put `<ref>` straight under `<asx>` with no `<entry>`.
    if entries.is_empty() {
        for tag in all_tags(text, "ref") {
            if let Some(href) = attr(tag, "href") {
                entries.push(M3uEntry::new(resolve(&href, base_dir)));
            }
        }
    }

    ParsedPlaylist { title, entries }
}

pub(crate) fn write_asx(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from("<asx version=\"3.0\">\n  <title>ShadowPlayer playlist</title>\n");
    for entry in entries {
        out.push_str("  <entry>\n");
        if let Some(name) = entry.display_name() {
            out.push_str(&format!("    <title>{}</title>\n", escape_xml(&name)));
        }
        out.push_str(&format!(
            "    <ref href=\"{}\" />\n",
            escape_xml(&path_for_playlist(&entry.path, base_dir))
        ));
        out.push_str("  </entry>\n");
    }
    out.push_str("</asx>\n");
    out
}

// ── WPL ─────────────────────────────────────────────────────────────────────

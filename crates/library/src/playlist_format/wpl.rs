use super::{all_tags, attr, escape_xml, path_for_playlist, resolve, M3uEntry, ParsedPlaylist};
use std::path::Path;

pub(crate) fn parse_wpl(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let mut entries = Vec::new();
    for tag in all_tags(text, "media") {
        if let Some(src) = attr(tag, "src") {
            if !src.trim().is_empty() {
                entries.push(M3uEntry::new(resolve(&src, base_dir)));
            }
        }
    }
    ParsedPlaylist {
        title: None,
        entries,
    }
}

pub(crate) fn write_wpl(entries: &[M3uEntry], base_dir: Option<&Path>) -> String {
    let mut out = String::from(
        "<?wpl version=\"1.0\"?>\n\
         <smil>\n\
         \x20 <head>\n\
         \x20   <meta name=\"Generator\" content=\"ShadowPlayer\"/>\n\
         \x20 </head>\n\
         \x20 <body>\n\
         \x20   <seq>\n",
    );
    for entry in entries {
        out.push_str(&format!(
            "      <media src=\"{}\"/>\n",
            escape_xml(&path_for_playlist(&entry.path, base_dir))
        ));
    }
    out.push_str("    </seq>\n  </body>\n</smil>\n");
    out
}

// ── Shared XML helpers

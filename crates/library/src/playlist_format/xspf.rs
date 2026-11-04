use super::{
    decode_xml_entities, escape_xml, file_uri, resolve, tag_contents, M3uEntry, ParsedPlaylist,
};
use std::path::Path;

pub(crate) fn parse_xspf(text: &str, base_dir: Option<&Path>) -> ParsedPlaylist {
    let title = tag_contents(text, "title")
        .into_iter()
        .next()
        .map(decode_xml_entities)
        .filter(|s| !s.trim().is_empty());

    let mut entries = Vec::new();
    for track in tag_contents(text, "track") {
        let location = tag_contents(track, "location")
            .into_iter()
            .next()
            .map(decode_xml_entities);
        let Some(location) = location else {
            continue;
        };
        let location = location.trim();
        if location.is_empty() {
            continue;
        }
        let path = resolve(location, base_dir);
        let track_title = tag_contents(track, "title")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        let artist = tag_contents(track, "creator")
            .into_iter()
            .next()
            .map(decode_xml_entities)
            .filter(|s| !s.trim().is_empty());
        // XSPF durations are milliseconds; M3UEntry carries seconds.
        let duration_secs = tag_contents(track, "duration")
            .into_iter()
            .next()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|ms| ms / 1000);
        entries.push(M3uEntry {
            path,
            duration_secs,
            artist,
            title: track_title,
        });
    }

    ParsedPlaylist { title, entries }
}

pub(crate) fn write_xspf(entries: &[M3uEntry]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <playlist version=\"1\" xmlns=\"http://xspf.org/ns/0/\">\n\
         \x20 <title>ShadowPlayer playlist</title>\n\
         \x20 <trackList>\n",
    );
    for entry in entries {
        out.push_str("    <track>\n");
        out.push_str(&format!(
            "      <location>{}</location>\n",
            escape_xml(&file_uri(&entry.path))
        ));
        if let Some(title) = entry.title.as_deref() {
            out.push_str(&format!("      <title>{}</title>\n", escape_xml(title)));
        }
        if let Some(artist) = entry.artist.as_deref() {
            out.push_str(&format!(
                "      <creator>{}</creator>\n",
                escape_xml(artist)
            ));
        }
        if let Some(seconds) = entry.duration_secs {
            out.push_str(&format!("      <duration>{}</duration>\n", seconds * 1000));
        }
        out.push_str("    </track>\n");
    }
    out.push_str("  </trackList>\n</playlist>\n");
    out
}

// ── ASX ─────────────────────────────────────────────────────────────────────

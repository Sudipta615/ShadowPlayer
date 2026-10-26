//! Lightweight, robust APEv2 tag parser.
//!
//! Parses APEv2 tag footers and headers commonly found at the end of WavPack (`.wv`)
//! and Monkey's Audio (`.ape`) files, extracting editorial metadata (title, artist,
//! album, date, track/disc) and ReplayGain / EBU R128 loudness tags.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Maximum size of an APEv2 tag payload to prevent unbounded allocations (16 MiB).
const MAX_APEV2_TAG_BYTES: usize = 16 * 1024 * 1024;
const APEV2_MAGIC: &[u8; 8] = b"APETAGEX";
const APEV2_HEADER_LEN: u64 = 32;

/// A parsed APEv2 metadata tag containing key-value items.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Apev2Tag {
    pub items: Vec<(String, String)>,
}

impl Apev2Tag {
    /// Look up the first value for a key (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.get("Title")
    }

    pub fn artist(&self) -> Option<&str> {
        self.get("Artist")
    }

    pub fn album(&self) -> Option<&str> {
        self.get("Album")
    }

    pub fn album_artist(&self) -> Option<&str> {
        self.get("Album Artist").or_else(|| self.get("ALBUMARTIST"))
    }

    pub fn genre(&self) -> Option<&str> {
        self.get("Genre")
    }

    pub fn date(&self) -> Option<&str> {
        self.get("Year")
            .or_else(|| self.get("Date"))
            .or_else(|| self.get("YEAR"))
            .or_else(|| self.get("DATE"))
    }

    /// Track number parsed from "Track" or "TrackNumber" (supports "3" or "3/12").
    pub fn track_number(&self) -> Option<u32> {
        let val = self.get("Track").or_else(|| self.get("TrackNumber"))?;
        parse_number_slash(val).0
    }

    /// Total tracks parsed from "TrackTotal" or from "3/12".
    pub fn track_total(&self) -> Option<u32> {
        if let Some(total) = self
            .get("TrackTotal")
            .or_else(|| self.get("TotalTracks"))
            .and_then(|v| v.trim().parse::<u32>().ok())
        {
            return Some(total);
        }
        let val = self.get("Track").or_else(|| self.get("TrackNumber"))?;
        parse_number_slash(val).1
    }

    /// Disc number parsed from "Disc" or "DiscNumber" (supports "1" or "1/2").
    pub fn disc_number(&self) -> Option<u32> {
        let val = self.get("Disc").or_else(|| self.get("DiscNumber"))?;
        parse_number_slash(val).0
    }

    /// ReplayGain track gain in dB (e.g. from `REPLAYGAIN_TRACK_GAIN = -6.50 dB`).
    pub fn replaygain_track_gain(&self) -> Option<f32> {
        let val = self
            .get("REPLAYGAIN_TRACK_GAIN")
            .or_else(|| self.get("replaygain_track_gain"))?;
        parse_rg_db(val)
    }

    /// ReplayGain track peak ratio (e.g. from `REPLAYGAIN_TRACK_PEAK = 0.988257`).
    pub fn replaygain_track_peak(&self) -> Option<f32> {
        let val = self
            .get("REPLAYGAIN_TRACK_PEAK")
            .or_else(|| self.get("replaygain_track_peak"))?;
        val.trim().parse::<f32>().ok()
    }

    /// ReplayGain album gain in dB (e.g. from `REPLAYGAIN_ALBUM_GAIN = -5.20 dB`).
    pub fn replaygain_album_gain(&self) -> Option<f32> {
        let val = self
            .get("REPLAYGAIN_ALBUM_GAIN")
            .or_else(|| self.get("replaygain_album_gain"))?;
        parse_rg_db(val)
    }

    /// ReplayGain album peak ratio (e.g. from `REPLAYGAIN_ALBUM_PEAK = 0.999908`).
    pub fn replaygain_album_peak(&self) -> Option<f32> {
        let val = self
            .get("REPLAYGAIN_ALBUM_PEAK")
            .or_else(|| self.get("replaygain_album_peak"))?;
        val.trim().parse::<f32>().ok()
    }

    /// EBU R128 track gain in dB if tagged.
    pub fn r128_track_gain(&self) -> Option<f32> {
        let val = self
            .get("R128_TRACK_GAIN")
            .or_else(|| self.get("r128_track_gain"))?;
        parse_rg_db(val)
    }

    /// EBU R128 album gain in dB if tagged.
    pub fn r128_album_gain(&self) -> Option<f32> {
        let val = self
            .get("R128_ALBUM_GAIN")
            .or_else(|| self.get("r128_album_gain"))?;
        parse_rg_db(val)
    }
}

fn parse_number_slash(val: &str) -> (Option<u32>, Option<u32>) {
    let mut parts = val.split('/');
    let num = parts.next().and_then(|p| p.trim().parse::<u32>().ok());
    let total = parts.next().and_then(|p| p.trim().parse::<u32>().ok());
    (num, total)
}

fn parse_rg_db(val: &str) -> Option<f32> {
    let clean = val.trim();
    let clean = clean
        .strip_suffix("dB")
        .or_else(|| clean.strip_suffix("db"))
        .or_else(|| clean.strip_suffix("DB"))
        .unwrap_or(clean)
        .trim();
    clean.parse::<f32>().ok()
}

/// Read an APEv2 tag from a file path. Returns `Ok(None)` if no tag is present.
pub fn read_from_file(path: &Path) -> std::io::Result<Option<Apev2Tag>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    read_from_reader(&mut reader)
}

/// Read an APEv2 tag from a seekable reader. Returns `Ok(None)` if no tag is found.
pub fn read_from_reader<R: Read + Seek>(reader: &mut R) -> std::io::Result<Option<Apev2Tag>> {
    let total_len = reader.seek(SeekFrom::End(0))?;
    if total_len < APEV2_HEADER_LEN {
        return Ok(None);
    }

    // Check if the file ends with a 128-byte ID3v1 tag trailer ("TAG").
    let mut footer_end = total_len;
    if total_len >= 128 + APEV2_HEADER_LEN {
        reader.seek(SeekFrom::Start(total_len - 128))?;
        let mut id3_probe = [0u8; 3];
        if reader.read_exact(&mut id3_probe).is_ok() && &id3_probe == b"TAG" {
            footer_end = total_len - 128;
        }
    }

    if footer_end < APEV2_HEADER_LEN {
        return Ok(None);
    }

    // Check for APEv2 footer at footer_end - 32.
    reader.seek(SeekFrom::Start(footer_end - APEV2_HEADER_LEN))?;
    let mut footer = [0u8; 32];
    reader.read_exact(&mut footer)?;

    if &footer[..8] == APEV2_MAGIC {
        return parse_tag_payload(reader, &footer, footer_end, false);
    }

    // If not at the end, check for APEv2 header at the start of the file.
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0u8; 32];
    if reader.read_exact(&mut header).is_ok() && &header[..8] == APEV2_MAGIC {
        return parse_tag_payload(reader, &header, APEV2_HEADER_LEN, true);
    }

    Ok(None)
}

fn parse_tag_payload<R: Read + Seek>(
    reader: &mut R,
    descriptor: &[u8; 32],
    anchor_offset: u64,
    is_header: bool,
) -> std::io::Result<Option<Apev2Tag>> {
    let tag_size = u32::from_le_bytes(descriptor[12..16].try_into().unwrap()) as u64;
    let item_count = u32::from_le_bytes(descriptor[16..20].try_into().unwrap()) as usize;

    // Sanity checks: tag_size includes descriptor (32 bytes); must be bounded.
    if tag_size < APEV2_HEADER_LEN || tag_size as usize > MAX_APEV2_TAG_BYTES {
        return Ok(None);
    }
    if !is_header && tag_size > anchor_offset {
        return Ok(None);
    }

    let payload_len = (tag_size - APEV2_HEADER_LEN) as usize;
    let payload_start = if is_header {
        anchor_offset
    } else {
        anchor_offset - tag_size
    };

    reader.seek(SeekFrom::Start(payload_start))?;
    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;

    let mut items = Vec::with_capacity(item_count.min(1024));
    let mut offset = 0;

    for _ in 0..item_count {
        if offset + 8 > payload.len() {
            break;
        }
        let val_len = u32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap()) as usize;
        let item_flags = u32::from_le_bytes(payload[offset + 4..offset + 8].try_into().unwrap());
        offset += 8;

        // Key is a null-terminated ASCII string (1..=255 chars).
        let key_start = offset;
        while offset < payload.len() && payload[offset] != 0 {
            offset += 1;
        }
        if offset >= payload.len() {
            break;
        }
        let key = String::from_utf8_lossy(&payload[key_start..offset]).to_string();
        offset += 1; // Skip null terminator

        if offset + val_len > payload.len() {
            break;
        }

        // Only parse UTF-8 text items (flags & 6 == 0). Binary items (flags & 6 == 2) are skipped.
        if (item_flags & 6) == 0 {
            let val = String::from_utf8_lossy(&payload[offset..offset + val_len]).to_string();
            items.push((key, val));
        }

        offset += val_len;
    }

    Ok(Some(Apev2Tag { items }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn build_apev2_bytes(items: &[(&str, &str)]) -> Vec<u8> {
        let mut payload = Vec::new();
        for (k, v) in items {
            payload.extend_from_slice(&(v.len() as u32).to_le_bytes());
            payload.extend_from_slice(&0u32.to_le_bytes()); // flags: utf8 text
            payload.extend_from_slice(k.as_bytes());
            payload.push(0x00); // null terminator
            payload.extend_from_slice(v.as_bytes());
        }

        let tag_size = (payload.len() + 32) as u32;
        let item_count = items.len() as u32;

        let mut footer = Vec::new();
        footer.extend_from_slice(b"APETAGEX");
        footer.extend_from_slice(&2000u32.to_le_bytes()); // version 2000
        footer.extend_from_slice(&tag_size.to_le_bytes());
        footer.extend_from_slice(&item_count.to_le_bytes());
        footer.extend_from_slice(&0u32.to_le_bytes()); // flags
        footer.extend_from_slice(&[0u8; 8]); // reserved

        let mut file_bytes = vec![0x57u8; 100]; // dummy audio content
        file_bytes.extend_from_slice(&payload);
        file_bytes.extend_from_slice(&footer);
        file_bytes
    }

    #[test]
    fn test_parse_apev2_tag() {
        let raw = build_apev2_bytes(&[
            ("Title", "Comfortably Numb"),
            ("Artist", "Pink Floyd"),
            ("Album", "The Wall"),
            ("Album Artist", "Pink Floyd"),
            ("Year", "1979"),
            ("Track", "19/26"),
            ("Disc", "2/2"),
            ("Genre", "Progressive Rock"),
            ("REPLAYGAIN_TRACK_GAIN", "-6.50 dB"),
            ("REPLAYGAIN_TRACK_PEAK", "0.988257"),
            ("REPLAYGAIN_ALBUM_GAIN", "-5.20 dB"),
            ("REPLAYGAIN_ALBUM_PEAK", "0.999908"),
        ]);

        let mut cursor = Cursor::new(raw);
        let tag = read_from_reader(&mut cursor)
            .expect("read tag")
            .expect("tag found");

        assert_eq!(tag.title(), Some("Comfortably Numb"));
        assert_eq!(tag.artist(), Some("Pink Floyd"));
        assert_eq!(tag.album(), Some("The Wall"));
        assert_eq!(tag.album_artist(), Some("Pink Floyd"));
        assert_eq!(tag.date(), Some("1979"));
        assert_eq!(tag.track_number(), Some(19));
        assert_eq!(tag.track_total(), Some(26));
        assert_eq!(tag.disc_number(), Some(2));
        assert_eq!(tag.genre(), Some("Progressive Rock"));
        assert_eq!(tag.replaygain_track_gain(), Some(-6.50));
        assert_eq!(tag.replaygain_track_peak(), Some(0.988257));
        assert_eq!(tag.replaygain_album_gain(), Some(-5.20));
        assert_eq!(tag.replaygain_album_peak(), Some(0.999908));
    }

    #[test]
    fn test_parse_apev2_with_trailing_id3v1() {
        let mut raw = build_apev2_bytes(&[("Title", "Echoes"), ("Artist", "Pink Floyd")]);
        // Append 128-byte ID3v1 trailer
        let mut id3 = vec![0u8; 128];
        id3[0..3].copy_from_slice(b"TAG");
        raw.extend_from_slice(&id3);

        let mut cursor = Cursor::new(raw);
        let tag = read_from_reader(&mut cursor)
            .expect("read tag")
            .expect("tag found");
        assert_eq!(tag.title(), Some("Echoes"));
        assert_eq!(tag.artist(), Some("Pink Floyd"));
    }

    #[test]
    fn test_no_tag_returns_none() {
        let raw = vec![0u8; 256];
        let mut cursor = Cursor::new(raw);
        let tag = read_from_reader(&mut cursor).expect("read tag");
        assert_eq!(tag, None);
    }
}

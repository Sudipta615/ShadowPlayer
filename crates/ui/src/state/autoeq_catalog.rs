//! The shipped AutoEQ parametric corpus.
//!
//! The binary is produced by `scripts/generate-autoeq-corpus.py` from AutoEQ
//! data (MIT, see `assets/autoeq/ATTRIBUTION.md`). It is decoded on a worker
//! thread by [`crate::services::AutoEqCatalogService`]; the UI only ever holds
//! an `Arc` of this immutable catalog.
//!
//! The decoder treats the asset as untrusted input. It bounds-checks every
//! length and number, rejects unknown filter ids and truncated data, and never
//! returns a partially populated catalog. A malformed row is a load error, not
//! a silent gap: applying a different curve than the one named would be worse
//! than showing an explicit failure.

use config::{EqBandConfig, FilterType, ImportedCurve};

const MAGIC: &[u8; 6] = b"SHAEQ\x01";
const MAX_ENTRIES: u32 = 50_000;
const MAX_TEXT: usize = 1_024;
const MAX_BANDS: usize = 64;

/// The number advertised in Help and the README. A test decodes the shipped
/// asset and fails if the corpus and this number drift apart.
pub const SHIPPED_CURVE_COUNT: usize = 8_850;

/// The MIT notice for the AutoEQ data, shown in the in-app manual. The data is
/// compiled into the binary, so the notice must travel with it.
pub const AUTO_EQ_LICENSE: &str = "\
MIT License

Copyright (c) 2018 Jaakko Pasanen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

/// One headphone/IEM correction curve.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoEqEntry {
    /// Manufacturer and model, as AutoEQ names it.
    pub name: String,
    /// The measurement source, e.g. `Crinacle 711 in-ear`.
    pub source: String,
    pub is_iem: bool,
    /// Headroom derived from the peak of the combined filter response.
    pub preamp_db: f32,
    pub bands: Vec<EqBandConfig>,
    /// Lowercase name + source, precomputed once so typing in the search box
    /// does not allocate a string per entry per frame.
    search_key: String,
    /// Lowercase full name, precomputed for exact device-name matches.
    name_key: String,
    /// Lowercase name tokens, precomputed so a suggestion check never scans
    /// or re-lowercases the corpus. Matching is whole-token: `Zen` must not
    /// satisfy a query for `Zenith`.
    match_tokens: Vec<String>,
}

impl AutoEqEntry {
    /// The same import representation a text curve uses, so applying a corpus
    /// entry and importing a file take exactly one code path.
    pub fn to_imported_curve(&self) -> ImportedCurve {
        ImportedCurve {
            name: format!("{} ({})", self.name, self.source),
            preamp_db: self.preamp_db,
            bands: self.bands.clone(),
            graphic: Vec::new(),
        }
    }
}

/// Immutable, searchable collection of corpus entries.
#[derive(Debug, Default)]
pub struct AutoEqCatalog {
    entries: Vec<AutoEqEntry>,
    /// Device name → matched entry index. A suggestion scan touches every
    /// corpus entry, so the result is cached and invalidated implicitly: the
    /// catalog is immutable once decoded.
    suggestion_cache: std::sync::Mutex<std::collections::HashMap<String, Option<usize>>>,
}

/// Generic audio endpoint / interface terms that carry no headphone identity.
const SUGGESTION_STOP_WORDS: &[&str] = &[
    "usb",
    "audio",
    "dac",
    "codec",
    "hw",
    "card",
    "dev",
    "bluetooth",
    "headphone",
    "headphones",
    "headset",
    "earphones",
    "wireless",
    "analog",
    "digital",
    "output",
    "speaker",
    "speakers",
    "default",
    "line",
    "front",
    "rear",
    "surround",
    "stereo",
    "sink",
    "source",
    "alsa",
    "pulse",
    "pipewire",
    "wasapi",
    "coreaudio",
    "directsound",
    "interface",
    "device",
    "pro",
    "sound",
    "built-in",
    "pnp",
    "to",
    "the",
    "and",
    "with",
    "for",
    "adapter",
    "converter",
];

/// Tokenize a name for suggestion matching: lowercase, split on
/// non-alphanumerics, drop one-character fragments and generic endpoint
/// terms. `adapter` is dropped because every OS appends it.
fn suggestion_tokens(name: &str) -> Vec<String> {
    let mut tokens: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !SUGGESTION_STOP_WORDS.contains(t))
        .map(str::to_lowercase)
        .collect();
    tokens.sort_unstable();
    tokens.dedup();
    tokens
}

impl AutoEqCatalog {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[AutoEqEntry] {
        &self.entries
    }

    pub fn get(&self, index: usize) -> Option<&AutoEqEntry> {
        self.entries.get(index)
    }

    /// Indices whose name or measurement source contains `query`, in catalog
    /// order. An empty query returns every entry.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return (0..self.entries.len()).collect();
        }
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.search_key.contains(&query).then_some(index))
            .collect()
    }

    /// Attempt to find an AutoEQ profile that matches a connected device name.
    ///
    /// The device name is tokenized (generic driver/endpoint terms such as
    /// `usb`, `dac`, `alsa` carry no headphone identity), and a candidate must
    /// contain **every** device token as a whole token — substring matches are
    /// exactly how `Zen` used to match `Audio Zenith PMx2`. One device token
    /// alone is too ambiguous to suggest from (the `Sony` → `Sony DR-Z7` false
    /// positive), so a lone token matches only by full-name equality. The
    /// closest candidate (fewest unmatched tokens, then catalog order) wins,
    /// and results are cached per device name because the corpus is immutable.
    pub fn find_suggestion(&self, device_name: &str) -> Option<&AutoEqEntry> {
        let key = device_name.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        let index = {
            let mut cache = self
                .suggestion_cache
                .lock()
                .expect("the AutoEQ suggestion cache mutex");
            match cache.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(slot) => *slot.get(),
                std::collections::hash_map::Entry::Vacant(slot) => {
                    let matched = self.compute_suggestion(&key);
                    slot.insert(matched);
                    matched
                }
            }
        };
        index.and_then(|i| self.entries.get(i))
    }

    /// The uncached matcher. Returns the entry index, not the entry, so the
    /// cache stays key → index and the borrow never fights the mutex guard.
    fn compute_suggestion(&self, name_key: &str) -> Option<usize> {
        let tokens = suggestion_tokens(name_key);
        if tokens.is_empty() {
            return None;
        }
        let mut best: Option<(usize, usize)> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            let exact = entry.name_key == name_key;
            if !exact && tokens.len() < 2 {
                // A lone device token ("Sony") cannot disambiguate among
                // every entry that carries the token.
                continue;
            }
            if !exact && !tokens.iter().all(|t| entry.match_tokens.contains(t)) {
                continue;
            }
            if exact {
                return Some(index);
            }
            let extra = entry.match_tokens.len().saturating_sub(tokens.len());
            if best.is_none() || extra < best.unwrap().1 {
                best = Some((index, extra));
            }
        }
        best.map(|(index, _)| index)
    }

    /// Decode the flat asset format documented in `ATTRIBUTION.md`.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes, pos: 0 };
        if reader.take(MAGIC.len())? != MAGIC {
            return Err("the AutoEQ corpus has the wrong magic".to_string());
        }
        let count = reader.u32()?;
        if count > MAX_ENTRIES {
            return Err(format!("the AutoEQ corpus claims {count} entries"));
        }

        let mut entries = Vec::with_capacity(count as usize);
        for index in 0..count as usize {
            let name = reader.text()?;
            if name.is_empty() {
                return Err(format!("AutoEQ entry {index} has an empty name"));
            }
            let source = reader.text()?;
            let flags = reader.u8()?;
            if flags & !1 != 0 {
                return Err(format!(
                    "AutoEQ entry {index} has unknown flags 0x{flags:02X}"
                ));
            }
            let preamp_db = reader.f32()?;
            if !preamp_db.is_finite() || !(-60.0..=24.0).contains(&preamp_db) {
                return Err(format!(
                    "AutoEQ entry {index} has an invalid preamp {preamp_db}"
                ));
            }
            let band_count = reader.u8()? as usize;
            if band_count == 0 || band_count > MAX_BANDS {
                return Err(format!("AutoEQ entry {index} has {band_count} bands"));
            }

            let mut bands = Vec::with_capacity(band_count);
            for band_index in 0..band_count {
                let kind = reader.u8()?;
                let filter_type = match kind {
                    0 => FilterType::Peaking,
                    1 => FilterType::LowShelf,
                    2 => FilterType::HighShelf,
                    other => {
                        return Err(format!(
                            "AutoEQ entry {index} band {band_index} has filter kind {other}"
                        ));
                    }
                };
                let frequency = reader.f32()?;
                let gain_db = reader.f32()?;
                let q = reader.f32()?;
                if !frequency.is_finite() || !(1.0..=24_000.0).contains(&frequency) {
                    return Err(format!(
                        "AutoEQ entry {index} band {band_index} has frequency {frequency}"
                    ));
                }
                if !gain_db.is_finite() || !(-30.0..=30.0).contains(&gain_db) {
                    return Err(format!(
                        "AutoEQ entry {index} band {band_index} has gain {gain_db}"
                    ));
                }
                if !q.is_finite() || !(0.05..=20.0).contains(&q) {
                    return Err(format!("AutoEQ entry {index} band {band_index} has Q {q}"));
                }
                bands.push(EqBandConfig {
                    enabled: true,
                    filter_type,
                    frequency,
                    gain_db,
                    q,
                });
            }

            entries.push(AutoEqEntry {
                name,
                source,
                is_iem: flags & 1 != 0,
                preamp_db,
                bands,
                search_key: String::new(),
                name_key: String::new(),
                match_tokens: Vec::new(),
            });
        }

        if reader.pos != bytes.len() {
            return Err(format!(
                "the AutoEQ corpus has {} trailing bytes",
                bytes.len() - reader.pos
            ));
        }

        // Build the search keys after decoding so the immutable catalog never
        // re-lowercases 8,850 names per keystroke.
        for entry in &mut entries {
            entry.search_key = format!("{} {}", entry.name, entry.source).to_lowercase();
            entry.name_key = entry.name.trim().to_lowercase();
            entry.match_tokens = suggestion_tokens(&entry.name);
        }
        Ok(Self {
            entries,
            suggestion_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "the AutoEQ corpus is truncated".to_string())?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f32(&mut self) -> Result<f32, String> {
        let bytes = self.take(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn text(&mut self) -> Result<String, String> {
        let len = self.u16()? as usize;
        if len > MAX_TEXT {
            return Err(format!("the AutoEQ corpus has a {len}-byte text field"));
        }
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| "the AutoEQ corpus contains invalid UTF-8".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(kind: u8, freq: f32, gain: f32, q: f32) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&freq.to_le_bytes());
        out.extend_from_slice(&gain.to_le_bytes());
        out.extend_from_slice(&q.to_le_bytes());
        out
    }

    type TestEntry<'a> = (&'a str, &'a str, bool, f32, Vec<Vec<u8>>);

    fn asset(entries: &[TestEntry<'_>]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (name, source, is_iem, preamp, bands) in entries {
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(source.len() as u16).to_le_bytes());
            out.extend_from_slice(source.as_bytes());
            out.push(u8::from(*is_iem));
            out.extend_from_slice(&preamp.to_le_bytes());
            out.push(bands.len() as u8);
            for band in bands {
                out.extend_from_slice(band);
            }
        }
        out
    }

    fn sample() -> Vec<u8> {
        asset(&[
            (
                "Truthear x Crinacle Zero RED",
                "Crinacle 711 in-ear",
                true,
                -5.4,
                vec![band(1, 105.0, 2.6, 0.7), band(2, 10_000.0, -4.7, 0.7)],
            ),
            (
                "Sennheiser HD 600",
                "Oratory1990 over-ear",
                false,
                -3.1,
                vec![band(0, 1_000.0, -2.0, 1.4)],
            ),
        ])
    }

    #[test]
    fn decodes_names_sources_types_and_preamp() {
        let catalog = AutoEqCatalog::decode(&sample()).unwrap();
        assert_eq!(catalog.len(), 2);
        let entry = catalog.get(0).unwrap();
        assert_eq!(entry.name, "Truthear x Crinacle Zero RED");
        assert_eq!(entry.source, "Crinacle 711 in-ear");
        assert!(entry.is_iem);
        assert!((entry.preamp_db + 5.4).abs() < 1e-6);
        assert_eq!(entry.bands[0].filter_type, FilterType::LowShelf);
        assert_eq!(entry.bands[1].filter_type, FilterType::HighShelf);
    }

    #[test]
    fn search_matches_name_and_source_without_rescanning_case() {
        let catalog = AutoEqCatalog::decode(&sample()).unwrap();
        assert_eq!(catalog.search("truthear"), vec![0]);
        assert_eq!(catalog.search("ORATORY"), vec![1]);
        assert_eq!(catalog.search("headphone"), Vec::<usize>::new());
        assert_eq!(catalog.search(""), vec![0, 1]);
    }

    #[test]
    fn imported_curve_carries_every_band_and_the_stored_preamp() {
        let catalog = AutoEqCatalog::decode(&sample()).unwrap();
        let curve = catalog.get(0).unwrap().to_imported_curve();
        assert_eq!(curve.bands.len(), 2);
        assert!(!curve.is_graphic());
        assert!((curve.preamp_db + 5.4).abs() < 1e-6);
        assert!(curve.name.contains("Crinacle"));
    }

    #[test]
    fn every_truncation_is_an_error_not_a_partial_catalog() {
        let asset = sample();
        for end in 0..asset.len() {
            assert!(
                AutoEqCatalog::decode(&asset[..end]).is_err(),
                "a {end}-byte prefix decoded as a catalog"
            );
        }
        assert!(AutoEqCatalog::decode(&asset).is_ok());
    }

    #[test]
    fn unknown_filter_ids_and_trailing_bytes_are_rejected() {
        let mut bad_filter = sample();
        let last = bad_filter.len() - 13;
        bad_filter[last] = 9;
        assert!(AutoEqCatalog::decode(&bad_filter).is_err());

        let mut trailing = sample();
        trailing.push(0);
        assert!(AutoEqCatalog::decode(&trailing).is_err());
    }

    #[test]
    fn empty_names_and_absurd_counts_are_rejected() {
        let mut empty_name = sample();
        // The first name length is at offset 10.
        empty_name[10] = 0;
        empty_name[11] = 0;
        assert!(AutoEqCatalog::decode(&empty_name).is_err());

        let mut absurd = MAGIC.to_vec();
        absurd.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(AutoEqCatalog::decode(&absurd).is_err());
    }

    #[test]
    fn the_shipped_corpus_decodes_and_matches_its_advertised_count() {
        let catalog =
            AutoEqCatalog::decode(include_bytes!("../../assets/autoeq/autoeq_parametric.bin"))
                .expect("the shipped AutoEQ corpus must decode");
        assert_eq!(catalog.len(), SHIPPED_CURVE_COUNT);
        for entry in catalog.entries() {
            assert!(!entry.name.is_empty());
            assert!(!entry.source.is_empty());
            assert_eq!(entry.bands.len(), 10, "{}", entry.name);
            assert!(entry.preamp_db <= 0.0, "{}", entry.name);
        }
    }
}

#[cfg(test)]
mod suggestion_tests {
    use super::*;

    fn band(kind: u8, freq: f32, gain: f32, q: f32) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&freq.to_le_bytes());
        out.extend_from_slice(&gain.to_le_bytes());
        out.extend_from_slice(&q.to_le_bytes());
        out
    }

    type Row<'a> = (&'a str, &'a str, bool, f32, Vec<Vec<u8>>);

    fn one_band() -> Vec<Vec<u8>> {
        vec![band(0, 1_000.0, -2.0, 1.4)]
    }

    /// A synthetic corpus shaped like the real one: brands share vocabulary
    /// and model numbers repeat across families, which is what let near
    /// misses resolve to a wrong curve.
    fn catalog() -> AutoEqCatalog {
        let rows: Vec<Row> = vec![
            (
                "EPOS Sennheiser GSP 600",
                "Oratory1990 over-ear",
                false,
                -1.0,
                one_band(),
            ),
            (
                "Sennheiser HD 600",
                "Oratory1990 over-ear",
                false,
                -3.1,
                one_band(),
            ),
            (
                "Creative Sound Blaster EVO ZxR",
                "Claria over-ear",
                false,
                -1.0,
                one_band(),
            ),
            (
                "Logitech G Pro X (3.5mm jack)",
                "Crinacle",
                false,
                -1.0,
                one_band(),
            ),
            (
                "Audio Zenith PMx2 (v2)",
                "Oratory1990 over-ear",
                false,
                -1.0,
                one_band(),
            ),
            (
                "Sony DR-Z7",
                "Oratory1990 over-ear",
                false,
                -1.0,
                one_band(),
            ),
        ];
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        for (name, source, is_iem, preamp, bands) in rows {
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(source.len() as u16).to_le_bytes());
            out.extend_from_slice(source.as_bytes());
            out.push(u8::from(is_iem));
            out.extend_from_slice(&preamp.to_le_bytes());
            out.push(bands.len() as u8);
            for b in bands {
                out.extend_from_slice(&b);
            }
        }
        AutoEqCatalog::decode(&out).unwrap()
    }

    #[test]
    fn near_miss_devices_yield_no_suggestion() {
        let catalog = catalog();
        for device in [
            // `BTD` is not a token of `EPOS Sennheiser GSP 600`.
            "Sennheiser BTD 600",
            // `GC7` is not `EVO ZxR`.
            "Creative Sound Blaster GC7",
            // No corpus entry carries `apple`.
            "Apple USB-C to 3.5mm Headphone Jack Adapter",
            // `Zen` is not the whole token `Zenith`.
            "iFi Zen DAC V2",
        ] {
            let suggested = catalog.find_suggestion(device).map(|e| e.name.clone());
            assert_eq!(suggested, None, "{device:?} must not suggest a curve");
        }
    }

    #[test]
    fn a_lone_brand_token_is_too_ambiguous_to_suggest() {
        let catalog = catalog();
        assert_eq!(
            catalog.find_suggestion("Sony").map(|e| e.name.clone()),
            None
        );
        // The same brand with its model is a complete token match.
        assert_eq!(
            catalog
                .find_suggestion("sony dr z7")
                .map(|e| e.name.clone()),
            Some("Sony DR-Z7".to_string())
        );
    }

    #[test]
    fn exact_names_and_complete_token_matches_resolve() {
        let catalog = catalog();
        assert_eq!(
            catalog
                .find_suggestion("Sony DR-Z7")
                .map(|e| e.name.clone()),
            Some("Sony DR-Z7".to_string())
        );
        assert_eq!(
            catalog
                .find_suggestion("sennheiser hd 600")
                .map(|e| e.name.clone()),
            Some("Sennheiser HD 600".to_string())
        );
        // Every device token is a whole token of the entry name.
        assert_eq!(
            catalog
                .find_suggestion("EPOS GSP 600")
                .map(|e| e.name.clone()),
            Some("EPOS Sennheiser GSP 600".to_string())
        );
    }

    #[test]
    fn the_shipped_corpus_rejects_the_known_false_positives() {
        let catalog =
            AutoEqCatalog::decode(include_bytes!("../../assets/autoeq/autoeq_parametric.bin"))
                .expect("the shipped AutoEQ corpus must decode");

        for device in [
            "Sennheiser BTD 600",
            "Creative Sound Blaster GC7",
            "Apple USB-C to 3.5mm Headphone Jack Adapter",
            "iFi Zen DAC V2",
            "Sony",
        ] {
            let suggested = catalog.find_suggestion(device).map(|e| e.name.clone());
            assert!(
                suggested.is_none(),
                "{device:?} must not match any curve, got {suggested:?}"
            );
        }

        for (device, expect) in [
            ("Sennheiser HD 600", "HD 600"),
            ("Beyerdynamic DT 1990 Pro", "DT 1990"),
        ] {
            let name = catalog.find_suggestion(device).map(|e| e.name.clone());
            assert!(
                name.as_ref().is_some_and(|n| n.contains(expect)),
                "{device:?} should resolve to {expect}, got {name:?}"
            );
        }
    }
}

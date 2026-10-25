//! Text EQ-curve interchange.
//!
//! Audiophiles trade curves as text, not as application state. The three
//! dialects that matter are:
//!
//! * **AutoEQ parametric** — `Preamp: -6.2 dB` followed by
//!   `Filter 1: ON PK Fc 105 Hz Gain 5.5 dB Q 0.70`.
//! * **Equalizer APO / REW** — the same `Filter:` grammar without the index,
//!   with `LS`/`HS` aliases and `Fc 1.0 kHz`.
//! * **AutoEQ graphic** — `GraphicEQ: 20 -1.2; 21 -1.1; …`.
//!
//! Parsing is pure and allocation-bounded: callers read the file on a worker
//! thread and hand the text here. Values are clamped to the ranges the
//! biquad stage can actually run ([`MAX_FREQ_HZ`], [`MIN_Q`], [`MAX_GAIN_DB`]),
//! so a malformed file cannot install a filter that is unstable or inaudible.
//!
//! The module deliberately does **not** know about files, dialogs, or the
//! database. Import/export is `ui`'s job; this is the format layer.

use std::fmt;

use crate::{EqBandConfig, EqPreset, FilterType};

/// Highest frequency an imported band may specify.
pub const MAX_FREQ_HZ: f32 = 24_000.0;
/// Lowest frequency an imported band may specify.
pub const MIN_FREQ_HZ: f32 = 1.0;
/// Lowest Q an imported band may specify.
pub const MIN_Q: f32 = 0.1;
/// Highest Q an imported band may specify.
pub const MAX_Q: f32 = 12.0;
/// Largest gain an imported band may specify (the engine's extended range).
pub const MAX_GAIN_DB: f32 = 24.0;

/// A curve read from a text file.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedCurve {
    /// Name suggested by the file (usually its stem).
    pub name: String,
    /// Shared preamp, in dB. Files that omit it import as unity.
    pub preamp_db: f32,
    /// Parametric bands. Empty for a graphic curve.
    pub bands: Vec<EqBandConfig>,
    /// Graphic (frequency, gain) points. Empty for a parametric curve.
    pub graphic: Vec<(f32, f32)>,
}

impl ImportedCurve {
    /// True when the file carried graphic points rather than filter bands.
    pub fn is_graphic(&self) -> bool {
        self.bands.is_empty() && !self.graphic.is_empty()
    }

    /// The number of points or bands the file carried.
    pub fn point_count(&self) -> usize {
        if self.is_graphic() {
            self.graphic.len()
        } else {
            self.bands.len()
        }
    }

    /// Convert a parametric curve into an [`EqPreset`].
    pub fn into_preset(self) -> EqPreset {
        EqPreset {
            name: self.name,
            output_device_pattern: None,
            preamp_db: self.preamp_db,
            bands: self.bands,
        }
    }
}

/// Why a text file did not produce a curve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EqTextError {
    /// The file had no non-comment content.
    Empty,
    /// No line matched a supported grammar.
    NoCurve,
    /// A header was present but the payload was unusable.
    Malformed(&'static str),
}

impl fmt::Display for EqTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the file is empty"),
            Self::NoCurve => f.write_str(
                "no EQ curve was found (expected AutoEQ, Equalizer APO, REW, or GraphicEQ text)",
            ),
            Self::Malformed(reason) => write!(f, "malformed EQ curve: {reason}"),
        }
    }
}

impl std::error::Error for EqTextError {}

/// Parse any supported text dialect.
///
/// The dialect is detected from content, never from the file extension: an
/// `.txt` from AutoEQ and a `.txt` from REW are both valid, and a renamed file
/// still imports. A file may contain both a `GraphicEQ:` line and `Filter:`
/// lines; the graphic payload wins because that is what the header promised.
pub fn parse_curve(name: &str, text: &str) -> Result<ImportedCurve, EqTextError> {
    if text.trim().is_empty() {
        return Err(EqTextError::Empty);
    }

    let mut preamp_db = 0.0_f32;
    let mut bands: Vec<EqBandConfig> = Vec::new();
    let mut graphic: Vec<(f32, f32)> = Vec::new();
    let mut saw_graphic = false;

    for raw_line in text.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();

        if let Some(rest) = lower.strip_prefix("preamp:") {
            if let Some(value) = first_number(rest) {
                preamp_db = clamp_gain(value as f32);
            }
            continue;
        }

        if let Some(payload) = lower.strip_prefix("graphiceq:") {
            saw_graphic = true;
            parse_graphic_payload(payload, &mut graphic);
            continue;
        }

        if lower.starts_with("filter") {
            if let Some(band) = parse_filter_line(line) {
                bands.push(band);
            }
            continue;
        }
    }

    // A `GraphicEQ:` header makes the file graphic even when our pair parser
    // rejected every point; reporting "no curve" beats silently importing the
    // zero filter bands that happened to follow it.
    if saw_graphic {
        normalize_graphic(&mut graphic);
        if graphic.len() < 2 {
            return Err(EqTextError::Malformed(
                "GraphicEQ header with fewer than two points",
            ));
        }
        return Ok(ImportedCurve {
            name: name.to_string(),
            preamp_db,
            bands: Vec::new(),
            graphic,
        });
    }

    if bands.is_empty() {
        if graphic.is_empty() {
            return Err(EqTextError::NoCurve);
        }
        normalize_graphic(&mut graphic);
        if graphic.len() < 2 {
            return Err(EqTextError::NoCurve);
        }
        return Ok(ImportedCurve {
            name: name.to_string(),
            preamp_db,
            bands: Vec::new(),
            graphic,
        });
    }

    Ok(ImportedCurve {
        name: name.to_string(),
        preamp_db,
        bands,
        graphic: Vec::new(),
    })
}

/// Write a parametric preset in AutoEQ's `ParametricEQ.txt` grammar.
pub fn write_parametric(preset: &EqPreset) -> String {
    let mut out = String::new();
    if preset.preamp_db.abs() > 0.001 {
        out.push_str(&format!("Preamp: {:.1} dB\n", preset.preamp_db));
    }
    for (index, band) in preset.bands.iter().enumerate() {
        let code = filter_code(band.filter_type);
        let gain = if matches!(
            band.filter_type,
            FilterType::LowPass | FilterType::HighPass | FilterType::AllPass
        ) {
            0.0
        } else {
            band.gain_db
        };
        out.push_str(&format!(
            "Filter {}: {} {} Fc {:.1} Hz Gain {:.1} dB Q {:.2}\n",
            index + 1,
            if band.enabled { "ON" } else { "OFF" },
            code,
            band.frequency,
            gain,
            band.q
        ));
    }
    out
}

/// Write a graphic curve in AutoEQ's `GraphicEQ.txt` grammar.
pub fn write_graphic(freqs: &[f32], gains: &[f32]) -> String {
    let mut out = String::from("GraphicEQ: ");
    for (index, (freq, gain)) in freqs.iter().zip(gains.iter()).enumerate() {
        if index > 0 {
            out.push_str("; ");
        }
        out.push_str(&format!("{freq:.1} {gain:.1}"));
    }
    out.push('\n');
    out
}

fn parse_filter_line(line: &str) -> Option<EqBandConfig> {
    let cleaned = line.replace(',', " ");
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // `Filter N: OFF …` describes a slot the file explicitly disabled. It is
    // not part of the curve and must not be imported as a 0 dB band.
    if tokens.iter().any(|t| t.eq_ignore_ascii_case("off")) {
        return None;
    }

    let filter_type = tokens
        .iter()
        .find_map(|token| filter_type_from_token(token.trim_matches(':')))?;

    let frequency = value_after(&tokens, "fc")
        .or_else(|| value_after(&tokens, "f"))
        .unwrap_or(1_000.0);
    let gain = value_after(&tokens, "gain").unwrap_or(0.0);
    let q = value_after(&tokens, "q").unwrap_or_else(|| default_q(filter_type));

    if !frequency.is_finite() || !gain.is_finite() || !q.is_finite() {
        return None;
    }

    Some(EqBandConfig {
        enabled: true,
        filter_type,
        frequency: clamp_freq(frequency as f32),
        gain_db: clamp_gain(gain as f32),
        q: clamp_q(q as f32),
    })
}

/// Read the numeric token that follows `key`, tolerating a unit suffix.
///
/// `Fc 100 Hz` and `Fc 1.0 kHz` are both common; the unit is either glued to
/// the number (`100Hz`) or a separate token (`100 Hz`), and both spellings
/// must land on the same frequency.
fn value_after(tokens: &[&str], key: &str) -> Option<f64> {
    let position = tokens
        .iter()
        .position(|t| t.trim_matches(':').eq_ignore_ascii_case(key))?;
    let raw = tokens.get(position + 1)?;
    let value = parse_number_with_unit(raw)?;
    let scale = if has_unit(raw) {
        1.0
    } else {
        tokens
            .get(position + 2)
            .map_or(1.0, |unit| unit_scale(unit))
    };
    Some(value * scale)
}

fn has_unit(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.ends_with("hz") || lower.ends_with('k')
}

fn unit_scale(token: &str) -> f64 {
    let lower = token
        .trim()
        .trim_matches(|c: char| !c.is_ascii_alphabetic())
        .to_ascii_lowercase();
    match lower.as_str() {
        "khz" | "k" => 1_000.0,
        _ => 1.0,
    }
}

fn parse_number_with_unit(raw: &str) -> Option<f64> {
    let trimmed = raw.trim().trim_matches(|c: char| {
        !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
            && !c.eq_ignore_ascii_case(&'k')
    });
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let (number, scale) = if let Some(stripped) = lower.strip_suffix("khz") {
        (stripped, 1_000.0)
    } else if let Some(stripped) = lower.strip_suffix("k") {
        (stripped, 1_000.0)
    } else if let Some(stripped) = lower.strip_suffix("hz") {
        (stripped, 1.0)
    } else {
        (lower.as_str(), 1.0)
    };
    number.parse::<f64>().ok().map(|value| value * scale)
}

fn first_number(text: &str) -> Option<f64> {
    text.split_whitespace()
        .find_map(parse_number_with_unit)
        .or_else(|| {
            let token = text
                .split_whitespace()
                .find(|t| t.chars().any(|c| c.is_ascii_digit()))?;
            parse_number_with_unit(token)
        })
}

fn parse_graphic_payload(payload: &str, out: &mut Vec<(f32, f32)>) {
    for segment in payload.split([';', ',']) {
        let mut parts = segment.split_whitespace();
        let (Some(freq), Some(gain)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Some(freq), Some(gain)) = (parse_number_with_unit(freq), parse_number_with_unit(gain))
        else {
            continue;
        };
        if !freq.is_finite() || !gain.is_finite() {
            continue;
        }
        out.push((clamp_freq(freq as f32), clamp_gain(gain as f32)));
    }
}

fn normalize_graphic(points: &mut Vec<(f32, f32)>) {
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5);
}

fn strip_comment(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with(';') {
        return "";
    }
    match line.find('#') {
        Some(index) => &line[..index],
        None => line,
    }
}

fn filter_type_from_token(token: &str) -> Option<FilterType> {
    match token.trim_matches(':').to_ascii_uppercase().as_str() {
        "PK" | "PEQ" | "PEAK" | "PEAKING" | "BELL" => Some(FilterType::Peaking),
        "LS" | "LSC" | "LC" | "LOWSHELF" | "LOW_SHELF" | "LOW-SHELF" => Some(FilterType::LowShelf),
        "HS" | "HSC" | "HC" | "HIGHSHELF" | "HIGH_SHELF" | "HIGH-SHELF" => {
            Some(FilterType::HighShelf)
        }
        "LP" | "LPF" | "LOWPASS" | "LOW_PASS" | "LOW-PASS" => Some(FilterType::LowPass),
        "HP" | "HPF" | "HIGHPASS" | "HIGH_PASS" | "HIGH-PASS" => Some(FilterType::HighPass),
        "NO" | "NOTCH" => Some(FilterType::Notch),
        "BP" | "BPF" | "BANDPASS" | "BAND_PASS" | "BAND-PASS" => Some(FilterType::Bandpass),
        "AP" | "APF" | "ALLPASS" | "ALL_PASS" | "ALL-PASS" => Some(FilterType::AllPass),
        _ => None,
    }
}

fn filter_code(filter_type: FilterType) -> &'static str {
    match filter_type {
        FilterType::Peaking => "PK",
        FilterType::LowShelf => "LSC",
        FilterType::HighShelf => "HSC",
        FilterType::LowPass => "LP",
        FilterType::HighPass => "HP",
        FilterType::Notch => "NO",
        FilterType::Bandpass => "BP",
        FilterType::AllPass => "AP",
    }
}

fn default_q(filter_type: FilterType) -> f64 {
    match filter_type {
        FilterType::Peaking => 1.0,
        _ => 0.707,
    }
}

fn clamp_freq(freq: f32) -> f32 {
    if freq.is_finite() {
        freq.clamp(MIN_FREQ_HZ, MAX_FREQ_HZ)
    } else {
        1_000.0
    }
}

fn clamp_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(-MAX_GAIN_DB, MAX_GAIN_DB)
    } else {
        0.0
    }
}

fn clamp_q(q: f32) -> f32 {
    if q.is_finite() {
        q.clamp(MIN_Q, MAX_Q)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTOEQ: &str = "\
Preamp: -6.2 dB
Filter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70
Filter 2: ON PK Fc 220 Hz Gain -2.4 dB Q 1.41
Filter 3: ON PK Fc 1.0 kHz Gain 3.1 dB Q 2.00
Filter 4: ON HSC Fc 10 kHz Gain -1.5 dB Q 0.70
";

    #[test]
    fn parses_autoeq_parametric_with_preamp_and_shelves() {
        let curve = parse_curve("HD 600", AUTOEQ).unwrap();
        assert_eq!(curve.name, "HD 600");
        assert!((curve.preamp_db + 6.2).abs() < 1e-6);
        assert_eq!(curve.bands.len(), 4);
        assert_eq!(curve.bands[0].filter_type, FilterType::LowShelf);
        assert_eq!(curve.bands[1].filter_type, FilterType::Peaking);
        assert!((curve.bands[2].frequency - 1_000.0).abs() < 1e-3);
        assert_eq!(curve.bands[3].filter_type, FilterType::HighShelf);
        assert!(!curve.is_graphic());
    }

    #[test]
    fn parses_equalizer_apo_without_an_index_and_with_khz() {
        let text = "Preamp: -3 dB\nFilter: ON HP Fc 30 Hz Gain 0 dB Q 0.7";
        let curve = parse_curve("apo", text).unwrap();
        assert_eq!(curve.bands.len(), 1);
        assert_eq!(curve.bands[0].filter_type, FilterType::HighPass);
        assert!((curve.bands[0].frequency - 30.0).abs() < 1e-3);
    }

    #[test]
    fn parses_rew_spacing_and_ignores_disabled_filters() {
        let text = "\
Filter  1: ON  PK       Fc    100.0 Hz  Gain  -3.0 dB  Q  1.000
Filter  2: OFF PK       Fc   1000.0 Hz  Gain   0.0 dB  Q  1.000
";
        let curve = parse_curve("rew", text).unwrap();
        assert_eq!(curve.bands.len(), 1);
        assert!((curve.bands[0].gain_db + 3.0).abs() < 1e-6);
    }

    #[test]
    fn parses_graphiceq_points() {
        let text = "GraphicEQ: 20 -1.2; 21 -1.1; 1000 0.0; 20000 -4.5";
        let curve = parse_curve("graphic", text).unwrap();
        assert!(curve.is_graphic());
        assert_eq!(curve.graphic.len(), 4);
        assert_eq!(curve.graphic[2], (1_000.0, 0.0));
    }

    #[test]
    fn a_graphic_header_with_junk_is_an_error_not_a_silent_flat_curve() {
        let err = parse_curve("broken", "GraphicEQ: forty two").unwrap_err();
        assert!(matches!(err, EqTextError::Malformed(_)));
    }

    #[test]
    fn empty_and_curve_less_files_are_rejected() {
        assert_eq!(
            parse_curve("empty", "   \n").unwrap_err(),
            EqTextError::Empty
        );
        assert_eq!(
            parse_curve("prose", "this is not a curve").unwrap_err(),
            EqTextError::NoCurve
        );
    }

    #[test]
    fn comments_and_bom_do_not_hide_a_curve() {
        let text = "\u{feff}# exported from somewhere\nGraphicEQ: 20 0; 20000 0\n";
        let curve = parse_curve("commented", text).unwrap();
        assert_eq!(curve.graphic.len(), 2);
    }

    #[test]
    fn out_of_range_values_are_clamped_to_what_the_biquad_can_run() {
        let text = "Filter 1: ON PK Fc 90000 Hz Gain 60 dB Q 99";
        let curve = parse_curve("extreme", text).unwrap();
        let band = &curve.bands[0];
        assert_eq!(band.frequency, MAX_FREQ_HZ);
        assert_eq!(band.gain_db, MAX_GAIN_DB);
        assert_eq!(band.q, MAX_Q);
    }

    #[test]
    fn parametric_export_round_trips_through_the_parser() {
        let preset = EqPreset {
            name: "room".into(),
            output_device_pattern: None,
            preamp_db: -4.5,
            bands: vec![
                EqBandConfig {
                    enabled: true,
                    filter_type: FilterType::LowShelf,
                    frequency: 105.0,
                    gain_db: 5.5,
                    q: 0.7,
                },
                EqBandConfig {
                    enabled: false,
                    filter_type: FilterType::Peaking,
                    frequency: 1_000.0,
                    gain_db: -3.0,
                    q: 1.4,
                },
            ],
        };
        let text = write_parametric(&preset);
        let parsed = parse_curve("room", &text).unwrap();
        assert_eq!(parsed.preamp_db, preset.preamp_db);
        // A disabled filter is exported as OFF and comes back out of the
        // curve, which is exactly what a reader should do with it.
        assert_eq!(parsed.bands.len(), 1);
        assert_eq!(parsed.bands[0].filter_type, FilterType::LowShelf);
    }

    #[test]
    fn graphic_export_round_trips() {
        let freqs = [20.0, 100.0, 1_000.0, 20_000.0];
        let gains = [-2.0, 1.5, 0.0, -1.0];
        let text = write_graphic(&freqs, &gains);
        let parsed = parse_curve("g", &text).unwrap();
        assert_eq!(parsed.graphic.len(), 4);
        for (point, expected) in parsed.graphic.iter().zip(freqs.iter().zip(gains.iter())) {
            assert!((point.0 - expected.0).abs() < 1e-3);
            assert!((point.1 - expected.1).abs() < 1e-3);
        }
    }

    #[test]
    fn into_preset_keeps_the_curve_and_drops_graphic_points() {
        let curve = parse_curve("p", AUTOEQ).unwrap();
        let preset = curve.into_preset();
        assert_eq!(preset.bands.len(), 4);
        assert_eq!(preset.name, "p");
    }
}

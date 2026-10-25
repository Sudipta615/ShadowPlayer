//! The durable EQ preset model shared by the panel and the database.
//!
//! The database stores a preset's curve as opaque JSON (see migration V15);
//! this module owns that JSON's shape. Keeping the shape here means a band
//! field can grow without a schema migration, and the repository stays a
//! plain row store.
//!
//! A stored curve is either graphic (a ladder plus one gain per band) or
//! parametric (filters plus a preamp), never both. Applying one goes through
//! the same path as a text import, so a preset, an AutoEQ file and a pasted
//! REW export all obey one set of clamps and one preamp rule.

use config::{EqBandConfig, EqMode, GraphicEqLayout, ImportedCurve};
use serde::{Deserialize, Serialize};

use crate::components::eq_panel::EqPanelState;

/// One saved curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum StoredCurve {
    /// A graphic ladder and its gains, in dB.
    Graphic {
        layout: GraphicEqLayout,
        gains: Vec<f32>,
        preamp_db: f32,
    },
    /// Parametric filters and their shared preamp.
    Parametric {
        preamp_db: f32,
        bands: Vec<EqBandConfig>,
    },
}

impl StoredCurve {
    /// Capture the curve the panel is currently playing.
    ///
    /// The mode decides which model is captured. A parametric panel with no
    /// bands falls back to the graphic ladder rather than saving an empty
    /// preset that would silently flatten the EQ when applied.
    pub fn capture(panel: &EqPanelState) -> Self {
        if panel.eq_mode == EqMode::Parametric && !panel.bands.is_empty() {
            Self::Parametric {
                preamp_db: panel.preamp_db,
                bands: panel
                    .bands
                    .iter()
                    .map(|band| EqBandConfig {
                        enabled: band.enabled,
                        filter_type: band.filter_type.into(),
                        frequency: band.freq,
                        gain_db: band.gain_db,
                        q: band.q,
                    })
                    .collect(),
            }
        } else {
            let count = panel
                .graphic_layout
                .num_bands()
                .min(panel.graphic_gains.len());
            Self::Graphic {
                layout: panel.graphic_layout.clone(),
                gains: panel.graphic_gains[..count].to_vec(),
                preamp_db: panel.preamp_db,
            }
        }
    }

    /// Install this curve into the panel.
    ///
    /// Graphic curves are resampled onto whatever ladder is active; a preset
    /// saved at 31 bands must not be copied index-for-index onto 10 sliders.
    pub fn apply(&self, panel: &mut EqPanelState) {
        panel.apply_imported_curve(&self.as_imported_curve());
    }

    /// The import representation, so presets and text files share one path.
    pub fn as_imported_curve(&self) -> ImportedCurve {
        match self {
            Self::Graphic {
                layout,
                gains,
                preamp_db,
            } => {
                let freqs = layout.frequencies();
                let count = freqs.len().min(gains.len());
                ImportedCurve {
                    name: String::new(),
                    preamp_db: *preamp_db,
                    bands: Vec::new(),
                    graphic: freqs[..count]
                        .iter()
                        .copied()
                        .zip(gains[..count].iter().copied())
                        .collect(),
                }
            }
            Self::Parametric { preamp_db, bands } => ImportedCurve {
                name: String::new(),
                preamp_db: *preamp_db,
                bands: bands.clone(),
                graphic: Vec::new(),
            },
        }
    }

    pub fn is_graphic(&self) -> bool {
        matches!(self, Self::Graphic { .. })
    }

    /// Number of points the curve carries, for the library row.
    pub fn point_count(&self) -> usize {
        match self {
            Self::Graphic { gains, .. } => gains.len(),
            Self::Parametric { bands, .. } => bands.len(),
        }
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("could not encode the curve: {e}"))
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let curve: Self = serde_json::from_str(json)
            .map_err(|e| format!("the saved curve is not readable: {e}"))?;
        // Reject a payload that names 64 bands but carries none: applying it
        // would look like a successful action that flattened the EQ.
        if curve.point_count() == 0 {
            return Err("the saved curve has no bands".to_string());
        }
        Ok(curve)
    }
}

/// A preset as the library presents it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredPreset {
    pub id: String,
    pub name: String,
    /// 0 user, 10 built-in, 20 AutoEQ (see `database::eq_presets`).
    pub source: i64,
    pub curve: StoredCurve,
    pub output_pattern: Option<String>,
    pub is_default: bool,
}

impl StoredPreset {
    pub fn from_record(record: &database::EqPresetRecord) -> Result<Self, String> {
        Ok(Self {
            id: record.id.clone(),
            name: record.name.clone(),
            source: record.source,
            curve: StoredCurve::from_json(&record.curve_json)?,
            output_pattern: record.output_pattern.clone(),
            is_default: record.is_default,
        })
    }

    pub fn curve_json(&self) -> Result<String, String> {
        self.curve.to_json()
    }

    /// Case-insensitive match against the name and the device pattern.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        self.name.to_lowercase().contains(&query)
            || self
                .output_pattern
                .as_deref()
                .is_some_and(|pattern| pattern.to_lowercase().contains(&query))
    }

    pub fn source_label(&self) -> &'static str {
        match self.source {
            10 => "Built-in",
            20 => "AutoEQ",
            _ => "User",
        }
    }
}

/// Library window state. Owned by the EQ panel; the app loads the rows.
#[derive(Default)]
pub struct PresetLibraryState {
    pub presets: Vec<StoredPreset>,
    pub search: String,
    /// Name draft for the save box.
    pub save_name: String,
    /// Device-pattern draft for the save box.
    pub save_pattern: String,
    /// Save as the global default.
    pub save_default: bool,
    pub saving: bool,
    /// Preset whose device pattern is being edited, plus the draft.
    pub binding_edit: Option<String>,
    pub binding_draft: String,
    /// Set once the app has read the database. Distinguishes "not loaded
    /// yet" from "the user has no presets".
    pub loaded: bool,
    /// Preset id bound to the track that is playing, mirrored by the app.
    /// Display only; the binding itself lives in `eq_preset_tracks`.
    pub track_binding: Option<String>,
    pub error: Option<String>,
}

impl PresetLibraryState {
    /// Replace the list from database rows, skipping rows whose curve JSON is
    /// unreadable (a corrupt row must not hide every other preset).
    pub fn replace_from_records(&mut self, records: &[database::EqPresetRecord]) {
        let mut presets = Vec::with_capacity(records.len());
        let mut bad = 0usize;
        for record in records {
            match StoredPreset::from_record(record) {
                Ok(preset) => presets.push(preset),
                Err(e) => {
                    bad += 1;
                    log::warn!("Skipping EQ preset {}: {e}", record.name);
                }
            }
        }
        self.presets = presets;
        self.loaded = true;
        self.error = if bad == 0 {
            None
        } else {
            Some(format!("{bad} saved preset(s) could not be read"))
        };
    }

    /// Indices of the presets matching the search box, in list order.
    pub fn filtered_indices(&self) -> Vec<usize> {
        self.presets
            .iter()
            .enumerate()
            .filter_map(|(index, preset)| preset.matches(&self.search).then_some(index))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::eq_panel::ParametricBand;
    use config::FilterType;

    fn parametric_panel() -> EqPanelState {
        let mut panel = EqPanelState {
            eq_enabled: true,
            eq_mode: EqMode::Parametric,
            bands: vec![
                ParametricBand {
                    freq: 105.0,
                    gain_db: 5.0,
                    q: 0.7,
                    filter_type: engine::dsp::equalizer::EqFilterType::LowShelf,
                    enabled: true,
                },
                ParametricBand {
                    freq: 1_000.0,
                    gain_db: -3.0,
                    q: 1.4,
                    filter_type: engine::dsp::equalizer::EqFilterType::Peaking,
                    enabled: true,
                },
            ],
            ..Default::default()
        };
        panel.set_preamp(-4.0);
        panel
    }

    #[test]
    fn parametric_capture_round_trips_through_json_and_apply() {
        let source = parametric_panel();
        let curve = StoredCurve::capture(&source);

        let json = curve.to_json().unwrap();
        let decoded = StoredCurve::from_json(&json).unwrap();

        let mut target = EqPanelState::default();
        decoded.apply(&mut target);
        assert_eq!(target.eq_mode, EqMode::Parametric);
        assert_eq!(target.bands.len(), 2);
        assert_eq!(
            target.bands[0].filter_type,
            engine::dsp::equalizer::EqFilterType::LowShelf
        );
        assert_eq!(
            target.bands[1].filter_type,
            engine::dsp::equalizer::EqFilterType::Peaking
        );
        assert!((target.preamp_db + 4.0).abs() < 1e-6);
        assert!(target.eq_enabled);
    }

    #[test]
    fn graphic_capture_keeps_the_ladder_and_preamp() {
        let mut panel = EqPanelState {
            eq_mode: EqMode::Graphic,
            graphic_layout: GraphicEqLayout::ThirtyOneBand,
            ..Default::default()
        };
        panel.graphic_gains[0] = 3.0;
        panel.graphic_gains[10] = -2.0;
        panel.set_preamp(-1.5);

        let curve = StoredCurve::capture(&panel);
        assert!(curve.is_graphic());
        assert_eq!(curve.point_count(), 31);
        let imported = curve.as_imported_curve();
        assert_eq!(imported.preamp_db, -1.5);
        assert_eq!(imported.graphic.len(), 31);
    }

    #[test]
    fn a_curve_with_no_bands_is_rejected_instead_of_flattening_the_eq() {
        let curve = StoredCurve::Parametric {
            preamp_db: 0.0,
            bands: Vec::new(),
        };
        let json = curve.to_json().unwrap();
        assert!(StoredCurve::from_json(&json).is_err());
    }

    #[test]
    fn corrupt_rows_are_skipped_without_hiding_good_ones() {
        let good_json = StoredCurve::Parametric {
            preamp_db: -2.0,
            bands: vec![EqBandConfig {
                enabled: true,
                filter_type: FilterType::Peaking,
                frequency: 1_000.0,
                gain_db: -2.0,
                q: 1.0,
            }],
        }
        .to_json()
        .unwrap();
        let records = vec![
            database::EqPresetRecord {
                id: "good".into(),
                name: "Good".into(),
                source: 0,
                curve_json: good_json,
                output_pattern: None,
                is_default: false,
                created_at: 0,
                updated_at: 0,
            },
            database::EqPresetRecord {
                id: "bad".into(),
                name: "Bad".into(),
                source: 0,
                curve_json: "{not json".into(),
                output_pattern: None,
                is_default: false,
                created_at: 0,
                updated_at: 0,
            },
        ];
        let mut library = PresetLibraryState::default();
        library.replace_from_records(&records);
        assert_eq!(library.presets.len(), 1);
        assert!(library.error.is_some());
    }

    #[test]
    fn search_matches_names_and_device_patterns_case_insensitively() {
        let preset = StoredPreset {
            id: "1".into(),
            name: "Living Room".into(),
            source: 0,
            curve: StoredCurve::Parametric {
                preamp_db: 0.0,
                bands: vec![EqBandConfig::default()],
            },
            output_pattern: Some("USB DAC".into()),
            is_default: false,
        };
        assert!(preset.matches("living"));
        assert!(preset.matches("usb"));
        assert!(!preset.matches("headphones"));
        assert!(preset.matches(""));
    }
}

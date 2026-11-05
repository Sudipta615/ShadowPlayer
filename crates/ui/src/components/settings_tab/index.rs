//! The settings search index: one addressable entry per rendered row, plus
//! the category model the tab strip and the jump targets use.

use eframe::egui::{self, Color32, Rect};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SettingsCategory {
    #[default]
    Playback,
    Output,
    Spatial,
    MixBus,
    Tools,
    Converter,
    Library,
    Appearance,
}

impl SettingsCategory {
    pub const ALL: [SettingsCategory; 8] = [
        SettingsCategory::Playback,
        SettingsCategory::Output,
        SettingsCategory::Spatial,
        SettingsCategory::MixBus,
        SettingsCategory::Tools,
        SettingsCategory::Converter,
        SettingsCategory::Library,
        SettingsCategory::Appearance,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::Playback => "Playback",
            Self::Output => "Hardware & DAC",
            Self::Spatial => "Spatial 3D",
            Self::MixBus => "Mix Bus",
            Self::Tools => "Tools",
            Self::Converter => "File Converter",
            Self::Library => "Library & App",
            Self::Appearance => "Themes & Appearance",
        }
    }

    /// The icon as a plain function, for the shared tab strip.
    pub fn draw_icon_fn(&self) -> fn(&egui::Ui, Rect, Color32) {
        match self {
            Self::Playback => crate::components::icons::draw_disc,
            Self::Output => draw_speaker_tab_icon,
            Self::Spatial => crate::components::icons::draw_bar_chart,
            Self::MixBus => crate::components::icons::draw_equalizer_sliders,
            Self::Tools => crate::components::icons::draw_gear,
            Self::Converter => crate::components::icons::draw_reset,
            Self::Library => crate::components::icons::draw_folder,
            Self::Appearance => crate::components::icons::draw_moon,
        }
    }

    pub fn draw_icon(&self, ui: &egui::Ui, rect: Rect, color: Color32) {
        match self {
            Self::Playback => crate::components::icons::draw_disc(ui, rect, color),
            Self::Output => crate::components::icons::draw_speaker(ui, rect, color, false),
            Self::Spatial => crate::components::icons::draw_bar_chart(ui, rect, color),
            Self::MixBus => crate::components::icons::draw_equalizer_sliders(ui, rect, color),
            Self::Tools => crate::components::icons::draw_gear(ui, rect, color),
            Self::Converter => crate::components::icons::draw_reset(ui, rect, color),
            Self::Library => crate::components::icons::draw_folder(ui, rect, color),
            Self::Appearance => crate::components::icons::draw_moon(ui, rect, color),
        }
    }

    /// Sub-heading cards rendered in this category
    pub fn cards(&self) -> &'static [&'static str] {
        match self {
            Self::Playback => &[
                "Playback Speed & Pitch Transposition",
                "Track Transitions & Crossfading",
                "Loudness Normalization (ReplayGain & EBU R128)",
            ],
            Self::Output => &[
                "Audio Driver Backend & Hardware Device",
                "Sample Rate Policies & Resampling Precision",
                "DSD Transport & Decimation",
                "Resampling Filter & Noise Shaping",
                "Native DSD Modulation & Apodizing",
                "DAC Correction Profiles",
            ],
            Self::Spatial => &[
                "Spatial Audio Master",
                "3D Soundstage & Spatial Radar (Interactive)",
                "Binaural HRTF & Head Tracking",
                "Virtual Room Acoustics Simulation",
                "Chain Presets",
            ],
            Self::MixBus => &[
                "Multitrack Lane Mixer",
                "Sidechain Ducking Compressor",
                "Auxiliary Bus & Psychoacoustic Enhancer",
                "Bass Management & Subwoofer Integration",
                "LFE Channel",
                "Up / Down Mixing",
                "CLAP Plugin Insert",
                "Matrix FIR Routing",
            ],
            Self::Tools => &[
                "High-Resolution Stream Capture",
                "ReplayGain & Loudness Tagging Tool",
                "Audio Engine Stream Recovery",
                "Runtime Diagnostics",
                "Room Correction Measurement",
                "CD Ripper",
                "Transcode Export",
                "Automatic Updates",
            ],
            Self::Converter => &[
                "Converter Format & Quality",
                "Destination & Naming",
                "Metadata & Tag Preservation",
                "Batch File Conversion Queue",
            ],
            Self::Library => &[
                "Interface & Language",
                "Smart Playlists",
                "Recommendation Engine",
                "Music Library Storage & Directories",
                "Organize Files",
                "Application Instances",
                "About ShadowPlayer",
            ],
            Self::Appearance => &["Interface", "Theme Gallery", "Theme Editor"],
        }
    }
}

/// Shorter, clean display names for sub-headings in the vertical sidebar
pub fn card_display_name(card: &str) -> &str {
    match card {
        "Playback Speed & Pitch Transposition" => "Playback Speed & Pitch",
        "Track Transitions & Crossfading" => "Track Transitions",
        "Loudness Normalization (ReplayGain & EBU R128)" => "Loudness Normalization",
        "Audio Driver Backend & Hardware Device" => "Audio Hardware & Driver",
        "Sample Rate Policies & Resampling Precision" => "Sample Rate & Resampling",
        "Spatial Audio Master" => "Spatial Master",
        "3D Soundstage & Spatial Radar (Interactive)" => "3D Soundstage & Radar",
        "Binaural HRTF & Head Tracking" => "Binaural HRTF",
        "Virtual Room Acoustics Simulation" => "Room Acoustics",
        "Chain Presets" => "DSP Chain Presets",
        "Multitrack Lane Mixer" => "Multitrack Mixer",
        "Sidechain Ducking Compressor" => "Sidechain Ducking",
        "Auxiliary Bus & Psychoacoustic Enhancer" => "Aux Bus & Enhancer",
        "Bass Management & Subwoofer Integration" => "Bass Management",
        "LFE Channel" => "LFE Channel",
        "Up / Down Mixing" => "Up / Down Mixing",
        "CLAP Plugin Insert" => "CLAP Plugins",
        "High-Resolution Stream Capture" => "Stream Capture",
        "ReplayGain & Loudness Tagging Tool" => "Loudness Tagging",
        "Audio Engine Stream Recovery" => "Stream Recovery",
        "Runtime Diagnostics" => "Diagnostics",
        "Room Correction Measurement" => "Room Measurement",
        "CD Ripper" => "CD Ripper",
        "Transcode Export" => "Transcode Export",
        "Automatic Updates" => "Automatic Updates",
        "Converter Format & Quality" => "Format & Quality",
        "Destination & Naming" => "Destination & Naming",
        "Metadata & Tag Preservation" => "Tag Preservation",
        "Batch File Conversion Queue" => "Batch Conversion",
        "Interface & Language" => "Interface & Language",
        "Smart Playlists" => "Smart Playlists",
        "Recommendation Engine" => "Recommendations",
        "Music Library Storage & Directories" => "Library Storage",
        "Organize Files" => "Organize Files",
        "Application Instances" => "App Instances",
        "About ShadowPlayer" => "About ShadowPlayer",
        "Interface" => "Interface",
        "Theme Gallery" => "Theme Gallery",
        "Theme Editor" => "Theme Editor",
        other => other,
    }
}

/// The speaker icon, adapted to the tab strip's plain-function signature.
fn draw_speaker_tab_icon(ui: &egui::Ui, rect: Rect, color: Color32) {
    crate::components::icons::draw_speaker(ui, rect, color, false);
}

// ─────────────────────────────────────────────────────────────────────────────
// Search index
// ─────────────────────────────────────────────────────────────────────────────

/// One settings row, as the search fields and the command palette see it.
///
/// This table is deliberately static: it is the contract between the search
/// and the rendering code, and a test parses the source to prove the two
/// still agree. A hand-written index could silently miss a setting; the
/// drift test is what makes that impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsEntry {
    pub category: SettingsCategory,
    /// The card the row lives under, e.g. "Bass Management & Subwoofer Integration".
    pub card: &'static str,
    /// The row label, exactly as `setting_item` receives it.
    pub label: &'static str,
    /// The row description, used as a search fallback.
    pub description: &'static str,
}

pub use index_table::SETTINGS_INDEX;

mod index_table;

/// The settings that match a query, best first.
///
/// The label is the strongest signal, the card next, and the description is a
/// fallback for the user who remembers what a setting *does* before what it is
/// called. The same scorer as the command palette keeps the two search boxes
/// feeling the same.
pub fn search_settings(query: &str) -> Vec<&'static SettingsEntry> {
    scored_settings(query)
        .into_iter()
        .map(|(_, entry)| entry)
        .collect()
}

/// Settings and their relevance, before the public result list drops scores.
pub(crate) fn scored_settings(query: &str) -> Vec<(i32, &'static SettingsEntry)> {
    use crate::components::command_palette::fuzzy_score;

    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(i32, &'static SettingsEntry)> = SETTINGS_INDEX
        .iter()
        .filter_map(|entry| {
            let label = fuzzy_score(entry.label, query);
            let card = fuzzy_score(entry.card, query).map(|score| score - 8);
            let description = fuzzy_score(entry.description, query).map(|score| score - 24);
            let best = label.max(card).max(description)?;
            Some((best, entry))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(b.1.label)));
    scored
}

/// Card headings are destinations too. Searching for "theme editor" used to
/// find only a single row because only rows lived in the index; the gallery and
/// editor were reachable by scrolling, not by search.
pub(crate) fn scored_setting_cards(query: &str) -> Vec<(i32, SettingsCategory, &'static str)> {
    use crate::components::command_palette::fuzzy_score;

    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let mut scored = Vec::new();
    for category in SettingsCategory::ALL {
        for &card in category.cards() {
            if let Some(score) = fuzzy_score(card, query) {
                // A card is a navigation target, not a setting: prefer a
                // row's label when both match equally well.
                scored.push((score - 4, category, card));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(b.2)));
    scored
}

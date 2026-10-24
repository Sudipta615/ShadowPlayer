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
    Library,
    Appearance,
}

impl SettingsCategory {
    pub const ALL: [SettingsCategory; 7] = [
        SettingsCategory::Playback,
        SettingsCategory::Output,
        SettingsCategory::Spatial,
        SettingsCategory::MixBus,
        SettingsCategory::Tools,
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
            ],
            Self::Spatial => &[
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
            Self::Library => &[
                "Interface & Language",
                "Smart Playlists",
                "Music Library Storage & Directories",
                "Organize Files",
                "Application Instances",
                "About ShadowPlayer",
            ],
            Self::Appearance => &["Theme Gallery"],
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
        "Interface & Language" => "Interface & Language",
        "Smart Playlists" => "Smart Playlists",
        "Music Library Storage & Directories" => "Library Storage",
        "Organize Files" => "Organize Files",
        "Application Instances" => "App Instances",
        "About ShadowPlayer" => "About ShadowPlayer",
        "Theme Gallery" => "Theme Gallery",
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

pub static SETTINGS_INDEX: [SettingsEntry; 83] = [
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Playback Speed & Pitch Transposition",
        label: "Playback Speed",
        description: "Tempo multiplier (0.25x slow down to 4.00x speed up)",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Playback Speed & Pitch Transposition",
        label: "Pitch Transposition",
        description: "Shift musical key in semitone increments (-24 to +24 semitones)",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Playback Speed & Pitch Transposition",
        label: "Processing Engine",
        description: "Varispeed (analog tape pitch shift) vs TimeStretch (formant-preserved tempo)",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Track Transitions & Crossfading",
        label: "Transition Mode",
        description: "Gapless mode provides seamless album playback without clicks or silence gaps",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Track Transitions & Crossfading",
        label: "Crossfade Duration",
        description: "Overlap time in seconds between fading out song and fading in next song",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Track Transitions & Crossfading",
        label: "Crossfade Curve",
        description: "Constant Power maintains perceived volume energy; Linear creates gradual dip",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Track Transitions & Crossfading",
        label: "Audition Transition",
        description: "Play a three-second local demo through this output device. It does not touch the current song or the playback queue.",
    },
    SettingsEntry {
        category: SettingsCategory::Playback,
        card: "Loudness Normalization (ReplayGain & EBU R128)",
        label: "Target Standard",
        description: "EBU R128 (-14 LUFS) or legacy ReplayGain 2.0 tags",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Audio Driver Backend & Hardware Device",
        label: "Driver Architecture",
        description: "Exclusive ALSA, WASAPI Exclusive, or cross-platform automatic detection",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Audio Driver Backend & Hardware Device",
        label: "Processing Profile",
        description: "Engine preset: resampler quality, dither, EQ, crossfeed and buffer sizes move together. Consumer keeps the lightest chain, Fidelity the strictest, Low Power the cheapest",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Audio Driver Backend & Hardware Device",
        label: "Output Endpoint",
        description: "Leave blank for system default, or enter specific ALSA/WASAPI endpoint string",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Audio Driver Backend & Hardware Device",
        label: "Fallback Policy",
        description: "Action to take if exclusive mode device acquisition is rejected by the OS",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Audio Driver Backend & Hardware Device",
        label: "Hardware Actions",
        description: "Open external driver control panels and reset device configuration profiles",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Sample Rate Policies & Resampling Precision",
        label: "Sample Rate Target",
        description: "Follow Track switches your DAC rate on song change; Fixed resamples everything",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Sample Rate Policies & Resampling Precision",
        label: "Resampler Engine",
        description: "Polyphase FIR filter quality used when rate conversion is required",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Sample Rate Policies & Resampling Precision",
        label: "DSP Precision Mode",
        description: "Internal processing precision. Quality routes the mix chain through the f64 core for extra headroom at a small CPU cost.",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Sample Rate Policies & Resampling Precision",
        label: "Volume Attenuation Mode",
        description: "Software volume scaling vs Fixed 100% line-out for external preamp/headphone amp",
    },
    SettingsEntry {
        category: SettingsCategory::Output,
        card: "Sample Rate Policies & Resampling Precision",
        label: "TPDF Dither",
        description: "Triangular PDF dithering eliminates quantization distortion when converting bit depth",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "3D Soundstage & Spatial Radar (Interactive)",
        label: "Listener Orientation",
        description: "Rotates the listener's head independently of the virtual centre, so the soundstage can be auditioned from an angle",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "3D Soundstage & Spatial Radar (Interactive)",
        label: "Air Absorption",
        description: "Distance-dependent high-frequency roll-off along each path, as air itself absorbs treble over distance",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Binaural HRTF & Head Tracking",
        label: "Spatial Synthesis Quality",
        description: "Spherical harmonics order and convolution precision",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Binaural HRTF & Head Tracking",
        label: "HRTF Profile ID",
        description: "SOFA dataset profile identifier (e.g. KEMAR_diffuse, IRC_1002)",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Binaural HRTF & Head Tracking",
        label: "Listener Motion Smoothing",
        description: "Low-pass filter time constant for head motion tracker interpolation",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Binaural HRTF & Head Tracking",
        label: "Listener Max Angular Velocity",
        description: "Clamps extreme head rotations (0 = uncapped natural tracking)",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Virtual Room Acoustics Simulation",
        label: "Enable Room Acoustics",
        description: "Apply virtual listening room reverberation model",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Virtual Room Acoustics Simulation",
        label: "Reverb Wet/Dry Mix",
        description: "Ratio of reverberant room reflection to direct dry audio",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Virtual Room Acoustics Simulation",
        label: "RT60 Decay Time",
        description: "Time required for acoustic reflections to decay by 60 dB",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Virtual Room Acoustics Simulation",
        label: "Room Dimensions (W x D x H)",
        description: "Physical space modeling in meters",
    },
    SettingsEntry {
        category: SettingsCategory::Spatial,
        card: "Chain Presets",
        label: "Save Current Chain",
        description: "Stores everything currently engaged. A note is handy for headphone or room profiles.",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Multitrack Lane Mixer",
        label: "Add Audio Source File",
        description: "Ingest secondary stems, reference tracks, or voice commentary channels",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Multitrack Lane Mixer",
        label: "Active Channel Slot",
        description: "Select mixer channel strip to control",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Multitrack Lane Mixer",
        label: "Channel Gain & Pan",
        description: "Fine-tune channel volume level and stereo pan balance",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Multitrack Lane Mixer",
        label: "Channel Status",
        description: "Mute and active playback enable state",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Sidechain Ducking Compressor",
        label: "Trigger Source Slot",
        description: "Mixer channel monitored to trigger ducking attenuation",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Sidechain Ducking Compressor",
        label: "Threshold & Depth",
        description: "Level that activates ducking (dB) and total volume reduction depth (dB)",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Sidechain Ducking Compressor",
        label: "Attack & Release Envelope",
        description: "Reaction speed (ms) and recovery return time (ms)",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Auxiliary Bus & Psychoacoustic Enhancer",
        label: "Stereo Field Enhancer",
        description: "Mid-side psychoacoustic stereo image width expander",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Auxiliary Bus & Psychoacoustic Enhancer",
        label: "Auxiliary Return Bus",
        description: "Enable parallel effects return loop with dedicated return gain",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Bass Management",
        description: "Enable a high-pass on the mains plus a matching low-pass for the subwoofer",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Crossover Frequency",
        description: "Hand-off point between mains and subwoofer (60–120 Hz is the usual home-theatre range)",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Crossover Slope",
        description: "Steeper slopes protect the mains but add group delay",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Filter Topology",
        description: "Linkwitz-Riley sums flat at the crossover; Butterworth is −3 dB there",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Filter Q",
        description: "1/√2 ≈ 0.707 is the Butterworth default",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Mains High-Pass",
        description: "Also high-pass the main speakers. Disable to high-pass only the sub path",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Subwoofer Distance/Delay",
        description: "Align the sub's arrival with the mains (0–50 ms; 1 ms ≈ 34 cm)",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Subwoofer Phase",
        description: "Continuously trim the sub's phase for the smoothest crossover sum",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Bass Management & Subwoofer Integration",
        label: "Subwoofer Polarity",
        description: "Invert the sub's polarity (use instead of, not with, coarse phase)",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "LFE Channel",
        label: "LFE Processing",
        description: "Apply gain and an optional low-pass to LFE-role channels",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "LFE Channel",
        label: "LFE Gain",
        description: "Trim the .1 channel relative to the other speakers",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "LFE Channel",
        label: "LFE Low-Pass",
        description: "Standard LFE content is band-limited to 120 Hz",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Up / Down Mixing",
        label: "Channel Mapping",
        description: "Identity by default; ITU-R BS.775 folds multichannel to stereo and can build 5.1/7.1 from stereo",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "Up / Down Mixing",
        label: "Mapping Template",
        description: "Choose the up-mix or down-mix matrix applied at the output boundary",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "CLAP Plugin Insert",
        label: "Hosted Plugin",
        description: "A plugin's own editor opens as a floating native window; the rack also exposes every parameter and named presets",
    },
    SettingsEntry {
        category: SettingsCategory::MixBus,
        card: "CLAP Plugin Insert",
        label: "Plugin Rack",
        description: "Parameters, host bypass, and named presets that capture the plugin's own state",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "High-Resolution Stream Capture",
        label: "Output Recording Path",
        description: "Destination WAV file on local storage",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "High-Resolution Stream Capture",
        label: "Input / Loopback Device",
        description: "Capture endpoint (leave blank for loopback master monitor)",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "High-Resolution Stream Capture",
        label: "Recording Controls",
        description: "Start or finalize audio recording file",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "ReplayGain & Loudness Tagging Tool",
        label: "Target File Path",
        description: "Path to audio file to measure integrated LUFS and inject tags",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "ReplayGain & Loudness Tagging Tool",
        label: "Library ReplayGain Scan",
        description: "Analyse every track that has no loudness data (EBU R128 + ReplayGain 2.0). Results are cached for playback.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "ReplayGain & Loudness Tagging Tool",
        label: "Write Tags Into Files",
        description: "Also save the measured ReplayGain / EBU R128 values into each file's tags (FLAC, MP3, M4A, WAV …). Off by default — this is the only setting that modifies your audio files; DSD and raw PCM are skipped.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Audio Engine Stream Recovery",
        label: "Stream Recovery",
        description: "Force re-initializes output pipeline without restarting ShadowPlayer",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Requested backend",
        description: "The backend selected in Settings. Auto asks the engine to choose the best available path.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Negotiated backend",
        description: "The backend that actually opened the device. A difference from the request means a fallback.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Access mode",
        description: "Shared streams pass through the OS mixer. Direct hardware and exclusive streams bypass it.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Verification",
        description: "Whether direct hardware access was independently verified by the driver, and why not when it was not.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Output device",
        description: "The operating system device identity the engine opened, including the ALSA hw: or plughw: node.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Negotiated rate / container",
        description: "The sample rate and sample container agreed with the driver, including dither when it modifies an integer stream.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "Period / buffer",
        description: "ALSA's settled period and buffer sizes, the callback buffer on other backends, and round-trip latency.",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Runtime Diagnostics",
        label: "DSP Graph Rebuilds",
        description: "Mean cost of rebuilding the processing graph (codec, sample-rate or routing change)",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Room Correction Measurement",
        label: "Measurement Wizard",
        description: "Choose the input device and sweep length, watch each stage, and cancel without keeping a partial recording",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "CD Ripper",
        label: "Disc Extraction",
        description: "Probe the drive, fetch CDDB metadata when available, pick tracks, and write new files without overwriting",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Transcode Export",
        label: "Format Export",
        description: "Batch export selected files to one output folder; existing files are never replaced unless you allow it",
    },
    SettingsEntry {
        category: SettingsCategory::Tools,
        card: "Automatic Updates",
        label: "Update Check",
        description: "Off by default. When enabled, Check contacts GitHub, compares versions, and offers a verified download for this platform",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Music Library Storage & Directories",
        label: "Cataloged Tracks",
        description: "Indexed audio files in local SQLite library database",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Music Library Storage & Directories",
        label: "Import Music",
        description: "Use the system file picker to add songs or whole folders to the library",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Music Library Storage & Directories",
        label: "Find Duplicate Tracks",
        description: "Group tracks with identical audio content (matched by fingerprint, not tags) and remove the extra copies",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Music Library Storage & Directories",
        label: "Biography Source Folder",
        description: "Folder of artist and album notes read into the biography panes",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Music Library Storage & Directories",
        label: "Or Type a Directory",
        description: "Advanced: index a folder by typing its absolute path",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Application Instances",
        label: "Allow multiple instances",
        description: "Off: a second launch activates this window. On: open concurrent windows (SQLite WAL); takes effect next launch",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Organize Files",
        label: "Batch Rename & Sort",
        description: "Build names from tags — e.g. {artist}/{album}/{track} - {title}. Existing files are never overwritten.",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Interface & Language",
        label: "Language",
        description: "Follow the system locale, or choose a language explicitly",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Smart Playlists",
        label: "Smart Playlist Refresh",
        description: "Re-evaluate the saved queries after every scan, on a timer, or only on request",
    },
    SettingsEntry {
        category: SettingsCategory::Library,
        card: "Smart Playlists",
        label: "Refresh Interval",
        description: "How many minutes pass between two refreshes under the timer policy",
    },
];

/// The settings that match a query, best first.
///
/// The label is the strongest signal, the card next, and the description is a
/// fallback for the user who remembers what a setting *does* before what it is
/// called. The same scorer as the command palette keeps the two search boxes
/// feeling the same.
pub fn search_settings(query: &str) -> Vec<&'static SettingsEntry> {
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
    scored.into_iter().map(|(_, entry)| entry).collect()
}

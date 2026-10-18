//! Read-only view of the CLAP plugin the engine is hosting.
//!
//! A CLAP plugin's own editor is a native window, which a headless host cannot
//! put on screen. What a host *can* offer is the plugin's parameter interface:
//! the CLAP `params` extension describes every automatable control (id, name,
//! grouping, range, default) and the engine can drive them by id. This module
//! defines the snapshot the engine publishes so the UI can draw a generic rack
//! — which is how headless hosts, DAW remote surfaces and batch renderers show
//! plugin state.
//!
//! These types are plain data and compile with or without the `clap-host`
//! feature: the feature decides whether anything ever fills them in, not
//! whether a UI can name them.
//!
//! The model's tests live in `tests/clap_rack_model.rs` rather than beside this
//! module, because the engine's unit-test target does not build (see that
//! file's header).

use serde::{Deserialize, Serialize};

/// A CLAP plugin as described by its factory descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginInfo {
    /// The plugin id within its library, e.g. `com.example.gain`.
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
}

/// One automatable parameter, as the plugin describes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClapParamInfo {
    /// The stable id the plugin expects in parameter events.
    pub id: u32,
    pub name: String,
    /// The plugin's grouping path (e.g. `filter/low`), empty when it has none.
    pub module: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// The value the host believes is current.
    pub value: f64,
    /// The plugin's own rendering of `value` — for an enumeration, the name of
    /// the mode rather than its number. Empty when the plugin offers no text.
    pub value_text: String,
    /// Discrete values only (a mode switch, not a continuous knob).
    pub is_stepped: bool,
    /// The value changes on its own over time (an LFO rate, a meter).
    pub is_periodic: bool,
    pub is_readonly: bool,
    /// The plugin asks hosts not to show this one.
    pub is_hidden: bool,
    /// This parameter is the plugin's bypass switch.
    pub is_bypass: bool,
    /// The plugin marks it as an enumeration, so `value_to_text` names it.
    pub is_enum: bool,
}

impl ClapParamInfo {
    /// Where the current value sits in the range, as 0..1.
    ///
    /// Clamped, and `0.0` for a degenerate range, so a UI slider can always
    /// convert between its own position and the plugin's value.
    pub fn normalised(&self) -> f64 {
        let span = self.max - self.min;
        if span.abs() < f64::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }

    /// Convert a 0..1 slider position back into the plugin's units.
    pub fn from_normalised(&self, position: f64) -> f64 {
        self.min + (self.max - self.min) * position.clamp(0.0, 1.0)
    }

    /// A parameter a user can actually move: not read-only, not hidden.
    pub fn is_editable(&self) -> bool {
        !self.is_readonly && !self.is_hidden
    }
}

/// Everything a host needs to draw the plugin rack.
///
/// Published by the engine thread through an `ArcSwap`, so a UI can read it on
/// every frame without taking a lock on the audio path.
#[derive(Debug, Clone, Default)]
pub struct ClapRackReport {
    /// `None` when no plugin is loaded.
    pub plugin: Option<ClapPluginInfo>,
    pub params: Vec<ClapParamInfo>,
    /// The dynamic library the plugin was loaded from.
    pub path: Option<String>,
    /// The plugin is instantiated but its processing is bypassed.
    pub bypassed: bool,
    /// The plugin implements the CLAP `state` extension, so its full state can
    /// be captured into a preset.
    pub has_state: bool,
    /// Processing latency the plugin reports, in samples.
    pub latency_samples: u32,
    /// The last load or state failure, kept until the next successful call so a
    /// host can show *why* a plugin did not appear.
    pub error: Option<String>,
}

/// The plugins found inside the most recently probed library.
#[derive(Debug, Clone, Default)]
pub struct ClapLibraryProbe {
    /// The library file that was probed.
    pub path: String,
    /// What it contains, in the order the factory lists them.
    pub plugins: Vec<ClapPluginInfo>,
    /// Why the probe failed, when it did.
    pub error: Option<String>,
}

impl ClapLibraryProbe {
    pub fn ok(&self) -> bool {
        self.error.is_none()
    }
}

/// Everything a host reads to draw the plugin rack.
///
/// One snapshot rather than two: the loaded plugin and whatever the last
/// library probe found are read together, and a UI that had to stitch two
/// independently-updated slots together could show a plugin list that does not
/// belong to the library it names.
#[derive(Debug, Clone, Default)]
pub struct ClapRackView {
    pub rack: ClapRackReport,
    /// `None` until a library has been probed this session.
    pub library: Option<ClapLibraryProbe>,
}

impl ClapRackView {
    pub fn is_loaded(&self) -> bool {
        self.rack.is_loaded()
    }
}

impl ClapRackReport {
    pub fn is_loaded(&self) -> bool {
        self.plugin.is_some()
    }

    /// Display name, falling back to the library file name.
    pub fn display_name(&self) -> String {
        if let Some(plugin) = &self.plugin {
            if !plugin.name.trim().is_empty() {
                return plugin.name.clone();
            }
        }
        self.path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "No plugin".to_string())
    }

    /// Parameters a user can move, in plugin order.
    pub fn editable_params(&self) -> impl Iterator<Item = &ClapParamInfo> {
        self.params.iter().filter(|p| p.is_editable())
    }

    /// The plugin's own bypass parameter, when it declares one.
    pub fn bypass_param(&self) -> Option<&ClapParamInfo> {
        self.params.iter().find(|p| p.is_bypass)
    }
}
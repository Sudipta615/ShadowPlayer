//! State for the CLAP plugin rack.
//!
//! A CLAP plugin's own editor is a native window, so an egui host cannot embed
//! it. What a host *can* offer is the plugin's parameter interface — every
//! automatable control the plugin declares, with its range and its name — plus
//! the plugin's opaque state. The rack window draws the first and captures the
//! second, which is how headless hosts and DAW remote surfaces present plugins.
//!
//! Two things live here:
//!
//! - [`ClapRackPreset`], a named snapshot: which plugin, the parameter values
//!   the user set, and the plugin's own state blob. Both halves are needed.
//!   A state blob alone cannot be edited or diffed, and parameter values alone
//!   miss whatever the plugin keeps internally (a loaded impulse response, a
//!   learned correction); each covers what the other cannot.
//! - [`ClapRackState`], the window's own state plus the preset library.
//!
//! Applying a preset pushes the values to the engine and loads the blob; the
//! caller decides whether the plugin must be loaded first.

use std::time::{Duration, Instant};

use engine::dsp::clap_rack::ClapRackView;
use serde::{Deserialize, Serialize};

/// How long a preset save waits for the engine to hand back the plugin's state
/// before saving the parameter values alone.
///
/// The engine answers within a frame or two; the deadline exists so a plugin
/// that refuses `state.save` cannot leave a save permanently half-done.
const STATE_CAPTURE_TIMEOUT: Duration = Duration::from_millis(750);

// ── Base64 ──────────────────────────────────────────────────────────────────

/// Base64 (RFC 4648, standard alphabet, padded).
///
/// A plugin's state can be megabytes and it has to fit in a JSON settings row.
/// Storing it as an array of byte values costs up to four characters per byte;
/// base64 costs 1.33. It is a page of arithmetic, so it is not worth pulling in
/// a dependency for.
const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0] as u32;
        let second = *chunk.get(1).unwrap_or(&0) as u32;
        let third = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (first << 16) | (second << 8) | third;
        out.push(BASE64_ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(BASE64_ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    for byte in text.bytes() {
        // Padding and any whitespace a hand-edited settings row picked up.
        if matches!(byte, b'=' | b'\n' | b'\r' | b' ' | b'\t') {
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

// ── Presets ─────────────────────────────────────────────────────────────────

/// One named snapshot of a plugin: which plugin, the values the user set, and
/// the plugin's own state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClapRackPreset {
    pub name: String,
    /// Free-form note, e.g. what the preset was dialled in for.
    #[serde(default)]
    pub note: String,
    /// The plugin the snapshot came from. Recalling it into a different plugin
    /// is refused rather than applied as nonsense: parameter ids are only
    /// meaningful within the plugin that declared them.
    #[serde(default)]
    pub plugin_id: String,
    #[serde(default)]
    pub plugin_name: String,
    /// Parameter values the host set: `(plugin parameter id, value)`.
    #[serde(default)]
    pub params: Vec<(u32, f64)>,
    /// The plugin's own state, base64. `None` when the plugin implements no
    /// state extension, or when it refused to be saved.
    #[serde(default)]
    pub state: Option<String>,
}

impl ClapRackPreset {
    /// Build a preset from live values and an already-captured state blob.
    pub fn capture(
        name: impl Into<String>,
        note: impl Into<String>,
        view: &ClapRackView,
        params: Vec<(u32, f64)>,
        state: Option<Vec<u8>>,
    ) -> Self {
        Self {
            name: name.into(),
            note: note.into(),
            plugin_id: view
                .rack
                .plugin
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_default(),
            plugin_name: view.rack.display_name(),
            params,
            state: state
                .filter(|bytes| !bytes.is_empty())
                .map(|bytes| base64_encode(&bytes)),
        }
    }

    /// The stored state blob, decoded.
    pub fn state_bytes(&self) -> Option<Vec<u8>> {
        self.state.as_deref().and_then(base64_decode)
    }

    /// Whether this preset was captured from `plugin_id`.
    ///
    /// An empty id means the preset predates the field; accepting it is kinder
    /// than refusing a preset that may well be fine.
    pub fn matches_plugin(&self, plugin_id: &str) -> bool {
        self.plugin_id.is_empty() || self.plugin_id == plugin_id
    }

    /// One-line description for the preset list.
    pub fn summary(&self) -> String {
        if self.note.trim().is_empty() {
            self.plugin_name.clone()
        } else {
            format!("{} · {}", self.plugin_name, self.note.trim())
        }
    }

    /// How much of the preset is more than the parameter values.
    pub fn has_state(&self) -> bool {
        self.state.is_some()
    }
}

/// Insert or replace a preset by name, returning its index.
///
/// Names are the user-facing identity, so saving the same name twice
/// overwrites rather than accumulating near-duplicates.
pub fn upsert(presets: &mut Vec<ClapRackPreset>, preset: ClapRackPreset) -> usize {
    if let Some(index) = presets
        .iter()
        .position(|p| p.name.eq_ignore_ascii_case(&preset.name))
    {
        presets[index] = preset;
        index
    } else {
        presets.push(preset);
        presets.len() - 1
    }
}

/// The parts of the rack worth keeping between sessions.
///
/// Only the presets and the last library are persisted. What is loaded lives in
/// the engine's own configuration — the engine owns the insert and restores it
/// with its parameters and state — so duplicating it here would give two
/// sources of truth for the same thing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClapRackPrefs {
    pub presets: Vec<ClapRackPreset>,
    /// The library the user last loaded, offered the next time the picker
    /// opens.
    pub last_library: String,
}

/// A preset save that is waiting for the engine to hand back the plugin state.
#[derive(Clone, Debug)]
pub struct PendingSave {
    pub name: String,
    pub note: String,
    /// The parameter values as they were when the user pressed Save, so a
    /// between-frames edit cannot leak into the snapshot.
    pub params: Vec<(u32, f64)>,
    pub armed_at: Instant,
}

impl PendingSave {
    /// Storing the parameter values alone is better than never saving.
    pub fn expired(&self) -> bool {
        self.armed_at.elapsed() >= STATE_CAPTURE_TIMEOUT
    }
}

/// Everything the settings panel needs to describe the rack in one line.
#[derive(Clone, Debug, Default)]
pub struct ClapCardSummary {
    pub loaded: bool,
    pub name: String,
    pub vendor: String,
    pub parameter_count: usize,
    /// The insert is skipped entirely (the host node's own bypass).
    pub bypassed: bool,
    pub error: Option<String>,
}

impl ClapCardSummary {
    pub fn from_view(view: &ClapRackView) -> Self {
        Self {
            loaded: view.rack.is_loaded(),
            name: view.rack.display_name(),
            vendor: view
                .rack
                .plugin
                .as_ref()
                .map(|p| p.vendor.clone())
                .unwrap_or_default(),
            parameter_count: view.rack.editable_params().count(),
            bypassed: view.rack.bypassed,
            error: view.rack.error.clone(),
        }
    }

    /// The settings card's status line.
    pub fn status_line(&self) -> String {
        if let Some(error) = &self.error {
            return format!("Last attempt failed: {error}");
        }
        if !self.loaded {
            return "No plugin loaded — the insert is pass-through".to_string();
        }
        let vendor = if self.vendor.trim().is_empty() {
            String::new()
        } else {
            format!(" · {}", self.vendor)
        };
        let state = if self.bypassed { " · bypassed" } else { "" };
        format!(
            "{}{} · {} parameter{}{}",
            self.name,
            vendor,
            self.parameter_count,
            if self.parameter_count == 1 { "" } else { "s" },
            state
        )
    }
}

// ── Window state ────────────────────────────────────────────────────────────

/// The rack window's state and the preset library.
#[derive(Default)]
pub struct ClapRackState {
    pub is_open: bool,
    pub window_pos: Option<[f32; 2]>,
    pub presets: Vec<ClapRackPreset>,
    pub preset_name: String,
    pub preset_note: String,
    pub selected_preset: Option<usize>,
    /// The library the user last asked about, so the panel can tell "nothing
    /// probed yet" from "this probe found nothing".
    pub probe_path: Option<String>,
    /// Set once an arriving probe has been acted on, so a single-plugin
    /// library is auto-loaded exactly once rather than every frame.
    pub pending_autoload_done: bool,
    /// A preset save that is waiting on the engine's state capture.
    pub pending_save: Option<PendingSave>,
    /// Last thing worth telling the user about the rack.
    pub notice: Option<String>,
    /// The same message, queued for the app to raise as a toast. Kept apart from
    /// `notice` because a toast is consumed once while the window's status line
    /// stays up until the next thing happens.
    pub pending_toast: Option<String>,
    /// The last library loaded, offered the next time the picker opens.
    pub last_library: String,
}

impl ClapRackState {
    /// Set the window's status line and queue the same text as a toast.
    pub fn announce(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.notice = Some(message.clone());
        self.pending_toast = Some(message);
    }

    /// Presets captured from the plugin currently loaded, in library order.
    pub fn presets_for(&self, plugin_id: &str) -> Vec<usize> {
        self.presets
            .iter()
            .enumerate()
            .filter(|(_, p)| p.matches_plugin(plugin_id))
            .map(|(index, _)| index)
            .collect()
    }

    /// Arm a preset save. The caller asks the engine for the state blob and
    /// calls [`Self::finish_save`] when it arrives.
    ///
    /// Returns `false` when there is no name to save under, or nothing loaded
    /// to capture.
    pub fn begin_save(&mut self, view: &ClapRackView) -> bool {
        let name = self.preset_name.trim().to_string();
        if name.is_empty() || !view.rack.is_loaded() {
            self.announce("Load a plugin and name the preset first".to_string());
            return false;
        }
        self.pending_save = Some(PendingSave {
            name,
            note: self.preset_note.trim().to_string(),
            // The editable parameters, the same set the rack draws: a read-only
            // parameter cannot be pushed back into the plugin, and one the
            // plugin asks hosts to hide is not the user's to restore. Whatever
            // those describe belongs in the state blob, not in a value list.
            params: view
                .rack
                .editable_params()
                .map(|p| (p.id, p.value))
                .collect(),
            armed_at: Instant::now(),
        });
        true
    }

    /// Complete a save once the state blob is known (or known to be missing).
    pub fn finish_save(&mut self, view: &ClapRackView, state: Option<Vec<u8>>) -> Option<usize> {
        let pending = self.pending_save.take()?;
        let had_state = state.as_ref().is_some_and(|bytes| !bytes.is_empty());
        let preset = ClapRackPreset::capture(
            pending.name.clone(),
            pending.note,
            view,
            pending.params,
            state,
        );
        let index = upsert(&mut self.presets, preset);
        self.selected_preset = Some(index);
        let plugin_declares_state = view.rack.has_state;
        self.announce(match (had_state, plugin_declares_state) {
            (true, _) => format!("Saved preset \u{201c}{}\u{201d}", pending.name),
            (false, false) => format!(
                "Saved preset \u{201c}{}\u{201d} (parameters only — the plugin exposes no state)",
                pending.name
            ),
            (false, true) => format!(
                "Saved preset \u{201c}{}\u{201d} (parameters only — the plugin did not return its state)",
                pending.name
            ),
        });
        Some(index)
    }

    /// Give up waiting and store the parameter values on their own.
    pub fn save_without_state(&mut self, view: &ClapRackView) -> Option<usize> {
        self.finish_save(view, None)
    }
}

/// What the rack window asked the app to do this frame.
#[derive(Debug, Default, Clone)]
pub struct ClapRackActions {
    /// Open the native file picker for a `.clap` library.
    pub choose_library: bool,
    /// Probe this library (list its plugins) without loading one.
    pub probe_library: Option<String>,
    /// Load `(library path, plugin id)`.
    pub load_plugin: Option<(String, String)>,
    pub unload: bool,
    /// Host-level bypass of the whole insert.
    pub set_enabled: Option<bool>,
    pub set_param: Option<(u32, f64)>,
    pub refresh: bool,
    /// Begin saving a preset under the name already in the panel.
    pub save_preset: bool,
    pub apply_preset: Option<usize>,
    pub delete_preset: Option<usize>,
    pub close: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::dsp::clap_rack::{ClapParamInfo, ClapPluginInfo, ClapRackReport};

    fn view_with(plugin: Option<&str>, params: Vec<ClapParamInfo>) -> ClapRackView {
        ClapRackView {
            rack: ClapRackReport {
                plugin: plugin.map(|name| ClapPluginInfo {
                    id: format!("com.test.{name}"),
                    name: name.to_string(),
                    vendor: "Test Vendor".to_string(),
                    ..Default::default()
                }),
                params,
                has_state: true,
                ..Default::default()
            },
            library: None,
        }
    }

    fn param(id: u32, value: f64) -> ClapParamInfo {
        ClapParamInfo {
            id,
            name: format!("P{id}"),
            min: -24.0,
            max: 24.0,
            value,
            ..Default::default()
        }
    }

    #[test]
    fn base64_round_trips_every_length_and_the_alphabet() {
        for length in 0..=64usize {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 7 + 3) as u8).collect();
            let encoded = base64_encode(&bytes);
            assert_eq!(
                base64_decode(&encoded).as_deref(),
                Some(bytes.as_slice()),
                "length {length}"
            );
        }
        // All 256 byte values, including the ones that land on `+` and `/`.
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all);

        // Padding is present for the two non-multiple-of-three lengths.
        assert_eq!(base64_encode(b"M"), "TQ==");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn base64_rejects_garbage_rather_than_producing_silent_nonsense() {
        assert!(base64_decode("TWFu$").is_none());
        // A truncated payload still yields what it can; the plugin is the
        // authority on whether that is a valid state blob.
        assert!(base64_decode("TWF").is_some());
    }

    #[test]
    fn a_captured_preset_survives_json_with_its_state_intact() {
        let view = view_with(Some("Verb"), vec![param(0, 1.5), param(1, -3.0)]);
        let state: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let preset = ClapRackPreset::capture(
            "Hall",
            "big room",
            &view,
            vec![(0, 1.5), (1, -3.0)],
            Some(state.clone()),
        );

        let json = serde_json::to_string(&preset).unwrap();
        let back: ClapRackPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, preset);
        assert_eq!(back.state_bytes().as_deref(), Some(state.as_slice()));
        assert_eq!(back.plugin_id, "com.test.Verb");
        assert_eq!(back.params, vec![(0, 1.5), (1, -3.0)]);
        assert!(back.has_state());
    }

    #[test]
    fn a_preset_without_a_state_blob_still_saves_and_loads() {
        let view = view_with(Some("Verb"), vec![param(0, 2.0)]);
        let preset = ClapRackPreset::capture("Dry", "", &view, vec![(0, 2.0)], None);
        assert!(!preset.has_state());
        assert!(preset.state_bytes().is_none());

        let json = serde_json::to_string(&preset).unwrap();
        let back: ClapRackPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back.params, vec![(0, 2.0)]);
    }

    #[test]
    fn an_empty_blob_is_treated_as_no_blob() {
        let view = view_with(Some("Verb"), vec![]);
        let preset = ClapRackPreset::capture("Empty", "", &view, vec![], Some(Vec::new()));
        assert!(
            !preset.has_state(),
            "an empty blob is not state worth keeping"
        );
    }

    #[test]
    fn upsert_replaces_case_insensitively_and_keeps_order() {
        let view = view_with(Some("Verb"), vec![param(0, 1.0)]);
        let mut presets = Vec::new();
        let first = ClapRackPreset::capture("Hall", "", &view, vec![(0, 1.0)], None);
        assert_eq!(upsert(&mut presets, first), 0);

        let second = ClapRackPreset::capture("Room", "", &view, vec![(0, 2.0)], None);
        assert_eq!(upsert(&mut presets, second), 1);

        let mut again = ClapRackPreset::capture("hall", "", &view, vec![(0, 9.0)], None);
        again.note = "updated".to_string();
        assert_eq!(upsert(&mut presets, again), 0, "same name overwrites");
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].params, vec![(0, 9.0)]);
        assert_eq!(presets[0].note, "updated");
    }

    #[test]
    fn presets_are_offered_only_for_the_plugin_they_came_from() {
        let mut state = ClapRackState::default();
        let verb = view_with(Some("Verb"), vec![]);
        let delay = view_with(Some("Delay"), vec![]);
        state
            .presets
            .push(ClapRackPreset::capture("v1", "", &verb, vec![], None));
        state
            .presets
            .push(ClapRackPreset::capture("d1", "", &delay, vec![], None));
        state
            .presets
            .push(ClapRackPreset::capture("v2", "", &verb, vec![], None));

        assert_eq!(state.presets_for("com.test.Verb"), vec![0, 2]);
        assert_eq!(state.presets_for("com.test.Delay"), vec![1]);
        assert!(state.presets_for("com.test.Other").is_empty());

        // A preset from before the field existed is offered everywhere rather
        // than hidden for ever.
        let mut legacy = ClapRackPreset::capture("old", "", &verb, vec![], None);
        legacy.plugin_id = String::new();
        assert!(legacy.matches_plugin("com.test.Anything"));
    }

    #[test]
    fn saving_needs_a_name_and_a_loaded_plugin() {
        let mut state = ClapRackState::default();
        let empty = view_with(None, vec![]);
        assert!(!state.begin_save(&empty));
        assert!(state.pending_save.is_none());
        assert!(state.notice.is_some());

        let view = view_with(Some("Verb"), vec![param(0, 4.0)]);
        assert!(!state.begin_save(&view), "still no name");
        state.preset_name = "  Hall  ".to_string();
        assert!(state.begin_save(&view));
        let pending = state.pending_save.clone().unwrap();
        assert_eq!(pending.name, "Hall", "the name is trimmed");
        assert_eq!(pending.params, vec![(0, 4.0)]);
    }

    #[test]
    fn finishing_a_save_stores_the_parameters_and_the_blob() {
        let view = view_with(Some("Verb"), vec![param(0, 4.0)]);
        let mut state = ClapRackState {
            preset_name: "Hall".to_string(),
            ..ClapRackState::default()
        };
        assert!(state.begin_save(&view));

        let index = state.finish_save(&view, Some(vec![1, 2, 3])).unwrap();
        assert_eq!(index, 0);
        assert!(state.pending_save.is_none());
        assert_eq!(state.presets[0].state_bytes(), Some(vec![1, 2, 3]));
        assert_eq!(state.presets[0].params, vec![(0, 4.0)]);
        assert!(state.notice.as_deref().unwrap().contains("Hall"));
    }

    #[test]
    fn a_save_that_never_gets_its_blob_still_stores_the_values() {
        let view = view_with(Some("Verb"), vec![param(0, 4.0)]);
        let mut state = ClapRackState {
            preset_name: "Hall".to_string(),
            ..ClapRackState::default()
        };
        assert!(state.begin_save(&view));

        // Simulate the deadline passing.
        state.pending_save.as_mut().unwrap().armed_at =
            Instant::now() - STATE_CAPTURE_TIMEOUT - Duration::from_millis(1);
        assert!(state.pending_save.as_ref().unwrap().expired());

        let index = state.save_without_state(&view).unwrap();
        assert_eq!(index, 0);
        assert!(!state.presets[0].has_state());
        let notice = state.notice.clone().unwrap();
        assert!(notice.contains("parameters only"), "notice was {notice:?}");
    }

    #[test]
    fn the_card_status_line_names_the_plugin_or_says_why_not() {
        let loaded = ClapCardSummary::from_view(&view_with(
            Some("Verb"),
            vec![param(0, 0.0), param(1, 0.0)],
        ));
        let line = loaded.status_line();
        assert!(line.contains("Verb"), "{line}");
        assert!(line.contains("Test Vendor"), "{line}");
        assert!(line.contains("2 parameters"), "{line}");
        assert!(!line.contains("bypassed"), "{line}");

        let mut bypassed = view_with(Some("Verb"), vec![]);
        bypassed.rack.bypassed = true;
        assert!(ClapCardSummary::from_view(&bypassed)
            .status_line()
            .contains("bypassed"));

        let empty = ClapCardSummary::from_view(&ClapRackView::default());
        assert!(!empty.loaded);
        assert!(empty.status_line().contains("pass-through"));

        let mut failed = ClapRackView::default();
        failed.rack.error = Some("no such file".to_string());
        assert!(ClapCardSummary::from_view(&failed)
            .status_line()
            .contains("no such file"));
    }

    #[test]
    fn a_save_captures_only_the_parameters_a_user_can_move() {
        let hidden = ClapParamInfo {
            is_hidden: true,
            ..param(9, 1.0)
        };
        let readonly = ClapParamInfo {
            is_readonly: true,
            ..param(8, 2.0)
        };
        // The editable set is what the rack draws, so it is what a snapshot has
        // to hold: pushing a read-only value back would be ignored, and a hidden
        // one is not the user's to restore.
        let view = view_with(Some("Verb"), vec![param(0, 4.0), hidden, readonly]);

        let mut state = ClapRackState {
            preset_name: "Hall".to_string(),
            ..ClapRackState::default()
        };
        assert!(state.begin_save(&view));
        let pending = state.pending_save.clone().unwrap();
        assert_eq!(pending.params, vec![(0, 4.0)]);
    }
}

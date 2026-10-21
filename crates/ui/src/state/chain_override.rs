//! Per-track overrides on top of the global DSP chain.
//!
//! A track's profile is a **diff**, not a snapshot. The user's global chain is
//! the base; the profile stores only the leaves that differ, and everything it
//! does not name is inherited at playback time. That is what makes a per-track
//! correction keep working when the global chain changes: the track keeps its
//! own curve and preamp, but it still follows a new compressor setting.
//!
//! The diff is computed over the serialised [`ChainPreset`] rather than over
//! its ~50 fields by hand. That is deliberately untyped: `ChainPreset` is the
//! single definition of what a chain is, so a field added to it later is
//! overridable (and testable) without another match arm here.

use crate::state::chain_preset::{ChainEq, ChainPreset};
use serde_json::{Map, Value};

/// The JSON object that turns `base` into `current`, or `None` when they are
/// the same chain.
///
/// The preset's name and note are excluded: they describe the preset, not the
/// signal, and a profile that carried them would rename the global chain when
/// it was applied.
pub fn diff_chain(base: &ChainPreset, current: &ChainPreset) -> Option<Value> {
    let mut base = serde_json::to_value(base).ok()?;
    let mut current = serde_json::to_value(current).ok()?;
    for value in [&mut base, &mut current] {
        if let Value::Object(map) = value {
            map.remove("name");
            map.remove("note");
        }
    }
    diff_value(&base, &current)
}

fn diff_value(base: &Value, current: &Value) -> Option<Value> {
    match (base, current) {
        (Value::Object(base), Value::Object(current)) => {
            let mut out = Map::new();
            for (key, value) in current {
                match base.get(key) {
                    Some(base_value) => {
                        if let Some(diff) = diff_value(base_value, value) {
                            out.insert(key.clone(), diff);
                        }
                    }
                    None => {
                        out.insert(key.clone(), value.clone());
                    }
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(Value::Object(out))
            }
        }
        // Arrays are compared whole: a single changed EQ band is stored as the
        // band array, which is also what an apply needs to restore.
        _ => {
            if base == current {
                None
            } else {
                Some(current.clone())
            }
        }
    }
}

/// `base` with `patch` merged over it, deserialised back into a chain.
///
/// A patch that does not fit the current chain shape is rejected (returns
/// `None`) rather than applied partially: a profile written by a newer build
/// must not silently half-apply.
pub fn merge_chain(base: &ChainPreset, patch: &Value) -> Option<ChainPreset> {
    let mut value = serde_json::to_value(base).ok()?;
    merge_value(&mut value, patch);
    serde_json::from_value(value).ok()
}

fn merge_value(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(base_value) => merge_value(base_value, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

/// One human-readable line of a profile's overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideLine {
    /// The block it belongs to: "Equalizer", "Compressor", "Mix bus", ...
    pub section: &'static str,
    /// The parameter, in words: "Preamp", "Graphic EQ", ...
    pub label: String,
    /// The value the track sets: "-3.0 dB", "On", "64 bands".
    pub value: String,
}

/// Flatten a diff into the lines the per-track editor shows.
///
/// The diff's JSON keys are alphabetical, which would scatter one panel's
/// settings through the list. The lines are regrouped into the order the EQ
/// panel presents its blocks, and alphabetised inside each block.
pub fn describe_overrides(patch: &Value) -> Vec<OverrideLine> {
    let mut lines = Vec::new();
    if let Value::Object(top) = patch {
        for (key, value) in top {
            describe_section(key, value, &mut lines);
        }
    }
    lines.sort_by(|a, b| {
        section_rank(a.section)
            .cmp(&section_rank(b.section))
            .then_with(|| a.label.cmp(&b.label))
    });
    lines
}

/// The order the override list presents its blocks in.
fn section_rank(section: &str) -> usize {
    match section {
        "Equalizer" => 0,
        "Graphic EQ" => 1,
        "Tone" => 2,
        "Compressor" => 3,
        "Limiter" => 4,
        "Crossfeed" => 5,
        "Correction" => 6,
        "Bass mgmt" => 7,
        "LFE" => 8,
        "Channel mix" => 9,
        _ => 10,
    }
}

fn describe_section(section: &str, value: &Value, out: &mut Vec<OverrideLine>) {
    match section {
        "eq" => {
            if let Value::Object(fields) = value {
                for (key, value) in fields {
                    out.push(OverrideLine {
                        section: eq_section(key),
                        label: label_for(key),
                        value: format_value(key, value),
                    });
                }
            }
        }
        "bass_management" => describe_object("Bass mgmt", value, out),
        "lfe" => describe_object("LFE", value, out),
        "channel_mix" => describe_object("Channel mix", value, out),
        _ => {}
    }
}

fn describe_object(section: &'static str, value: &Value, out: &mut Vec<OverrideLine>) {
    if let Value::Object(fields) = value {
        for (key, value) in fields {
            out.push(OverrideLine {
                section,
                label: label_for(key),
                value: format_value(key, value),
            });
        }
    }
}

/// Which block of the EQ panel a chain field belongs to.
fn eq_section(key: &str) -> &'static str {
    match key {
        "eq_enabled" | "bands" | "preamp_db" | "auto_headroom" => "Equalizer",
        "graphic_enabled" | "graphic_layout" | "graphic_gains" | "graphic_preamp_db" => {
            "Graphic EQ"
        }
        "bass_shelf_db" | "treble_shelf_db" | "midside_eq" | "stereo_width" | "balance"
        | "dither_enabled" => "Tone",
        "limiter_enabled"
        | "limiter_mode"
        | "limiter_true_peak"
        | "limiter_lookahead_ms"
        | "limiter_attack_ms"
        | "limiter_release_ms"
        | "limiter_ceiling_db"
        | "limiter_soft_clip" => "Limiter",
        "compressor_enabled" | "comp_threshold" | "comp_ratio" | "comp_attack_ms"
        | "comp_release_ms" | "comp_makeup_db" | "comp_knee_db" | "comp_stereo_link" => {
            "Compressor"
        }
        "crossfeed_enabled" | "crossfeed_profile" | "crossfeed_freq" | "crossfeed_q"
        | "crossfeed_delay_ms" => "Crossfeed",
        "correction_enabled" | "correction_depth" | "correction_ir_path" => "Correction",
        _ => "Chain",
    }
}

fn label_for(key: &str) -> String {
    match key {
        "eq_enabled" => "Enabled".to_string(),
        "preamp_db" => "Preamp".to_string(),
        "auto_headroom" => "Auto headroom".to_string(),
        "bands" => "Parametric bands".to_string(),
        "graphic_enabled" => "Enabled".to_string(),
        "graphic_layout" => "Graphic layout".to_string(),
        "graphic_gains" => "Graphic bands".to_string(),
        "graphic_preamp_db" => "Graphic preamp".to_string(),
        "bass_shelf_db" => "Bass shelf".to_string(),
        "treble_shelf_db" => "Treble shelf".to_string(),
        "midside_eq" => "Mid/side EQ".to_string(),
        "stereo_width" => "Stereo width".to_string(),
        "dither_enabled" => "Dither".to_string(),
        "limiter_enabled" => "Enabled".to_string(),
        "limiter_mode" => "Limiter mode".to_string(),
        "limiter_true_peak" => "True peak".to_string(),
        "limiter_lookahead_ms" => "Lookahead".to_string(),
        "limiter_attack_ms" => "Attack".to_string(),
        "limiter_release_ms" => "Release".to_string(),
        "limiter_ceiling_db" => "Ceiling".to_string(),
        "limiter_soft_clip" => "Soft clip".to_string(),
        "compressor_enabled" => "Enabled".to_string(),
        "comp_threshold" => "Threshold".to_string(),
        "comp_ratio" => "Ratio".to_string(),
        "comp_attack_ms" => "Attack".to_string(),
        "comp_release_ms" => "Release".to_string(),
        "comp_makeup_db" => "Makeup gain".to_string(),
        "comp_knee_db" => "Knee".to_string(),
        "comp_stereo_link" => "Stereo link".to_string(),
        "crossfeed_enabled" => "Enabled".to_string(),
        "crossfeed_profile" => "Profile".to_string(),
        "crossfeed_freq" => "Frequency".to_string(),
        "crossfeed_q" => "Q".to_string(),
        "crossfeed_delay_ms" => "Delay".to_string(),
        "correction_enabled" => "Enabled".to_string(),
        "correction_depth" => "Correction depth".to_string(),
        "correction_ir_path" => "Impulse response".to_string(),
        "enabled" => "Enabled".to_string(),
        "crossover_hz" => "Crossover".to_string(),
        "sub_delay_ms" => "Sub delay".to_string(),
        "sub_gain_db" => "Sub gain".to_string(),
        "gain_db" => "Gain".to_string(),
        "template" => "Template".to_string(),
        _ => prettify(key),
    }
}

/// `sub_delay_ms` -> "Sub delay ms", for fields without a hand-written name.
fn prettify(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut first = true;
    for word in key.split('_') {
        if !first {
            out.push(' ');
        }
        first = false;
        let mut chars = word.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.extend(chars);
        }
    }
    out
}

fn format_value(key: &str, value: &Value) -> String {
    match value {
        Value::Bool(true) => "On".to_string(),
        Value::Bool(false) => "Off".to_string(),
        Value::Number(number) => {
            let number = number.as_f64().unwrap_or(0.0);
            if key.ends_with("_db") || key == "preamp_db" {
                format!("{number:+.1} dB")
            } else if key.ends_with("_hz") {
                format!("{number:.0} Hz")
            } else if key.ends_with("_ms") {
                format!("{number:.1} ms")
            } else if key == "stereo_width" {
                format!("{number:.2}×")
            } else if key == "balance" {
                format!("{number:+.2}")
            } else {
                format!("{number}")
            }
        }
        Value::Array(items) => format!("{} values", items.len()),
        Value::String(text) => {
            if text.is_empty() {
                "(none)".to_string()
            } else {
                text.clone()
            }
        }
        Value::Object(_) => "custom".to_string(),
        Value::Null => "—".to_string(),
    }
}

/// The patch a pre-overrides profile implies on `base`.
///
/// Profiles written before whole-chain overrides stored only the graphic EQ
/// and its preamp. Converting them here means an upgrade keeps the curve the
/// user saved, without a migration that would need the global chain (which
/// lives in the UI, not the database).
pub fn legacy_patch(
    profile: &domain::models::TrackDspProfile,
    base: &ChainPreset,
) -> Option<Value> {
    if profile.overrides.is_some() {
        return profile.overrides.clone();
    }
    if !profile.graphic_enabled
        && profile.graphic_gains.is_empty()
        && profile.graphic_preamp_db.abs() < 0.001
    {
        return None;
    }
    let mut chain = base.clone();
    chain.eq.graphic_enabled = profile.graphic_enabled;
    if !profile.graphic_gains.is_empty() {
        // A legacy profile stored the gains array it saw. Lay it over the
        // global array rather than replacing it, so a profile saved against a
        // shorter layout keeps the global values in the remaining slots.
        let mut gains = base.eq.graphic_gains.clone();
        for (slot, value) in gains.iter_mut().zip(&profile.graphic_gains) {
            *slot = *value;
        }
        chain.eq.graphic_gains = gains;
    }
    chain.eq.graphic_preamp_db = profile.graphic_preamp_db;
    diff_chain(base, &chain)
}

/// A neutral chain, for tests and for callers that need a base.
pub fn default_chain() -> ChainPreset {
    ChainPreset {
        name: String::new(),
        note: String::new(),
        eq: ChainEq::default(),
        bass_management: config::BassManagementConfig::default(),
        lfe: config::LfeConfig::default(),
        channel_mix: config::ChannelMixConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::eq_panel::ParametricBand;

    fn base() -> ChainPreset {
        let mut chain = default_chain();
        chain.eq.preamp_db = -1.0;
        chain.eq.compressor_enabled = true;
        chain.eq.bands[0] = ParametricBand {
            freq: 120.0,
            gain_db: 2.0,
            q: 1.0,
            ..ParametricBand::default()
        };
        chain
    }

    #[test]
    fn identical_chains_have_no_diff() {
        assert_eq!(diff_chain(&base(), &base()), None);
    }

    #[test]
    fn a_diff_names_only_the_changed_leaf() {
        let mut current = base();
        current.eq.preamp_db = -4.5;
        let patch = diff_chain(&base(), &current).expect("one field changed");
        assert_eq!(patch, serde_json::json!({"eq": {"preamp_db": -4.5}}));
    }

    #[test]
    fn the_name_and_note_are_never_part_of_the_diff() {
        let mut current = base();
        current.name = "Something else".to_string();
        current.note = "a note".to_string();
        assert_eq!(diff_chain(&base(), &current), None);
    }

    #[test]
    fn a_changed_band_is_stored_as_the_whole_array() {
        let mut current = base();
        current.eq.bands[3].gain_db = 6.0;
        let patch = diff_chain(&base(), &current).unwrap();
        let bands = patch["eq"]["bands"].as_array().unwrap();
        assert_eq!(bands.len(), current.eq.bands.len());
        assert_eq!(bands[3]["gain_db"], 6.0);
    }

    #[test]
    fn merging_a_diff_reproduces_the_current_chain() {
        let mut current = base();
        current.eq.preamp_db = -4.5;
        current.eq.graphic_enabled = true;
        current.eq.graphic_gains[2] = 3.0;
        current.lfe.enabled = true;
        current.channel_mix.enabled = true;
        current.eq.crossfeed_enabled = true;

        let patch = diff_chain(&base(), &current).unwrap();
        let merged = merge_chain(&base(), &patch).unwrap();

        assert_eq!(merged.eq.preamp_db, -4.5);
        assert!(merged.eq.graphic_enabled);
        assert_eq!(merged.eq.graphic_gains[2], 3.0);
        assert!(merged.lfe.enabled);
        assert!(merged.channel_mix.enabled);
        assert!(merged.eq.crossfeed_enabled);
        // The parts the track did not name are inherited from the base.
        assert!(merged.eq.compressor_enabled);
        assert_eq!(merged.eq.bands[0].gain_db, 2.0);
    }

    #[test]
    fn an_inherited_global_change_reaches_the_track() {
        let mut current = base();
        current.eq.preamp_db = -4.5;
        let patch = diff_chain(&base(), &current).unwrap();

        // The user changes the global compressor and preamp later. Only the
        // compressor is inherited; the track's own preamp wins.
        let mut later_global = base();
        later_global.eq.compressor_enabled = false;
        later_global.eq.preamp_db = 0.0;
        let merged = merge_chain(&later_global, &patch).unwrap();
        assert!(
            !merged.eq.compressor_enabled,
            "the global change is inherited"
        );
        assert_eq!(merged.eq.preamp_db, -4.5, "the track override still wins");
    }

    #[test]
    fn a_legacy_graphic_eq_profile_becomes_a_patch() {
        let profile = domain::models::TrackDspProfile {
            graphic_gains: vec![1.0, 2.0],
            graphic_enabled: true,
            graphic_preamp_db: -3.0,
            overrides: None,
        };
        let patch = legacy_patch(&profile, &base()).expect("a legacy profile must convert");
        let merged = merge_chain(&base(), &patch).unwrap();
        assert!(merged.eq.graphic_enabled);
        assert_eq!(merged.eq.graphic_preamp_db, -3.0);
        assert_eq!(merged.eq.graphic_gains[1], 2.0);
        // Untouched graphic bands keep the global value, not zero.
        assert_eq!(merged.eq.graphic_gains[9], base().eq.graphic_gains[9]);
    }

    #[test]
    fn the_override_lines_name_the_blocks_and_the_values() {
        let mut current = base();
        current.eq.preamp_db = -4.5;
        current.eq.compressor_enabled = false;
        current.lfe.enabled = true;
        let patch = diff_chain(&base(), &current).unwrap();
        let lines = describe_overrides(&patch);

        assert!(lines
            .iter()
            .any(|l| l.section == "Equalizer" && l.label == "Preamp" && l.value == "-4.5 dB"));
        assert!(lines
            .iter()
            .any(|l| l.section == "Compressor" && l.label == "Enabled" && l.value == "Off"));
        assert!(lines
            .iter()
            .any(|l| l.section == "LFE" && l.label == "Enabled" && l.value == "On"));
    }

    #[test]
    fn the_override_lines_are_grouped_in_panel_order() {
        let mut current = base();
        current.eq.preamp_db = -4.5;
        current.eq.compressor_enabled = false;
        current.lfe.enabled = true;
        let patch = diff_chain(&base(), &current).unwrap();
        let lines = describe_overrides(&patch);

        let sections: Vec<_> = lines.iter().map(|line| line.section).collect();
        assert_eq!(sections, ["Equalizer", "Compressor", "LFE"]);
    }

    #[test]
    fn a_patch_that_does_not_fit_is_rejected_rather_than_half_applied() {
        let patch = serde_json::json!({"eq": {"preamp_db": "not a number"}});
        assert!(merge_chain(&base(), &patch).is_none());
    }
}

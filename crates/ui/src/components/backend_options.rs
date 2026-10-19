//! Output-backend picker.
//!
//! The list of choices comes from the engine rather than from a hand-written
//! array. The old fixed list offered WASAPI exclusive, CoreAudio hog and ASIO
//! on every platform and in every build, including ones where the native
//! backend is not compiled; choosing one of those silently opened a shared-mode
//! cpal stream instead, so the picker claimed exclusivity the engine never
//! negotiated.

use config::AudioBackend;
use eframe::egui;

use crate::theme::Theme;

/// Human-readable name for a backend, used by the picker and by the runtime
/// diagnostics card.
pub fn backend_label(backend: AudioBackend) -> String {
    match backend {
        AudioBackend::Auto => "Auto Detect".to_string(),
        AudioBackend::ExclusiveAlsa => "ALSA Exclusive".to_string(),
        AudioBackend::ExclusiveWasapi => "WASAPI Exclusive".to_string(),
        AudioBackend::ExclusiveCoreAudioHog => "CoreAudio Hog".to_string(),
        AudioBackend::ExclusiveAsio => "ASIO Direct".to_string(),
        AudioBackend::PipeWire => "PipeWire".to_string(),
        AudioBackend::Jack => "JACK".to_string(),
        AudioBackend::Null => "Null (no device)".to_string(),
        other => format!("{other:?}"),
    }
}

/// Render the backend picker and report a change.
pub fn backend_picker(
    ui: &mut egui::Ui,
    selected: &mut AudioBackend,
    theme: &Theme,
    changed: &mut Option<AudioBackend>,
) {
    let available = engine::output::available_backends();

    // A selection this build cannot open (settings copied between machines, or
    // a build without the native backend) would otherwise show no selected
    // chip at all and stay in force.
    if !available.contains(selected) {
        *selected = AudioBackend::Auto;
        *changed = Some(*selected);
    }

    let options: Vec<(AudioBackend, String)> =
        available.iter().map(|b| (*b, backend_label(*b))).collect();
    let options: Vec<(AudioBackend, &str)> = options
        .iter()
        .map(|(b, label)| (*b, label.as_str()))
        .collect();

    if crate::components::settings_tab::segment_picker(ui, selected, &options, theme) {
        *changed = Some(*selected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_backend_has_a_distinct_human_label() {
        let all = [
            AudioBackend::Auto,
            AudioBackend::ExclusiveAlsa,
            AudioBackend::ExclusiveWasapi,
            AudioBackend::ExclusiveCoreAudioHog,
            AudioBackend::ExclusiveAsio,
            AudioBackend::PipeWire,
            AudioBackend::Jack,
            AudioBackend::Null,
        ];
        let labels: Vec<String> = all.iter().map(|b| backend_label(*b)).collect();
        for label in &labels {
            assert!(!label.is_empty());
            assert!(!label.starts_with('('), "unhandled backend: {label}");
        }
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len(), "labels must be distinct");
    }
}

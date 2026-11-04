//! AutoEQ device-name resolution tests.

use super::*;

fn device(id: &str, name: &str, is_default: bool) -> OutputDeviceInfo {
    OutputDeviceInfo {
        backend: AudioBackend::Auto,
        id: id.to_string(),
        name: name.to_string(),
        is_default,
    }
}

#[test]
fn autoeq_matches_a_display_name_not_a_backend_id() {
    let dsp = DspState {
        output_devices: vec![
            device("wasapi:{endpoint-guid}", "Sennheiser HD 600", false),
            device("wasapi:{default-guid}", "Speakers (Realtek)", true),
        ],
        settings_tab: SettingsTabState {
            output_device: "wasapi:{endpoint-guid}".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    assert_eq!(dsp.resolve_autoeq_device_name(), "Sennheiser HD 600");
}

#[test]
fn an_unpinned_selection_resolves_the_default_device() {
    let dsp = DspState {
        output_devices: vec![device("default", "Sennheiser HD 600", true)],
        ..Default::default()
    };

    assert_eq!(dsp.resolve_autoeq_device_name(), "Sennheiser HD 600");
}

#[test]
fn an_unenumerated_selection_keeps_the_configured_value() {
    let dsp = DspState {
        settings_tab: SettingsTabState {
            output_device: "hw:0,1".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    assert_eq!(dsp.resolve_autoeq_device_name(), "hw:0,1");
}

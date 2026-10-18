//! The CLAP rack model: pure data the engine publishes and a UI draws.
//!
//! These live in an integration test rather than beside the module because the
//! engine crate's unit-test target has been unbuildable since before this
//! feature landed — several inline `#[cfg(test)]` modules under `src/decode/`
//! reference dev-dependencies (`toml`, `rusty-opus`, `wavicle/encode`) the
//! manifest does not declare. Testing the model here means it actually runs.
//!
//! Nothing here needs the `clap-host` feature: the model is plain data and the
//! feature only decides whether the engine ever fills it in.

use engine::dsp::clap_rack::{ClapParamInfo, ClapPluginInfo, ClapRackReport, ClapRackView};

fn param(id: u32, min: f64, max: f64, value: f64) -> ClapParamInfo {
    ClapParamInfo {
        id,
        min,
        max,
        value,
        ..Default::default()
    }
}

#[test]
fn normalisation_round_trips_and_survives_a_degenerate_range() {
    let p = param(1, -12.0, 12.0, 6.0);
    assert!((p.normalised() - 0.75).abs() < 1e-9);
    assert!((p.from_normalised(0.75) - 6.0).abs() < 1e-9);

    // min == max would divide by zero; 0.0 is the safe answer.
    let flat = param(2, 1.0, 1.0, 1.0);
    assert_eq!(flat.normalised(), 0.0);
    assert_eq!(flat.from_normalised(0.5), 1.0);
}

#[test]
fn out_of_range_positions_are_clamped() {
    let p = param(1, 0.0, 10.0, 20.0);
    assert_eq!(p.normalised(), 1.0);
    assert_eq!(p.from_normalised(-3.0), 0.0);
    assert_eq!(p.from_normalised(7.0), 10.0);
}

#[test]
fn hidden_and_readonly_parameters_are_not_editable() {
    let mut p = param(1, 0.0, 1.0, 0.5);
    assert!(p.is_editable());
    p.is_readonly = true;
    assert!(!p.is_editable());
    p.is_readonly = false;
    p.is_hidden = true;
    assert!(!p.is_editable());
}

#[test]
fn an_empty_report_names_itself_rather_than_panicking() {
    let report = ClapRackReport::default();
    assert!(!report.is_loaded());
    assert_eq!(report.display_name(), "No plugin");
    assert_eq!(report.editable_params().count(), 0);
    assert!(report.bypass_param().is_none());
    assert!(!ClapRackView::default().is_loaded());
}

#[test]
fn the_display_name_falls_back_to_the_library_file() {
    let report = ClapRackReport {
        path: Some("/plugins/SuperVerb.clap".to_string()),
        ..Default::default()
    };
    assert_eq!(report.display_name(), "SuperVerb");

    // A plugin that reports a name always wins over the file name.
    let report = ClapRackReport {
        path: Some("/plugins/SuperVerb.clap".to_string()),
        plugin: Some(ClapPluginInfo {
            name: "Super Verb".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(report.display_name(), "Super Verb");
}

#[test]
fn only_editable_parameters_are_offered_and_bypass_is_found() {
    let report = ClapRackReport {
        plugin: Some(ClapPluginInfo {
            name: "Verb".to_string(),
            ..Default::default()
        }),
        params: vec![
            param(0, 0.0, 1.0, 0.5),
            ClapParamInfo {
                is_hidden: true,
                ..param(1, 0.0, 1.0, 0.5)
            },
            ClapParamInfo {
                is_readonly: true,
                ..param(2, 0.0, 1.0, 0.5)
            },
            ClapParamInfo {
                is_bypass: true,
                ..param(3, 0.0, 1.0, 0.0)
            },
        ],
        ..Default::default()
    };

    let offered: Vec<u32> = report.editable_params().map(|p| p.id).collect();
    assert_eq!(offered, vec![0, 3], "hidden and read-only are not controls");
    assert_eq!(report.bypass_param().map(|p| p.id), Some(3));
    assert!(report.is_loaded());
}

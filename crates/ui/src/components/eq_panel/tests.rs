use super::*;
use engine::dsp::equalizer::EqFilterType;

#[test]
fn layout_change_interpolates_the_visible_curve_and_clears_stale_bands() {
    let mut state = EqPanelState::default();
    state.graphic_gains[..10]
        .copy_from_slice(&[4.0, 3.0, 2.5, 0.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0]);
    state.graphic_gains[10] = 99.0; // stale slot outside the 10-band layout

    state.set_graphic_layout(GraphicEqLayout::ThirtyOneBand);

    assert_eq!(state.graphic_layout, GraphicEqLayout::ThirtyOneBand);
    let freqs = state.graphic_layout.frequencies();
    for (freq, expected) in [
        (31.5, 4.0),
        (63.0, 3.0),
        (125.0, 2.5),
        (500.0, -1.0),
        (1000.0, -2.0),
        (16000.0, 4.0),
    ] {
        let index = freqs.iter().position(|f| (*f - freq).abs() < 1e-3).unwrap();
        assert!(
            (state.graphic_gains[index] - expected).abs() < 0.05,
            "{freq} Hz should survive the layout change, got {} dB",
            state.graphic_gains[index]
        );
    }
    assert!(
        state.graphic_gains[31..].iter().all(|g| *g == 0.0),
        "bands beyond the new layout must not keep stale gains"
    );

    state.set_graphic_layout(GraphicEqLayout::TenBand);
    assert_eq!(
        &state.graphic_gains[..10],
        &[4.0, 3.0, 2.5, 0.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0]
    );
    assert!(state.graphic_gains[10..].iter().all(|g| *g == 0.0));
}

#[test]
fn saved_window_size_round_trips_and_rejects_garbage() {
    let saved = PersistentEqState {
        window_size: Some([640.0, 420.0]),
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    saved.apply_to(&mut state);
    assert_eq!(state.window_size, Some([640.0, 420.0]));

    let round_trip = PersistentEqState::from(&state);
    assert_eq!(
        round_trip.window_size,
        Some([640.0, 420.0]),
        "a user resize must survive the next launch"
    );

    let invalid = PersistentEqState {
        window_size: Some([f32::NAN, 0.0]),
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    invalid.apply_to(&mut state);
    assert_eq!(state.window_size, None);
}

#[test]
fn a_saved_tab_is_reconciled_with_the_audible_mode() {
    let parametric = PersistentEqState {
        eq_enabled: true,
        eq_mode: Some(EqMode::Parametric),
        active_tab: EqTab::Graphic,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    parametric.apply_to(&mut state);
    assert_eq!(state.active_tab, EqTab::Parametric);

    let graphic = PersistentEqState {
        eq_enabled: true,
        eq_mode: Some(EqMode::Graphic),
        active_tab: EqTab::Parametric,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    graphic.apply_to(&mut state);
    assert_eq!(state.active_tab, EqTab::Graphic);
}

#[test]
fn legacy_persisted_state_infers_graphic_only_from_the_legacy_flag() {
    let legacy = PersistentEqState {
        eq_enabled: true,
        eq_mode: None,
        graphic_enabled: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    legacy.apply_to(&mut state);
    assert_eq!(state.eq_mode, EqMode::Graphic);
    assert!(state.graphic_enabled);

    let active_legacy_graphic = PersistentEqState {
        eq_enabled: false,
        eq_mode: None,
        graphic_enabled: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    active_legacy_graphic.apply_to(&mut state);
    assert!(state.eq_enabled, "the legacy graphic layer was audible");
    assert!(state.graphic_enabled);

    let parametric = PersistentEqState {
        eq_enabled: true,
        eq_mode: None,
        graphic_enabled: false,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    parametric.apply_to(&mut state);
    assert_eq!(state.eq_mode, EqMode::Parametric);
    assert!(!state.graphic_enabled);

    // An explicit mode is authoritative even if the old layer flag still
    // says the graphic EQ used to be active.
    let explicit = PersistentEqState {
        eq_enabled: true,
        eq_mode: Some(EqMode::Parametric),
        graphic_enabled: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    explicit.apply_to(&mut state);
    assert_eq!(state.eq_mode, EqMode::Parametric);
    assert!(!state.graphic_enabled);
}

#[test]
fn persisted_extreme_graphic_gain_widens_the_visible_range() {
    let saved = PersistentEqState {
        eq_mode: Some(EqMode::Graphic),
        graphic_gains: vec![18.0; 10],
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    saved.apply_to(&mut state);
    assert!(state.extended_range);
    assert!((state.graphic_gains[0] - 18.0).abs() < 1e-6);
}

#[test]
fn persisted_preamp_prefers_the_shared_field_and_backfills_the_mirror() {
    let saved = PersistentEqState {
        eq_mode: Some(EqMode::Parametric),
        preamp_db: -3.0,
        graphic_preamp_db: -8.0,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    saved.apply_to(&mut state);
    assert_eq!(state.preamp_db, -3.0);
    assert_eq!(state.graphic_preamp_db, -3.0);

    let legacy_graphic = PersistentEqState {
        eq_mode: None,
        graphic_enabled: true,
        preamp_db: 0.0,
        graphic_preamp_db: -6.0,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    legacy_graphic.apply_to(&mut state);
    assert_eq!(state.eq_mode, EqMode::Graphic);
    assert_eq!(state.preamp_db, -6.0);
    assert_eq!(state.graphic_preamp_db, -6.0);

    let inert_graphic = PersistentEqState {
        eq_mode: None,
        preamp_db: 0.0,
        graphic_preamp_db: -6.0,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    inert_graphic.apply_to(&mut state);
    assert_eq!(state.eq_mode, EqMode::Parametric);
    assert_eq!(state.preamp_db, 0.0);
}

#[test]
fn preamp_is_clamped_to_the_range_the_engine_can_represent() {
    let mut state = EqPanelState::default();
    state.set_preamp(45.0);
    assert_eq!(state.preamp_db, 30.0);
    assert_eq!(state.graphic_preamp_db, 30.0);
    state.set_preamp(-45.0);
    assert_eq!(state.preamp_db, -30.0);
    state.set_preamp(f32::NAN);
    assert_eq!(state.preamp_db, -30.0);
}

#[test]
fn parametric_import_installs_every_band_its_preamp_and_the_mode() {
    let text = "\
Preamp: -5.0 dB
Filter 1: ON LSC Fc 105 Hz Gain 4.0 dB Q 0.70
Filter 2: ON PK Fc 1000 Hz Gain -3.0 dB Q 1.20
Filter 3: ON HSC Fc 10000 Hz Gain 2.0 dB Q 0.70
";
    let curve = config::parse_curve("imported", text).unwrap();
    let mut state = EqPanelState {
        eq_enabled: false,
        auto_headroom: true,
        ..Default::default()
    };

    assert!(state.apply_imported_curve(&curve));

    assert!(state.eq_enabled);
    assert_eq!(state.eq_mode, EqMode::Parametric);
    assert!(!state.graphic_enabled);
    assert_eq!(state.bands.len(), 3);
    assert_eq!(state.bands[0].filter_type, EqFilterType::LowShelf);
    assert!((state.preamp_db + 5.0).abs() < 1e-6);
    assert!(
        !state.auto_headroom,
        "an imported curve carries its own level; attenuating it twice is wrong"
    );
}

#[test]
fn graphic_import_lands_on_the_active_ladder_without_dropping_points() {
    let text = "GraphicEQ: 31.5 4.0; 1000 -2.0; 16000 3.0";
    let curve = config::parse_curve("graphic", text).unwrap();
    let mut state = EqPanelState {
        graphic_layout: GraphicEqLayout::ThirtyOneBand,
        ..Default::default()
    };

    assert!(!state.apply_imported_curve(&curve));

    assert_eq!(state.eq_mode, EqMode::Graphic);
    let freqs = state.graphic_layout.frequencies();
    let low = freqs.iter().position(|f| (*f - 31.5).abs() < 0.01).unwrap();
    let mid = freqs
        .iter()
        .position(|f| (*f - 1000.0).abs() < 0.01)
        .unwrap();
    assert!((state.graphic_gains[low] - 4.0).abs() < 0.1);
    assert!((state.graphic_gains[mid] + 2.0).abs() < 0.1);
}

#[test]
fn exported_parametric_text_reimports_to_the_same_curve() {
    let text = "Preamp: -2.0 dB\nFilter 1: ON PK Fc 500 Hz Gain -4.0 dB Q 1.00";
    let curve = config::parse_curve("exported", text).unwrap();
    let mut state = EqPanelState::default();
    state.apply_imported_curve(&curve);

    let (name, exported) = state.export_curve_text();
    assert!(name.ends_with(".txt"));
    let reparsed = config::parse_curve("reimported", &exported).unwrap();
    assert_eq!(reparsed.bands.len(), 1);
    assert!((reparsed.preamp_db + 2.0).abs() < 1e-6);
    assert!((reparsed.bands[0].gain_db + 4.0).abs() < 1e-6);
}

use super::*;
use audio_session::engine::dsp::equalizer::{EqFilterType, SHELF_Q};

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
fn a_pre_migration_snapshot_drops_the_stale_auto_headroom_default() {
    // Version 0 blobs were written while `auto_headroom` defaulted to ON. That
    // default silently attenuated the whole mix by the curve's peak boost, so
    // every boost stayed at its old level and the EQ looked inert. The value
    // cannot be told apart from a deliberate choice, so the migration resets
    // it once instead of leaving it on forever.
    let legacy = PersistentEqState {
        schema_version: 0,
        auto_headroom: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    legacy.apply_to(&mut state);
    assert!(
        !state.auto_headroom,
        "a snapshot from the old default must not keep auto headroom on"
    );

    // A v1 snapshot is the user's own choice and survives the round trip.
    let chosen = PersistentEqState {
        schema_version: 1,
        auto_headroom: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    chosen.apply_to(&mut state);
    assert!(state.auto_headroom);

    let written = PersistentEqState::from(&state);
    assert_eq!(written.schema_version, 1);
    assert!(written.auto_headroom);
}

#[test]
fn a_persisted_midside_toggle_is_not_restored() {
    // The control is unavailable because one shared curve for mid and side is
    // identical to L/R filtering. A stale snapshot holding the old toggle must
    // not bring it back: the engine would report a mode change — and drop the
    // bit-perfect claim — for something inaudible.
    let legacy = PersistentEqState {
        midside_eq: true,
        ..Default::default()
    };
    let mut state = EqPanelState::default();
    legacy.apply_to(&mut state);
    assert!(!state.midside_eq);
}

#[test]
fn a_legacy_blob_without_a_version_field_is_treated_as_version_zero() {
    // The real upgrade path is JSON, not a struct literal. A blob written by
    // the old build has no `schema_version` key at all, and the container's
    // `#[serde(default)]` would otherwise fill it from a fresh snapshot — the
    // *current* version — so the migration would never see a legacy blob.
    let legacy: PersistentEqState =
        serde_json::from_str(r#"{"auto_headroom": true}"#).expect("legacy blob must parse");
    assert_eq!(
        legacy.schema_version, 0,
        "a missing schema_version must read as 0, not as a fresh snapshot's version"
    );

    let mut state = EqPanelState::default();
    legacy.apply_to(&mut state);
    assert!(
        !state.auto_headroom,
        "the JSON a user actually has on disk must lose the stale default"
    );

    // The same JSON written back carries the version and the reset choice, so
    // the migration does not run a second time.
    let rewritten = PersistentEqState::from(&state);
    let json = serde_json::to_string(&rewritten).unwrap();
    let reloaded: PersistentEqState = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded.schema_version, 1);
    assert!(!reloaded.auto_headroom);
}

#[test]
fn an_explicit_auto_headroom_choice_survives_the_json_round_trip() {
    // Once the user turns auto-headroom on in a migrated build, that decision
    // is theirs: the version gate must not reset it on the next launch.
    let chosen = EqPanelState {
        auto_headroom: true,
        ..Default::default()
    };
    let json = serde_json::to_string(&PersistentEqState::from(&chosen)).unwrap();
    let reloaded: PersistentEqState = serde_json::from_str(&json).unwrap();
    let mut state = EqPanelState::default();
    reloaded.apply_to(&mut state);
    assert!(
        state.auto_headroom,
        "a deliberate v1 choice must survive the version gate"
    );
}

#[test]
fn the_default_parametric_ladder_is_octave_wide_with_shelving_ends() {
    let state = EqPanelState::default();
    assert_eq!(state.bands.len(), 10);
    let first = &state.bands[0];
    let last = state.bands.last().unwrap();
    assert_eq!(first.filter_type, EqFilterType::LowShelf);
    assert_eq!(last.filter_type, EqFilterType::HighShelf);
    assert!(state.bands[1..state.bands.len() - 1]
        .iter()
        .all(|band| band.filter_type == EqFilterType::Peaking));
    // Q = 0.707 is a *two-octave* band: at octave spacing the bands overlap
    // into one broad hump, so three +6 dB moves became +13 dB. The ladder's
    // peaking bands carry the octave's own Q, and the shelves carry the
    // maximally flat shelf slope — a shelving filter's Q is not a bandwidth,
    // and handing it 1.4 makes it ring above the corner.
    for band in &state.bands[1..state.bands.len() - 1] {
        assert!(
            (band.q - 1.4).abs() < 1e-6,
            "{} Hz band has Q {}, expected the octave Q",
            band.freq,
            band.q
        );
    }
    for shelf in [first, last] {
        assert!(
            (shelf.q - SHELF_Q).abs() < 1e-6,
            "{} Hz shelf has Q {}, expected the maximally flat shelf slope",
            shelf.freq,
            shelf.q
        );
    }
    assert!((first.freq - 31.5).abs() < 1e-3);
    assert!((last.freq - 16000.0).abs() < 1e-3);
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

#[test]
fn undo_redo_curve_buffer_preserves_history() {
    let mut state = EqPanelState::default();
    assert!(!state.undo());
    assert!(!state.redo());

    // Step 1: Push undo before tweaking
    state.push_undo();
    state.graphic_gains[0] = 3.5;

    // Step 2: Push undo again before second tweak
    state.push_undo();
    state.graphic_gains[0] = 6.0;

    // Undo step 2
    assert!(state.undo());
    assert!((state.graphic_gains[0] - 3.5).abs() < 1e-4);

    // Undo step 1
    assert!(state.undo());
    assert!((state.graphic_gains[0] - 0.0).abs() < 1e-4);

    // Redo step 1
    assert!(state.redo());
    assert!((state.graphic_gains[0] - 3.5).abs() < 1e-4);

    // Redo step 2
    assert!(state.redo());
    assert!((state.graphic_gains[0] - 6.0).abs() < 1e-4);
    assert!(!state.redo());
}

#[test]
fn ab_audition_swaps_and_restores_slots() {
    let mut state = EqPanelState::default();
    state.graphic_gains[0] = 5.0;
    state.preamp_db = -2.5;

    // Toggle to Slot B (saves current state to Slot A, baseline to Slot B)
    state.toggle_ab_audition();
    assert!(state.is_auditioning_b);

    // Modify Slot B
    state.graphic_gains[0] = 0.0;
    state.preamp_db = 0.0;

    // Toggle back to Slot A
    state.toggle_ab_audition();
    assert!(!state.is_auditioning_b);
    assert!((state.graphic_gains[0] - 5.0).abs() < 1e-4);
    assert!((state.preamp_db - (-2.5)).abs() < 1e-4);

    // Toggle to Slot B again
    state.toggle_ab_audition();
    assert!(state.is_auditioning_b);
    assert!((state.graphic_gains[0] - 0.0).abs() < 1e-4);
    assert!((state.preamp_db - 0.0).abs() < 1e-4);
}

/// A/B must also work when the parametric curve is the audible one: the
/// comparison previously carried no band data, so both slots sounded
/// identical and the toggle did nothing.
#[test]
fn ab_audition_swaps_the_parametric_curve_and_tone() {
    let mut state = EqPanelState {
        eq_mode: config::EqMode::Parametric,
        ..Default::default()
    };
    state.bands[0].gain_db = 8.0;
    state.bands[1].gain_db = -3.0;
    state.bass_shelf_db = 4.0;
    state.treble_shelf_db = -2.0;

    state.toggle_ab_audition();
    assert!(state.is_auditioning_b);
    assert!(
        state.bands.iter().all(|band| band.gain_db == 0.0),
        "the B slot must be the flat parametric reference"
    );
    assert_eq!(state.bass_shelf_db, 0.0);
    assert_eq!(state.treble_shelf_db, 0.0);
    // A zeroed high-pass still filters, so every band is switched off too.
    assert!(
        state.bands.iter().all(|band| !band.enabled),
        "the flat reference must disable every band, not just zero its gain"
    );

    state.toggle_ab_audition();
    assert!(!state.is_auditioning_b);
    assert!((state.bands[0].gain_db - 8.0).abs() < 1e-4);
    assert!((state.bands[1].gain_db + 3.0).abs() < 1e-4);
    assert!((state.bass_shelf_db - 4.0).abs() < 1e-4);
    assert!((state.treble_shelf_db + 2.0).abs() < 1e-4);
}

#[test]
fn view_mode_persists_round_trip() {
    let state = EqPanelState {
        view_mode: EqViewMode::QuickTune,
        ..Default::default()
    };

    let persistent = PersistentEqState::from(&state);
    assert_eq!(persistent.view_mode, EqViewMode::QuickTune);

    let mut restored = EqPanelState::default();
    persistent.apply_to(&mut restored);
    assert_eq!(restored.view_mode, EqViewMode::QuickTune);
}

//! Pure Direct round-trip and device-name resolution tests.

use super::*;

#[test]
fn pure_direct_round_trip_restores_the_chain_state() {
    let mut state = DspState::default();
    state.eq_panel.eq_enabled = true;
    state.eq_panel.set_eq_mode(config::EqMode::Graphic);
    state.eq_panel.set_preamp(-7.5);
    state.eq_panel.limiter_enabled = true;
    state.eq_panel.compressor_enabled = true;
    state.settings_tab.spatial_enabled = true;
    state.settings_tab.stereo_enhancer_enabled = true;

    state.settings_tab.speed = 1.25;
    state.capture_pure_direct(0.7);
    // The same fields enter_pure_direct then mirrors as off/unity.
    state.settings_tab.speed = 1.0;
    state.eq_panel.eq_enabled = false;
    state.eq_panel.graphic_enabled = false;
    state.eq_panel.limiter_enabled = false;
    state.eq_panel.compressor_enabled = false;
    state.eq_panel.set_preamp(0.0);
    state.settings_tab.spatial_enabled = false;
    state.settings_tab.stereo_enhancer_enabled = false;

    let saved = state
        .restore_pure_direct_state()
        .expect("the bypass must have saved a snapshot");
    assert_eq!(saved.eq_mode, config::EqMode::Graphic);
    assert!((saved.volume - 0.7).abs() < 1e-6);
    assert!((state.settings_tab.speed - 1.25).abs() < 1e-6);
    assert!(state.eq_panel.eq_enabled);
    assert_eq!(state.eq_panel.eq_mode, config::EqMode::Graphic);
    assert!(state.eq_panel.graphic_enabled);
    assert!((state.eq_panel.preamp_db - (-7.5)).abs() < 1e-6);
    assert!(state.eq_panel.limiter_enabled);
    assert!(state.eq_panel.compressor_enabled);
    assert!(state.settings_tab.spatial_enabled);
    assert!(state.settings_tab.stereo_enhancer_enabled);
}

#[test]
fn entering_pure_direct_twice_does_not_overwrite_the_snapshot() {
    let mut state = DspState::default();
    state.eq_panel.eq_enabled = true;
    state.eq_panel.set_eq_mode(config::EqMode::Graphic);
    state.eq_panel.set_preamp(-4.0);
    state.capture_pure_direct(0.7);
    state.eq_panel.eq_enabled = false;
    state.eq_panel.set_preamp(0.0);
    state.capture_pure_direct(0.3);
    state.restore_pure_direct_state().unwrap();
    assert!(state.eq_panel.eq_enabled);
    assert!((state.eq_panel.preamp_db - (-4.0)).abs() < 1e-6);
}

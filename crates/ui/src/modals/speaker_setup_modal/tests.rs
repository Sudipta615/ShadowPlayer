//! Unit tests for acoustic alignment and speaker setup suite.

use super::types::*;
use config::{CrossoverFilterType, CrossoverSlope};

#[test]
fn test_unit_conversion_and_speed_of_sound() {
    let m = 2.5;
    let ft = DistanceUnit::Meters.convert_to(m, DistanceUnit::Feet);
    assert!((ft - 8.2021).abs() < 1e-3);

    let back_to_m = DistanceUnit::Feet.convert_to(ft, DistanceUnit::Meters);
    assert!((back_to_m - m).abs() < 1e-4);

    assert_eq!(DistanceUnit::Meters.speed_of_sound(), SPEED_OF_SOUND_MPS);
    assert_eq!(DistanceUnit::Feet.speed_of_sound(), SPEED_OF_SOUND_FPS);
}

#[test]
fn test_layout_channel_counts_and_roles() {
    let mut state = SpeakerSetupModalState::default();

    state.set_layout(SpeakerLayout::Stereo20);
    assert_eq!(state.speakers.len(), 2);
    assert_eq!(state.speakers[0].role, SpeakerRole::FrontLeft);
    assert_eq!(state.speakers[1].role, SpeakerRole::FrontRight);

    state.set_layout(SpeakerLayout::Stereo21);
    assert_eq!(state.speakers.len(), 3);
    assert_eq!(state.speakers[2].role, SpeakerRole::Subwoofer);

    state.set_layout(SpeakerLayout::Lcr31);
    assert_eq!(state.speakers.len(), 4);

    state.set_layout(SpeakerLayout::Surround51);
    assert_eq!(state.speakers.len(), 6);

    state.set_layout(SpeakerLayout::Surround71);
    assert_eq!(state.speakers.len(), 8);
    assert_eq!(state.speakers[6].role, SpeakerRole::RearLeft);
    assert_eq!(state.speakers[7].role, SpeakerRole::RearRight);
}

#[test]
fn test_distance_delay_physics_compensation() {
    let mut state = SpeakerSetupModalState {
        unit: DistanceUnit::Meters,
        ..Default::default()
    };
    state.set_layout(SpeakerLayout::Stereo21);

    // Set furthest speaker (subwoofer at 3.432 m)
    state.speakers[0].distance = 2.432; // FL: 1.0 m closer
    state.speakers[1].distance = 3.432; // FR: furthest
    state.speakers[2].distance = 3.432; // SUB: furthest

    assert_eq!(state.max_distance(), 3.432);

    // Furthest speakers must have 0.0 ms arrival delay
    let fr_delay = state.delay_ms_for(&state.speakers[1]);
    assert_eq!(fr_delay, 0.0);

    // Speaker 1.0 m closer must have: 1.0 / 343.2 * 1000 ≈ 2.9137 ms delay
    let fl_delay = state.delay_ms_for(&state.speakers[0]);
    let expected_ms = 1.0 / SPEED_OF_SOUND_MPS * 1000.0;
    assert!((fl_delay - expected_ms).abs() < 1e-3);
}

#[test]
fn test_to_engine_configs_generation() {
    let mut state = SpeakerSetupModalState::default();
    state.set_layout(SpeakerLayout::Stereo21);
    state.speakers[0].trim_db = -1.5;
    state.speakers[0].invert = false;
    state.speakers[1].trim_db = 0.5;
    state.speakers[1].invert = true;

    state.bass_management_enabled = true;
    state.crossover_hz = 80.0;
    state.slope = CrossoverSlope::Db24;
    state.filter_type = CrossoverFilterType::LinkwitzRiley;
    state.sub_phase_deg = 45.0;
    state.sub_polarity_invert = true;
    state.sub_fine_delay_ms = 1.5;

    let trim_cfg = state.to_engine_trim_config();
    assert!(trim_cfg.enabled);
    assert_eq!(trim_cfg.entries.len(), 3);
    assert_eq!(trim_cfg.entries[0].gain_db, -1.5);
    assert!(!trim_cfg.entries[0].invert);
    assert_eq!(trim_cfg.entries[1].gain_db, 0.5);
    assert!(trim_cfg.entries[1].invert);

    let bass_cfg = state.to_engine_bass_config();
    assert!(bass_cfg.enabled);
    assert_eq!(bass_cfg.crossover_hz, 80.0);
    assert_eq!(bass_cfg.slope, CrossoverSlope::Db24);
    assert_eq!(bass_cfg.filter_type, CrossoverFilterType::LinkwitzRiley);
    assert_eq!(bass_cfg.sub_phase_degrees, 45.0);
    assert!(bass_cfg.sub_polarity_invert);
    assert!(bass_cfg.sub_delay_ms >= 1.5);
}

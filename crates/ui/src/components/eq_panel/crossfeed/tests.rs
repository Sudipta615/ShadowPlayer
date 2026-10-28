//! Unit tests for crossfeed presets and acoustic models.

use super::types::*;
use config::CrossfeedProfile;

#[test]
fn itd_and_path_offset_are_physically_grounded() {
    let delay_ms = 0.30;
    let us = calculate_itd_delay_us(delay_ms);
    assert!((us - 300.0).abs() < 1e-4);

    let cm = calculate_acoustic_path_cm(delay_ms);
    assert!((cm - 10.296).abs() < 0.01);
}

#[test]
fn biquad_transfer_function_has_lowpass_characteristics() {
    let fc = 700.0;
    let q = 0.707;

    let gain_dc = biquad_crossfeed_gain_db(50.0, fc, q);
    let gain_fc = biquad_crossfeed_gain_db(700.0, fc, q);
    let gain_hf = biquad_crossfeed_gain_db(7000.0, fc, q);

    assert!((gain_dc - (-6.0)).abs() < 0.5);
    assert!((gain_fc - (-9.0)).abs() < 0.5);
    assert!(gain_hf < -24.0);
}

#[test]
fn preset_detection_detects_all_profiles() {
    assert_eq!(
        CalibratedCrossfeedPreset::detect(CrossfeedProfile::Bauer, 700.0, 0.5, 0.3),
        CalibratedCrossfeedPreset::Bauer
    );
    assert_eq!(
        CalibratedCrossfeedPreset::detect(CrossfeedProfile::Custom, 700.0, 0.707, 0.30),
        CalibratedCrossfeedPreset::Subtle
    );
    assert_eq!(
        CalibratedCrossfeedPreset::detect(CrossfeedProfile::Custom, 850.0, 0.707, 0.30),
        CalibratedCrossfeedPreset::Pronounced
    );
    assert_eq!(
        CalibratedCrossfeedPreset::detect(CrossfeedProfile::Custom, 600.0, 0.707, 0.28),
        CalibratedCrossfeedPreset::StudioMonitor
    );
}

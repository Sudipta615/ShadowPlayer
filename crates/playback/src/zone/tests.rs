//! Tests for ZoneCoordinator, multi-zone queue isolation, clock drift, and ZoneSwitch routing.

use domain::ids::TrackId;
use domain::models::Track;
use domain::smart::SmartOp;
use domain::zone::{ZoneConfig, ZoneMatchField, ZoneSwitchCondition, ZoneSwitchRule};
use std::path::PathBuf;

use super::coordinator::ZoneCoordinator;

fn make_track(title: &str, format: &str, sample_rate: u32, channels: usize) -> Track {
    Track {
        id: TrackId::new(),
        path: PathBuf::from(format!("/music/{title}.{format}")),
        title: title.into(),
        artist: "Test Artist".into(),
        album: "Test Album".into(),
        genre: Some("Jazz".into()),
        year: Some(2023),
        track_number: Some(1),
        disc_number: Some(1),
        duration_secs: 180.0,
        bpm: None,
        sample_rate,
        channels,
        format: format.into(),
        bitrate_kbps: Some(1411),
        bit_depth: Some(24),
        is_lossless: true,
        replaygain_track_db: None,
        replaygain_track_peak: None,
        replaygain_album_db: None,
        replaygain_album_peak: None,
        r128_loudness_lufs: None,
        file_size_bytes: 30_000_000,
        mtime_unix_secs: 1_700_000_000,
        mtime_nanos: 0,
        content_hash: None,
        file_id: None,
        is_favorite: false,
        rating: 4,
        has_lyrics: false,
        play_count: 5,
        skip_count: 0,
        last_played_at: None,
        added_at: 1_700_000_000,
        sort_artist_folded: None,
        sort_title_folded: None,
        sort_album_folded: None,
    }
}

#[test]
fn multi_zone_queue_isolation() {
    let default_cfg = ZoneConfig::main_default();
    let mut coordinator = ZoneCoordinator::new(default_cfg.clone());

    let office_cfg = ZoneConfig::new_custom("Office Zone", Some("Office DAC".into()));
    let office_id = office_cfg.id;
    coordinator.add_zone(office_cfg);

    let t1 = make_track("Song 1", "FLAC", 44100, 2);
    let t2 = make_track("Song 2", "FLAC", 96000, 2);

    // Play in main zone
    let main_id = default_cfg.id;
    coordinator.set_active_zone(main_id);
    let main_state = coordinator.active_zone_mut().unwrap();
    main_state.play_track(t1.clone(), Some(vec![t1.clone()]));
    assert!(main_state.is_playing);
    assert_eq!(main_state.queue.len(), 1);

    // Office zone queue should remain completely empty and idle
    let office_state = coordinator.get_zone(office_id).unwrap();
    assert!(!office_state.is_playing);
    assert!(office_state.queue.is_empty());

    // Switch to office zone and play t2
    coordinator.set_active_zone(office_id);
    let office_state_mut = coordinator.active_zone_mut().unwrap();
    office_state_mut.play_track(t2.clone(), Some(vec![t2.clone()]));
    assert!(office_state_mut.is_playing);
    assert_eq!(office_state_mut.queue[0].title, "Song 2");

    // Main zone queue is still untouched
    let main_check = coordinator.get_zone(main_id).unwrap();
    assert_eq!(main_check.queue[0].title, "Song 1");
}

#[test]
fn master_slave_link_and_sync() {
    let main_cfg = ZoneConfig::main_default();
    let main_id = main_cfg.id;
    let mut coordinator = ZoneCoordinator::new(main_cfg);

    let patio_cfg = ZoneConfig::new_custom("Patio Zone", Some("Bluetooth Outdoor".into()));
    let patio_id = patio_cfg.id;
    coordinator.add_zone(patio_cfg);

    // Link patio as slave to main with +25 ms offset
    coordinator.link_zones(main_id, patio_id, 25.0);
    assert_eq!(coordinator.get_linked_slaves(main_id), vec![patio_id]);

    // Master starts playback at 40.0s
    coordinator.sync_linked_playback(main_id, true, 40.0);

    let patio_state = coordinator.get_zone(patio_id).unwrap();
    assert!(patio_state.is_playing);
    // 40.0s + 0.025s offset = 40.025s
    assert!((patio_state.position_secs - 40.025).abs() < 1e-4);

    // Unlink patio
    coordinator.unlink_zone(patio_id);
    assert!(coordinator.get_linked_slaves(main_id).is_empty());
}

#[test]
fn clock_drift_compensation_adjusts_rate() {
    let main_cfg = ZoneConfig::main_default();
    let mut coordinator = ZoneCoordinator::new(main_cfg);

    let dac_cfg = ZoneConfig::new_custom("Secondary DAC", None);
    let dac_id = dac_cfg.id;
    coordinator.add_zone(dac_cfg);

    // Buffer lag of 4.0 ms produces 4.0 * 0.005 = +0.02% adjustment
    let drift = coordinator.compensate_clock_drift(dac_id, 4.0);
    assert!((drift - 0.02).abs() < 1e-4);

    let dac_state = coordinator.get_zone(dac_id).unwrap();
    assert!((dac_state.clock_drift.resampler_rate_ratio() - 1.0002).abs() < 1e-5);
}

#[test]
fn zone_switch_rule_prioritized_routing() {
    let main_cfg = ZoneConfig::main_default();
    let mut coordinator = ZoneCoordinator::new(main_cfg);

    let dsd_dac = ZoneConfig::new_custom("DSD DAC", None);
    let dsd_id = dsd_dac.id;
    coordinator.add_zone(dsd_dac);

    let avr_zone = ZoneConfig::new_custom("Home Theater AVR", None);
    let avr_id = avr_zone.id;
    coordinator.add_zone(avr_zone);

    let rule_dsd = ZoneSwitchRule::new(
        "DSD Rule",
        dsd_id,
        vec![ZoneSwitchCondition::new(
            ZoneMatchField::Format,
            SmartOp::Equals,
            "DSD",
        )],
        10,
    );

    let rule_surround = ZoneSwitchRule::new(
        "Surround 5.1",
        avr_id,
        vec![ZoneSwitchCondition::new(
            ZoneMatchField::Channels,
            SmartOp::GreaterThan,
            "2",
        )],
        20, // Higher priority
    );

    coordinator.set_rules(vec![rule_dsd, rule_surround]);

    let dsd_track = make_track("DSD Track", "DSD", 2822400, 2);
    let surround_track = make_track("Multichannel Track", "FLAC", 48000, 6);
    let ordinary_track = make_track("Stereo FLAC", "FLAC", 44100, 2);

    assert_eq!(coordinator.evaluate_zone_switch(&dsd_track), Some(dsd_id));
    assert_eq!(
        coordinator.evaluate_zone_switch(&surround_track),
        Some(avr_id)
    );
    assert_eq!(coordinator.evaluate_zone_switch(&ordinary_track), None);
}

#[test]
fn protected_default_zone_cannot_be_deleted() {
    let main_cfg = ZoneConfig::main_default();
    let main_id = main_cfg.id;
    let mut coordinator = ZoneCoordinator::new(main_cfg);

    let custom_cfg = ZoneConfig::new_custom("Temp Zone", None);
    let custom_id = custom_cfg.id;
    coordinator.add_zone(custom_cfg);

    // Default zone cannot be removed
    assert!(!coordinator.remove_zone(main_id));
    assert!(coordinator.get_zone(main_id).is_some());

    // Custom zone can be removed
    assert!(coordinator.remove_zone(custom_id));
    assert!(coordinator.get_zone(custom_id).is_none());
}

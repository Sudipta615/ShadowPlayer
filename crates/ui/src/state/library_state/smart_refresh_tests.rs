//! Smart-playlist refresh-policy tests.

use super::*;
use database::{Database, TrackRepo};
use domain::ids::{PlaylistId, TrackId};
use domain::smart::{SmartField, SmartOp, SmartRule};
use std::path::PathBuf;

fn track(title: &str) -> Track {
    Track {
        id: TrackId::new(),
        // The path is the track's identity in the database, so two
        // tracks need two paths.
        path: PathBuf::from(format!("/music/{title}.flac")),
        title: title.to_string(),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        genre: None,
        year: None,
        track_number: Some(1),
        disc_number: Some(1),
        duration_secs: 100.0,
        bpm: None,
        sample_rate: 44100,
        channels: 2,
        format: "FLAC".to_string(),
        bitrate_kbps: None,
        bit_depth: None,
        is_lossless: true,
        replaygain_track_db: None,
        replaygain_track_peak: None,
        replaygain_album_db: None,
        replaygain_album_peak: None,
        r128_loudness_lufs: None,
        file_size_bytes: 0,
        mtime_unix_secs: 0,
        mtime_nanos: 0,
        content_hash: None,
        file_id: None,
        is_favorite: false,
        rating: 0,
        has_lyrics: false,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        added_at: 0,
    }
}

/// A smart playlist that matches every track whose title contains "Keep".
fn definition() -> SmartPlaylist {
    SmartPlaylist {
        id: PlaylistId::new(),
        name: "Keep".to_string(),
        match_all: true,
        rules: vec![SmartRule {
            field: SmartField::Title,
            op: SmartOp::Contains,
            value: "Keep".to_string(),
        }],
        limit_count: None,
        sort_by: None,
        sort_desc: false,
        created_at: 0,
        updated_at: 0,
        rules_unreadable: false,
    }
}

fn library_with(titles: &[&str]) -> (Database, SmartPlaylistRepo) {
    let db = Database::in_memory().expect("in-memory database");
    let tracks = TrackRepo::new(db.clone());
    for title in titles {
        tracks
            .upsert(&track(title))
            .expect("the track should be indexed");
    }
    let repo = SmartPlaylistRepo::new(db.clone());
    (db, repo)
}

#[test]
fn the_default_policy_re_evaluates_when_the_library_changes() {
    let (db, repo) = library_with(&["Keep me"]);
    let mut state = LibraryState::default();
    assert_eq!(
        state.smart_refresh.policy,
        SmartRefreshPolicy::LibraryChange
    );

    let sp = definition();
    assert_eq!(state.smart_playlist_tracks(&repo, &sp, 100).len(), 1);

    // A new track arrives, and the library changes.
    TrackRepo::new(db.clone())
        .upsert(&track("Keep me too"))
        .expect("the track should be indexed");
    state.note_library_change();

    assert_eq!(
        state.smart_playlist_tracks(&repo, &sp, 200).len(),
        2,
        "the library-change policy re-runs the query after a change"
    );
}

#[test]
fn the_default_policy_serves_the_cached_result_otherwise() {
    let (_db, repo) = library_with(&["Keep me"]);
    let mut state = LibraryState::default();
    let sp = definition();

    assert_eq!(state.smart_playlist_tracks(&repo, &sp, 100).len(), 1);
    assert_eq!(
        state.smart_playlist_evaluated_at(&sp.id),
        Some(100),
        "the time of the evaluation is remembered"
    );
    assert_eq!(
        state.smart_playlist_tracks(&repo, &sp, 200).len(),
        1,
        "without a library change the cached result is served"
    );
    assert_eq!(state.smart_playlist_evaluated_at(&sp.id), Some(100));
}

#[test]
fn the_manual_policy_keeps_the_result_until_it_is_asked_for() {
    let (db, repo) = library_with(&["Keep me"]);
    let mut state = LibraryState::default();
    state.smart_refresh.policy = SmartRefreshPolicy::Manual;
    let sp = definition();

    assert_eq!(state.smart_playlist_tracks(&repo, &sp, 100).len(), 1);

    TrackRepo::new(db.clone())
        .upsert(&track("Keep me too"))
        .expect("the track should be indexed");
    state.note_library_change();

    assert_eq!(
        state.smart_playlist_tracks(&repo, &sp, 200).len(),
        1,
        "the manual policy freezes the result across a library change"
    );

    state.request_smart_refresh();
    assert_eq!(
        state.smart_playlist_tracks(&repo, &sp, 300).len(),
        2,
        "the manual refresh re-runs the query"
    );
}

#[test]
fn the_timer_policy_re_evaluates_when_the_interval_has_passed() {
    let (_db, repo) = library_with(&["Keep me"]);
    let mut state = LibraryState::default();
    state.smart_refresh.policy = SmartRefreshPolicy::Interval;
    state.smart_refresh.interval_minutes = 1;
    let sp = definition();

    assert_eq!(state.smart_playlist_tracks(&repo, &sp, 100).len(), 1);
    assert_eq!(
        state.smart_playlist_evaluated_at(&sp.id),
        Some(100),
        "the time of the evaluation is remembered"
    );
    assert_eq!(
        state.smart_playlist_tracks(&repo, &sp, 130).len(),
        1,
        "the interval has not passed, so the cached result is served"
    );
    assert_eq!(state.smart_playlist_evaluated_at(&sp.id), Some(100));

    // A minute later the timer is due.
    assert_eq!(state.smart_playlist_tracks(&repo, &sp, 161).len(), 1);
    assert_eq!(
        state.smart_playlist_evaluated_at(&sp.id),
        Some(161),
        "the timer re-ran the query"
    );
}

#[test]
fn the_interval_is_clamped_to_something_useful() {
    let settings = SmartRefreshSettings {
        policy: SmartRefreshPolicy::Interval,
        interval_minutes: 0,
    }
    .clamped();
    assert_eq!(settings.interval_minutes, 1);
    assert_eq!(settings.interval_secs(), 60);
}

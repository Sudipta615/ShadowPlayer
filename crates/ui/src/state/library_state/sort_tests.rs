//! Track-ordering and quick-filter tests.

use super::sort::apply_quick_filter;
use super::*;
use domain::ids::TrackId;
use std::path::PathBuf;

fn track(title: &str, artist: &str, album: &str, favorite: bool, duration: f32) -> Track {
    Track {
        id: TrackId::new(),
        path: PathBuf::from("/music/x.flac"),
        title: title.to_string(),
        artist: artist.to_string(),
        album: album.to_string(),
        genre: None,
        year: None,
        track_number: Some(1),
        disc_number: Some(1),
        duration_secs: duration,
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
        is_favorite: favorite,
        rating: 0,
        has_lyrics: false,
        play_count: 0,
        skip_count: 0,
        last_played_at: None,
        added_at: 0,
    }
}

fn titles(tracks: &[Track]) -> Vec<&str> {
    tracks.iter().map(|t| t.title.as_str()).collect()
}

#[test]
fn sorting_is_case_insensitive_and_direction_aware() {
    let mut tracks = vec![
        track("banana", "Zed", "Beta", false, 3.0),
        track("Apple", "alpha", "alpha", false, 1.0),
        track("cherry", "Beta", "Gamma", false, 2.0),
    ];

    sort_tracks(&mut tracks, SortField::Title, SortOrder::Ascending);
    assert_eq!(titles(&tracks), ["Apple", "banana", "cherry"]);

    sort_tracks(&mut tracks, SortField::Title, SortOrder::Descending);
    assert_eq!(titles(&tracks), ["cherry", "banana", "Apple"]);

    sort_tracks(&mut tracks, SortField::Artist, SortOrder::Ascending);
    assert_eq!(titles(&tracks), ["Apple", "cherry", "banana"]);

    // Album used to compare case-sensitively, which put `Beta` before
    // `alpha` even though `alpha` comes first alphabetically.
    sort_tracks(&mut tracks, SortField::Album, SortOrder::Ascending);
    assert_eq!(titles(&tracks), ["Apple", "banana", "cherry"]);

    sort_tracks(&mut tracks, SortField::Duration, SortOrder::Ascending);
    assert_eq!(titles(&tracks), ["Apple", "cherry", "banana"]);
}

#[test]
fn favorites_lead_in_both_directions() {
    let mut tracks = vec![
        track("one", "A", "A", false, 1.0),
        track("two", "A", "A", true, 1.0),
        track("three", "A", "A", false, 1.0),
    ];
    sort_tracks(&mut tracks, SortField::Favorite, SortOrder::Ascending);
    assert_eq!(tracks[0].title, "two");
    sort_tracks(&mut tracks, SortField::Favorite, SortOrder::Descending);
    assert_eq!(tracks[0].title, "two", "a favorite is never last");
}

#[test]
fn rating_sort_puts_five_stars_first_when_descending() {
    let mut tracks = vec![
        track("one", "A", "A", false, 1.0),
        track("two", "A", "A", false, 1.0),
        track("three", "A", "A", false, 1.0),
    ];
    tracks[0].rating = 2;
    tracks[1].rating = 5;
    // tracks[2] is deliberately unrated.
    sort_tracks(&mut tracks, SortField::Rating, SortOrder::Descending);
    assert_eq!(titles(&tracks), ["two", "one", "three"]);
    sort_tracks(&mut tracks, SortField::Rating, SortOrder::Ascending);
    assert_eq!(titles(&tracks), ["three", "one", "two"]);
}

#[test]
fn the_rating_chip_hides_unrated_tracks() {
    let mut tracks = vec![
        track("one", "A", "A", false, 1.0),
        track("two", "A", "A", false, 1.0),
        track("three", "A", "A", false, 1.0),
    ];
    tracks[1].rating = 3;
    let favorites = HashSet::new();
    apply_quick_filter(&mut tracks, &favorites, &[], QuickFilter::Rating);
    assert_eq!(titles(&tracks), ["two"]);
    assert!(tracks[0].rating > 0);
}

#[test]
fn for_you_keeps_the_engine_ranking_and_hides_other_tracks() {
    let mut state = LibraryState::default();
    let one = track("one", "A", "A", false, 1.0);
    let two = track("two", "A", "A", false, 2.0);
    let three = track("three", "A", "A", false, 3.0);
    let (one_id, two_id, three_id) = (one.id, two.id, three.id);
    state.all_tracks = vec![one, two, three];

    state.set_recommendation_ids(vec![three_id, one_id]);
    assert_eq!(titles(&state.recommended_tracks()), ["three", "one"]);

    // A played track drops out; the rest keeps its order.
    state.set_recommendation_ids(vec![one_id, three_id]);
    assert_eq!(titles(&state.recommended_tracks()), ["one", "three"]);

    state.clear_recommendation_ids();
    assert!(state.recommended_tracks().is_empty());

    // The empty filter must not fall through to the whole library.
    let mut tracks = state.all_tracks.clone();
    let favorites = HashSet::new();
    apply_quick_filter(&mut tracks, &favorites, &[], QuickFilter::ForYou);
    assert!(tracks.is_empty());

    let mut tracks = state.all_tracks.clone();
    apply_quick_filter(&mut tracks, &favorites, &[two_id], QuickFilter::ForYou);
    assert_eq!(titles(&tracks), ["two"]);
}

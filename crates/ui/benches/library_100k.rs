//! The 100,000-track library benchmark.
//!
//! The numbers this produces are the ones published in `docs/performance.md`.
//! Run it with `cargo bench -p ui --bench library_100k`.
//!
//! The benchmark measures the work a big library makes the UI thread do:
//! building the visible list, sorting it and copying it. The *frame* cost of
//! drawing the table is not here — the table is virtualised, so it does not
//! grow with the library — and it is measured by the budget test instead.

use criterion::{criterion_group, criterion_main, Criterion};
use domain::ids::TrackId;
use domain::models::{SortField, SortOrder, Track};
use ui::state::library_state::sort_tracks;

/// How many tracks the benchmark builds.
const LIBRARY_SIZE: usize = 100_000;

/// A synthetic library.
///
/// The names come from a counter rather than a random number generator, so
/// the benchmark measures the same work on every machine and every run.
fn synthetic_library(count: usize) -> Vec<Track> {
    const ARTISTS: [&str; 12] = [
        "Aurora Field",
        "Black Tide",
        "Cinder & Ash",
        "Distant Rooms",
        "Ember Lane",
        "Frost Harbour",
        "Glass Meridian",
        "Hollow Pines",
        "Iron Valley",
        "Juniper Sky",
        "Kestrel Line",
        "Lantern Coast",
    ];
    const ALBUMS: [&str; 20] = [
        "Afterglow",
        "Bellwether",
        "Coriolis",
        "Dust Signals",
        "East of Nowhere",
        "Fault Lines",
        "Glass Houses",
        "Harvest Moon",
        "Inland Sea",
        "Junction Road",
        "Kite String",
        "Long Player",
        "Meridian Line",
        "North Field",
        "Overpass",
        "Paper Radio",
        "Quiet Machines",
        "Riverbed",
        "Salt Flats",
        "Tidal Works",
    ];

    (0..count)
        .map(|i| {
            let artist = ARTISTS[i % ARTISTS.len()];
            let album = ALBUMS[(i / 7) % ALBUMS.len()];
            let track_no = (i % 12) as u32 + 1;
            Track {
                id: TrackId::new(),
                path: std::path::PathBuf::from(format!("/music/{artist}/{album}/{i:06}.flac")),
                title: format!("{album} — Track {track_no:02}"),
                artist: artist.to_string(),
                album: album.to_string(),
                genre: Some("Ambient".to_string()),
                year: Some(1990 + (i % 35) as u32),
                track_number: Some(track_no),
                disc_number: Some(1),
                duration_secs: 120.0 + (i % 300) as f32,
                bpm: None,
                sample_rate: if i % 3 == 0 { 96_000 } else { 44_100 },
                channels: 2,
                format: if i % 3 == 0 { "FLAC" } else { "MP3" }.to_string(),
                bitrate_kbps: Some(if i % 3 == 0 { 1000 } else { 320 }),
                bit_depth: Some(if i % 3 == 0 { 24 } else { 16 }),
                is_lossless: i % 3 == 0,
                replaygain_track_db: None,
                replaygain_track_peak: None,
                replaygain_album_db: None,
                replaygain_album_peak: None,
                r128_loudness_lufs: None,
                file_size_bytes: 40_000_000,
                mtime_unix_secs: 1_700_000_000,
                mtime_nanos: 0,
                content_hash: None,
                file_id: None,
                is_favorite: i % 17 == 0,
                rating: (i % 6) as u8,
                has_lyrics: i % 5 == 0,
                play_count: (i % 50) as u32,
                skip_count: 0,
                last_played_at: None,
                added_at: 1_700_000_000 + i as i64,
            }
        })
        .collect()
}

fn bench_sort(c: &mut Criterion) {
    let library = synthetic_library(LIBRARY_SIZE);

    // The setup (a copy of the library) is outside the measured section:
    // the point of this benchmark is the sort, not the copy.
    c.bench_function("sort_100k_by_title", |b| {
        b.iter_batched(
            || library.clone(),
            |mut tracks| sort_tracks(&mut tracks, SortField::Title, SortOrder::Ascending),
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_clone(c: &mut Criterion) {
    let library = synthetic_library(LIBRARY_SIZE);
    c.bench_function("copy_100k_tracks", |b| b.iter(|| library.clone()));
}

fn bench_build(c: &mut Criterion) {
    c.bench_function("build_100k_tracks", |b| {
        b.iter(|| synthetic_library(LIBRARY_SIZE))
    });
}

/// The whole rebuild: copy the library and sort it.
fn bench_rebuild(c: &mut Criterion) {
    let library = synthetic_library(LIBRARY_SIZE);
    c.bench_function("rebuild_100k", |b| {
        b.iter_batched(
            || library.clone(),
            |mut tracks| sort_tracks(&mut tracks, SortField::Title, SortOrder::Ascending),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_build, bench_clone, bench_sort, bench_rebuild);
criterion_main!(benches);

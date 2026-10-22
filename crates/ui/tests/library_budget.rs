//! The library-size budget: what a 100,000-track library costs the UI.
//!
//! A player that lists songs is judged on two things: how long it takes to
//! open a library, and whether the window keeps up while the user works in
//! it. The second one is the frame time — the work the UI thread does
//! between two frames. At 100,000 tracks the interesting work is not the
//! drawing (the table is virtualised: only the visible rows are built) but
//! the *rebuild* that happens when the view changes: the list is copied out
//! of the library and sorted.
//!
//! These numbers are budgets, not benchmarks: a machine that is slower than
//! the one the numbers were measured on must not fail the build, so every
//! budget has slack. What the budget catches is a change that makes the
//! rebuild an order of magnitude slower — a clone that became a deep copy, a
//! sort key that started allocating per comparison.
//!
//! The measured code is the production code: the same `update_displayed_tracks`
//! the app calls, with the same sorting.

use std::time::{Duration, Instant};

use database::{
    AlbumRepo, ArtistRepo, Database, GenreRepo, HistoryRepo, PlaylistRepo, SmartPlaylistRepo,
    TrackRepo,
};
use domain::ids::TrackId;
use domain::models::{SortField, SortOrder, Track};
use search::SearchEngine;
use ui::components::sidebar::NavItem;
use ui::state::library_state::LibraryState;

/// How many tracks the budgets are stated for.
const LIBRARY_SIZE: usize = 100_000;

/// The budgets, in milliseconds.
///
/// They are generous multiples of the measured times (see
/// `docs/performance.md`): the point is to catch a regression, not to
/// certify a machine.
const BUDGET_REBUILD_MS: u128 = 250;
const BUDGET_SORT_MS: u128 = 250;
const BUDGET_CLONE_MS: u128 = 250;

/// A synthetic library.
///
/// The names are generated from a counter rather than a random number
/// generator, so the library is the same on every machine and every run: a
/// measurement that changes with the seed measures the seed, not the code.
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

/// The state a rebuild runs against: the library in memory, an empty
/// database behind it.
fn state_with(library: Vec<Track>) -> (LibraryState, Database) {
    let db = Database::in_memory().expect("an in-memory database");
    let mut state = LibraryState::default();
    state.all_tracks = library;
    (state, db)
}

/// The rebuild the app performs when the visible list changes.
///
/// This is the production call: the same one the app makes with the same
/// arguments, minus the repositories that the songs view does not touch.
fn rebuild(state: &mut LibraryState, db: &Database, sort: SortField) -> Duration {
    let started = Instant::now();
    state.update_displayed_tracks(
        &SearchEngine::new(db.clone()),
        &HistoryRepo::new(db.clone()),
        &PlaylistRepo::new(db.clone()),
        &SmartPlaylistRepo::new(db.clone()),
        &AlbumRepo::new(db.clone()),
        &ArtistRepo::new(db.clone()),
        &GenreRepo::new(db.clone()),
        &TrackRepo::new(db.clone()),
        NavItem::Home,
        "",
        sort,
        SortOrder::Ascending,
    );
    started.elapsed()
}

fn ms(duration: Duration) -> u128 {
    duration.as_millis()
}

/// A 100,000-track library rebuilds inside the budget.
#[test]
fn a_hundred_thousand_track_library_rebuilds_inside_the_budget() {
    let library = synthetic_library(LIBRARY_SIZE);
    let (mut state, db) = state_with(library);

    // The first rebuild sorts the library by title.
    let first = rebuild(&mut state, &db, SortField::Title);
    assert_eq!(
        state.displayed_tracks.len(),
        LIBRARY_SIZE,
        "every track is listed"
    );

    // The second rebuild is the same work again — the user changed the sort
    // back and forth, or opened another tab and came back.
    let second = rebuild(&mut state, &db, SortField::Title);

    println!(
        "library of {LIBRARY_SIZE}: first rebuild {} ms, second rebuild {} ms",
        ms(first),
        ms(second)
    );

    assert!(
        ms(first) < BUDGET_REBUILD_MS,
        "the first rebuild of a {LIBRARY_SIZE}-track library took {} ms, \
         which is over the {BUDGET_REBUILD_MS} ms budget",
        ms(first)
    );
    assert!(
        ms(second) < BUDGET_REBUILD_MS,
        "the rebuild of a {LIBRARY_SIZE}-track library took {} ms, \
         which is over the {BUDGET_REBUILD_MS} ms budget",
        ms(second)
    );
}

/// Sorting a 100,000-track library stays inside the budget.
///
/// The sort is the part of the rebuild that scales with the *comparisons*,
/// so it is measured on its own: a sort that started allocating a key per
/// comparison would still pass a rebuild budget on a small library and fail
/// here.
#[test]
fn sorting_a_hundred_thousand_tracks_stays_inside_the_budget() {
    let mut tracks = synthetic_library(LIBRARY_SIZE);

    let started = Instant::now();
    ui::state::library_state::sort_tracks(&mut tracks, SortField::Title, SortOrder::Ascending);
    let elapsed = started.elapsed();
    println!("sort by title: {} ms", ms(elapsed));

    assert!(
        ms(elapsed) < BUDGET_SORT_MS,
        "sorting {LIBRARY_SIZE} tracks took {} ms, which is over the \
         {BUDGET_SORT_MS} ms budget",
        ms(elapsed)
    );

    // The sort must actually sort.
    assert!(
        tracks
            .windows(2)
            .all(|w| { w[0].title.to_lowercase() <= w[1].title.to_lowercase() }),
        "the library is sorted by title"
    );
}

/// Copying the library — the other half of the rebuild — stays inside the
/// budget.
#[test]
fn copying_a_hundred_thousand_tracks_stays_inside_the_budget() {
    let library = synthetic_library(LIBRARY_SIZE);

    let started = Instant::now();
    let copy = library.clone();
    let elapsed = started.elapsed();
    println!("clone: {} ms", ms(elapsed));
    assert_eq!(copy.len(), LIBRARY_SIZE);

    assert!(
        ms(elapsed) < BUDGET_CLONE_MS,
        "copying {LIBRARY_SIZE} tracks took {} ms, which is over the \
         {BUDGET_CLONE_MS} ms budget",
        ms(elapsed)
    );
}

/// The rebuild is skipped when nothing that affects the list changed.
///
/// This is what keeps a 100,000-track library from being copied on every
/// frame: the state remembers what it last built.
#[test]
fn an_unchanged_view_is_not_rebuilt() {
    let library = synthetic_library(LIBRARY_SIZE);
    let (mut state, db) = state_with(library);

    let first = rebuild(&mut state, &db, SortField::Title);
    let second = rebuild(&mut state, &db, SortField::Title);

    assert!(
        second < first,
        "an unchanged view is not rebuilt again ({} ms against {} ms)",
        ms(second),
        ms(first)
    );
}

/// Changing the sort re-orders the list that is already there.
///
/// The sort is the interaction a user makes most often on a big library, and
/// the list it re-orders is the same list: copying the library again would
/// cost a hundred thousand string allocations for nothing. The counter is
/// what proves it — a rebuild that came back would show up here.
#[test]
fn changing_the_sort_does_not_rebuild_the_list() {
    let library = synthetic_library(LIBRARY_SIZE);
    let (mut state, db) = state_with(library);

    rebuild(&mut state, &db, SortField::Title);
    let after_the_first = state.rebuilds;
    assert_eq!(after_the_first, 1, "the list was built once");

    rebuild(&mut state, &db, SortField::Artist);
    assert_eq!(
        state.rebuilds, after_the_first,
        "a sort change re-orders the list instead of rebuilding it"
    );

    // The order really did change.
    let first = &state.displayed_tracks[0];
    let second = &state.displayed_tracks[1];
    assert!(
        first.artist.to_lowercase() <= second.artist.to_lowercase(),
        "the list is sorted by artist"
    );
}

/// The songs table renders a 100,000-track library inside a frame.
///
/// The table is virtualised — only the rows on screen are built — so the
/// size of the library must not change the cost of a frame. This is the
/// measurement that says so.
#[test]
fn the_table_renders_a_hundred_thousand_tracks_inside_a_frame() {
    use std::collections::HashSet;

    use eframe::egui;
    use ui::components::songs_table::{ColumnWidths, RowDensity, SongsTable};
    use ui::services::artwork_service::ArtworkService;
    use ui::theme::Theme;

    let library = synthetic_library(LIBRARY_SIZE);
    let theme = Theme::dark();
    let mut artwork = ArtworkService::new();
    let mut selected: HashSet<TrackId> = HashSet::new();
    let mut last_clicked: Option<usize> = None;
    let favorites: HashSet<TrackId> = HashSet::new();
    let playlists: Vec<domain::models::Playlist> = Vec::new();

    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 800.0),
        )),
        ..Default::default()
    };

    // The first frame builds the layout; the following ones are the steady
    // state the user actually sees while scrolling.
    let mut frames = Vec::new();
    for frame in 0..5 {
        let started = Instant::now();
        let _ = context.run_ui(input.clone(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let table = SongsTable {
                    tracks: &library,
                    active_track_id: None,
                    is_playing: false,
                    favorite_track_ids: &favorites,
                    playlists: &playlists,
                    current_sort: SortField::Title,
                    sort_order: SortOrder::Ascending,
                    is_grid_view: false,
                    quick_filter: ui::components::songs_table::QuickFilter::All,
                    column_widths: ColumnWidths::default(),
                    rating_width: 64.0,
                    density: RowDensity::default(),
                    selected_track_ids: &mut selected,
                    last_clicked_index: &mut last_clicked,
                    artwork: &mut artwork,
                    theme: &theme,
                    show_header: true,
                    search_query: "",
                    library_track_count: LIBRARY_SIZE,
                };
                let _ = table.show(ui);
            });
        });
        if frame > 0 {
            frames.push(started.elapsed());
        }
    }

    let worst = frames.iter().max().copied().unwrap_or_default();
    let mean = frames.iter().sum::<Duration>() / frames.len() as u32;
    println!(
        "frame with {LIBRARY_SIZE} tracks: mean {} us, worst {} us",
        mean.as_micros(),
        worst.as_micros()
    );

    assert!(
        mean < std::time::Duration::from_millis(16),
        "a frame of the songs table took {} ms on average, which is over \
         the 16 ms budget for a 60 fps frame",
        ms(mean)
    );
}

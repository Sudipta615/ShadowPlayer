//! CUE sheets: the album-level index and the virtual track paths.
//!
//! A CUE sheet describes one continuous audio file divided into named tracks
//! by `INDEX` timestamps. Two things are checked here:
//!
//! * **The album-level index** — the sheet's tracks, their start times and
//!   their durations (the last track's duration coming from the file's
//!   length, because a CUE sheet has no end marker for it).
//! * **The virtual track paths** — the paths the expansion resolves for each
//!   virtual track: relative to the sheet's own directory, with the Windows
//!   backslashes a ripper writes normalised away.
//!
//! The CUE timestamp format is `MM:SS:FF`, where `FF` is a **CD frame** of
//! 1/75 s — not milliseconds and not a third field to ignore.

mod common;

use engine::decode::cue::{CueIndex, CueParseError, CueSheet};
use engine::engine::cue_split::{expand_cue_sheet, find_sibling_cue, PregapPolicy};

/// Sample rate for the frame arithmetic.
const SR: u32 = 48_000;

/// A three-track sheet over 40 s of audio, with a 1 s pregap on track 2.
///
/// The timestamps are written through `cd()` so the intent is in one place:
/// `cd(10)` is ten *seconds*, not ten CD frames.
fn cd(seconds: u32) -> String {
    format!("00:{:02}:00", seconds)
}

fn sheet_text() -> String {
    format!(
        r#"
PERFORMER "The Band"
TITLE "The Album"
REM GENRE "Progressive Rock"
REM DATE 1973
FILE "album.wav" WAVE
  TRACK 01 AUDIO
    TITLE "First"
    PERFORMER "The Band"
    INDEX 00 00:00:00
    INDEX 01 {t0}
  TRACK 02 AUDIO
    TITLE "Second"
    INDEX 00 {t1}
    INDEX 01 {t2}
  TRACK 03 AUDIO
    TITLE "Third"
    INDEX 01 {t3}
"#,
        t0 = cd(10),
        t1 = cd(19),
        t2 = cd(20),
        t3 = cd(30),
    )
}

/// Write a 40-second silent WAV and the sheet beside it, and return the
/// directory they live in.
fn fixture(dir: &common::TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let audio = dir.path().join("album.wav");
    let sheet_path = dir.path().join("album.cue");
    common::write_wav_to(&audio, &vec![0.0f32; 40 * SR as usize * 2], 2, SR);
    std::fs::write(&sheet_path, sheet_text()).expect("the sheet must be writable");
    (audio, sheet_path)
}

#[test]
fn a_sheet_names_its_files_and_tracks() {
    let sheet = CueSheet::parse(&sheet_text()).expect("the sheet must parse");
    assert_eq!(sheet.files, vec!["album.wav".to_string()]);
    assert_eq!(sheet.track_count(), 3);
    assert_eq!(sheet.title.as_deref(), Some("The Album"));
    assert_eq!(sheet.performer.as_deref(), Some("The Band"));
    assert_eq!(sheet.genre.as_deref(), Some("Progressive Rock"));
    assert_eq!(sheet.date.as_deref(), Some("1973"));

    let track = sheet.get_track(2).expect("track 2 must exist");
    assert_eq!(track.number, 2);
    assert_eq!(track.title.as_deref(), Some("Second"));
    assert_eq!(track.track_type, "AUDIO");
    assert_eq!(
        track.file.as_deref(),
        Some("album.wav"),
        "a track must inherit the file that was current when it appeared"
    );
    assert!(
        sheet.get_track(9).is_none(),
        "a sheet must not invent tracks"
    );
}

#[test]
fn the_timestamps_are_cd_frames_not_milliseconds() {
    // The third field is a CD frame of 1/75 s. `00:00:75` is one second
    // because 75 frames make a second; `00:01:00` is also one second,
    // because the middle field is seconds and the last one is zero frames.
    let index = CueIndex::parse(1, "00:00:75").expect("a valid timestamp");
    assert!((index.total_seconds() - 1.0).abs() < 1e-9);
    assert_eq!(index.total_samples(75), 75);

    let index = CueIndex::parse(1, "00:01:00").expect("a valid timestamp");
    assert!(
        (index.total_seconds() - 1.0).abs() < 1e-9,
        "one second and no frames is one second"
    );

    // A quarter of a second is 18.75 CD frames, so 00:00:19 is a little
    // over a quarter of a second — never 19 milliseconds.
    let index = CueIndex::parse(1, "00:00:19").expect("a valid timestamp");
    assert!(
        (index.total_seconds() - 19.0 / 75.0).abs() < 1e-9,
        "19 CD frames is 19/75 s, not 19 ms"
    );

    let index = CueIndex::parse(1, "01:00:00").expect("a valid timestamp");
    assert!((index.total_seconds() - 60.0).abs() < 1e-9);
    assert_eq!(index.total_samples(48_000), 60 * 48_000);
}

#[test]
fn the_album_level_index_gives_every_track_a_start_and_a_duration() {
    let sheet = CueSheet::parse(&sheet_text()).expect("the sheet must parse");
    let durations = sheet.calculate_track_durations(Some(40.0));

    assert_eq!(durations.len(), 3);
    // The first two tracks are bounded by the next track's start.
    assert_eq!(durations[0].0, 1);
    assert!(
        (durations[0].1 - 10.0).abs() < 1e-6,
        "track 1 starts at 10 s"
    );
    assert!(
        (durations[0].2.expect("track 1 has a duration") - 10.0).abs() < 1e-6,
        "track 1 runs from 10 s to track 2's start at 20 s"
    );

    assert_eq!(durations[1].0, 2);
    assert!(
        (durations[1].1 - 20.0).abs() < 1e-6,
        "track 2 starts at 20 s"
    );
    assert!(
        (durations[1].2.expect("track 2 has a duration") - 10.0).abs() < 1e-6,
        "track 2 runs from 20 s to track 3's start at 30 s"
    );

    // The last track has no following index, so the file's length bounds it.
    assert_eq!(durations[2].0, 3);
    assert!((durations[2].1 - 30.0).abs() < 1e-6);
    assert!(
        (durations[2].2.expect("track 3 has a duration") - 10.0).abs() < 1e-6,
        "track 3 runs from 30 s to the end of the 40 s file"
    );
}

#[test]
fn the_last_track_has_no_duration_without_a_file_length() {
    // Without the decoded length the last track cannot be bounded. Reporting
    // "no duration" is honest; guessing would not be.
    let sheet = CueSheet::parse(&sheet_text()).expect("the sheet must parse");
    let durations = sheet.calculate_track_durations(None);
    assert_eq!(durations.len(), 3);
    assert!(
        durations[0].2.is_some(),
        "the bounded tracks keep their duration"
    );
    assert!(
        durations[2].2.is_none(),
        "the unbounded last track must report no duration"
    );
}

#[test]
fn a_track_reports_its_pregap_separately_from_its_start() {
    let sheet = CueSheet::parse(&sheet_text()).expect("the sheet must parse");
    let track = sheet.get_track(2).expect("track 2 must exist");
    assert!(
        (track.pregap_start_seconds().expect("track 2 has a pregap") - 19.0).abs() < 1e-6,
        "the pregap starts at 19 s"
    );
    assert!(
        (track.start_time_seconds() - 20.0).abs() < 1e-6,
        "the track itself starts at 20 s"
    );
    assert_eq!(track.start_sample(SR), 20 * SR as u64);

    let first = sheet.get_track(1).expect("track 1 must exist");
    assert!(
        (first.pregap_start_seconds().expect("track 1 has a pregap") - 0.0).abs() < 1e-6,
        "track 1's pregap starts at 0 s"
    );
}

#[test]
fn the_virtual_tracks_resolve_to_the_file_beside_the_sheet() {
    let dir = common::temp_dir("cue_paths");
    let (audio, sheet_path) = fixture(&dir);
    let sheet = CueSheet::parse_file(&sheet_path).expect("the sheet file must parse");

    let segments = expand_cue_sheet(
        &sheet,
        &sheet_path,
        Some(40.0),
        SR,
        PregapPolicy::IncludeInTrack,
    );

    assert_eq!(segments.len(), 3, "one virtual track per sheet track");
    for segment in &segments {
        assert_eq!(
            segment.path, audio,
            "every virtual track must resolve to the file beside the sheet"
        );
    }
    assert_eq!(segments[0].title.as_deref(), Some("First"));
    assert_eq!(segments[1].title.as_deref(), Some("Second"));
    assert_eq!(segments[2].title.as_deref(), Some("Third"));
    assert_eq!(segments[0].number, 1);
    assert_eq!(segments[2].number, 3);
    assert_eq!(segments[0].performer.as_deref(), Some("The Band"));
}

#[test]
fn the_virtual_track_boundaries_are_exact_and_contiguous() {
    let dir = common::temp_dir("cue_bounds");
    let (_audio, sheet_path) = fixture(&dir);
    let sheet = CueSheet::parse_file(&sheet_path).expect("the sheet file must parse");

    let segments = expand_cue_sheet(
        &sheet,
        &sheet_path,
        Some(40.0),
        SR,
        PregapPolicy::IncludeInTrack,
    );

    // Gapless matters: both tracks come from one file, so a discontinuity at
    // the boundary would be an audible click.
    assert_eq!(
        segments[0].start_frame + segments[0].frame_count,
        segments[1].start_frame,
        "the virtual tracks must be contiguous"
    );
    assert_eq!(
        segments[1].start_frame + segments[1].frame_count,
        segments[2].start_frame
    );
    // And the last one ends exactly at the end of the file.
    let last = segments.last().expect("there must be a last track");
    assert_eq!(
        last.start_frame + last.frame_count,
        40 * SR as u64,
        "the final virtual track must end at the end of the file"
    );
}

#[test]
fn the_pregap_policy_decides_where_a_track_starts() {
    let dir = common::temp_dir("cue_pregap");
    let (_audio, sheet_path) = fixture(&dir);
    let sheet = CueSheet::parse_file(&sheet_path).expect("the sheet file must parse");

    // The default: a track's INDEX 00 is its own pregap, so the track starts
    // there. Playing it as part of the previous track is the classic CUE
    // player bug.
    let included = expand_cue_sheet(
        &sheet,
        &sheet_path,
        Some(40.0),
        SR,
        PregapPolicy::IncludeInTrack,
    );
    assert_eq!(included[1].start_frame, 19 * SR as u64);

    let previous = expand_cue_sheet(
        &sheet,
        &sheet_path,
        Some(40.0),
        SR,
        PregapPolicy::AssignToPrevious,
    );
    assert_eq!(previous[1].start_frame, 20 * SR as u64);
    assert_eq!(
        previous[1].start_frame - included[1].start_frame,
        SR as u64,
        "the pregap is one second and the policy must account for all of it"
    );
}

#[test]
fn a_windows_backslash_path_resolves_to_the_real_file() {
    let dir = common::temp_dir("cue_backslash");
    std::fs::create_dir_all(dir.path().join("sub")).expect("the sub-directory must be creatable");
    let audio = dir.path().join("sub").join("album.wav");
    common::write_wav_to(&audio, &vec![0.0f32; 20 * SR as usize * 2], 2, SR);
    let sheet_path = dir.path().join("album.cue");
    let body = r#"
FILE "sub\album.wav" WAVE
  TRACK 01 AUDIO
    TITLE "A"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "B"
    INDEX 01 00:00:10
"#;
    std::fs::write(&sheet_path, body).expect("the sheet must be writable");
    let sheet = CueSheet::parse(body).expect("the sheet must parse");

    let segments = expand_cue_sheet(&sheet, &sheet_path, Some(20.0), SR, PregapPolicy::default());
    assert_eq!(segments.len(), 2, "the backslash path must resolve");
    assert_eq!(segments[0].path, audio);
}

#[test]
fn a_missing_audio_file_yields_no_virtual_track_rather_than_an_error() {
    // The caller is expanding a queue, not validating an installation.
    let dir = common::temp_dir("cue_missing");
    let sheet_path = dir.path().join("ghost.cue");
    std::fs::write(&sheet_path, sheet_text()).expect("the sheet must be writable");
    let sheet = CueSheet::parse_file(&sheet_path).expect("the sheet file must parse");

    let segments = expand_cue_sheet(&sheet, &sheet_path, Some(40.0), SR, PregapPolicy::default());
    assert!(
        segments.is_empty(),
        "a sheet whose audio is missing has nothing to play"
    );
}

#[test]
fn a_sibling_cue_is_found_by_its_stem_and_nothing_else() {
    let dir = common::temp_dir("cue_sibling");
    let audio = dir.path().join("album.wav");
    common::write_wav_to(&audio, &vec![0.0f32; SR as usize * 2], 2, SR);

    // A cue with a different stem is a different album and must not be
    // adopted.
    std::fs::write(dir.path().join("other.cue"), sheet_text()).expect("write");
    assert!(
        find_sibling_cue(&audio).is_none(),
        "a cue that does not share the audio's name must not be adopted"
    );

    std::fs::write(dir.path().join("album.CUE"), sheet_text()).expect("write");
    let found = find_sibling_cue(&audio).expect("a sibling cue must be found");
    assert_eq!(
        found
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .to_lowercase(),
        "album.cue"
    );
}

#[test]
fn a_broken_sheet_is_refused_with_a_typed_error() {
    // An empty sheet.
    assert_eq!(CueSheet::parse("").unwrap_err(), CueParseError::Empty);
    assert_eq!(
        CueSheet::parse("REM COMMENT \"nothing here\"").unwrap_err(),
        CueParseError::Empty
    );

    // An index outside a track.
    let error = CueSheet::parse("FILE \"a.wav\" WAVE\nINDEX 01 00:00:00").unwrap_err();
    assert!(
        matches!(error, CueParseError::SyntaxError { .. }),
        "an INDEX outside a TRACK must be a syntax error, got {error:?}"
    );

    // A malformed timestamp.
    let error =
        CueSheet::parse("FILE \"a.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 not-a-time").unwrap_err();
    assert!(
        matches!(error, CueParseError::InvalidTime(_)),
        "a malformed timestamp must be an InvalidTime error, got {error:?}"
    );
}

#[test]
fn a_sheet_with_crlf_line_endings_parses_the_same_way() {
    let crlf = sheet_text().replace('\n', "\r\n");
    let sheet = CueSheet::parse(&crlf).expect("a CRLF sheet must parse");
    assert_eq!(sheet.track_count(), 3);
    assert_eq!(sheet.title.as_deref(), Some("The Album"));
}

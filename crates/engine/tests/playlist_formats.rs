//! Playlist serialisation: M3U, PLS and XSPF must round trip.
//!
//! The engine supports three playlist formats because that is what exists in
//! the wild. This suite checks that each of them can be written and read
//! back, that the paths survive the trip, and that the two things a
//! playlist file is *for* — relative paths and honest refusals — behave as
//! documented.

mod common;

use std::path::PathBuf;

use engine::playlist::{Playlist, PlaylistFormat, PlaylistIoError, RepeatMode};
use engine::source::AudioSource;

/// The formats the engine claims to support.
const FORMATS: [PlaylistFormat; 3] = [
    PlaylistFormat::M3u,
    PlaylistFormat::Pls,
    PlaylistFormat::Xspf,
];

/// A queue of three file-backed entries.
fn playlist_with_three_tracks() -> Playlist {
    let mut playlist = Playlist::new();
    for name in ["one.flac", "two.flac", "three.flac"] {
        playlist.enqueue(AudioSource::File(PathBuf::from("/music/album").join(name)));
    }
    playlist
}

/// The paths a parsed playlist holds, as strings.
fn parsed_paths(parsed: &engine::playlist::ParsedPlaylist) -> Vec<String> {
    parsed
        .entries
        .iter()
        .map(|source| match source {
            AudioSource::File(path) => path.to_string_lossy().replace('\\', "/"),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn every_supported_format_round_trips_through_a_file() {
    let dir = common::temp_dir("playlist");
    for format in FORMATS {
        let playlist = playlist_with_three_tracks();
        let path = dir.path().join(format!("playlist.{}", format.extension()));
        format
            .write_to(&path, &playlist)
            .unwrap_or_else(|e| panic!("{format:?} must be writable: {e}"));

        let parsed = format
            .read_from(&path)
            .unwrap_or_else(|e| panic!("{format:?} must be readable: {e}"));
        assert_eq!(
            parsed_paths(&parsed),
            parsed_paths(&engine::playlist::ParsedPlaylist {
                entries: playlist.items().to_vec(),
                metadata: Vec::new(),
                repeat: None,
            }),
            "{format:?} must round trip the queue's paths"
        );
        assert_eq!(parsed.entries.len(), 3, "{format:?} must keep every entry");
    }
}

#[test]
fn a_relative_entry_resolves_against_the_playlists_own_directory() {
    // The whole point of the format: a user moves the folder containing the
    // playlist and its tracks together, and the playlist still works.
    let dir = common::temp_dir("playlist_relative");
    let path = dir.path().join("queue.m3u");
    std::fs::write(&path, "#EXTM3U\nmusic/one.flac\nmusic/two.flac\n")
        .expect("the playlist must be writable");

    let parsed = PlaylistFormat::M3u
        .read_from(&path)
        .expect("the playlist must be readable");
    assert_eq!(parsed.entries.len(), 2);
    for entry in &parsed.entries {
        match entry {
            AudioSource::File(resolved) => {
                assert!(
                    resolved.is_absolute(),
                    "a relative entry must resolve to an absolute path, got {resolved:?}"
                );
                assert!(
                    resolved.starts_with(dir.path()),
                    "the resolved path must sit beside the playlist, got {resolved:?}"
                );
                assert!(
                    resolved.ends_with("one.flac") || resolved.ends_with("two.flac"),
                    "the file name must survive the resolution, got {resolved:?}"
                );
            }
            other => panic!("expected a file entry, got {other:?}"),
        }
    }
}

#[test]
fn an_entry_written_below_the_output_directory_is_written_relative() {
    // The mirror image: a path under the playlist's directory is written
    // relative, so the saved file keeps working when the folder moves.
    let dir = common::temp_dir("playlist_write_relative");
    let path = dir.path().join("queue.m3u");
    let mut playlist = Playlist::new();
    playlist.enqueue(AudioSource::File(dir.path().join("music").join("one.flac")));
    playlist.enqueue(AudioSource::File(PathBuf::from("/elsewhere/two.flac")));

    PlaylistFormat::M3u
        .write_to(&path, &playlist)
        .expect("the playlist must be writable");
    let text = std::fs::read_to_string(&path).expect("the playlist must be readable");
    assert!(
        text.contains("music/one.flac") || text.contains(r"music\one.flac"),
        "a path under the output directory must be written relative, got:\n{text}"
    );
    assert!(
        text.contains("/elsewhere/two.flac") || text.contains(r"\elsewhere\two.flac"),
        "a path outside the output directory must stay absolute, got:\n{text}"
    );
}

#[test]
fn an_in_memory_entry_is_refused_rather_than_silently_dropped() {
    // A playlist that quietly loses three of its five tracks because the
    // user had buffered them is worse than one that refuses to save.
    let mut playlist = Playlist::new();
    playlist.enqueue(AudioSource::File(PathBuf::from("/music/one.flac")));
    playlist.enqueue(AudioSource::Memory {
        data: vec![0u8; 16],
        extension_hint: Some("wav".to_string()),
    });

    let rendered = PlaylistFormat::M3u.render(&playlist);
    assert!(
        matches!(rendered, Err(PlaylistIoError::NotRepresentable(_))),
        "an in-memory entry must be refused, got {rendered:?}"
    );
}

#[test]
fn the_format_is_inferred_from_the_extension() {
    assert_eq!(
        PlaylistFormat::from_extension("m3u"),
        Some(PlaylistFormat::M3u)
    );
    assert_eq!(
        PlaylistFormat::from_extension("M3U8"),
        Some(PlaylistFormat::M3u)
    );
    assert_eq!(
        PlaylistFormat::from_extension("pls"),
        Some(PlaylistFormat::Pls)
    );
    assert_eq!(
        PlaylistFormat::from_extension("xspf"),
        Some(PlaylistFormat::Xspf)
    );
    assert_eq!(PlaylistFormat::from_extension("txt"), None);

    assert_eq!(
        PlaylistFormat::from_path(std::path::Path::new("/music/queue.pls")),
        Some(PlaylistFormat::Pls)
    );
    assert_eq!(
        PlaylistFormat::from_path(std::path::Path::new("/music/queue.txt")),
        None
    );

    assert_eq!(PlaylistFormat::M3u.extension(), "m3u");
    assert_eq!(PlaylistFormat::Pls.extension(), "pls");
    assert_eq!(PlaylistFormat::Xspf.extension(), "xspf");
}

#[test]
fn an_unsupported_extension_is_reported_rather_than_guessed() {
    let dir = common::temp_dir("playlist_unknown");
    let path = dir.path().join("queue.txt");
    std::fs::write(&path, "not a playlist\n").expect("the file must be writable");
    let error = PlaylistFormat::read_auto(&path).expect_err("an unknown extension must fail");
    assert!(
        matches!(error, PlaylistIoError::UnknownFormat { .. }),
        "an unknown extension must be reported, got {error:?}"
    );
}

#[test]
fn a_loaded_playlist_keeps_its_entries_but_starts_with_nothing_selected() {
    // A playlist says what to play, not what is playing.
    let dir = common::temp_dir("playlist_queue");
    let path = dir.path().join("queue.m3u");
    let playlist = playlist_with_three_tracks();
    PlaylistFormat::M3u
        .write_to(&path, &playlist)
        .expect("the playlist must be writable");

    let mut queue = Playlist::new();
    queue.load_from_path(&path).expect("the playlist must load");
    assert_eq!(queue.len(), 3);
    assert_eq!(
        queue.current_index(),
        None,
        "a loaded playlist must not start playing on its own"
    );
    assert!(queue.current_source().is_none());

    let played = queue.play_index(1).expect("index 1 must be playable");
    assert_eq!(played, playlist.items()[1]);
    assert_eq!(queue.current_index(), Some(1));
    assert_eq!(queue.current_source(), Some(&playlist.items()[1]));
}

#[test]
fn a_failed_load_leaves_the_queue_untouched() {
    // A caller that has just asked to load a broken file and silently lost
    // its existing 200-track queue is worse off than one that got an error
    // and kept playing.
    let dir = common::temp_dir("playlist_broken");
    let path = dir.path().join("queue.txt");
    std::fs::write(&path, "not a playlist\n").expect("the file must be writable");

    let mut queue = playlist_with_three_tracks();
    let before = queue.len();
    assert!(queue.load_from_path(&path).is_err());
    assert_eq!(
        queue.len(),
        before,
        "a failed load must not empty the queue"
    );
}

#[test]
fn the_queue_advances_and_wraps_according_to_the_repeat_mode() {
    let mut playlist = playlist_with_three_tracks();
    playlist.set_repeat(RepeatMode::Off);

    assert_eq!(
        playlist.play_index(0).expect("playable"),
        playlist.items()[0]
    );
    assert_eq!(playlist.advance(), Some(playlist.items()[1].clone()));
    assert_eq!(playlist.advance(), Some(playlist.items()[2].clone()));
    assert_eq!(
        playlist.advance(),
        None,
        "with repeat off the queue must stop at the end"
    );

    playlist.set_repeat(RepeatMode::All);
    assert_eq!(
        playlist.advance(),
        Some(playlist.items()[0].clone()),
        "with repeat all the queue must wrap to the start"
    );

    playlist.set_repeat(RepeatMode::One);
    assert_eq!(playlist.repeat(), RepeatMode::One);
}

#[test]
fn removing_an_entry_keeps_the_current_index_pointing_at_the_same_track() {
    let mut playlist = playlist_with_three_tracks();
    let first = playlist.items()[0].clone();
    let third = playlist.items()[2].clone();
    playlist.play_index(2).expect("playable");

    let removed = playlist.remove(0).expect("index 0 must exist");
    assert_eq!(
        removed, first,
        "the removed entry must be the one asked for"
    );
    assert_eq!(playlist.len(), 2);
    assert_eq!(
        playlist.current_index(),
        Some(1),
        "the current index must follow the track, not the position"
    );
    assert_eq!(
        playlist.current_source(),
        Some(&third),
        "the current track must still be the third one"
    );
}

#[test]
fn every_supported_format_writes_and_reads_the_same_number_of_entries() {
    // A format that quietly drops an entry would reorder the user's queue.
    let dir = common::temp_dir("playlist_counts");
    let mut playlist = Playlist::new();
    for i in 0..7 {
        playlist.enqueue(AudioSource::File(PathBuf::from(format!(
            "/music/track{i}.flac"
        ))));
    }
    for format in FORMATS {
        let path = dir.path().join(format!("queue.{}", format.extension()));
        format
            .write_to(&path, &playlist)
            .unwrap_or_else(|e| panic!("{format:?} must be writable: {e}"));
        let parsed = format
            .read_from(&path)
            .unwrap_or_else(|e| panic!("{format:?} must be readable: {e}"));
        assert_eq!(
            parsed.entries.len(),
            7,
            "{format:?} must keep every entry it was given"
        );
    }
}

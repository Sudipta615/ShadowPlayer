//! Unit tests for the queue semantics in [`super::Playlist`].
//!
//! Split out of `mod.rs` so the module holds the queue implementation and
//! this file holds the cases.

use super::*;

/// A file-backed source, the shape almost every queue test needs.
fn src(name: &str) -> AudioSource {
    AudioSource::File(std::path::PathBuf::from(name))
}

#[test]
fn empty_playlist_returns_none() {
    let mut q = Playlist::new();
    assert!(q.current_source().is_none());
    assert!(q.advance().is_none());
    assert!(q.previous().is_none());
    assert_eq!(q.len(), 0);
}

#[test]
fn sequential_playback_with_repeat_off() {
    let mut q = Playlist::new();
    q.enqueue(src("a.flac"));
    q.enqueue(src("b.flac"));
    q.enqueue(src("c.flac"));

    // Play first track explicitly.
    let a = q.play_index(0).unwrap();
    assert_eq!(a.to_string(), "a.flac");
    assert_eq!(q.current_index(), Some(0));
    assert_eq!(q.peek_previous(), None);

    let b = q.advance().unwrap();
    assert_eq!(b.to_string(), "b.flac");
    assert_eq!(q.current_index(), Some(1));
    assert_eq!(q.peek_previous().unwrap().to_string(), "a.flac");

    let c = q.advance().unwrap();
    assert_eq!(c.to_string(), "c.flac");
    assert_eq!(q.current_index(), Some(2));

    // Repeat off → exhausted.
    assert!(q.advance().is_none());
    assert!(q.current_index().is_none());
}

#[test]
fn repeat_all_wraps() {
    let mut q = Playlist::new();
    q.set_repeat(RepeatMode::All);
    q.enqueue(src("a.flac"));
    q.enqueue(src("b.flac"));
    q.play_index(0);
    q.advance().unwrap(); // b
    let wrap = q.advance().unwrap(); // wraps back to a
    assert_eq!(wrap.to_string(), "a.flac");
}

#[test]
fn repeat_one_preserves_current_on_advance() {
    // RepeatOne does NOT make advance() return the same track — manual
    // Next always skips.  The engine handles repeat-one at EOS by seeking
    // to 0 without calling advance().
    let mut q = Playlist::new();
    q.set_repeat(RepeatMode::One);
    q.enqueue(src("song.flac"));
    q.play_index(0);
    // Single-track queue: advance returns None (nothing follows).
    assert!(q.advance().is_none());
    assert!(q.current_index().is_none());
}

#[test]
fn previous_rewinds_history() {
    let mut q = Playlist::new();
    q.enqueue(src("1.flac"));
    q.enqueue(src("2.flac"));
    q.enqueue(src("3.flac"));
    q.play_index(0);
    q.advance(); // 1→2
    q.advance(); // 2→3

    let back = q.previous().unwrap();
    assert_eq!(back.to_string(), "2.flac");
    let back2 = q.previous().unwrap();
    assert_eq!(back2.to_string(), "1.flac");
    assert!(q.previous().is_none());
}

#[test]
fn remove_fixes_indices() {
    let mut q = Playlist::new();
    q.enqueue(src("a.flac"));
    q.enqueue(src("b.flac"));
    q.enqueue(src("c.flac"));
    q.play_index(2); // "c.flac" at index 2

    q.remove(1); // remove "b.flac"
    assert_eq!(q.len(), 2);
    assert_eq!(q.current_index(), Some(1)); // c moved from 2→1
    assert_eq!(q.items[0].to_string(), "a.flac");
    assert_eq!(q.items[1].to_string(), "c.flac");
}

#[test]
fn clear_resets_everything() {
    let mut q = Playlist::new();
    q.enqueue(src("x.flac"));
    q.enqueue(src("y.flac"));
    q.play_index(0);
    q.advance();
    assert!(q.current_source().is_some());
    q.clear();
    assert!(q.is_empty());
    assert!(q.current_index().is_none());
    assert!(q.history.is_empty());
}

#[test]
fn sequential_advances_play_all_tracks() {
    // Deterministic: no shuffle, nothing played yet, advance from start.
    let mut q = Playlist::new();
    q.set_shuffle(false);
    q.enqueue(src("a.flac"));
    q.enqueue(src("b.flac"));
    q.enqueue(src("c.flac"));

    // No current track — advance picks index 0.
    assert_eq!(q.advance().unwrap().to_string(), "a.flac");
    assert_eq!(q.advance().unwrap().to_string(), "b.flac");
    assert_eq!(q.advance().unwrap().to_string(), "c.flac");
    assert!(q.advance().is_none());
}

#[test]
fn peek_next_does_not_mutate() {
    let mut q = Playlist::new();
    q.enqueue(src("a.flac"));
    q.enqueue(src("b.flac"));
    q.play_index(0);
    assert_eq!(q.peek_next().unwrap().to_string(), "b.flac");
    // State unchanged.
    assert_eq!(q.current_index(), Some(0));
    assert!(q.advance().is_some()); // still b
}

#[test]
fn metadata_is_carried_into_the_queue_and_stays_aligned() {
    // `from_parsed` used to drop the parsed per-entry metadata on the floor.
    // The queue now stores it parallel to its entries, and queue mutations
    // must keep the two vectors aligned — a drift here would show one track's
    // titles for another.
    let parsed = ParsedPlaylist {
        entries: vec![src("a.flac"), src("b.flac"), src("c.flac")],
        metadata: vec![
            TrackMetadata {
                title: Some("First".into()),
                ..TrackMetadata::default()
            },
            TrackMetadata::default(), // the file said nothing about b
            TrackMetadata {
                artist: Some("Third Artist".into()),
                ..TrackMetadata::default()
            },
        ],
        repeat: None,
    };
    let mut q = Playlist::from_parsed(parsed);
    assert_eq!(
        q.metadata(0).and_then(|m| m.title.as_deref()),
        Some("First")
    );
    assert_eq!(q.metadata(1), None, "an empty record means 'no metadata'");
    assert_eq!(
        q.metadata(2).and_then(|m| m.artist.as_deref()),
        Some("Third Artist")
    );
    assert_eq!(q.metadata(3), None, "out of range");
    assert_eq!(q.metadata.len(), q.len());

    // Removal shifts metadata with its entry.
    assert_eq!(q.remove(0).unwrap().to_string(), "a.flac");
    assert_eq!(q.metadata(0), None, "b's absent record moved up with b");
    assert_eq!(
        q.metadata(1).and_then(|m| m.artist.as_deref()),
        Some("Third Artist")
    );
    assert_eq!(q.metadata.len(), q.len());

    // A live enqueue carries no file metadata.
    q.enqueue(src("d.flac"));
    assert_eq!(q.metadata(2), None, "enqueue adds no metadata");
    assert_eq!(q.metadata.len(), q.len());

    // `clear` and `set_single` keep the vectors consistent too.
    q.clear();
    assert_eq!(q.metadata(0), None);
    assert_eq!(q.metadata.len(), q.len());
    q.set_single(src("only.flac"));
    assert_eq!(q.metadata(0), None);
    assert_eq!(q.metadata.len(), q.len());
}

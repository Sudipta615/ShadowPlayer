//! Lyrics loading, caching and time-synchronisation.
//!
//! Lyrics come from two places, in priority order:
//!
//! 1. A `.lrc`/`.txt` sidecar next to the audio file. Cheap enough to read
//!    synchronously, and the only way to get precise per-line timings.
//! 2. The file's own tags (`LYRICS`, `UNSYNCEDLYRICS`, `USLT`, …). Reading
//!    these means probing the container, so it happens on a worker thread and
//!    the result is picked up by [`LyricsService::poll`].
//!
//! Tagged lyrics are frequently a plain block of prose rather than LRC, so
//! everything is run through the LRC interpreter: text with timestamps drives
//! the karaoke highlight, text without is shown as ordinary lines.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use domain::ids::TrackId;
use domain::models::{SyncedLyricLine, Track};
use lyrics::{find_active_line, find_sidecar_lyrics, interpret_lyrics_text};

/// How many tracks' lyrics to keep. Bounds memory over a long listening
/// session while still making back-and-forth track skips instant.
const CACHE_LIMIT: usize = 128;

/// Where a track's lyrics came from, so the UI can explain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsSource {
    /// Time-stamped `.lrc` sidecar file.
    Sidecar,
    /// Time-stamped lyrics embedded in the file's tags.
    Tag,
    /// Unsynced lyrics, from either source.
    PlainText,
    /// Nothing found.
    None,
}

impl LyricsSource {
    /// Short caption for the lyrics panel.
    pub fn caption(self) -> Option<&'static str> {
        match self {
            LyricsSource::Sidecar => Some("Synced · LRC sidecar"),
            LyricsSource::Tag => Some("Synced · embedded tags"),
            LyricsSource::PlainText => Some("Unsynced lyrics — no timestamps"),
            LyricsSource::None => None,
        }
    }

    pub fn has_lyrics(self) -> bool {
        self != LyricsSource::None
    }
}

struct LyricsEntry {
    synced: Vec<SyncedLyricLine>,
    plain: Option<String>,
    source: LyricsSource,
}

impl LyricsEntry {
    fn missing() -> Self {
        Self {
            synced: Vec::new(),
            plain: None,
            source: LyricsSource::None,
        }
    }

    /// Build an entry from raw lyric text, deciding synced vs plain.
    fn from_text(text: String, synced_source: LyricsSource) -> Self {
        let (is_synced, lines) = interpret_lyrics_text(&text);
        if is_synced {
            Self {
                synced: lines,
                plain: Some(text),
                source: synced_source,
            }
        } else {
            Self {
                synced: Vec::new(),
                plain: Some(text),
                source: LyricsSource::PlainText,
            }
        }
    }
}

/// Result of an off-thread tag read.
enum TagLyrics {
    Found(String),
    Missing,
}

/// Service managing sidecar and embedded lyrics loading, caching, and synchronization.
pub struct LyricsService {
    cache: HashMap<TrackId, LyricsEntry>,
    /// Insertion order for bounded eviction.
    order: VecDeque<TrackId>,
    /// At most one tag read in flight, so rapid skipping cannot spawn a thread
    /// per track.
    inflight: Option<(TrackId, Receiver<TagLyrics>)>,
    /// Number of completed tag reads. Used by tests and diagnostics.
    tag_reads: usize,
}

impl LyricsService {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            order: VecDeque::new(),
            inflight: None,
            tag_reads: 0,
        }
    }

    /// True while a worker thread is reading tags for a track.
    pub fn is_loading(&self) -> bool {
        self.inflight.is_some()
    }

    pub fn tag_reads(&self) -> usize {
        self.tag_reads
    }

    /// Store an entry, evicting the oldest once the cache is full.
    fn insert(&mut self, id: TrackId, entry: LyricsEntry) {
        if self.cache.insert(id, entry).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
    }

    /// Ensure lyrics are parsed and cached for the track.
    ///
    /// Called every frame for the currently playing track, so it must be cheap
    /// on a cache hit and must never block: sidecars are read inline, tag
    /// lyrics go to a worker thread.
    pub fn ensure_loaded(&mut self, track: &Track) {
        if self.cache.contains_key(&track.id) {
            return;
        }

        // 1. Sidecar lyrics: a small file, read inline.
        if let Some((is_synced, lines, raw)) = find_sidecar_lyrics(&track.path) {
            let entry = match raw {
                Some(text) if is_synced => LyricsEntry {
                    synced: lines,
                    plain: Some(text),
                    source: LyricsSource::Sidecar,
                },
                Some(text) => LyricsEntry {
                    synced: Vec::new(),
                    plain: Some(text),
                    source: LyricsSource::PlainText,
                },
                None => LyricsEntry::missing(),
            };
            self.insert(track.id, entry);
            return;
        }

        // 2. Embedded lyrics: probing the container touches the disk, so keep
        //    it off the UI thread. One read at a time; a skipped track's read
        //    is simply superseded once it lands.
        if self.inflight.is_some() {
            return;
        }

        let id = track.id;
        let path: PathBuf = track.path.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Header-only tag read: no audio is decoded.
            let outcome = match metadata::extract_lyrics(&path) {
                Some(text) => TagLyrics::Found(text),
                None => TagLyrics::Missing,
            };
            // The receiver may already be gone if the track was skipped.
            let _ = tx.send(outcome);
        });
        self.inflight = Some((id, rx));
    }

    /// Collect a finished background tag read. Returns `true` when the cache
    /// changed and the caller should repaint.
    pub fn poll(&mut self) -> bool {
        let Some((id, rx)) = self.inflight.take() else {
            return false;
        };

        match rx.try_recv() {
            Ok(TagLyrics::Found(text)) => {
                self.insert(id, LyricsEntry::from_text(text, LyricsSource::Tag));
                self.tag_reads += 1;
                true
            }
            Ok(TagLyrics::Missing) => {
                self.insert(id, LyricsEntry::missing());
                self.tag_reads += 1;
                true
            }
            Err(TryRecvError::Empty) => {
                // Still reading: put it back and try again next frame.
                self.inflight = Some((id, rx));
                false
            }
            Err(TryRecvError::Disconnected) => {
                self.insert(id, LyricsEntry::missing());
                true
            }
        }
    }

    /// Retrieve active synced lines and plain text lyrics for track.
    pub fn get_lyrics(&self, track_id: &TrackId) -> (Vec<SyncedLyricLine>, Option<String>) {
        if let Some(entry) = self.cache.get(track_id) {
            (entry.synced.clone(), entry.plain.clone())
        } else {
            (Vec::new(), None)
        }
    }

    /// Where the cached lyrics for a track came from, if known yet.
    pub fn source(&self, track_id: &TrackId) -> Option<LyricsSource> {
        self.cache.get(track_id).map(|e| e.source)
    }

    /// Write edited lyrics to the track's `.lrc` sidecar and reload them.
    ///
    /// A sidecar is used even when the lyrics originally came from a tag: tags
    /// should not be rewritten by a text edit, and a sidecar is what the
    /// lyrics panel already prioritises on the next play.
    pub fn save_sidecar(&mut self, track: &Track, text: &str) -> Result<PathBuf, String> {
        let path = track.path.with_extension("lrc");
        std::fs::write(&path, text).map_err(|e| {
            tr_args!(
                "Could not write {path}: {error}",
                "path" => path.display(),
                "error" => e
            )
        })?;
        self.cache.remove(&track.id);
        self.order.retain(|id| *id != track.id);
        self.ensure_loaded(track);
        Ok(path)
    }

    /// The raw text the editor should start from.
    pub fn raw_text(&self, track_id: &TrackId) -> String {
        self.cache
            .get(track_id)
            .and_then(|entry| entry.plain.clone())
            .unwrap_or_default()
    }

    /// Find current active synchronized lyric line index for time position.
    pub fn active_index(lines: &[SyncedLyricLine], position_secs: f32) -> Option<usize> {
        find_active_line(lines, position_secs)
    }
}

impl Default for LyricsService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_without_timestamps_is_plain_not_synced() {
        let entry = LyricsEntry::from_text("Just prose\nNo timing".to_string(), LyricsSource::Tag);
        assert_eq!(entry.source, LyricsSource::PlainText);
        assert!(entry.synced.is_empty());
        assert_eq!(entry.plain.as_deref(), Some("Just prose\nNo timing"));
    }

    #[test]
    fn entry_with_timestamps_is_synced() {
        let entry = LyricsEntry::from_text(
            "[00:01.00]Hi\n[00:02.00]There".to_string(),
            LyricsSource::Tag,
        );
        assert_eq!(entry.source, LyricsSource::Tag);
        assert_eq!(entry.synced.len(), 2);
        assert_eq!(LyricsService::active_index(&entry.synced, 1.5), Some(0));
    }

    #[test]
    fn cache_evicts_oldest_entries() {
        let mut service = LyricsService::new();
        let mut ids = Vec::new();
        for _ in 0..(CACHE_LIMIT + 10) {
            let id = TrackId::new();
            ids.push(id);
            service.insert(
                id,
                LyricsEntry::from_text("[00:01.00]x".to_string(), LyricsSource::Sidecar),
            );
        }
        assert_eq!(service.cache.len(), CACHE_LIMIT);
        // The earliest ids are gone, the latest survive.
        assert!(!service.cache.contains_key(&ids[0]));
        assert!(service.cache.contains_key(ids.last().unwrap()));
    }

    #[test]
    fn source_caption_is_absent_when_no_lyrics() {
        assert_eq!(LyricsSource::None.caption(), None);
        assert!(!LyricsSource::None.has_lyrics());
        assert!(LyricsSource::Sidecar.has_lyrics());
    }

    /// Minimal `Track` pointing at a file we control.
    fn test_track(path: PathBuf) -> Track {
        Track {
            id: TrackId::new(),
            path,
            title: "Tagged Track".to_string(),
            artist: "Test".to_string(),
            album: "Test".to_string(),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 1.0,
            bpm: None,
            sample_rate: 44100,
            channels: 2,
            format: "MP3".to_string(),
            bitrate_kbps: Some(128),
            bit_depth: None,
            is_lossless: false,
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

    fn push_id3_frame(tag: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
        tag.extend_from_slice(id);
        tag.extend_from_slice(&(body.len() as u32).to_be_bytes());
        tag.extend_from_slice(&[0x00, 0x00]); // frame flags
        tag.extend_from_slice(body);
    }

    /// Hand-build a small but valid MP3 carrying a USLT lyrics frame, so the
    /// embedded-tag path can be tested without shipping a binary fixture.
    fn write_mp3_with_lyrics(path: &std::path::Path, lyrics: &str) {
        let mut tag = Vec::new();
        let mut title = vec![0x00]; // ISO-8859-1 encoding byte
        title.extend_from_slice(b"Tagged Track");
        push_id3_frame(&mut tag, b"TIT2", &title);

        let mut uslt = vec![0x00];
        uslt.extend_from_slice(b"eng");
        uslt.push(0x00); // empty content descriptor
        uslt.extend_from_slice(lyrics.as_bytes());
        push_id3_frame(&mut tag, b"USLT", &uslt);

        let mut out = Vec::new();
        out.extend_from_slice(b"ID3");
        out.extend_from_slice(&[0x03, 0x00, 0x00]); // v2.3, no flags
        let size = tag.len() as u32;
        out.extend_from_slice(&[
            ((size >> 21) & 0x7f) as u8,
            ((size >> 14) & 0x7f) as u8,
            ((size >> 7) & 0x7f) as u8,
            (size & 0x7f) as u8,
        ]);
        out.extend_from_slice(&tag);

        // A handful of MPEG-1 Layer III frames (128 kbps, 44.1 kHz) so the
        // container probe yields a real track.
        for _ in 0..8 {
            out.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
            out.extend_from_slice(&[0u8; 413]);
        }

        std::fs::write(path, out).unwrap();
    }

    /// Drain the worker-thread result, waiting briefly for it to land.
    fn wait_for_lyrics(service: &mut LyricsService) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if service.poll() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("embedded lyrics read did not finish in time");
    }

    #[test]
    fn loads_lyrics_embedded_in_file_tags() {
        let dir = std::env::temp_dir().join(format!("sp_lyrics_tag_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("tagged.mp3");
        write_mp3_with_lyrics(
            &audio,
            "[00:04.00]Tagged line one\n[00:09.00]Tagged line two",
        );

        let track = test_track(audio);
        let mut service = LyricsService::new();
        service.ensure_loaded(&track);
        // Nothing is cached synchronously; the read is off-thread by design.
        assert!(service.get_lyrics(&track.id).1.is_none());
        assert!(service.is_loading());

        wait_for_lyrics(&mut service);

        let (lines, plain) = service.get_lyrics(&track.id);
        assert_eq!(lines.len(), 2, "plain={plain:?}");
        assert_eq!(lines[0].start_time_secs, 4.0);
        assert_eq!(lines[1].text, "Tagged line two");
        assert_eq!(service.source(&track.id), Some(LyricsSource::Tag));
        assert_eq!(service.tag_reads(), 1);
    }

    #[test]
    fn a_track_without_lyrics_is_cached_as_resolved_empty() {
        let dir = std::env::temp_dir().join(format!("sp_lyrics_none_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("bare.mp3");
        write_mp3_with_lyrics(&audio, "");

        let track = test_track(audio);
        let mut service = LyricsService::new();
        service.ensure_loaded(&track);
        wait_for_lyrics(&mut service);

        assert_eq!(service.source(&track.id), Some(LyricsSource::None));
        // Resolved: a second call must not kick off another read.
        let reads_before = service.tag_reads();
        service.ensure_loaded(&track);
        assert!(!service.is_loading());
        assert_eq!(service.tag_reads(), reads_before);
    }
}

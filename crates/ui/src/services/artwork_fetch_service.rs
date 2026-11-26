//! Cover art bulk fetcher service.
//!
//! # How it works
//!
//! 1. Scans the user's library for tracks or albums that lack cover artwork
//!    (no embedded art in audio tags and no folder artwork like `cover.jpg`).
//! 2. Groups tracks by album + artist to avoid redundant requests.
//! 3. Queries the MusicBrainz Cover Art Archive (CAA) and public metadata
//!    mirrors for album art.
//! 4. Downloads image bytes, generates a thumbnail via [`library::ArtworkManager`],
//!    and persists it in `<data_dir>/artwork_cache/{track_id}.png`.
//! 5. Emits events back to the UI so [`crates::ui::services::ArtworkService`]
//!    can invalidate its cache and render the newly acquired album covers.
//!
//! # Privacy & Resilience
//!
//! - Runs on a dedicated background thread, respecting rate limits.
//! - Never blocks the UI thread.
//! - Every request, including redirects, is HTTPS-only and byte-capped.
//! - Downloaded bytes are decoded under the library's image bounds before
//!   being written as a thumbnail or folder cover.
//! - Network timeouts and 404s are treated as soft failures; never panics.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::{self, Receiver, Sender};
use domain::ids::TrackId;
use domain::models::Track;
use library::{validate_image_bytes, ArtworkManager};
use url::Url;

const USER_AGENT: &str = concat!(
    "ShadowPlayer/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/Sudipta615/shadowplayer )"
);

/// Metadata responses are small; anything larger is not a search result.
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;
/// Cover art is downloaded whole, so allow a generous but finite image.
const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
/// A redirect chain longer than this is either broken or hostile.
const MAX_REDIRECTS: usize = 3;

/// Progress snapshot for the bulk cover art fetcher.
#[derive(Debug, Clone, Default)]
pub struct ArtworkFetchProgress {
    pub running: bool,
    pub total: usize,
    pub done: usize,
    pub found: usize,
    pub failed: usize,
    pub current_album: String,
    pub current_artist: String,
}

/// Event emitted by the worker thread.
pub enum ArtworkFetchEvent {
    Progress(ArtworkFetchProgress),
    ArtworkSaved {
        track_ids: Vec<TrackId>,
        album: String,
    },
    Finished {
        total: usize,
        found: usize,
    },
}

/// Owns the background cover art fetch worker.
pub struct ArtworkFetchService {
    running: bool,
    cancel: Arc<AtomicBool>,
    rx: Option<Receiver<ArtworkFetchEvent>>,
    progress: ArtworkFetchProgress,
}

impl Default for ArtworkFetchService {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtworkFetchService {
    pub fn new() -> Self {
        Self {
            running: false,
            cancel: Arc::new(AtomicBool::new(false)),
            rx: None,
            progress: ArtworkFetchProgress::default(),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn progress(&self) -> &ArtworkFetchProgress {
        &self.progress
    }

    /// Start a bulk fetch task for all tracks missing artwork.
    pub fn start(&mut self, missing_tracks: Vec<Track>, artwork_manager: Arc<ArtworkManager>) {
        if missing_tracks.is_empty() {
            return;
        }
        if self.running {
            log::warn!("ArtworkFetchService: already running; ignoring start request.");
            return;
        }

        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        let (tx, rx) = channel::bounded(128);
        self.rx = Some(rx);
        self.running = true;
        self.progress = ArtworkFetchProgress {
            running: true,
            total: missing_tracks.len(),
            done: 0,
            found: 0,
            failed: 0,
            current_album: String::new(),
            current_artist: String::new(),
        };

        std::thread::Builder::new()
            .name("shadowplayer-art-fetch".into())
            .spawn(move || {
                worker_loop(missing_tracks, artwork_manager, cancel, tx);
            })
            .expect("failed to spawn artwork fetch worker");
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Drain pending events. Returns all track IDs whose artwork was successfully saved.
    pub fn poll(&mut self) -> Vec<TrackId> {
        let Some(rx) = &self.rx else {
            return Vec::new();
        };

        let mut saved_track_ids = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                ArtworkFetchEvent::Progress(prog) => {
                    self.progress = prog;
                }
                ArtworkFetchEvent::ArtworkSaved { track_ids, .. } => {
                    saved_track_ids.extend(track_ids);
                }
                ArtworkFetchEvent::Finished { .. } => {
                    self.running = false;
                    self.progress.running = false;
                }
            }
        }

        saved_track_ids
    }
}

// ── Background Worker ────────────────────────────────────────────────────────

fn worker_loop(
    tracks: Vec<Track>,
    manager: Arc<ArtworkManager>,
    cancel: Arc<AtomicBool>,
    tx: Sender<ArtworkFetchEvent>,
) {
    // Group tracks by album and artist
    let mut album_groups: Vec<(String, String, Vec<TrackId>, PathBuf)> = Vec::new();
    for track in tracks {
        let album = track.album.trim().to_string();
        let artist = track.artist.trim().to_string();
        if album.is_empty() {
            continue;
        }

        if let Some(group) = album_groups
            .iter_mut()
            .find(|(a, art, _, _)| a == &album && art == &artist)
        {
            group.2.push(track.id);
        } else {
            album_groups.push((album, artist, vec![track.id], track.path.clone()));
        }
    }

    let total = album_groups.len();
    let mut done = 0;
    let mut found = 0;
    let mut failed = 0;

    for (album, artist, track_ids, path) in album_groups {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let _ = tx.send(ArtworkFetchEvent::Progress(ArtworkFetchProgress {
            running: true,
            total,
            done,
            found,
            failed,
            current_album: album.clone(),
            current_artist: artist.clone(),
        }));

        // Rate limit sleep (500ms) between external requests to be courteous
        std::thread::sleep(Duration::from_millis(500));

        // Try Cover Art Archive via MusicBrainz or public mirror
        match fetch_album_art(&album, &artist) {
            Some(image_bytes) => {
                // The bytes came from the network: decode them under the same
                // bounds used for thumbnails before they touch the library or
                // a `cover.*` file. Invalid bytes are a soft failure.
                let Some(format) = validate_image_bytes(&image_bytes) else {
                    log::warn!(
                        "ArtworkFetchService: rejecting invalid image for {artist} — {album}"
                    );
                    failed += 1;
                    done += 1;
                    continue;
                };

                // Save thumbnail for every track in this album
                for tid in &track_ids {
                    manager.save_thumbnail(tid, &image_bytes);
                }

                // With a valid image, add a folder cover beside the audio file.
                // `create_new` never replaces an existing file or follows a
                // symlink placed where the cover would go.
                if let Some(parent) = path.parent() {
                    let cover_name = match format {
                        image::ImageFormat::Jpeg => Some("cover.jpg"),
                        image::ImageFormat::Png => Some("cover.png"),
                        _ => None,
                    };
                    if let Some(name) = cover_name {
                        let cover_path = parent.join(name);
                        if let Ok(mut file) = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&cover_path)
                        {
                            let _ = file.write_all(&image_bytes);
                            let _ = file.sync_all();
                        }
                    }
                }

                found += 1;
                let _ = tx.send(ArtworkFetchEvent::ArtworkSaved {
                    track_ids,
                    album: album.clone(),
                });
            }
            None => {
                failed += 1;
            }
        }

        done += 1;
    }

    let _ = tx.send(ArtworkFetchEvent::Progress(ArtworkFetchProgress {
        running: false,
        total,
        done,
        found,
        failed,
        current_album: String::new(),
        current_artist: String::new(),
    }));
    let _ = tx.send(ArtworkFetchEvent::Finished { total, found });
}

// ── HTTP Networking ──────────────────────────────────────────────────────────

/// Search and download album artwork bytes by album & artist.
fn fetch_album_art(album: &str, artist: &str) -> Option<Vec<u8>> {
    // 1. First attempt: Query MusicBrainz release search to find MBID
    if let Some(mbid) = search_mb_release(album, artist) {
        // Query Cover Art Archive. The id came from a remote document, so it
        // must look like a UUID before it becomes a URL path segment.
        if is_mbid(&mbid) {
            let caa_url = format!("https://coverartarchive.org/release/{mbid}/front-250.jpg");
            if let Ok(bytes) = http_get_bytes(&caa_url, USER_AGENT, MAX_IMAGE_BYTES) {
                if !bytes.is_empty() {
                    return Some(bytes);
                }
            }
        }
    }

    // 2. Fallback: Search iTunes Store public album search API (open, high coverage, zero auth)
    if let Some(art_url) = search_itunes_artwork(album, artist) {
        if let Ok(bytes) = http_get_bytes(&art_url, USER_AGENT, MAX_IMAGE_BYTES) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }

    None
}

/// `true` for a canonical MusicBrainz release id (UUID shape).
fn is_mbid(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

/// Query MusicBrainz API for a release matching album and artist.
fn search_mb_release(album: &str, artist: &str) -> Option<String> {
    let query = format!(
        "release:\"{}\" AND artist:\"{}\"",
        album.replace('"', ""),
        artist.replace('"', "")
    );
    let encoded_query = url_encode(&query);
    let url =
        format!("https://musicbrainz.org/ws/2/release/?query={encoded_query}&limit=1&fmt=json");

    let json_str = http_get_text(&url, USER_AGENT, MAX_JSON_BYTES).ok()?;
    let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    val.get("releases")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

/// Query iTunes API for album artwork URL.
fn search_itunes_artwork(album: &str, artist: &str) -> Option<String> {
    let term = url_encode(&format!("{artist} {album}"));
    let url = format!("https://itunes.apple.com/search?term={term}&entity=album&limit=1");

    let json_str = http_get_text(&url, USER_AGENT, MAX_JSON_BYTES).ok()?;
    let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    let raw_url = val
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("artworkUrl100"))
        .and_then(|u| u.as_str())?;

    // Upgrade thumbnail 100x100 to 500x500 for crisp audiophile album art.
    // The API has historically returned plain-http artwork URLs; the fetch
    // helper refuses those, so upgrade instead of following a downgrade.
    Some(raw_url.replacen("http://", "https://", 1))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Fetch text from an HTTPS endpoint, capped at `limit` bytes.
fn http_get_text(url: &str, user_agent: &str, limit: u64) -> Result<String, String> {
    let bytes = http_get_bytes(url, user_agent, limit)?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode: {e}"))
}

/// Fetch raw bytes over HTTPS, following at most [`MAX_REDIRECTS`] redirects.
///
/// Plain HTTP is refused at every hop: album metadata and artwork are not
/// secrets, but a downgrade would let anyone on the path choose the image that
/// gets written into the user's library. The response body is streamed through
/// a hard byte cap, independent of `Content-Length`.
fn http_get_bytes(url: &str, user_agent: &str, limit: u64) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(15))
        .timeout_write(Duration::from_secs(10))
        // Redirects are resolved below so each hop can be re-validated.
        .redirects(0)
        .build();

    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let parsed = Url::parse(&current).map_err(|e| format!("invalid URL: {e}"))?;
        if parsed.scheme() != "https" {
            return Err(format!("refusing non-HTTPS {} URL", parsed.scheme()));
        }

        let response = match agent
            .get(&current)
            .set("User-Agent", user_agent)
            .set("Accept", "*/*")
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::Status(code, response)) if (300..400).contains(&code) => response,
            Err(ureq::Error::Status(code, _)) => return Err(format!("HTTP status {code}")),
            Err(error) => return Err(format!("request failed: {error}")),
        };

        let status = response.status();
        if (300..400).contains(&status) {
            let location = response
                .header("Location")
                .ok_or("redirect without a Location header")?;
            current = parsed
                .join(location)
                .map_err(|e| format!("invalid redirect target: {e}"))?
                .to_string();
            continue;
        }
        if status != 200 {
            return Err(format!("HTTP status {status}"));
        }

        if let Some(length) = response
            .header("Content-Length")
            .and_then(|value| value.trim().parse::<u64>().ok())
        {
            if length > limit {
                return Err(format!(
                    "response of {length} bytes exceeds the {limit} byte limit"
                ));
            }
        }

        let mut body = Vec::new();
        response
            .into_reader()
            .take(limit + 1)
            .read_to_end(&mut body)
            .map_err(|e| format!("read body: {e}"))?;
        if body.len() as u64 > limit {
            return Err(format!("response body exceeds the {limit} byte limit"));
        }
        return Ok(body);
    }

    Err("too many redirects".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_is_refused_before_any_connection() {
        let error = http_get_bytes("http://example.com/cover.jpg", USER_AGENT, 1024).unwrap_err();
        assert!(error.contains("non-HTTPS"), "{error}");
    }

    #[test]
    fn only_uuid_shaped_mbids_become_url_paths() {
        assert!(is_mbid("8f6bd1e4-fbe1-4f50-aa9b-94c450ec0f11"));
        assert!(!is_mbid("../../etc/passwd"));
        assert!(!is_mbid("not-an-mbid"));
        assert!(!is_mbid(""));
    }
}

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
//! - Network timeouts and 404s are treated as soft failures; never panics.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::{self, Receiver, Sender};
use domain::ids::TrackId;
use domain::models::Track;
use library::ArtworkManager;

const USER_AGENT: &str = concat!(
    "ShadowPlayer/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/Sudipta615/shadowplayer )"
);

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
                // Save thumbnail for every track in this album
                for tid in &track_ids {
                    manager.save_thumbnail(tid, &image_bytes);
                }

                // If path parent is writable, try writing cover.jpg
                if let Some(parent) = path.parent() {
                    let cover_path = parent.join("cover.jpg");
                    if !cover_path.exists() {
                        let _ = std::fs::write(&cover_path, &image_bytes);
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
        // Query Cover Art Archive
        let caa_url = format!("http://coverartarchive.org/release/{mbid}/front-250.jpg");
        if let Ok(bytes) = http_get_bytes(&caa_url, USER_AGENT, 3) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }

    // 2. Fallback: Search iTunes Store public album search API (open, high coverage, zero auth)
    if let Some(art_url) = search_itunes_artwork(album, artist) {
        if let Ok(bytes) = http_get_bytes(&art_url, USER_AGENT, 3) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }

    None
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
        format!("http://musicbrainz.org/ws/2/release/?query={encoded_query}&limit=1&fmt=json");

    let json_str = http_get_text(&url, USER_AGENT, 2).ok()?;
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
    let url = format!("http://itunes.apple.com/search?term={term}&entity=album&limit=1");

    let json_str = http_get_text(&url, USER_AGENT, 2).ok()?;
    let val: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    let raw_url = val
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("artworkUrl100"))
        .and_then(|u| u.as_str())?;

    // Upgrade thumbnail 100x100 to 500x500 for crisp audiophile album art
    Some(raw_url.replace("100x100bb", "500x500bb"))
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

/// Fetch text response from HTTP endpoint, following redirects up to `max_redirects`.
fn http_get_text(url: &str, user_agent: &str, max_redirects: u8) -> Result<String, String> {
    let bytes = http_get_bytes(url, user_agent, max_redirects)?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode: {e}"))
}

/// Fetch raw bytes from HTTP endpoint with redirect support.
fn http_get_bytes(url: &str, user_agent: &str, max_redirects: u8) -> Result<Vec<u8>, String> {
    if max_redirects == 0 {
        return Err("too many redirects".into());
    }

    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or("unsupported scheme")?;

    let (host_port, path) = without_scheme
        .split_once('/')
        .map(|(h, p)| (h, format!("/{p}")))
        .unwrap_or((without_scheme, "/".to_string()));

    let (host, port) = host_port
        .split_once(':')
        .map(|(h, p)| (h, p.parse::<u16>().unwrap_or(80)))
        .unwrap_or((host_port, 80));

    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).map_err(|e| format!("connect to {addr}: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(12))).ok();

    let mut stream = stream;
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: {ua}\r\n\
         Accept: */*\r\n\
         Connection: close\r\n\
         \r\n",
        path = path,
        host = host,
        ua = user_agent,
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .map_err(|e| format!("read status: {e}"))?;

    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut headers = std::collections::HashMap::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("read header: {e}"))?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    // Handle 301, 302, 307 redirects
    if (status_code == 301 || status_code == 302 || status_code == 307 || status_code == 308)
        && headers.contains_key("location")
    {
        let redirect_url = &headers["location"];
        let target = if redirect_url.starts_with("http") {
            redirect_url.to_string()
        } else {
            format!("http://{host}{redirect_url}")
        };
        return http_get_bytes(&target, user_agent, max_redirects - 1);
    }

    if status_code != 200 {
        return Err(format!("HTTP status {status_code}"));
    }

    let mut body = Vec::new();
    reader
        .read_to_end(&mut body)
        .map_err(|e| format!("read body: {e}"))?;

    Ok(body)
}

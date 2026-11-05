//! Desktop media-control bridge.
//!
//! The player's authoritative transport lives in [`crate::PlaybackCoordinator`],
//! *not* in the audio engine, so this service implements the integration in the
//! app and bridges desktop media keys back into the UI loop:
//!
//! * the UI loop publishes an [`MprisSnapshot`] every frame,
//! * on Linux a background thread turns it into MPRIS2 `PropertiesChanged`
//!   signals; on Windows/macOS `souvlaki` publishes SMTC / Now Playing state,
//! * transport calls from the shell are queued as [`MprisCommand`]s which the
//!   UI loop drains and forwards to the coordinator / session.
//!
//! When the platform service is unavailable the handle is inert, so callers
//! need no `cfg`.

use std::sync::{Arc, Mutex};

use crossbeam::channel::{unbounded, Receiver};
use eframe::egui;

/// A transport request originating from the desktop (media keys, shell UI).
#[derive(Debug, Clone, PartialEq)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    /// Seek by a relative offset, in seconds (may be negative).
    SeekRelative(f64),
    /// Seek to an absolute position, in seconds.
    SetPosition(f64),
    SetVolume(f32),
    /// Set the MPRIS `LoopStatus` ("None", "Track" or "Playlist").
    SetLoopStatus(String),
    /// Set MPRIS `Shuffle` on or off.
    SetShuffle(bool),
    /// Set MPRIS `Rate` in the advertised 0.5..=2.0 range.
    SetRate(f64),
    /// Bring the application window to the front.
    Raise,
    /// Request the application to quit.
    Quit,
}

/// Turn an artwork path into a URI the desktop media APIs can fetch.
///
/// MPRIS and SMTC both want a URI, not a filesystem path. Windows paths need
/// forward slashes and a drive/UNC form; spaces and non-ASCII bytes need
/// percent-encoding. Getting this wrong is invisible on Linux and shows up as
/// a missing thumbnail in the Windows flyout, so the conversion has tests for
/// every platform shape rather than only the host's.
pub fn artwork_file_url(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    // On a Unix host `Path::is_absolute` says a Windows drive or UNC path is
    // relative. The URL conversion still has to recognise those shapes: they
    // are exactly what arrives from a Windows library on a shared drive, and
    // prepending the current directory would make the URI point nowhere.
    let windows_absolute = raw.starts_with("//")
        || (raw.len() >= 2 && raw.as_bytes()[0].is_ascii_alphabetic() && raw.as_bytes()[1] == b':');
    let text = if path.is_absolute() || windows_absolute {
        raw
    } else {
        let absolute = std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf());
        absolute.to_string_lossy().replace('\\', "/")
    };
    let encoded = percent_encode_uri_path(&text);

    if let Some(unc) = encoded.strip_prefix("//") {
        // `\\server\share\art.jpg` is a URI authority, not a local path.
        format!("file://{unc}")
    } else if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn percent_encode_uri_path(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(*byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~');
        if keep {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The player state published to the desktop. Refreshed once per UI frame.
///
/// Also the loopback control protocol's `state` payload, so a remote sees
/// exactly what a desktop media widget sees.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MprisSnapshot {
    /// Whether a track is loaded at all (distinguishes Stopped from Paused).
    pub loaded: bool,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length_secs: f64,
    pub position_secs: f64,
    pub volume: f32,
    /// MPRIS `LoopStatus`: "None", "Track" or "Playlist".
    #[serde(default = "default_loop_status")]
    pub loop_status: String,
    /// MPRIS `Shuffle`.
    #[serde(default)]
    pub shuffle: bool,
    /// MPRIS `Rate` in the advertised 0.5..=2.0 range.
    #[serde(default = "default_rate")]
    pub rate: f64,
    /// `file://` URL of the current cover art, if known.
    pub art_url: Option<String>,
}

fn default_loop_status() -> String {
    "None".to_string()
}

fn default_rate() -> f64 {
    1.0
}

/// Owns the MPRIS2 D-Bus service and the command/snapshot channels.
pub struct MprisService {
    commands: Receiver<MprisCommand>,
    snapshot: Arc<Mutex<MprisSnapshot>>,
    /// Set from the UI so D-Bus method calls can wake the (possibly idle)
    /// event loop to drain the command queue immediately.
    repaint: Arc<Mutex<Option<egui::Context>>>,
    active: bool,
    /// Kept purely for RAII: dropping it joins the D-Bus worker thread.
    #[cfg(target_os = "linux")]
    #[allow(dead_code)]
    inner: Option<linux::ServerHandle>,

    /// Windows SMTC / macOS Now Playing controls, created once the first
    /// window handle is available.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    media: Option<souvlaki::MediaControls>,
    /// Sender used by the souvlaki callback thread to wake the UI.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    command_tx: crossbeam::channel::Sender<MprisCommand>,
    /// Avoids re-uploading artwork/metadata every frame on Windows/macOS.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    last_media: Option<(String, String, String, Option<String>, u64)>,
    /// Playback state/progress is throttled to ~10 updates per second. Windows
    /// timeline updates re-render the system flyout, so doing that per frame
    /// would be wasteful.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    last_playback: Option<(bool, bool, u64)>,
}

mod linux;

impl MprisService {
    /// Start the service. Never fails: if the session bus is unavailable the
    /// returned handle is simply inert.
    pub fn start() -> Self {
        let (tx, commands) = unbounded();
        let snapshot = Arc::new(Mutex::new(MprisSnapshot::default()));
        let repaint = Arc::new(Mutex::new(None));

        #[cfg(target_os = "linux")]
        let inner = match linux::spawn(tx, Arc::clone(&snapshot), Arc::clone(&repaint)) {
            Ok(handle) => Some(handle),
            Err(e) => {
                log::warn!("MPRIS2 desktop integration unavailable: {e}");
                None
            }
        };
        #[cfg(not(target_os = "linux"))]
        {
            let _ = tx;
        }

        #[cfg(target_os = "linux")]
        let active = inner.is_some();
        #[cfg(not(target_os = "linux"))]
        let active = false;

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let command_tx = tx.clone();

        Self {
            commands,
            snapshot,
            repaint,
            active,
            #[cfg(target_os = "linux")]
            inner,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            media: None,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            command_tx,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            last_media: None,
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            last_playback: None,
        }
    }

    /// Whether the desktop media service is actually running.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Give the service an `egui::Context` so incoming desktop requests can
    /// wake the UI even when it is otherwise idle. Cheap to call every frame.
    pub fn attach_context(&self, ctx: &egui::Context) {
        if let Ok(mut slot) = self.repaint.lock() {
            if slot.is_none() {
                *slot = Some(ctx.clone());
            }
        }
    }

    /// Create the Windows/macOS media integration once the window exists.
    /// Linux already has its D-Bus service, so this is a no-op there.
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub fn attach_window(&mut self, _frame: &eframe::Frame) {
        if self.media.is_some() {
            return;
        }

        #[cfg(target_os = "windows")]
        let hwnd = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            _frame
                .window_handle()
                .ok()
                .and_then(|handle| match handle.as_raw() {
                    RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
                    _ => None,
                })
        };
        #[cfg(target_os = "macos")]
        let hwnd: Option<*mut std::ffi::c_void> = None;

        #[cfg(target_os = "windows")]
        if hwnd.is_none() {
            log::warn!("Windows media controls unavailable: no HWND yet");
            return;
        }

        let config = souvlaki::PlatformConfig {
            display_name: "ShadowPlayer",
            dbus_name: "org.shadowplayer.ShadowPlayer",
            hwnd,
        };
        let mut controls = match souvlaki::MediaControls::new(config) {
            Ok(controls) => controls,
            Err(e) => {
                log::warn!("Windows/macOS media integration unavailable: {e}");
                return;
            }
        };

        let tx = self.command_tx.clone();
        let repaint = Arc::clone(&self.repaint);
        let attached = controls.attach(move |event| {
            use souvlaki::{MediaControlEvent, SeekDirection};
            let command = match event {
                MediaControlEvent::Play => MprisCommand::Play,
                MediaControlEvent::Pause => MprisCommand::Pause,
                MediaControlEvent::Toggle => MprisCommand::PlayPause,
                MediaControlEvent::Next => MprisCommand::Next,
                MediaControlEvent::Previous => MprisCommand::Previous,
                MediaControlEvent::Stop => MprisCommand::Stop,
                MediaControlEvent::Seek(direction) => {
                    MprisCommand::SeekRelative(if direction == SeekDirection::Forward {
                        10.0
                    } else {
                        -10.0
                    })
                }
                MediaControlEvent::SeekBy(direction, amount) => {
                    let seconds = amount.as_secs_f64();
                    MprisCommand::SeekRelative(if direction == SeekDirection::Forward {
                        seconds
                    } else {
                        -seconds
                    })
                }
                MediaControlEvent::SetPosition(position) => {
                    MprisCommand::SetPosition(position.0.as_secs_f64())
                }
                MediaControlEvent::SetVolume(volume) => MprisCommand::SetVolume(volume as f32),
                MediaControlEvent::Raise => MprisCommand::Raise,
                MediaControlEvent::Quit => MprisCommand::Quit,
                MediaControlEvent::OpenUri(uri) => {
                    log::info!("Ignoring media-control open request for {uri}");
                    return;
                }
            };
            let _ = tx.send(command);
            if let Ok(slot) = repaint.lock() {
                if let Some(ctx) = slot.as_ref() {
                    ctx.request_repaint();
                }
            }
        });

        if let Err(e) = attached {
            log::warn!("Could not attach Windows/macOS media controls: {e}");
            return;
        }
        self.media = Some(controls);
        self.active = true;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    pub fn attach_window(&mut self, _frame: &eframe::Frame) {}

    /// Non-blocking read of the next desktop transport request.
    pub fn try_recv(&self) -> Option<MprisCommand> {
        self.commands.try_recv().ok()
    }

    /// Publish the current player state to the desktop.
    pub fn publish(&mut self, snap: MprisSnapshot) {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let length_ms = (snap.length_secs * 1000.0).max(0.0) as u64;
            if let Some(controls) = self.media.as_mut() {
                // Compare borrowed fields first: this runs every frame while
                // playing, and cloning four strings just to throw them away is
                // needless allocation churn.
                let unchanged =
                    self.last_media
                        .as_ref()
                        .is_some_and(|(title, artist, album, art, length)| {
                            title == &snap.title
                                && artist == &snap.artist
                                && album == &snap.album
                                && art == &snap.art_url
                                && *length == length_ms
                        });
                if !unchanged {
                    let metadata = souvlaki::MediaMetadata {
                        title: Some(snap.title.as_str()),
                        album: Some(snap.album.as_str()),
                        artist: Some(snap.artist.as_str()),
                        cover_url: snap.art_url.as_deref(),
                        duration: Some(std::time::Duration::from_secs_f64(
                            snap.length_secs.max(0.0),
                        )),
                    };
                    if let Err(e) = controls.set_metadata(metadata) {
                        log::debug!("Could not publish media metadata: {e}");
                    }
                    self.last_media = Some((
                        snap.title.clone(),
                        snap.artist.clone(),
                        snap.album.clone(),
                        snap.art_url.clone(),
                        length_ms,
                    ));
                }

                let position_tenths = (snap.position_secs.max(0.0) * 10.0) as u64;
                let playback_key = (snap.loaded, snap.playing, position_tenths);
                if self.last_playback != Some(playback_key) {
                    let progress = souvlaki::MediaPosition(std::time::Duration::from_secs_f64(
                        snap.position_secs.max(0.0),
                    ));
                    let playback = if !snap.loaded {
                        souvlaki::MediaPlayback::Stopped
                    } else if snap.playing {
                        souvlaki::MediaPlayback::Playing {
                            progress: Some(progress),
                        }
                    } else {
                        souvlaki::MediaPlayback::Paused {
                            progress: Some(progress),
                        }
                    };
                    if let Err(e) = controls.set_playback(playback) {
                        log::debug!("Could not publish media playback state: {e}");
                    }
                    self.last_playback = Some(playback_key);
                }
            }
        }

        if let Ok(mut s) = self.snapshot.lock() {
            *s = snap;
        }
    }
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod artifact_url_tests {
    use super::artwork_file_url;
    use std::path::Path;

    #[test]
    fn unix_paths_become_file_uris() {
        assert_eq!(
            artwork_file_url(Path::new("/home/user/Music/art 1.jpg")),
            "file:///home/user/Music/art%201.jpg"
        );
    }

    #[test]
    fn windows_drive_paths_get_the_uri_slash() {
        assert_eq!(
            artwork_file_url(Path::new(r"C:\Users\me\Artwork\cover.jpg")),
            "file:///C:/Users/me/Artwork/cover.jpg"
        );
    }

    #[test]
    fn unc_paths_become_an_authority() {
        assert_eq!(
            artwork_file_url(Path::new(r"\\server\share\cover art.jpg")),
            "file://server/share/cover%20art.jpg"
        );
    }

    #[test]
    fn non_ascii_is_percent_encoded_as_utf8() {
        assert_eq!(
            artwork_file_url(Path::new("/music/Björk/cover.jpg")),
            "file:///music/Bj%C3%B6rk/cover.jpg"
        );
    }
}

#[cfg(all(test, target_os = "linux"))]
mod mpris_integration_tests {
    use super::{MprisCommand, MprisService};
    use std::time::{Duration, Instant};

    /// Drive the real D-Bus service the way `playerctl` does.
    ///
    /// This is skipped (not failed) when the test host has no session bus,
    /// which is true of a bare container. CI runs it under `dbus-run-session`.
    #[test]
    fn mpris_service_appears_on_the_session_bus_and_accepts_play_pause() {
        if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none()
            || pollster::block_on(zbus::Connection::session()).is_err()
        {
            return;
        }

        let service = MprisService::start();
        if !service.is_active() {
            return;
        }

        let bus_name = "org.mpris.MediaPlayer2.shadowplayer";
        let object_path = "/org/mpris/MediaPlayer2";
        let player_iface = "org.mpris.MediaPlayer2.Player";

        let connected = pollster::block_on(async {
            let conn = zbus::Connection::session().await?;
            let bus = zbus::Proxy::new(
                &conn,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
            )
            .await?;

            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let names: Vec<String> = bus.call("ListNames", &()).await?;
                if names.iter().any(|name| name == bus_name) {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err::<(), zbus::Error>(zbus::Error::Failure(
                        "MPRIS service name never appeared".into(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }

            let player = zbus::Proxy::new(&conn, bus_name, object_path, player_iface).await?;
            let status: String = player.get_property("PlaybackStatus").await?;
            assert_eq!(status, "Stopped");

            // This is the same method playerctl calls for `playerctl play-pause`.
            player.call::<_, _, ()>("PlayPause", &()).await?;
            Ok(())
        });

        if let Err(e) = connected {
            panic!("MPRIS D-Bus integration failed: {e}");
        }

        // The method call crossed D-Bus and landed in the command queue the UI
        // drains, which is the complete playerctl path.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(command) = service.try_recv() {
                assert_eq!(command, MprisCommand::PlayPause);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "PlayPause never reached the command queue"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

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
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// Bring the application window to the front.
    Raise,
    /// Request the application to quit.
    Quit,
}

/// The player state published to the desktop. Refreshed once per UI frame.
#[derive(Debug, Clone, Default)]
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
    /// `file://` URL of the current cover art, if known.
    pub art_url: Option<String>,
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
mod linux {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use crossbeam::channel::Sender;
    use eframe::egui;
    use zbus::{ObjectPath, Variant};

    use super::{MprisCommand, MprisSnapshot};

    /// Shared slot for the UI's `egui::Context`, used to wake the event loop.
    type RepaintSlot = Arc<Mutex<Option<egui::Context>>>;

    const BUS_NAME: &str = "org.mpris.MediaPlayer2.shadowplayer";
    const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
    const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
    const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

    /// Keeps the MPRIS worker thread alive; shutting it down joins the thread.
    pub struct ServerHandle {
        stop: Arc<AtomicBool>,
        worker: Option<JoinHandle<()>>,
    }

    impl ServerHandle {
        pub fn shutdown(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    impl Drop for ServerHandle {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    /// Spawn the D-Bus service. Returns `Err` when D-Bus is unavailable.
    pub fn spawn(
        tx: Sender<MprisCommand>,
        snapshot: Arc<Mutex<MprisSnapshot>>,
        repaint: RepaintSlot,
    ) -> Result<ServerHandle, String> {
        let builder = zbus::connection::Builder::session()
            .name(BUS_NAME)
            .serve_at(
                OBJECT_PATH,
                Root {
                    tx: tx.clone(),
                    repaint: Arc::clone(&repaint),
                },
            )
            .serve_at(
                OBJECT_PATH,
                Player {
                    snapshot: Arc::clone(&snapshot),
                    tx,
                    repaint,
                },
            );
        let conn = pollster::block_on(builder.build()).map_err(|e| e.to_string())?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_worker = Arc::clone(&stop);

        let worker = std::thread::Builder::new()
            .name("mpris-bridge".to_string())
            .spawn(move || {
                log::info!("MPRIS2 service active as {BUS_NAME}");
                let mut prev = MprisSnapshot::default();
                let mut first = true;

                while !stop_worker.load(Ordering::Acquire) {
                    let cur = snapshot.lock().map(|s| s.clone()).unwrap_or_default();

                    let mut changed: HashMap<String, Variant<'static>> = HashMap::new();

                    if first || cur.playing != prev.playing || cur.loaded != prev.loaded {
                        changed.insert(
                            "PlaybackStatus".to_string(),
                            Variant::from(playback_status(&cur)),
                        );
                    }
                    if first || (cur.volume - prev.volume).abs() > 0.001 {
                        changed.insert("Volume".to_string(), Variant::from(cur.volume as f64));
                    }
                    if first || metadata_changed(&cur, &prev) {
                        changed.insert("Metadata".to_string(), Variant::from(metadata_map(&cur)));
                    }

                    if !changed.is_empty() {
                        let emitted = pollster::block_on(conn.emit_signal(
                            None::<&str>,
                            OBJECT_PATH,
                            PROPS_IFACE,
                            "PropertiesChanged",
                            &(PLAYER_IFACE, changed, Vec::<String>::new()),
                        ));
                        if let Err(e) = emitted {
                            log::warn!("MPRIS2 PropertiesChanged failed: {e}");
                            break;
                        }
                        prev = cur;
                        first = false;
                    }

                    std::thread::sleep(Duration::from_millis(200));
                }

                log::info!("MPRIS2 service stopped");
            })
            .map_err(|e| e.to_string())?;

        Ok(ServerHandle {
            stop,
            worker: Some(worker),
        })
    }

    /// Nudge the UI loop so it drains the command queue right away.
    fn wake(repaint: &RepaintSlot) {
        if let Ok(slot) = repaint.lock() {
            if let Some(ctx) = slot.as_ref() {
                ctx.request_repaint();
            }
        }
    }

    fn playback_status(s: &MprisSnapshot) -> String {
        if !s.loaded {
            "Stopped".to_string()
        } else if s.playing {
            "Playing".to_string()
        } else {
            "Paused".to_string()
        }
    }

    fn metadata_changed(a: &MprisSnapshot, b: &MprisSnapshot) -> bool {
        a.title != b.title
            || a.artist != b.artist
            || a.album != b.album
            || (a.length_secs - b.length_secs).abs() > 0.001
            || a.art_url != b.art_url
    }

    fn metadata_map(s: &MprisSnapshot) -> HashMap<String, Variant<'static>> {
        let mut m: HashMap<String, Variant<'static>> = HashMap::new();

        let track_id =
            ObjectPath::try_from("/org/mpris/MediaPlayer2/Track/0").unwrap_or_else(|_| {
                ObjectPath::from_static_str_unchecked("/org/mpris/MediaPlayer2/Track/0")
            });
        m.insert("mpris:trackid".to_string(), Variant::from(track_id));

        if s.length_secs > 0.0 {
            m.insert(
                "mpris:length".to_string(),
                Variant::from((s.length_secs * 1_000_000.0) as i64),
            );
        }
        if !s.title.is_empty() {
            m.insert("xesam:title".to_string(), Variant::from(s.title.clone()));
        }
        if !s.artist.is_empty() {
            m.insert(
                "xesam:artist".to_string(),
                Variant::from(vec![s.artist.clone()]),
            );
        }
        if !s.album.is_empty() {
            m.insert("xesam:album".to_string(), Variant::from(s.album.clone()));
        }
        if let Some(url) = &s.art_url {
            m.insert("mpris:artUrl".to_string(), Variant::from(url.clone()));
        }
        m
    }

    struct Root {
        tx: Sender<MprisCommand>,
        repaint: RepaintSlot,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        fn raise(&self) {
            let _ = self.tx.send(MprisCommand::Raise);
            wake(&self.repaint);
        }

        fn quit(&self) {
            let _ = self.tx.send(MprisCommand::Quit);
            wake(&self.repaint);
        }

        #[zbus(property)]
        fn can_quit(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_raise(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn has_track_list(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn identity(&self) -> String {
            "ShadowPlayer".to_string()
        }

        #[zbus(property)]
        fn desktop_entry(&self) -> String {
            "shadowplayer".to_string()
        }

        #[zbus(property)]
        fn supported_uri_schemes(&self) -> Vec<String> {
            vec!["file".to_string()]
        }

        #[zbus(property)]
        fn supported_mime_types(&self) -> Vec<String> {
            vec![
                "audio/flac".to_string(),
                "audio/mpeg".to_string(),
                "audio/ogg".to_string(),
                "audio/wav".to_string(),
                "audio/aac".to_string(),
                "audio/mp4".to_string(),
            ]
        }
    }

    struct Player {
        snapshot: Arc<Mutex<MprisSnapshot>>,
        tx: Sender<MprisCommand>,
        repaint: RepaintSlot,
    }

    impl Player {
        fn snap(&self) -> MprisSnapshot {
            self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
        }

        fn send(&self, cmd: MprisCommand) {
            let _ = self.tx.send(cmd);
            wake(&self.repaint);
        }
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        fn next(&self) {
            self.send(MprisCommand::Next);
        }

        fn previous(&self) {
            self.send(MprisCommand::Previous);
        }

        fn pause(&self) {
            self.send(MprisCommand::Pause);
        }

        fn play_pause(&self) {
            self.send(MprisCommand::PlayPause);
        }

        fn stop(&self) {
            self.send(MprisCommand::Stop);
        }

        fn play(&self) {
            self.send(MprisCommand::Play);
        }

        fn seek(&self, offset_us: i64) {
            self.send(MprisCommand::SeekRelative(offset_us as f64 / 1_000_000.0));
        }

        fn set_position(&self, _track_id: ObjectPath<'_>, position_us: i64) {
            self.send(MprisCommand::SetPosition(position_us as f64 / 1_000_000.0));
        }

        fn open_uri(&self, _uri: String) {
            // Deliberately unsupported: the app owns queue/library semantics.
        }

        #[zbus(property)]
        fn playback_status(&self) -> String {
            playback_status(&self.snap())
        }

        #[zbus(property)]
        fn loop_status(&self) -> String {
            "None".to_string()
        }

        #[zbus(property)]
        fn set_loop_status(&self, _status: String) {}

        #[zbus(property)]
        fn rate(&self) -> f64 {
            1.0
        }

        #[zbus(property)]
        fn set_rate(&self, _rate: f64) {}

        #[zbus(property)]
        fn shuffle(&self) -> bool {
            false
        }

        #[zbus(property)]
        fn set_shuffle(&self, _shuffle: bool) {}

        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, Variant<'static>> {
            metadata_map(&self.snap())
        }

        #[zbus(property)]
        fn volume(&self) -> f64 {
            self.snap().volume as f64
        }

        #[zbus(property)]
        fn set_volume(&self, volume: f64) {
            self.send(MprisCommand::SetVolume(volume.clamp(0.0, 1.0) as f32));
        }

        #[zbus(property)]
        fn position(&self) -> i64 {
            (self.snap().position_secs * 1_000_000.0) as i64
        }

        #[zbus(property)]
        fn minimum_rate(&self) -> f64 {
            0.5
        }

        #[zbus(property)]
        fn maximum_rate(&self) -> f64 {
            2.0
        }

        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_seek(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
    }
}

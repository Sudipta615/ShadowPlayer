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

    let track_id = ObjectPath::try_from("/org/mpris/MediaPlayer2/Track/0").unwrap_or_else(|_| {
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
        let snapshot = self.snap();
        if snapshot.loop_status.is_empty() {
            let mode = crate::services::transport::loop_status_mpris(
                domain::models::RepeatModeChoice::Off,
            );
            return mode.to_string();
        }
        snapshot.loop_status
    }

    #[zbus(property)]
    fn set_loop_status(&self, status: String) {
        self.send(MprisCommand::SetLoopStatus(status));
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        let rate = self.snap().rate;
        if rate.is_finite() {
            crate::services::transport::clamp_mpris_rate(rate)
        } else {
            1.0
        }
    }

    #[zbus(property)]
    fn set_rate(&self, rate: f64) {
        self.send(MprisCommand::SetRate(rate));
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.snap().shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&self, shuffle: bool) {
        self.send(MprisCommand::SetShuffle(shuffle));
    }

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
        crate::services::transport::MIN_MPRIS_RATE
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        crate::services::transport::MAX_MPRIS_RATE
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

// A windowed subsystem binary on Windows: launching the app from the
// Explorer does not flash a console window.
//
// This is a *crate-level* attribute, so it has to be at the top of the crate
// root — putting it on `main` compiles, warns, and does nothing, which is
// worse than not having it. The log still goes to the file in the data
// directory (see `TeeLog`), so a launch with no console loses nothing.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod instance;

use std::path::PathBuf;
use std::sync::Arc;

use instance::{InstanceLock, LockOutcome};

use audio_session::AudioSession;
use database::{Database, QueueRepo, SettingsRepo, TrackRepo};
use eframe::egui::Vec2;
use log::info;
use ui::ShadowPlayerApp;

/// One-time removal of the development demo library that older builds seeded
/// into the database and data directory. Only tracks living under the demo
/// folder are touched, so real imported music is never affected.
fn purge_legacy_demo_data(db: &Database, data_dir: &std::path::Path) {
    const KEY: &str = "legacy_demo_data_purged_v1";

    let settings = SettingsRepo::new(db.clone());
    if matches!(settings.get_value(KEY), Ok(Some(_))) {
        return;
    }

    let demo_dir = data_dir.join("demo_sample_music");

    match TrackRepo::new(db.clone()).delete_by_path_prefix(&demo_dir) {
        Ok(n) if n > 0 => info!("Removed {} legacy demo track(s)", n),
        Err(e) => log::warn!("Could not purge legacy demo tracks: {}", e),
        _ => {}
    }
    if let Err(e) = QueueRepo::new(db.clone()).purge_path_prefix(&demo_dir) {
        log::warn!("Could not purge legacy demo queue entries: {e}");
    }

    if demo_dir.exists() {
        match std::fs::remove_dir_all(&demo_dir) {
            Ok(()) => info!("Removed legacy demo media directory"),
            Err(e) => log::warn!("Could not remove demo media directory: {}", e),
        }
    }

    // Record that the purge ran, so later launches do not repeat it. Failing
    // to record it only means trying again next time.
    if let Err(e) = settings.set_value(KEY, "1") {
        log::warn!("Could not record the legacy demo purge: {e}");
    }
}

/// The smallest normal window. The responsive shell collapses the sidebar to
/// an icon rail and turns the queue drawer into an overlay before this point;
/// the mini player has its own smaller floor.
const MIN_WINDOW_WIDTH: f32 = 640.0;
const MIN_WINDOW_HEIGHT: f32 = 360.0;

/// The application's own icon, in the form the window manager asks for.
///
/// The PNG is committed next to `icon.svg` and generated from it by
/// `assets/make-icons.sh`. Decoding it here — rather than relying on a
/// framework helper — keeps the icon identical on every platform, including
/// the ones whose shells read the icon from the executable's resources
/// instead of the window.
fn window_icon() -> Option<eframe::egui::IconData> {
    const ICON_PNG: &[u8] = include_bytes!("../assets/icon-256.png");
    let decoded = image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(eframe::egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

/// The log file's path inside the data directory.
fn log_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("shadowplayer.log")
}

/// A writer that sends every log line to stderr *and* to the log file.
///
/// The file is the point: on Windows the app is built as a windowed
/// subsystem binary, so a launch from the Explorer has no console attached and
/// stderr goes nowhere. The file is where a user (or a bug report) finds out
/// what happened.
struct TeeLog {
    stderr: std::io::Stderr,
    file: Option<std::fs::File>,
}

impl std::io::Write for TeeLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.stderr.write(buf)?;
        if let Some(file) = self.file.as_mut() {
            // A failing log file must never take the app down.
            let _ = file.write_all(buf);
            let _ = file.flush();
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stderr.flush()
    }
}

/// The entry point.
///
/// On Windows this is a *windowed* subsystem binary (see the crate-level
/// attribute at the top of this file), so launching it from the Explorer does
/// not flash a console window. The trade-off is that a launch failure has
/// nowhere to print to, which is why the error is written to the log file in
/// the data directory and, on Windows, shown in a message box.
fn main() {
    if let Err(e) = run() {
        let message = format!("ShadowPlayer failed to start:\n\n{e}");
        log::error!("{message}");
        eprintln!("{message}");
        show_fatal_error(&message);
        std::process::exit(1);
    }
}

/// Tell the user, in the platform's own way, that the app could not start.
///
/// On Windows the log file is invisible to a user who has never opened it, so
/// a message box is the difference between "it did nothing" and an actionable
/// sentence. Everywhere else, stderr is enough.
fn show_fatal_error(message: &str) {
    #[cfg(windows)]
    {
        // `MessageBoxW` via the shell's own API, so no extra dependency is
        // needed for a dialog that is shown exactly once.
        unsafe {
            #[link(name = "user32")]
            extern "system" {
                fn MessageBoxW(
                    hwnd: isize,
                    text: *const u16,
                    caption: *const u16,
                    kind: u32,
                ) -> i32;
            }
            let wide =
                |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
            let text = wide(message);
            let caption = wide("ShadowPlayer");
            MessageBoxW(0, text.as_ptr(), caption.as_ptr(), 0x10);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = message;
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Resolve persistent app data directory. The environment override keeps
    // tests and portable runs off the real user profile; both the app and
    // `shadowplayer-remote` honour it.
    let data_dir = std::env::var_os("SHADOWPLAYER_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::data_local_dir().map(|d| d.join("shadowplayer")))
        .unwrap_or_else(|| PathBuf::from("."));

    // Initialize logging: stderr plus a file in the data directory.
    {
        std::fs::create_dir_all(&data_dir).ok();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path(&data_dir))
            .ok();
        let tee = Box::new(TeeLog {
            stderr: std::io::stderr(),
            file,
        });
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .target(env_logger::Target::Pipe(tee))
            .init();
    }

    info!("Starting ShadowPlayer - Audiophile Local Desktop Music Player");

    // The data directory was resolved above, before the logger was built, so
    // the log file could live inside it.
    std::fs::create_dir_all(&data_dir)?;

    // Decide ownership before opening the database or the audio device. The
    // opt-in is a marker file precisely so this decision does not require the
    // settings database, which a second launch must not touch yet.
    let allow_multiple = ui::services::multi_instance_enabled(&data_dir);
    // Take the lock even when multiple instances are allowed. The first
    // window then owns the control socket, and a later window that starts
    // after the opt-in is turned off still sees the lock and activates that
    // owner instead of racing it as a second transport.
    let instance_lock = match InstanceLock::acquire(&data_dir)? {
        LockOutcome::Acquired(lock) => {
            info!("Single-instance lock held at {}", lock.path().display());
            Some(lock)
        }
        LockOutcome::AlreadyRunning if allow_multiple => {
            info!(
                "Multiple instances enabled; running alongside the owner without a control socket"
            );
            None
        }
        LockOutcome::AlreadyRunning => {
            info!("Another instance owns ShadowPlayer; activating it and exiting");
            ui::services::activate_existing_instance(&data_dir).map_err(|e| {
                std::io::Error::other(format!(
                    "another instance is running but did not accept activation: {e}"
                ))
            })?;
            return Ok(());
        }
    };

    let db_path = data_dir.join("library.db");
    info!("Opening database at {:?}", db_path);
    let db = Database::open(&db_path)?;

    // Strip any demo data left behind by earlier development builds.
    purge_legacy_demo_data(&db, &data_dir);

    // The user's own settings decide the engine profile and the window size.
    // This used to force `Fidelity` on every launch — silently overriding the
    // choice saved in Settings, and quietly changing a working setup for anyone
    // whose machine cannot afford the strict preset.
    let saved_settings = SettingsRepo::new(db.clone()).load().unwrap_or_default();

    // Keep the pre-database marker in step with the persisted preference. A
    // change takes effect on the next launch, which is the only time it can be
    // honoured before the database is opened.
    if saved_settings.allow_multiple_instances != allow_multiple {
        if let Err(e) = ui::services::set_multi_instance_enabled(
            &data_dir,
            saved_settings.allow_multiple_instances,
        ) {
            log::warn!("Could not update the multi-instance marker: {e}");
        }
    }

    // The owner advertises a loopback-only control socket. A multi-instance
    // process deliberately starts none: there is one transport owner, and
    // `shadowplayer-remote` must not race two advertised endpoints.
    let control_service = if instance_lock.is_some() {
        match ui::services::ControlService::start(&data_dir) {
            Ok(service) => Some(service),
            Err(e) => {
                log::warn!("Loopback control unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    // Initialize authoritative Audio Engine session
    //
    // Both versions are printed: the player and the engine are versioned
    // separately, so the log is where you check which pair a build actually
    // carries.
    info!(
        "Version: ShadowPlayer v{} · {} v{}",
        env!("CARGO_PKG_VERSION"),
        engine::NAME,
        engine::VERSION
    );
    info!(
        "Initializing {} v{} session...",
        engine::NAME,
        engine::VERSION
    );
    let session = Arc::new(AudioSession::new()?);

    // Apply the persisted profile
    session.apply_profile(saved_settings.audio_profile);

    // Native window options. Development screenshot runs can choose a size
    // with `SHADOWPLAYER_EVAL_SIZE=640x360`.
    let (start_w, start_h) = {
        #[cfg(feature = "eval-tools")]
        {
            std::env::var("SHADOWPLAYER_EVAL_SIZE")
                .ok()
                .and_then(|raw| {
                    let (w, h) = raw.split_once('x')?;
                    Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
                })
                .unwrap_or((
                    saved_settings.window_width.max(MIN_WINDOW_WIDTH),
                    saved_settings.window_height.max(MIN_WINDOW_HEIGHT),
                ))
        }
        #[cfg(not(feature = "eval-tools"))]
        {
            (
                saved_settings.window_width.max(MIN_WINDOW_WIDTH),
                saved_settings.window_height.max(MIN_WINDOW_HEIGHT),
            )
        }
    };
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ShadowPlayer")
            .with_inner_size(Vec2::new(start_w, start_h))
            .with_min_inner_size(Vec2::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT))
            .with_decorations(true)
            .with_transparent(false)
            // The app's own mark, on the taskbar, in the title bar and in the
            // window manager's switcher.
            .with_icon(window_icon().unwrap_or_default()),
        ..Default::default()
    };

    let session_clone = session.clone();
    let db_clone = db.clone();
    let app_data_dir = data_dir.clone();

    eframe::run_native(
        "ShadowPlayer",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(ShadowPlayerApp::new(
                cc,
                session_clone,
                db_clone,
                control_service,
                app_data_dir,
            )))
        }),
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(id: domain::ids::TrackId, path: &std::path::Path) -> domain::models::Track {
        domain::models::Track {
            id,
            path: path.to_path_buf(),
            title: "Song".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            genre: None,
            year: None,
            track_number: None,
            disc_number: None,
            duration_secs: 60.0,
            bpm: None,
            sample_rate: 44_100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
            bit_depth: None,
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            replaygain_album_db: None,
            replaygain_album_peak: None,
            r128_loudness_lufs: None,
            file_size_bytes: 1_000,
            mtime_unix_secs: 1_700_000_000,
            mtime_nanos: 0,
            content_hash: None,
            file_id: None,
            is_favorite: false,
            rating: 0,
            has_lyrics: false,
            play_count: 0,
            skip_count: 0,
            last_played_at: None,
            added_at: 1_700_000_000,
        }
    }

    /// The purge removes what it claims to remove and nothing else: only rows
    /// under the demo directory, and it runs once.
    #[test]
    fn the_legacy_demo_purge_removes_only_the_demo_library() {
        let data_dir = std::env::temp_dir().join(format!(
            "sp_app_purge_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let demo_dir = data_dir.join("demo_sample_music");
        std::fs::create_dir_all(&demo_dir).unwrap();
        std::fs::write(demo_dir.join("demo.flac"), b"demo").unwrap();

        let db = Database::in_memory().unwrap();
        let tracks = TrackRepo::new(db.clone());
        let demo_id = domain::ids::TrackId::new();
        tracks
            .upsert(&sample_track(demo_id, &demo_dir.join("demo.flac")))
            .unwrap();
        let real_id = domain::ids::TrackId::new();
        let real_path = data_dir.join("my music").join("real.flac");
        std::fs::create_dir_all(real_path.parent().unwrap()).unwrap();
        tracks.upsert(&sample_track(real_id, &real_path)).unwrap();

        purge_legacy_demo_data(&db, &data_dir);

        assert!(
            tracks.get_by_id(&demo_id).unwrap().is_none(),
            "demo row gone"
        );
        assert!(
            tracks.get_by_id(&real_id).unwrap().is_some(),
            "real row kept"
        );
        assert!(!demo_dir.exists(), "demo directory removed");

        // A second run is a no-op rather than a second attempt.
        purge_legacy_demo_data(&db, &data_dir);
        assert!(tracks.get_by_id(&real_id).unwrap().is_some());

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

use std::path::PathBuf;
use std::sync::Arc;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting ShadowPlayer - Audiophile Local Desktop Music Player");

    // Resolve persistent app data directory
    let data_dir = dirs::data_local_dir()
        .map(|d| d.join("shadowplayer"))
        .unwrap_or_else(|| PathBuf::from("./data"));
    std::fs::create_dir_all(&data_dir)?;

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
            .with_transparent(false),
        ..Default::default()
    };

    let session_clone = session.clone();
    let db_clone = db.clone();

    eframe::run_native(
        "ShadowPlayer",
        native_options,
        Box::new(move |cc| Ok(Box::new(ShadowPlayerApp::new(cc, session_clone, db_clone)))),
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
            sample_rate: 44_100,
            channels: 2,
            format: "FLAC".to_string(),
            bitrate_kbps: None,
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
            is_favorite: false,
            play_count: 0,
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

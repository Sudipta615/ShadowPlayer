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
    let _ = QueueRepo::new(db.clone()).purge_path_prefix(&demo_dir);

    if demo_dir.exists() {
        match std::fs::remove_dir_all(&demo_dir) {
            Ok(()) => info!("Removed legacy demo media directory"),
            Err(e) => log::warn!("Could not remove demo media directory: {}", e),
        }
    }

    let _ = settings.set_value(KEY, "1");
}

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
    info!(
        "Initializing {} v{} session...",
        engine::NAME,
        engine::VERSION
    );
    let session = Arc::new(AudioSession::new()?);

    // Apply the persisted profile
    session.apply_profile(saved_settings.audio_profile);

    // Native window options
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ShadowPlayer")
            .with_inner_size(Vec2::new(
                saved_settings.window_width.max(1024.0),
                saved_settings.window_height.max(682.0),
            ))
            .with_min_inner_size(Vec2::new(1024.0, 682.0))
            .with_decorations(true)
            .with_transparent(false),
        ..Default::default()
    };

    let session_clone = session.clone();
    let db_clone = db.clone();

    eframe::run_native(
        "ShadowPlayer",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(ShadowPlayerApp::new(cc, session_clone, db_clone)))
        }),
    )?;

    Ok(())
}

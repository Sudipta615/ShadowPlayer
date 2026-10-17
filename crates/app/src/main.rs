use std::path::PathBuf;
use std::sync::Arc;

use audio_session::AudioSession;
use database::Database;
use eframe::egui::Vec2;
use log::info;
use ui::ShadowPlayerApp;

mod demo_seed;

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

    // Seed development demo data if database is empty
    if let Err(e) = demo_seed::seed_demo_data_if_empty(&db, &data_dir) {
        log::warn!("Demo seeding encountered notice: {}", e);
    }

    // Initialize authoritative Audio Engine session
    info!("Initializing Audio Engine v1.2.1 session...");
    let session = Arc::new(AudioSession::new()?);

    // Apply initial profile
    session.apply_profile(domain::models::AudioProfileChoice::Fidelity);

    // Native window options
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ShadowPlayer")
            .with_inner_size(Vec2::new(1280.0, 830.0))
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

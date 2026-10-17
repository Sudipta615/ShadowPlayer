use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use database::{Database, FavoritesRepo, QueueRepo, TrackRepo};
use domain::ids::TrackId;
use domain::models::Track;
use log::info;

/// Development seed data flag. Can be set to false or deleted easily.
pub const ENABLE_DEV_DEMO_SEED: bool = true;

struct DemoTrackDef {
    title: &'static str,
    artist: &'static str,
    album: &'static str,
    duration_secs: f32,
    freq: f32,
    is_favorite: bool,
}

const DEMO_TRACKS: &[DemoTrackDef] = &[
    DemoTrackDef {
        title: "Midnight Dreams",
        artist: "Horizon Lines",
        album: "Lost in Reverie",
        duration_secs: 238.0, // 3:58
        freq: 220.0,
        is_favorite: true,
    },
    DemoTrackDef {
        title: "Echoes",
        artist: "Aurora Skies",
        album: "Waves of Time",
        duration_secs: 261.0, // 4:21
        freq: 246.94,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Starlight",
        artist: "Nova Bloom",
        album: "Starlight EP",
        duration_secs: 227.0, // 3:47
        freq: 261.63,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Endless Road",
        artist: "The Wanderers",
        album: "Paths",
        duration_secs: 302.0, // 5:02
        freq: 293.66,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Breathe Again",
        artist: "Luna Track",
        album: "Moments",
        duration_secs: 250.0, // 4:10
        freq: 329.63,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Sailing Home",
        artist: "Ocean Avenue",
        album: "Tides",
        duration_secs: 275.0, // 4:35
        freq: 349.23,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Letting Go",
        artist: "Paper Kites",
        album: "On the Corner",
        duration_secs: 213.0, // 3:33
        freq: 392.00,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Golden Hours",
        artist: "Sunset Kids",
        album: "Chapter One",
        duration_secs: 258.0, // 4:18
        freq: 440.00,
        is_favorite: false,
    },
    DemoTrackDef {
        title: "Night Drive",
        artist: "Coastal Run",
        album: "Neon Highway",
        duration_secs: 239.0, // 3:59
        freq: 196.00,
        is_favorite: false,
    },
];

const MIDNIGHT_DREAMS_LRC: &str = r#"[00:00.00]Midnight dreams beneath the silver moon
[00:15.00]The silent night begins to fade so soon
[00:32.00]Horizon lines across the purple skies
[00:54.00]I see the starlight shining in your eyes
[01:14.00]Lost in reverie, where thoughts run free
[01:24.00]Drifting through the calm and open sea
[01:45.00]Every whisper carries through the night
[02:10.00]Guided by the glowing neon light
[02:40.00]Midnight dreams that never fade away
[03:20.00]Until the sunrise of another day
"#;

pub fn seed_demo_data_if_empty(db: &Database, app_data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !ENABLE_DEV_DEMO_SEED {
        return Ok(());
    }

    let track_repo = TrackRepo::new(db.clone());
    let fav_repo = FavoritesRepo::new(db.clone());
    let queue_repo = QueueRepo::new(db.clone());

    if track_repo.count()? > 0 {
        // DB is already populated, but verify audio files are full-length for seeking
        let demo_music_dir = app_data_dir.join("demo_sample_music");
        if demo_music_dir.exists() {
            for def in DEMO_TRACKS.iter() {
                let safe_name = def.title.to_lowercase().replace(' ', "_");
                let wav_path = demo_music_dir.join(format!("{}.wav", safe_name));
                let expected_min_bytes = (44100.0 * def.duration_secs * 4.0) as u64;
                let needs_wav = match wav_path.metadata() {
                    Ok(m) => m.len() < expected_min_bytes,
                    Err(_) => true,
                };
                if needs_wav {
                    let _ = write_synthetic_ambient_wav(&wav_path, def.freq, def.duration_secs);
                }
            }
        }
        return Ok(());
    }

    info!("Seeding ShadowPlayer development demo tracks matching mockup UI...");

    let demo_music_dir = app_data_dir.join("demo_sample_music");
    fs::create_dir_all(&demo_music_dir)?;

    let mut seeded_tracks = Vec::new();

    for (idx, def) in DEMO_TRACKS.iter().enumerate() {
        let safe_name = def.title.to_lowercase().replace(' ', "_");
        let wav_path = demo_music_dir.join(format!("{}.wav", safe_name));

        // Generate synthetic ambient audio matching track duration (so seeking to 1:24 does not hit EOF)
        let expected_min_bytes = (44100.0 * def.duration_secs * 4.0) as u64;
        let needs_wav = match wav_path.metadata() {
            Ok(m) => m.len() < expected_min_bytes,
            Err(_) => true,
        };
        if needs_wav {
            write_synthetic_ambient_wav(&wav_path, def.freq, def.duration_secs)?;
        }

        // Generate matching artwork PNG
        let art_path = demo_music_dir.join(format!("{}.png", safe_name));
        let _ = generate_track_artwork(&art_path, def.title);

        // Sidecar LRC for Midnight Dreams
        if def.title == "Midnight Dreams" {
            let lrc_path = demo_music_dir.join(format!("{}.lrc", safe_name));
            if !lrc_path.exists() {
                fs::write(lrc_path, MIDNIGHT_DREAMS_LRC)?;
            }
        }

        let id = TrackId::from_canonical_path(&wav_path);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let track = Track {
            id,
            path: wav_path,
            title: def.title.to_string(),
            artist: def.artist.to_string(),
            album: def.album.to_string(),
            genre: Some("Synthwave / Ambient".into()),
            year: Some(2026),
            track_number: Some((idx + 1) as u32),
            disc_number: Some(1),
            duration_secs: def.duration_secs,
            sample_rate: 44100,
            channels: 2,
            format: "wav".into(),
            bitrate_kbps: Some(1411),
            is_lossless: true,
            replaygain_track_db: None,
            replaygain_track_peak: None,
            r128_loudness_lufs: Some(-14.0),
            file_size_bytes: 44100 * 2 * 2 * 5,
            mtime_unix_secs: now,
            is_favorite: def.is_favorite,
            play_count: if def.title == "Midnight Dreams" { 14 } else { 2 },
            last_played_at: Some(now),
            added_at: now + idx as i64,
        };

        track_repo.upsert(&track)?;

        if def.is_favorite {
            fav_repo.add(&track.id)?;
        }

        seeded_tracks.push(track);
    }

    // Populate queue with demo tracks
    let queue_entries: Vec<_> = seeded_tracks.iter().map(|t| (t.id, t.path.clone())).collect();
    let _ = queue_repo.save_queue(&queue_entries);

    info!("Successfully seeded {} demo tracks!", seeded_tracks.len());
    Ok(())
}

/// Easily delete all development demo tracks and files when ready for production.
#[allow(dead_code)]
pub fn remove_demo_data(db: &Database, app_data_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let demo_music_dir = app_data_dir.join("demo_sample_music");
    if demo_music_dir.exists() {
        let _ = fs::remove_dir_all(&demo_music_dir);
    }

    let track_repo = TrackRepo::new(db.clone());
    let fav_repo = FavoritesRepo::new(db.clone());
    let queue_repo = QueueRepo::new(db.clone());
    let all = track_repo.list(domain::models::SortField::Title, domain::models::SortOrder::Ascending)?;
    for t in all {
        let _ = fav_repo.remove(&t.id);
        let _ = track_repo.delete(&t.id);
    }
    let _ = queue_repo.save_queue(&[]);

    info!("All demo seed data removed successfully.");
    Ok(())
}

/// Generates a valid stereo 16-bit 44.1kHz PCM WAV file with gentle soothing harmonic tones
fn write_synthetic_ambient_wav(path: &Path, base_freq: f32, duration_secs: f32) -> Result<(), Box<dyn std::error::Error>> {
    let sample_rate = 44100u32;
    let num_channels = 2u16;
    let bits_per_sample = 16u16;
    let total_secs = duration_secs.ceil() as usize;
    let num_samples = sample_rate as usize * total_secs;
    let data_chunk_size = (num_samples * num_channels as usize * (bits_per_sample / 8) as usize) as u32;

    let file = File::create(path)?;
    let mut writer = BufWriter::with_capacity(256 * 1024, file);

    // RIFF header
    writer.write_all(b"RIFF")?;
    writer.write_all(&(36u32 + data_chunk_size).to_le_bytes())?;
    writer.write_all(b"WAVE")?;

    // fmt subchunk
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?; // PCM format
    writer.write_all(&num_channels.to_le_bytes())?;
    writer.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * num_channels as u32 * (bits_per_sample / 8) as u32;
    writer.write_all(&byte_rate.to_le_bytes())?;
    let block_align = num_channels * (bits_per_sample / 8);
    writer.write_all(&block_align.to_le_bytes())?;
    writer.write_all(&bits_per_sample.to_le_bytes())?;

    // data subchunk
    writer.write_all(b"data")?;
    writer.write_all(&data_chunk_size.to_le_bytes())?;

    // Precalculate 1 second of smooth ambient audio
    let two_pi = std::f32::consts::PI * 2.0;
    let mut one_sec = Vec::with_capacity(sample_rate as usize * 4);
    for i in 0..sample_rate {
        let t = i as f32 / sample_rate as f32;
        let s1 = (base_freq * two_pi * t).sin() * 0.4;
        let s2 = (base_freq * 1.5 * two_pi * t).sin() * 0.25;
        let s3 = (base_freq * 2.0 * two_pi * t).sin() * 0.15;
        let sample_f = s1 + s2 + s3;

        let sample_i16 = (sample_f * 24000.0).clamp(-32767.0, 32767.0) as i16;
        let [b0, b1] = sample_i16.to_le_bytes();

        // Stereo Left & Right
        one_sec.extend_from_slice(&[b0, b1, b0, b1]);
    }

    for _ in 0..total_secs {
        writer.write_all(&one_sec)?;
    }

    writer.flush()?;
    Ok(())
}

fn generate_track_artwork(path: &Path, title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let embedded: Option<&'static [u8]> = match title {
        "Midnight Dreams" => Some(include_bytes!("../assets/demo_artwork/midnight_dreams.png")),
        "Echoes" => Some(include_bytes!("../assets/demo_artwork/echoes.png")),
        "Starlight" => Some(include_bytes!("../assets/demo_artwork/starlight.png")),
        "Endless Road" => Some(include_bytes!("../assets/demo_artwork/endless_road.png")),
        "Breathe Again" => Some(include_bytes!("../assets/demo_artwork/breathe_again.png")),
        "Sailing Home" => Some(include_bytes!("../assets/demo_artwork/sailing_home.png")),
        "Letting Go" => Some(include_bytes!("../assets/demo_artwork/letting_go.png")),
        "Golden Hours" => Some(include_bytes!("../assets/demo_artwork/golden_hours.png")),
        "Night Drive" => Some(include_bytes!("../assets/demo_artwork/night_drive.png")),
        _ => None,
    };

    if let Some(bytes) = embedded {
        fs::write(path, bytes)?;
        return Ok(());
    }

    let width = 300;
    let height = 300;
    let img = image::RgbaImage::from_fn(width, height, |x, y| {
        let nx = x as f32 / width as f32;
        let ny = y as f32 / height as f32;
        let dist_center = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt();

        match title {
            "Midnight Dreams" => {
                // Giant glowing surreal moon with crater marbling over purple mountain lake
                let moon_c = (0.50_f32, 0.34_f32);
                let d_moon = ((nx - moon_c.0).powi(2) + (ny - moon_c.1).powi(2)).sqrt();

                let m_ridge = 0.54 + 0.08 * (nx * 14.0).sin().abs() + 0.04 * (nx * 26.0).cos();
                let is_mountain = ny > m_ridge && ny < 0.65;
                let is_water = ny >= 0.65;

                if is_mountain {
                    // Deep violet mountain silhouette with glowing pink crest
                    let crest = ((ny - m_ridge) / 0.03).clamp(0.0, 1.0);
                    let mr = (140.0 * (1.0 - crest) + 25.0 * crest) as u8;
                    let mg = (30.0 * (1.0 - crest) + 12.0 * crest) as u8;
                    let mb = (160.0 * (1.0 - crest) + 45.0 * crest) as u8;
                    image::Rgba([mr, mg, mb, 255])
                } else if is_water {
                    // Dark lake water with shimmering magenta moon reflection
                    let shimmer_dist = (nx - moon_c.0).abs();
                    let wave = ((ny * 90.0).sin() * 0.5 + 0.5) * ((1.0 - (shimmer_dist / 0.28)).clamp(0.0, 1.0));
                    let r = (25.0 + 175.0 * wave) as u8;
                    let g = (10.0 + 40.0 * wave) as u8;
                    let b = (50.0 + 150.0 * wave + 30.0 * (1.0 - ny)) as u8;
                    image::Rgba([r, g, b, 255])
                } else if d_moon < 0.23 {
                    // Luminous cratered moon
                    let edge = (d_moon / 0.23).clamp(0.0, 1.0);
                    let crater = ((nx * 22.0).sin() * (ny * 22.0).cos() * 0.12).clamp(-0.1, 0.1);
                    let glow = (1.0 - edge + crater).clamp(0.0, 1.0);
                    let r = (255.0 * glow + 180.0 * (1.0 - glow)) as u8;
                    let g = (195.0 * glow + 60.0 * (1.0 - glow)) as u8;
                    let b = (235.0 * glow + 140.0 * (1.0 - glow)) as u8;
                    image::Rgba([r, g, b, 255])
                } else {
                    // Sky gradient: indigo into glowing magenta horizon
                    let sky_t = (ny / 0.55).clamp(0.0, 1.0);
                    let r = (20.0 + 160.0 * sky_t.powf(2.0)) as u8;
                    let g = (8.0 + 30.0 * sky_t) as u8;
                    let b = (65.0 + 80.0 * (1.0 - sky_t) + 60.0 * sky_t) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            "Echoes" => {
                // Aurora borealis waves over turquoise ocean
                let wave_y = 0.40 + 0.12 * (nx * 6.0).sin() * (nx * 3.0).cos();
                let dist_aurora = (ny - wave_y).abs();
                let aurora_glow = (-dist_aurora * 14.0).exp();

                let is_water = ny > 0.65;
                if is_water {
                    let r = (15.0 + 30.0 * nx) as u8;
                    let g = (45.0 + 110.0 * (1.0 - ny) * (nx * 20.0).sin().abs()) as u8;
                    let b = (95.0 + 120.0 * (1.0 - ny)) as u8;
                    image::Rgba([r, g, b, 255])
                } else {
                    let r = (10.0 + 30.0 * aurora_glow) as u8;
                    let g = (25.0 + 200.0 * aurora_glow) as u8;
                    let b = (70.0 + 140.0 * (1.0 - ny) + 80.0 * aurora_glow) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            "Starlight" => {
                // Blazing celestial starburst explosion with cross flares
                let flare_x = (-((ny - 0.5).abs() * 26.0)).exp() * (-((nx - 0.5).powi(2) / 0.12)).exp();
                let flare_y = (-((nx - 0.5).abs() * 26.0)).exp() * (-((ny - 0.5).powi(2) / 0.12)).exp();
                let diag = (-((nx - ny).abs() * 32.0)).exp() * 0.4;
                let core = (-(dist_center * 8.0)).exp();
                let total = (core * 1.5 + flare_x * 0.9 + flare_y * 0.9 + diag).clamp(0.0, 1.0);

                let r = (255.0 * total + 18.0 * (1.0 - total)) as u8;
                let g = (140.0 * total.powi(2) + 5.0) as u8;
                let b = (40.0 * total.powi(3) + 15.0) as u8;
                image::Rgba([r, g, b, 255])
            }
            "Endless Road" => {
                // Highway disappearing into night horizon
                let road_center = 0.5;
                let road_half_w = 0.04 + 0.36 * (ny - 0.52).max(0.0);
                let in_road = ny >= 0.52 && (nx - road_center).abs() < road_half_w;

                if in_road {
                    let is_center_line = (nx - road_center).abs() < 0.015 && ((ny * 40.0) as i32 % 4 < 2);
                    if is_center_line {
                        image::Rgba([255, 230, 80, 255])
                    } else {
                        let r = (35.0 + 20.0 * ny) as u8;
                        let g = (25.0 + 20.0 * ny) as u8;
                        let b = (55.0 + 40.0 * ny) as u8;
                        image::Rgba([r, g, b, 255])
                    }
                } else if ny >= 0.52 {
                    // Dark roadside terrain
                    image::Rgba([18, 15, 32, 255])
                } else {
                    // Deep indigo twilight sky with pink horizon
                    let horizon_t = ((ny - 0.2) / 0.32).clamp(0.0, 1.0);
                    let r = (20.0 + 130.0 * horizon_t) as u8;
                    let g = (15.0 + 40.0 * horizon_t) as u8;
                    let b = (60.0 + 80.0 * (1.0 - horizon_t)) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            "Breathe Again" => {
                // Serene pastel teal with glowing golden sunrise
                let sun_c = (0.50_f32, 0.55_f32);
                let d_sun = ((nx - sun_c.0).powi(2) + (ny - sun_c.1).powi(2)).sqrt();
                let sun_glow = (-d_sun * 5.0).exp();

                let r = (70.0 + 180.0 * sun_glow + 30.0 * (1.0 - ny)) as u8;
                let g = (120.0 + 110.0 * sun_glow + 50.0 * nx) as u8;
                let b = (170.0 + 40.0 * (1.0 - sun_glow)) as u8;
                image::Rgba([r, g, b, 255])
            }
            "Sailing Home" => {
                // Turquoise tropical ocean with gleaming sun flare
                let is_sea = ny > 0.60;
                if is_sea {
                    let wave = ((nx * 25.0 + ny * 15.0).sin() * 0.5 + 0.5) * 0.2;
                    let r = (10.0 + 40.0 * wave) as u8;
                    let g = (150.0 + 60.0 * wave + 30.0 * (1.0 - ny)) as u8;
                    let b = (190.0 + 50.0 * (1.0 - ny)) as u8;
                    image::Rgba([r, g, b, 255])
                } else {
                    let d_flare = ((nx - 0.5).powi(2) + (ny - 0.6).powi(2)).sqrt();
                    let flare = (-d_flare * 7.0).exp();
                    let r = (30.0 + 225.0 * flare) as u8;
                    let g = (160.0 + 95.0 * flare) as u8;
                    let b = (220.0 + 35.0 * flare) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            "Letting Go" => {
                // Magenta and fuchsia cloudscape
                let cloud = ((nx * 8.0).sin() * (ny * 8.0).cos() * 0.5 + 0.5) * 0.3;
                let grad = (ny + cloud).clamp(0.0, 1.0);
                let r = (230.0 - 40.0 * grad) as u8;
                let g = (40.0 + 90.0 * grad) as u8;
                let b = (140.0 + 80.0 * (1.0 - grad)) as u8;
                image::Rgba([r, g, b, 255])
            }
            "Golden Hours" => {
                // Warm golden amber desert dunes and sun rays
                let sun_glow = (-(((nx - 0.5).powi(2) + (ny - 0.7).powi(2)).sqrt() * 4.5)).exp();
                let dune = 0.68 + 0.08 * (nx * 4.0).sin();
                let in_dune = ny > dune;

                if in_dune {
                    let r = (180.0 - 30.0 * nx) as u8;
                    let g = (90.0 - 20.0 * nx) as u8;
                    let b = (30.0) as u8;
                    image::Rgba([r, g, b, 255])
                } else {
                    let r = (250.0) as u8;
                    let g = (140.0 + 90.0 * sun_glow) as u8;
                    let b = (30.0 + 70.0 * (1.0 - ny)) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            "Night Drive" => {
                // Cyberpunk electric cyan perspective grid lines
                let is_floor = ny > 0.52;
                if is_floor {
                    let z = (ny - 0.52) / 0.48;
                    let grid_x = ((nx - 0.5) / z * 8.0).fract().abs();
                    let grid_y = (1.0 / z * 2.0).fract().abs();
                    let is_line = grid_x < 0.08 || grid_y < 0.08;
                    if is_line {
                        image::Rgba([0, 240, 255, 255])
                    } else {
                        image::Rgba([18, 14, 40, 255])
                    }
                } else {
                    let r = (20.0 + 50.0 * ny) as u8;
                    let g = (10.0 + 20.0 * ny) as u8;
                    let b = (60.0 + 40.0 * ny) as u8;
                    image::Rgba([r, g, b, 255])
                }
            }
            _ => {
                let r = (50.0 + 150.0 * nx) as u8;
                let g = (50.0 + 100.0 * ny) as u8;
                let b = (150.0 + 100.0 * (1.0 - nx)) as u8;
                image::Rgba([r, g, b, 255])
            }
        }
    });
    img.save(path)?;
    Ok(())
}

use super::*;

/// Fixture titles for the synthetic library rows.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_TITLES: [&str; 6] = [
    "Weightless",
    "An Ending (Ascent)",
    "Nightcall",
    "Teardrop",
    "Xtal",
    "Halcyon Drift",
];
/// Fixture artists.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_ARTISTS: [&str; 3] = ["Marconi Union", "Neon Harbour", "Aurelia Vox"];
/// Fixture albums.
#[cfg(feature = "eval-tools")]
const EVAL_TRACK_ALBUMS: [&str; 2] = ["Ambient Transmissions", "Low Orbit"];
/// Fixture sample rates, so the Hi-Res tag has something to mark.
#[cfg(feature = "eval-tools")]
const EVAL_SAMPLE_RATES: [u32; 4] = [44100, 48000, 96000, 192000];

impl ShadowPlayerApp {
    pub(super) fn eval_hook(&mut self, ctx: &egui::Context) {
        // 4. Automated Evaluation Screenshot Hook.
        //
        // Development only: the whole block (and the CLAP eval script it
        // drives) is compiled out unless the `eval-tools` feature is on, so a
        // shipped binary has no `process::exit` reachable from an environment
        // variable.
        self.app_state.frame_count += 1;
        #[cfg(not(feature = "eval-tools"))]
        let _ = ctx;
        #[cfg(feature = "eval-tools")]
        if std::env::var("SHADOWPLAYER_EVAL").is_ok() {
            // Screenshot runs render as fast as the CPU allows, so egui's
            // wall-clock animations (smooth scrolling, the jump flash) barely
            // advance by the capture frame. A small sleep makes a screenshot
            // show the settled state.
            if std::env::var("SHADOWPLAYER_EVAL_SLOW").is_ok() && self.app_state.frame_count < 30 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_MINI").is_ok() {
                self.app_state.mini_player = true;
            }
            if self.app_state.frame_count == 1
                && std::env::var("SHADOWPLAYER_EVAL_SETTINGS").is_ok()
            {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Settings;
                self.dsp_state.eq_panel.is_open = false;
                if let Ok(cat_str) = std::env::var("SHADOWPLAYER_EVAL_SETTINGS_CAT") {
                    match cat_str.as_str() {
                        "spatial" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Spatial
                        }
                        "playback" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Playback
                        }
                        "output" | "hardware" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Output
                        }
                        "library" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Library
                        }
                        "mixbus" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::MixBus
                        }
                        "tools" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Tools
                        }
                        "appearance" | "theme" | "themes" => {
                            self.dsp_state.settings_tab.active_category =
                                crate::components::settings_tab::SettingsCategory::Appearance
                        }
                        _ => {}
                    }
                }
                if std::env::var("SHADOWPLAYER_EVAL_BASS").is_ok() {
                    {
                        let st = &mut self.dsp_state.settings_tab;
                        st.bass_management.enabled = true;
                        st.lfe.enabled = true;
                        st.lfe.crossover_hz = Some(120.0);
                    }
                    let bass = self.dsp_state.settings_tab.bass_management.clone();
                    let lfe = self.dsp_state.settings_tab.lfe.clone();
                    let a = crate::components::settings_tab::SettingsTabActions {
                        bass_management_changed: Some(bass),
                        lfe_changed: Some(lfe),
                        ..Default::default()
                    };
                    self.dsp_state.handle_settings_actions(&self.session, &a);
                }
                if let Ok(query) = std::env::var("SHADOWPLAYER_EVAL_SETTINGS_SEARCH") {
                    self.dsp_state.settings_tab.search_query = query;
                }
                if let Ok(label) = std::env::var("SHADOWPLAYER_EVAL_SETTINGS_JUMP") {
                    if let Some(entry) = crate::components::settings_tab::SETTINGS_INDEX
                        .iter()
                        .find(|entry| entry.label == label)
                    {
                        self.dsp_state.settings_tab.active_category = entry.category;
                        crate::components::settings_tab::request_settings_jump(entry.label);
                    }
                }
            }

            // Chain presets: drive the real save/apply path and verify the whole
            // chain survives the round trip. `SHADOWPLAYER_EVAL_CHAIN=seed`
            // saves profiles and leaves them on screen; `=roundtrip` also wipes
            // the chain, recalls a profile and asserts every field came back.
            if self.app_state.frame_count == 1 {
                if let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_CHAIN") {
                    {
                        let eq = &mut self.dsp_state.eq_panel;
                        eq.eq_enabled = true;
                        eq.graphic_enabled = true;
                        eq.preamp_db = -7.5;
                        eq.bass_shelf_db = 4.0;
                        eq.limiter_enabled = true;
                        eq.crossfeed_enabled = true;
                        eq.crossfeed_profile = config::CrossfeedProfile::Jmeier;
                        eq.bands[2].freq = 315.0;
                        eq.bands[2].gain_db = -5.5;
                        eq.comp_threshold[0] = -28.0;
                        let st = &mut self.dsp_state.settings_tab;
                        st.bass_management.enabled = true;
                        st.bass_management.crossover_hz = 85.0;
                        st.lfe.enabled = true;
                        st.lfe.gain_db = -3.0;
                        st.channel_mix.enabled = true;
                    }

                    for (name, note) in [
                        ("Headphones", "HD650 + crossfeed"),
                        ("Speakers", "2.1 living room"),
                    ] {
                        self.dsp_state.settings_tab.chain_preset_draft_note = note.to_string();
                        let a = crate::components::settings_tab::SettingsTabActions {
                            chain_preset_save: Some(name.to_string()),
                            ..Default::default()
                        };
                        self.dsp_state.handle_settings_actions(&self.session, &a);
                    }

                    if mode == "roundtrip" {
                        // Flatten the chain, then recall the first profile.
                        {
                            let eq = &mut self.dsp_state.eq_panel;
                            eq.eq_enabled = false;
                            eq.graphic_enabled = false;
                            eq.preamp_db = 0.0;
                            eq.bass_shelf_db = 0.0;
                            eq.limiter_enabled = false;
                            eq.crossfeed_enabled = false;
                            eq.bands[2].gain_db = 0.0;
                            eq.comp_threshold[0] = 0.0;
                            let st = &mut self.dsp_state.settings_tab;
                            st.bass_management.enabled = false;
                            st.lfe.enabled = false;
                            st.channel_mix.enabled = false;
                        }
                        let a = crate::components::settings_tab::SettingsTabActions {
                            chain_preset_apply: Some(0),
                            ..Default::default()
                        };
                        self.dsp_state.handle_settings_actions(&self.session, &a);

                        let eq = &self.dsp_state.eq_panel;
                        let st = &self.dsp_state.settings_tab;
                        assert!(eq.eq_enabled, "eq_enabled lost");
                        assert!((eq.preamp_db + 7.5).abs() < 1e-6, "preamp lost");
                        assert!((eq.bass_shelf_db - 4.0).abs() < 1e-6, "bass shelf lost");
                        assert!(eq.limiter_enabled, "limiter lost");
                        assert!(eq.crossfeed_enabled, "crossfeed lost");
                        assert_eq!(eq.crossfeed_profile, config::CrossfeedProfile::Jmeier);
                        assert!((eq.bands[2].gain_db + 5.5).abs() < 1e-6, "EQ band lost");
                        assert!(
                            (eq.comp_threshold[0] + 28.0).abs() < 1e-6,
                            "compressor lost"
                        );
                        assert!(st.bass_management.enabled, "bass management lost");
                        assert!((st.bass_management.crossover_hz - 85.0).abs() < 1e-6);
                        assert!(st.lfe.enabled, "LFE lost");
                        assert!(st.channel_mix.enabled, "channel mix lost");
                        log::info!(
                            "CHAIN PRESET ROUND TRIP OK ({} saved): {}",
                            st.chain_presets.len(),
                            st.chain_presets[0].summary()
                        );
                    }
                }
            }

            // Batch organizer: drive the real preview -> apply -> relink path
            // with real files, so the app-level code (DB relink, queue repoint,
            // watcher suppression) is exercised, not just the library crate.
            // `preview` leaves the modal on screen; `apply` also moves and
            // verifies. Rows are created fresh so their ids are real UUIDs, the
            // same shape the scanner produces.
            if self.app_state.frame_count == 1 {
                if let Ok(mode) = std::env::var("SHADOWPLAYER_EVAL_ORGANIZE") {
                    let root = std::path::PathBuf::from("/tmp/opencode/sp_organize");
                    let incoming = root.join("incoming");
                    let library_dir = root.join("library");
                    // Start clean so a re-run sees the same empty destinations.
                    let _ = std::fs::remove_dir_all(&root);
                    std::fs::create_dir_all(&incoming).unwrap();
                    std::fs::create_dir_all(&library_dir).unwrap();

                    let seed = incoming.join("_seed.mp3");
                    std::fs::write(&seed, b"organizer fixture").unwrap();

                    const SPECS: [(&str, &str, &str, u32); 3] = [
                        ("Midnight Drive", "Nova Pulse", "Neon Horizon", 1),
                        ("Gravity Well", "Orbital Drift", "Aphelion", 3),
                        ("Cathedral of Rain", "Aurelia Vox", "Hymnal", 2),
                    ];
                    let mut scope = Vec::new();
                    for (i, (title, artist, album, track_no)) in SPECS.iter().enumerate() {
                        let copy = incoming.join(format!("src{i}.mp3"));
                        std::fs::copy(&seed, &copy).unwrap();
                        let track = domain::models::Track {
                            id: domain::ids::TrackId::new(),
                            path: copy,
                            title: title.to_string(),
                            artist: artist.to_string(),
                            album: album.to_string(),
                            genre: Some("Ambient".to_string()),
                            year: Some(1997),
                            track_number: Some(*track_no),
                            disc_number: None,
                            duration_secs: 200.0,
                            bpm: None,
                            sample_rate: 44100,
                            channels: 2,
                            format: "MP3".to_string(),
                            bitrate_kbps: Some(128),
                            bit_depth: None,
                            is_lossless: false,
                            replaygain_track_db: None,
                            replaygain_track_peak: None,
                            replaygain_album_db: None,
                            replaygain_album_peak: None,
                            r128_loudness_lufs: None,
                            file_size_bytes: 18,
                            mtime_unix_secs: 1,
                            mtime_nanos: 0,
                            content_hash: Some(format!("eval-hash-{i}")),
                            file_id: None,
                            is_favorite: false,
                            rating: 0,
                            has_lyrics: false,
                            play_count: 0,
                            skip_count: 0,
                            last_played_at: None,
                            added_at: 0,
                            sort_artist_folded: None,
                            sort_title_folded: None,
                            sort_album_folded: None,
                        };
                        self.track_repo.upsert(&track).unwrap();
                        scope.push(track);
                    }
                    std::fs::remove_file(&seed).unwrap();
                    self.reload_library();

                    // A play count must survive the move, so bump one.
                    self.track_repo.increment_play_count(&scope[0].id).unwrap();

                    // Occupy one destination so the plan has a real conflict.
                    let conflict = library_dir.join("Orbital Drift/Aphelion/03 - Gravity Well.mp3");
                    std::fs::create_dir_all(conflict.parent().unwrap()).unwrap();
                    std::fs::write(&conflict, b"existing").unwrap();

                    // Queue one so the path remap is exercised too.
                    self.coordinator.add_to_queue(scope[0].clone());

                    let rows_before = self.track_repo.count().unwrap();
                    self.open_organize(scope.clone(), format!("{mode} fixture"));
                    self.organize.set_destination(Some(library_dir.clone()));
                    self.organize.pattern = "{artist}/{album}/{track} - {title}".to_string();
                    self.organize.plan_dirty = true;
                    self.organize.rebuild_if_due();

                    log::info!(
                        "ORGANIZE PREVIEW: {} to move, {} conflict(s), {} unchanged",
                        self.organize.plan.pending_count(),
                        self.organize.plan.conflict_count(),
                        self.organize.plan.unchanged_count()
                    );

                    if mode == "apply" {
                        self.apply_organize_plan();

                        let mut moved = 0usize;
                        for track in &scope {
                            let row = self
                                .track_repo
                                .get_by_id(&track.id)
                                .unwrap()
                                .expect("row must survive a move");
                            assert_eq!(row.id, track.id, "identity must be preserved");
                            if row.path == track.path {
                                log::info!("  left in place: {}", track.title);
                                continue;
                            }
                            moved += 1;
                            assert!(
                                row.path.exists(),
                                "relinked path must exist: {}",
                                row.path.display()
                            );
                            assert!(!track.path.exists(), "old path should be gone");
                            assert!(row.path.starts_with(&library_dir));
                            log::info!("  moved: {} -> {}", track.title, row.path.display());
                        }
                        assert_eq!(moved, 2, "two move, the occupied destination is skipped");

                        let played = self
                            .track_repo
                            .get_by_id(&scope[0].id)
                            .unwrap()
                            .unwrap()
                            .play_count;
                        assert_eq!(played, 1, "play count must survive a move");

                        let queued = self
                            .coordinator
                            .queue()
                            .iter()
                            .find(|t| t.id == scope[0].id)
                            .expect("track is queued");
                        assert!(
                            queued.path.starts_with(&library_dir),
                            "queued path must be repointed: {}",
                            queued.path.display()
                        );

                        assert_eq!(
                            self.track_repo.count().unwrap(),
                            rows_before,
                            "a move must not duplicate rows"
                        );
                        log::info!(
                            "ORGANIZE APPLY OK: {moved} moved, {rows_before} rows, play count + queue path preserved"
                        );
                    }
                }
            }

            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SELECT").is_ok()
            {
                let n: usize = std::env::var("SHADOWPLAYER_EVAL_SELECT")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(3);
                for t in self.library_state.displayed_tracks.iter().take(n) {
                    self.library_state.selected_track_ids.insert(t.id);
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_SMART").is_ok() {
                self.smart_playlist_editor.open_new();
            }
            if self.app_state.frame_count == 1
                && std::env::var("SHADOWPLAYER_EVAL_DUPLICATES").is_ok()
            {
                self.scan_duplicates();
            }
            // Generic navigation / modal selectors used to capture screenshots
            // of individual views.
            if self.app_state.frame_count == 1 {
                use crate::components::sidebar::NavItem as Nav;
                if let Ok(nav) = std::env::var("SHADOWPLAYER_EVAL_NAV") {
                    self.app_state.active_nav = match nav.as_str() {
                        "albums" => Nav::Albums,
                        "artists" => Nav::Artists,
                        "genres" => Nav::Genres,
                        "folders" => Nav::Folders,
                        "favorites" => Nav::Favorites,
                        "recent" => Nav::RecentlyPlayed,
                        "most" => Nav::MostPlayed,
                        "added" => Nav::RecentlyAdded,
                        "settings" => Nav::Settings,
                        "help" => Nav::Help,
                        _ => Nav::Home,
                    };
                }
                match std::env::var("SHADOWPLAYER_EVAL_MODAL").as_deref() {
                    Ok("bitperfect") => self.app_state.bit_perfect_modal_open = true,
                    Ok("autoeq") => self.dsp_state.autoeq.is_open = true,
                    Ok("room_measurement") => self.room_measurement.open(),
                    Ok("listening") => self.app_state.listening_modal.open(),
                    Ok("scrobble") => self.app_state.scrobble_log_open = true,
                    Ok("track_dsp") => {
                        if let Some(track) = self.library_state.displayed_tracks.first().cloned() {
                            self.open_track_dsp(&track);
                        }
                    }
                    Ok("playlist") => {
                        let first = self.library_state.playlists.first().map(|p| p.id);
                        self.app_state.playlist_editor.open(first);
                    }
                    _ => {}
                }
                if let Ok(idx) = std::env::var("SHADOWPLAYER_EVAL_LYRICS") {
                    let n: usize = idx.parse().unwrap_or(0);
                    if let Some(track) = self.library_state.displayed_tracks.get(n).cloned() {
                        self.coordinator.play_track(track, None);
                    }
                    self.app_state.right_tab = crate::components::right_panel::RightTab::Lyrics;
                }
                if std::env::var("SHADOWPLAYER_EVAL_TOAST").is_ok() {
                    self.toasts.success(tr!(Msg::S943B3EFDC31F6DE7));
                    self.toasts.info(tr!(Msg::SB10EA0D7A7D70A46));
                }
                if let Ok(f) = std::env::var("SHADOWPLAYER_EVAL_FILTER") {
                    use crate::components::songs_table::QuickFilter as QF;
                    self.library_state.quick_filter = match f.as_str() {
                        "lossless" => QF::Lossless,
                        "hires" => QF::HiRes,
                        "favorites" => QF::Favorites,
                        "played" => QF::Played,
                        _ => QF::All,
                    };
                    self.refresh_view();
                }
            }
            // Synthetic library rows, so a screenshot run can show a
            // populated library: the song table with its quality tags, the
            // populated state of every view, and the loudness inspector.
            if self.app_state.frame_count == 1 {
                if let Ok(raw) = std::env::var("SHADOWPLAYER_EVAL_TRACKS") {
                    let count: usize = raw.parse().unwrap_or(0);
                    for i in 0..count {
                        let track = domain::models::Track {
                            id: domain::ids::TrackId::new(),
                            path: std::path::PathBuf::from(format!("/eval/track_{i}.flac")),
                            title: EVAL_TRACK_TITLES[i % EVAL_TRACK_TITLES.len()].to_string(),
                            artist: EVAL_TRACK_ARTISTS[i % EVAL_TRACK_ARTISTS.len()].to_string(),
                            album: EVAL_TRACK_ALBUMS[i % EVAL_TRACK_ALBUMS.len()].to_string(),
                            genre: Some("Ambient".to_string()),
                            year: Some(2019 + (i % 6) as u32),
                            track_number: Some((i % 6) as u32 + 1),
                            disc_number: None,
                            duration_secs: 180.0 + (i as f32) * 23.5,
                            bpm: None,
                            sample_rate: EVAL_SAMPLE_RATES[i % EVAL_SAMPLE_RATES.len()],
                            channels: 2,
                            format: if i % 4 == 3 { "MP3" } else { "FLAC" }.to_string(),
                            bitrate_kbps: Some(if i % 4 == 3 { 320 } else { 1000 }),
                            bit_depth: Some(if i % 4 == 3 { 16 } else { 24 }),
                            is_lossless: i % 4 != 3,
                            replaygain_track_db: Some(-6.0 - (i as f32) * 0.4),
                            replaygain_track_peak: Some(0.91),
                            replaygain_album_db: Some(-5.2),
                            replaygain_album_peak: Some(0.88),
                            r128_loudness_lufs: Some(-16.0 - (i as f32) * 0.7),
                            file_size_bytes: 48_000_000,
                            mtime_unix_secs: 1_600_000_000,
                            mtime_nanos: 0,
                            content_hash: None,
                            file_id: None,
                            is_favorite: i % 3 == 0,
                            rating: (i % 6) as u8,
                            has_lyrics: i % 5 == 0,
                            play_count: (i as u32) * 7,
                            skip_count: 0,
                            last_played_at: Some(1_700_000_000),
                            added_at: 1_600_000_000 + (i as i64) * 86_400,
                            sort_artist_folded: None,
                            sort_title_folded: None,
                            sort_album_folded: None,
                        };
                        let _ = self.track_repo.upsert(&track);
                    }
                    self.reload_library();
                }

                // Synthetic listening history, so the charts and aggregates
                // have something to show in a screenshot run. The rows go
                // through the real repository, so the SQL aggregation is
                // exercised rather than mocked.
                if let Ok(raw) = std::env::var("SHADOWPLAYER_EVAL_SCROBBLES") {
                    let count: usize = raw.parse().unwrap_or(0);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let artists = [
                        "Aphex Twin",
                        "Autechre",
                        "Boards of Canada",
                        "Burial",
                        "Four Tet",
                    ];
                    let albums = [
                        "Selected Ambient Works",
                        "Amber",
                        "Music Has the Right to Children",
                        "Untrue",
                        "Rounds",
                    ];
                    let titles = [
                        "Xtal",
                        "Roygbiv",
                        "Teardrop",
                        "Archangel",
                        "She Moves She",
                        "Windowlicker",
                        "Everything You Do Is a Balloon",
                        "Olson",
                    ];
                    for i in 0..count {
                        let slot = i % artists.len();
                        let track = domain::models::Track {
                            id: domain::ids::TrackId::new(),
                            path: std::path::PathBuf::from(format!("/eval/scrobble_{i}.flac")),
                            title: titles[i % titles.len()].to_string(),
                            artist: artists[slot].to_string(),
                            album: albums[slot].to_string(),
                            genre: Some("Electronic".to_string()),
                            year: Some(1998),
                            track_number: Some((i % 8) as u32 + 1),
                            disc_number: None,
                            duration_secs: 240.0,
                            bpm: None,
                            sample_rate: 44_100,
                            channels: 2,
                            format: "FLAC".to_string(),
                            bitrate_kbps: Some(900),
                            bit_depth: Some(16),
                            is_lossless: true,
                            replaygain_track_db: Some(-6.0),
                            replaygain_track_peak: Some(0.9),
                            replaygain_album_db: Some(-6.0),
                            replaygain_album_peak: Some(0.9),
                            r128_loudness_lufs: Some(-14.0),
                            file_size_bytes: 42_000_000,
                            mtime_unix_secs: 1_600_000_000,
                            mtime_nanos: 0,
                            content_hash: None,
                            file_id: None,
                            is_favorite: false,
                            rating: 0,
                            has_lyrics: false,
                            play_count: 0,
                            skip_count: 0,
                            last_played_at: None,
                            added_at: 1_600_000_000,
                            sort_artist_folded: None,
                            sort_title_folded: None,
                            sort_album_folded: None,
                        };
                        let _ = self.track_repo.upsert(&track);
                        let day_ago = (i % 20) as i64;
                        let hour = ((i * 7) % 24) as i64;
                        let played_at = now - day_ago * 86_400 - hour * 3_600;
                        let _ = self.scrobble_repo.record_at(
                            &track,
                            played_at,
                            150.0 + (i % 60) as f32,
                        );
                    }
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_FULL").is_ok() {
                self.app_state.now_playing_full = true;
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_STATS").is_ok() {
                self.open_library_stats();
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_IMPORT").is_ok()
            {
                self.open_import_playlists();
            }
            if self.app_state.frame_count == 1
                && std::env::var("SHADOWPLAYER_EVAL_SCROBBLE").is_ok()
            {
                self.app_state.scrobble_log_open = true;
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_DSP").is_ok() {
                if let Some(track) = self.library_state.all_tracks.first().cloned() {
                    self.app_state.track_dsp_modal.track_id = Some(track.id);
                    self.app_state.track_dsp_modal.is_open = true;
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_EDIT").is_ok() {
                if let Some(track) = self.library_state.all_tracks.first().cloned() {
                    self.app_state.edit_track.open_with(&track);
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_PALETTE").is_ok()
            {
                self.app_state.command_palette_open = true;
                if let Ok(q) = std::env::var("SHADOWPLAYER_EVAL_PALETTE_QUERY") {
                    self.app_state.command_palette.query = q;
                }
            }
            if self.app_state.frame_count == 1 {
                if let Ok(theme_name) = std::env::var("SHADOWPLAYER_EVAL_THEME") {
                    self.theme_manager.stage_theme(&theme_name);
                    let _ = self.theme_manager.commit_staged();
                    self.sync_effective_theme();
                }
                if std::env::var("SHADOWPLAYER_EVAL_LIGHT").is_ok() {
                    self.theme = crate::theme::Theme::light();
                }
                if std::env::var("SHADOWPLAYER_EVAL_FROSTED").is_ok() {
                    self.dsp_state.settings_tab.frosted_glass_enabled = true;
                    self.sync_effective_theme();
                }
                if std::env::var("SHADOWPLAYER_EVAL_THEME_EDITOR").is_ok() {
                    self.app_state.active_nav = crate::components::sidebar::NavItem::Settings;
                    self.dsp_state.settings_tab.active_category =
                        crate::components::settings_tab::SettingsCategory::Appearance;
                    let active_id = self.theme_manager.active_id().to_string();
                    let card = self
                        .theme_manager
                        .all_theme_cards()
                        .into_iter()
                        .find(|card| card.id == active_id);
                    let name = card
                        .as_ref()
                        .map(|card| card.name.clone())
                        .unwrap_or_else(|| active_id.clone());
                    let fonts = card
                        .as_ref()
                        .map(|card| card.fonts.clone())
                        .unwrap_or_default();
                    let base = self
                        .theme_manager
                        .resolve_theme(&active_id, self.theme.is_light);
                    self.dsp_state
                        .settings_tab
                        .theme_editor
                        .request_load_for_customizing(&base, &name, &active_id, fonts);
                    crate::components::settings_tab::request_settings_jump("Theme Editor");
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_HELP").is_ok() {
                self.app_state.active_nav = crate::components::sidebar::NavItem::Help;
                if let Ok(tab_str) = std::env::var("SHADOWPLAYER_EVAL_HELP_TAB") {
                    match tab_str.as_str() {
                        "thisbuild" => {
                            self.help_state.active_tab = crate::views::help_view::HelpTab::ThisBuild
                        }
                        "signal" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::SignalPath
                        }
                        "eq" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::EqualizerAcoustics
                        }
                        "bitperfect" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::BitPerfectDac
                        }
                        "spatial" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::SpatialBinaural
                        }
                        "playback" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::PlaybackDsp
                        }
                        "mixbus" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::MixBusStudio
                        }
                        "shortcuts" => {
                            self.help_state.active_tab =
                                crate::views::help_view::HelpTab::KeyboardShortcuts
                        }
                        "glossary" => {
                            self.help_state.active_tab = crate::views::help_view::HelpTab::Glossary
                        }
                        _ => {}
                    }
                }
                if let Ok(search_str) = std::env::var("SHADOWPLAYER_EVAL_HELP_SEARCH") {
                    self.help_state.search_query = search_str;
                }
            }
            if self.app_state.frame_count == 1 && std::env::var("SHADOWPLAYER_EVAL_EQ").is_ok() {
                self.dsp_state.eq_panel.is_open = true;
                if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_ok() {
                    self.dsp_state.eq_panel.graphic_enabled = false;
                    self.dsp_state.eq_panel.eq_enabled = false;
                    self.dsp_state.eq_panel.selected_preset =
                        crate::components::eq_panel::EqPreset::Flat;
                    for g in self.dsp_state.eq_panel.graphic_gains.iter_mut() {
                        *g = 0.0;
                    }
                    self.dsp_state.eq_panel.graphic_preamp_db = 0.0;
                    self.dsp_state.eq_panel.bass_shelf_db = 0.0;
                    self.dsp_state.eq_panel.treble_shelf_db = 0.0;
                    self.dsp_state.eq_panel.stereo_width = 1.0;
                    self.dsp_state.eq_panel.balance = 0.0;
                    self.dsp_state.eq_panel.auto_headroom = true;
                } else if std::env::var("SHADOWPLAYER_EVAL_EQ_DISABLED").is_ok() {
                    self.dsp_state.eq_panel.graphic_enabled = false;
                    self.dsp_state.eq_panel.eq_enabled = false;
                } else {
                    self.dsp_state.eq_panel.graphic_enabled = true;
                    self.dsp_state.eq_panel.eq_enabled = true;
                }
                if std::env::var("SHADOWPLAYER_EVAL_TONE").is_ok() {
                    self.dsp_state.eq_panel.active_tab = crate::components::eq_panel::EqTab::Tone;
                    if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_err() {
                        self.dsp_state.eq_panel.bass_shelf_db = 4.0;
                        self.dsp_state.eq_panel.treble_shelf_db = -1.5;
                        self.dsp_state.eq_panel.stereo_width = 1.35;
                        self.dsp_state.eq_panel.balance = 0.20;
                        self.dsp_state.eq_panel.graphic_preamp_db = 1.5;
                    }
                } else if std::env::var("SHADOWPLAYER_EVAL_ADVANCED").is_ok() {
                    self.dsp_state.eq_panel.active_tab =
                        crate::components::eq_panel::EqTab::Advanced;
                    self.dsp_state.eq_panel.extended_range = false;
                    self.dsp_state.eq_panel.auto_headroom = true;
                    self.dsp_state.eq_panel.midside_eq = false;
                    self.dsp_state.eq_panel.graphic_preamp_db = 0.0;
                } else if std::env::var("SHADOWPLAYER_EVAL_EQ_DEFAULT").is_err() {
                    self.dsp_state.eq_panel.selected_preset =
                        crate::components::eq_panel::EqPreset::Pop;
                    if let Some(g) = crate::components::eq_panel::EqPreset::Pop.gains() {
                        for i in 0..10 {
                            self.dsp_state.eq_panel.graphic_gains[i] = g[i];
                        }
                    }
                }
            }
            if let Ok(info_key) = std::env::var("SHADOWPLAYER_EVAL_INFO_KEY") {
                let active_key = egui::Id::NULL.with("shadowplayer_active_info_id");
                let target_id = egui::Id::NULL.with(info_key);
                ctx.data_mut(|d| d.insert_temp(active_key, target_id));
            }
            self.drive_clap_eval();
            if self.app_state.frame_count < 30 {
                ctx.request_repaint();
            }
            if self.app_state.frame_count == 24 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::ScreenshotCallback::new(
                        move |image: std::sync::Arc<egui::ColorImage>| {
                            let _ = image::save_buffer(
                                "eval_screenshot.png",
                                image.as_raw(),
                                image.width() as u32,
                                image.height() as u32,
                                image::ExtendedColorType::Rgba8,
                            );
                            std::process::exit(0);
                        },
                    ),
                ));
            }
            if self.app_state.frame_count > 45 {
                std::process::exit(0);
            }
        }
    }
}

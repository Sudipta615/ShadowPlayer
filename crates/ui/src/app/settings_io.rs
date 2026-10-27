use super::*;

impl ShadowPlayerApp {
    pub fn load_persisted_settings(&mut self) {
        if let Ok(app_settings) = self.settings_repo.load() {
            // The language is decided before anything is rendered, so the
            // first frame is already in the right language. An empty
            // preference means "ask the environment"; the POSIX locale
            // variables decide, and English is the fallback.
            self.locale = app_settings.locale.clone();
            crate::i18n::init(&self.locale);
            self.theme = if app_settings.theme_dark {
                Theme::dark()
            } else {
                Theme::light()
            };
            let active_theme_pref = self
                .settings_repo
                .get_value("active_theme")
                .ok()
                .flatten()
                .filter(|s| !s.is_empty());

            if let Some(theme_name) = active_theme_pref {
                self.theme_manager.active_id = theme_name.clone();
                self.theme = self
                    .theme_manager
                    .resolve_theme(&theme_name, !app_settings.theme_dark);
            }
            self.app_state.volume = app_settings.volume;
            self.app_state.is_muted = app_settings.is_muted;
            self.app_state.current_sort = app_settings.sort_field;
            self.app_state.current_sort_order = app_settings.sort_order;
            self.app_state.is_grid_view = app_settings.view_mode == domain::models::ViewMode::Grid;
            self.window_size = (
                app_settings.window_width.max(1080.0),
                app_settings.window_height.max(640.0),
            );
            self.coordinator.set_shuffle(app_settings.shuffle);
            self.coordinator.set_repeat_mode(app_settings.repeat_mode);
            self.dsp_state.settings_tab.bit_perfect = app_settings.bit_perfect_enabled;
            self.dsp_state.settings_tab.allow_multiple_instances =
                app_settings.allow_multiple_instances;
            self.multi_instance_allowed = app_settings.allow_multiple_instances;
            self.dsp_state.settings_tab.audio_profile = app_settings.audio_profile;
            if let Some(device) = &app_settings.selected_device {
                self.dsp_state.settings_tab.output_device = device.clone();
            }

            let eff_vol = if app_settings.is_muted {
                0.0
            } else {
                app_settings.volume
            };
            self.session.set_volume(eff_vol);
        }

        // 1b. Song-table layout: column shares and row density. Stored as its
        // own JSON blob so adding a column in a later version does not make
        // older settings fail to deserialize.
        if let Ok(Some(json)) = self.settings_repo.get_value("song_table_prefs") {
            match serde_json::from_str::<crate::components::songs_table::SongTablePrefs>(&json) {
                Ok(prefs) => {
                    self.app_state.column_widths = prefs.widths;
                    self.app_state.rating_width = prefs.rating_width;
                    self.app_state.row_density = prefs.density;
                    let mut presets = if prefs.presets.is_empty() {
                        // Settings from a build before column presets have no
                        // preset list, but do have the user's Title/Artist/Album
                        // proportions. Carry those into the Default layout rather
                        // than silently snapping back to the factory widths.
                        let mut builtins = crate::components::songs_table::ColumnPreset::builtins();
                        builtins[0] =
                            crate::components::songs_table::ColumnPreset::from_legacy_widths(
                                prefs.widths,
                            );
                        builtins
                    } else {
                        prefs.presets
                    };
                    if presets.is_empty() {
                        presets = crate::components::songs_table::ColumnPreset::builtins();
                    }
                    for preset in &mut presets {
                        *preset = preset.clone().normalized();
                    }
                    self.app_state.active_column_preset =
                        prefs.active_preset.min(presets.len().saturating_sub(1));
                    self.app_state.column_presets = presets;
                }
                Err(e) => log::warn!("Could not read the saved song-table layout: {e}"),
            }
        }

        // 2. Persistent Eq Settings (EQ layout, bands, gains, preamp, tone controls, custom presets, window position)
        if let Ok(Some(json)) = self.settings_repo.get_value("eq_settings") {
            if let Ok(saved_eq) =
                serde_json::from_str::<crate::components::eq_panel::PersistentEqState>(&json)
            {
                saved_eq.apply_to(&mut self.dsp_state.eq_panel);
            }
        }

        // 3. Persistent Audio / DSP Settings (Speed, pitch, crossfade, spatial acoustics, dac config)
        if let Ok(Some(json)) = self.settings_repo.get_value("dsp_settings") {
            match serde_json::from_str::<crate::components::settings_tab::SettingsTabState>(&json) {
                Ok(saved) => {
                    // Presets and the live CLAP summary are skipped by serde:
                    // they keep their own storage key / are refreshed each
                    // frame.
                    let clap = std::mem::take(&mut self.dsp_state.settings_tab.clap);
                    // This preference lives in `AppSettings` because the next
                    // launch reads it before the database opens. The older
                    // `dsp_settings` blob also contains the field, and may be
                    // stale, so keep the value just loaded from AppSettings.
                    let allow_multiple = self.dsp_state.settings_tab.allow_multiple_instances;
                    self.dsp_state.settings_tab = saved;
                    self.dsp_state.settings_tab.clap = clap;
                    self.dsp_state.settings_tab.allow_multiple_instances = allow_multiple;
                    if self.dsp_state.settings_tab.frosted_glass_enabled {
                        self.theme = self.theme.with_frosted_glass(true);
                    }
                }
                Err(e) => {
                    log::warn!("Could not read the saved audio settings, using defaults: {e}");
                }
            }
        }

        // 4. Directories of individually imported song files (shown in Settings)
        if let Ok(Some(json)) = self.settings_repo.get_value("imported_file_dirs") {
            if let Ok(dirs) = serde_json::from_str::<Vec<PathBuf>>(&json) {
                self.app_state.imported_dirs = dirs;
            }
        }

        // 5. Whole-chain DSP presets. Stored separately from the live EQ/DSP
        // state: a preset outlives the settings it was captured from.
        if let Ok(Some(json)) = self.settings_repo.get_value("chain_presets") {
            match serde_json::from_str::<Vec<crate::state::ChainPreset>>(&json) {
                Ok(presets) => self.dsp_state.settings_tab.chain_presets = presets,
                Err(e) => log::warn!("Ignoring unreadable chain presets: {}", e),
            }
        }

        // 6. Batch-organizer preferences (last pattern and destination).
        if let Ok(Some(json)) = self.settings_repo.get_value("organize_prefs") {
            match serde_json::from_str::<crate::modals::OrganizePrefs>(&json) {
                Ok(prefs) => {
                    self.organize.pattern = prefs.pattern;
                    self.organize.destination = prefs.destination;
                }
                Err(e) => log::warn!("Ignoring unreadable organizer preferences: {e}"),
            }
        }

        // 7. CLAP plugin rack presets. The plugin itself is restored by the
        // engine (it owns the insert); what lives here is the named snapshots
        // the user saved, which the engine knows nothing about.
        if let Ok(Some(json)) = self.settings_repo.get_value("clap_rack") {
            match serde_json::from_str::<crate::state::clap_rack::ClapRackPrefs>(&json) {
                Ok(prefs) => {
                    self.dsp_state.clap_rack.presets = prefs.presets;
                    self.dsp_state.clap_rack.last_library = prefs.last_library;
                }
                Err(e) => log::warn!("Ignoring unreadable CLAP rack presets: {e}"),
            }
        }

        // Sync restored state to audio session
        self.dsp_state.sync_all_to_session(&self.session);

        // The engine rebuilds the DSP graph on its first tick, so a plugin
        // restored from the persisted engine config only becomes visible after
        // that. Ask for the rack view for a few frames rather than once, so the
        // answer does not depend on which frame the graph happened to be built
        // in.
        self.clap_refresh_frames = CLAP_STARTUP_REFRESHES;
    }

    /// Change the interface language.
    ///
    /// An empty preference means "follow the environment", so the POSIX
    /// locale variables decide again. The change takes effect on the next
    /// frame, and is written to the settings with the next save.
    pub(super) fn set_locale(&mut self, preference: String) {
        self.locale = preference.clone();
        crate::i18n::init(&preference);
        self.settings_dirty = true;
        if let Some(ctx) = &self.wake_ctx {
            ctx.request_repaint();
        }
    }

    pub fn save_persisted_settings(&self) {
        // 1. Save AppSettings
        let app_settings = domain::models::AppSettings {
            theme_dark: !self.theme.is_light,
            volume: self.app_state.volume,
            is_muted: self.app_state.is_muted,
            sort_field: self.app_state.current_sort,
            sort_order: self.app_state.current_sort_order,
            view_mode: if self.app_state.is_grid_view {
                domain::models::ViewMode::Grid
            } else {
                domain::models::ViewMode::List
            },
            audio_profile: self.dsp_state.settings_tab.audio_profile,
            repeat_mode: self.coordinator.repeat_mode(),
            shuffle: self.coordinator.is_shuffle(),
            selected_device: if self.dsp_state.settings_tab.output_device.is_empty() {
                None
            } else {
                Some(self.dsp_state.settings_tab.output_device.clone())
            },
            bit_perfect_enabled: self.dsp_state.settings_tab.bit_perfect,
            allow_multiple_instances: self.multi_instance_allowed,
            // Read from the live window, not a fixed pair of numbers: the value
            // written here is what the next launch restores.
            window_width: if self.app_state.mini_player {
                self.normal_window_size.0
            } else {
                self.window_size.0
            },
            window_height: if self.app_state.mini_player {
                self.normal_window_size.1
            } else {
                self.window_size.1
            },
            locale: self.locale.clone(),
        };
        if let Err(e) = self.settings_repo.save(&app_settings) {
            log::warn!("Could not persist app settings: {e}");
        }

        // 2. Save EQ Settings
        let eq_persisted =
            crate::components::eq_panel::PersistentEqState::from(&self.dsp_state.eq_panel);
        if let Ok(json) = serde_json::to_string(&eq_persisted) {
            if let Err(e) = self.settings_repo.set_value("eq_settings", &json) {
                log::warn!("Could not save the EQ settings: {e}");
            }
        }

        // 3. Save DSP & Settings Tab State
        match serde_json::to_string(&self.dsp_state.settings_tab) {
            Ok(json) => {
                if let Err(e) = self.settings_repo.set_value("dsp_settings", &json) {
                    log::warn!("Could not save the audio settings: {e}");
                }
            }
            Err(e) => log::warn!("Could not serialize the audio settings: {e}"),
        }

        // 4. Save imported-file directory list
        if let Ok(json) = serde_json::to_string(&self.app_state.imported_dirs) {
            if let Err(e) = self.settings_repo.set_value("imported_file_dirs", &json) {
                log::warn!("Could not save the imported-file list: {e}");
            }
        }

        // 5. Save whole-chain DSP presets
        if let Ok(json) = serde_json::to_string(&self.dsp_state.settings_tab.chain_presets) {
            if let Err(e) = self.settings_repo.set_value("chain_presets", &json) {
                log::warn!("Could not save the chain presets: {e}");
            }
        }

        // 6. Save batch-organizer preferences
        let organize_prefs = crate::modals::OrganizePrefs {
            pattern: self.organize.pattern.clone(),
            destination: self.organize.destination.clone(),
        };
        if let Ok(json) = serde_json::to_string(&organize_prefs) {
            if let Err(e) = self.settings_repo.set_value("organize_prefs", &json) {
                log::warn!("Could not save the organizer preferences: {e}");
            }
        }

        // 7. Save CLAP rack presets
        let clap_prefs = crate::state::clap_rack::ClapRackPrefs {
            presets: self.dsp_state.clap_rack.presets.clone(),
            last_library: self.dsp_state.clap_rack.last_library.clone(),
        };
        if let Ok(json) = serde_json::to_string(&clap_prefs) {
            if let Err(e) = self.settings_repo.set_value("clap_rack", &json) {
                log::warn!("Could not save the CLAP rack presets: {e}");
            }
        }

        // 8. Song-table layout (column shares and row density)
        let table_prefs = crate::components::songs_table::SongTablePrefs {
            widths: self.app_state.column_widths,
            density: self.app_state.row_density,
            rating_width: self.app_state.rating_width,
            presets: self.app_state.column_presets.clone(),
            active_preset: self.app_state.active_column_preset,
        };
        if let Ok(json) = serde_json::to_string(&table_prefs) {
            if let Err(e) = self.settings_repo.set_value("song_table_prefs", &json) {
                log::warn!("Could not save the song-table layout: {e}");
            }
        }
    }
}

/// `true` while the eval harness is driving the UI: it must not write the real
/// settings file from a screenshot run. In a normal build this is a constant
/// `false` and the environment variable is never read.
pub(super) fn eval_holds_persistence() -> bool {
    #[cfg(feature = "eval-tools")]
    {
        std::env::var("SHADOWPLAYER_EVAL").is_ok()
    }
    #[cfg(not(feature = "eval-tools"))]
    {
        false
    }
}

impl ShadowPlayerApp {
    /// Event-driven settings persistence across sessions.
    ///
    /// Window size, theme and transport changes mark the state dirty; writing
    /// on every 120th frame made an otherwise idle app wake the disk twice a
    /// second.
    ///
    /// Skips the write while the eval harness is driving the UI, so a
    /// screenshot run never overwrites the real settings file.
    pub(super) fn persist_settings_if_dirty(&mut self) {
        // 11. Event-driven settings persistence across sessions.
        if !eval_holds_persistence() && self.settings_dirty {
            self.save_persisted_settings();
            self.settings_dirty = false;
        }
    }

    /// Update effective theme combining active or staged theme with frosted glass modifier.
    pub(in crate::app) fn sync_effective_theme(&mut self) {
        let base = self.theme_manager.effective_theme();
        self.theme = base.with_frosted_glass(self.dsp_state.settings_tab.frosted_glass_enabled);
        self.applied_theme = None;
    }
}

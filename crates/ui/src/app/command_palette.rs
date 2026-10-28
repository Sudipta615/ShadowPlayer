use super::*;

impl ShadowPlayerApp {
    /// Everything the command palette can reach.
    ///
    /// The static entries are cheap and always present. Tracks are only
    /// matched when there is a query, and only the best few are turned into
    /// entries, so a ten-thousand-track library does not allocate ten
    /// thousand rows on every keystroke.
    pub(super) fn command_palette_entries(&self) -> Vec<PaletteEntry> {
        use crate::components::command_palette::fuzzy_score;

        let mut entries: Vec<PaletteEntry> = Vec::new();

        // ── Navigation ─────────────────────────────────────────────────────
        let nav: [(&str, NavItem, &str); 11] = [
            ("Go to", NavItem::Home, "Home"),
            ("Go to", NavItem::Albums, "Albums"),
            ("Go to", NavItem::Artists, "Artists"),
            ("Go to", NavItem::Genres, "Genres"),
            ("Go to", NavItem::Radio, "Radio"),
            ("Go to", NavItem::Podcasts, "Podcasts"),
            ("Go to", NavItem::Folders, "Folders & library manager"),
            ("Go to", NavItem::Favorites, "Favorites"),
            ("Go to", NavItem::RecentlyPlayed, "Recently played"),
            ("Go to", NavItem::MostPlayed, "Most played"),
            ("Go to", NavItem::RecentlyAdded, "Recently added"),
        ];
        for (category, item, title) in nav {
            entries.push(PaletteEntry::new(
                category,
                title,
                "",
                PaletteAction::Nav(item),
            ));
        }
        entries.push(PaletteEntry::new(
            "Go to",
            "Settings",
            "",
            PaletteAction::Nav(NavItem::Settings),
        ));
        entries.push(PaletteEntry::new(
            "Go to",
            "Help & manual",
            "",
            PaletteAction::Nav(NavItem::Help),
        ));

        // ── Playlists ─────────────────────────────────────────────────────
        for pl in &self.library_state.playlists {
            entries.push(PaletteEntry::new(
                "Playlist",
                pl.name.clone(),
                format!("{} tracks", pl.track_count),
                PaletteAction::Nav(NavItem::Playlist(pl.id)),
            ));
        }
        for sp in &self.library_state.smart_playlists {
            entries.push(PaletteEntry::new(
                "Smart playlist",
                sp.name.clone(),
                "rule-based",
                PaletteAction::Nav(NavItem::SmartPlaylist(sp.id)),
            ));
        }

        // ── Toggles and panels ─────────────────────────────────────────────
        entries.push(
            PaletteEntry::new(
                "Toggle",
                "Light / dark theme",
                "",
                PaletteAction::ToggleTheme,
            )
            .with_shortcut("Ctrl+Space"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Shuffle", "", PaletteAction::ToggleShuffle)
                .with_shortcut("S"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Repeat mode", "", PaletteAction::ToggleRepeat)
                .with_shortcut("R"),
        );
        entries.push(
            PaletteEntry::new("Toggle", "Mute", "", PaletteAction::ToggleMute).with_shortcut("M"),
        );
        entries.push(
            PaletteEntry::new(
                "Toggle",
                "Bit-perfect direct stream",
                "",
                PaletteAction::ToggleBitPerfect,
            )
            .with_shortcut("Ctrl+B"),
        );
        entries.push(
            PaletteEntry::new(
                "Panel",
                "DSP studio",
                "equalizer, dynamics and filters",
                PaletteAction::OpenEq,
            )
            .with_shortcut("Ctrl+E"),
        );
        entries.push(
            PaletteEntry::new(
                "Panel",
                "Playlist studio",
                "create and curate playlists",
                PaletteAction::OpenPlaylistEditor,
            )
            .with_shortcut("Ctrl+N"),
        );
        entries.push(PaletteEntry::new(
            "Panel",
            "AutoEQ calibration",
            "headphone and IEM correction",
            PaletteAction::OpenAutoEq,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Bit-perfect inspector",
            "what the signal path is doing",
            PaletteAction::OpenBitPerfect,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Speaker setup & acoustic alignment",
            "calibrate arrival delays, distances, trims and crossover",
            PaletteAction::OpenSpeakerSetup,
        ));
        entries.push(PaletteEntry::new(
            "Audio",
            "Headphone crossfeed & spatializer",
            "models acoustic soundstage, binaural ITD and fatigue relief",
            PaletteAction::OpenCrossfeed,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Loudness inspector",
            "how loud this track is",
            PaletteAction::OpenLoudness,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Library statistics",
            "formats, resolution and listening",
            PaletteAction::OpenLibraryStats,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Scrobble log",
            "what you actually listened to",
            PaletteAction::OpenScrobbleLog,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Listening history",
            "top artists, charts, streaks and export",
            PaletteAction::OpenListening,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Import from other players",
            "Rhythmbox, Audacious, Winamp, foobar2000, MusicBee, iTunes and more",
            PaletteAction::OpenImportPlaylists,
        ));
        entries.push(PaletteEntry::new(
            "Track",
            "Per-track DSP",
            "give this track its own version of the chain",
            PaletteAction::OpenTrackDsp,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Duplicate tracks",
            "find identical audio",
            PaletteAction::OpenDuplicates,
        ));
        entries.push(PaletteEntry::new(
            "Panel",
            "Organize files",
            "batch rename and move",
            PaletteAction::OpenOrganizer,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Import songs",
            "choose files",
            PaletteAction::ImportFiles,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Import a folder",
            "scan a music folder",
            PaletteAction::ImportFolder,
        ));
        entries.push(PaletteEntry::new(
            "Library",
            "Search the library",
            "",
            PaletteAction::FocusSearch,
        ));

        // ── Settings ───────────────────────────────────────────────────────
        // The same static index the Settings page searches, so a command
        // palette and the page can never disagree about what a setting is
        // called. Only with a query: seventy rows would bury the commands
        // when the palette first opens.
        if !self.app_state.command_palette.query.trim().is_empty() {
            for entry in crate::components::settings_tab::SETTINGS_INDEX.iter() {
                entries.push(PaletteEntry::new(
                    "Settings",
                    entry.label,
                    format!("{} \u{203a} {}", entry.category.title(), entry.card),
                    PaletteAction::JumpToSetting(entry.category, entry.label),
                ));
            }
        }

        // ── Tracks ─────────────────────────────────────────────────────────
        // Scored against the raw titles first, so only the best few are ever
        // turned into rows.
        let query = self.app_state.command_palette.query.trim();
        if !query.is_empty() {
            let mut scored: Vec<(i32, &Track)> = self
                .library_state
                .all_tracks
                .iter()
                .filter_map(|tr| fuzzy_score(&tr.title, query).map(|s| (s, tr)))
                .collect();
            scored.sort_by_key(|a| std::cmp::Reverse(a.0));
            for (_, tr) in scored.into_iter().take(24) {
                let hint = if tr.artist.is_empty() {
                    tr.album.clone()
                } else {
                    format!("{} — {}", tr.artist, tr.album)
                };
                entries.push(PaletteEntry::new(
                    "Track",
                    tr.title.clone(),
                    hint,
                    PaletteAction::PlayTrack(tr.id),
                ));
            }
        }

        entries
    }

    /// Act on a command palette choice.
    pub(super) fn run_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::Nav(item) => {
                self.app_state.active_nav = item;
                self.library_state.selected_album = None;
                self.library_state.selected_artist = None;
                self.library_state.selected_genre = None;
                self.library_state.selected_folder = None;
                self.refresh_view();
            }
            PaletteAction::ToggleTheme => {
                self.theme.toggle();
                self.settings_dirty = true;
            }
            PaletteAction::ToggleShuffle => {
                let new_shuffle = !self.coordinator.is_shuffle();
                self.coordinator.set_shuffle(new_shuffle);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleRepeat => {
                let next_mode = match self.coordinator.repeat_mode() {
                    domain::models::RepeatModeChoice::Off => domain::models::RepeatModeChoice::All,
                    domain::models::RepeatModeChoice::All => domain::models::RepeatModeChoice::One,
                    domain::models::RepeatModeChoice::One => domain::models::RepeatModeChoice::Off,
                };
                self.coordinator.set_repeat_mode(next_mode);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleMute => {
                self.app_state.is_muted = !self.app_state.is_muted;
                let target_vol = if self.app_state.is_muted {
                    0.0
                } else {
                    self.app_state.volume
                };
                self.session.set_volume(target_vol);
                self.settings_dirty = true;
            }
            PaletteAction::ToggleBitPerfect => {
                self.toggle_bit_perfect();
                self.settings_dirty = true;
            }
            PaletteAction::OpenEq => {
                self.dsp_state.eq_panel.is_open = true;
            }
            PaletteAction::OpenPlaylistEditor => {
                self.app_state.open_playlist_editor = true;
            }
            PaletteAction::OpenAutoEq => {
                self.dsp_state.autoeq.is_open = true;
            }
            PaletteAction::OpenBitPerfect => {
                self.app_state.bit_perfect_modal_open = true;
            }
            PaletteAction::OpenSpeakerSetup => {
                self.app_state.speaker_setup_modal.open();
            }
            PaletteAction::OpenCrossfeed => {
                self.dsp_state.eq_panel.active_tab = crate::components::eq_panel::EqTab::Filters;
                self.dsp_state.eq_panel.is_open = true;
            }
            PaletteAction::OpenLoudness => {
                self.open_loudness_inspector();
            }
            PaletteAction::OpenLibraryStats => {
                self.open_library_stats();
            }
            PaletteAction::OpenTrackDsp => {
                if let Some(track) = self.coordinator.current_track().cloned() {
                    self.open_track_dsp(&track);
                }
            }
            PaletteAction::OpenScrobbleLog => {
                self.app_state.scrobble_log_open = true;
            }
            PaletteAction::OpenListening => {
                self.app_state.listening_modal.open();
            }
            PaletteAction::JumpToSetting(category, label) => {
                self.app_state.active_nav = NavItem::Settings;
                self.refresh_view();
                self.dsp_state.settings_tab.active_category = category;
                crate::components::settings_tab::request_settings_jump(label);
            }
            PaletteAction::OpenImportPlaylists => {
                self.open_import_playlists();
            }
            PaletteAction::OpenDuplicates => {
                self.scan_duplicates();
            }
            PaletteAction::OpenOrganizer => {
                self.open_organize(
                    self.library_state.displayed_tracks.clone(),
                    "the current view".to_string(),
                );
            }
            PaletteAction::OpenFolders => {
                self.app_state.active_nav = NavItem::Folders;
                self.refresh_view();
            }
            PaletteAction::ImportFiles => {
                self.request_import(crate::services::ImportKind::Files);
            }
            PaletteAction::ImportFolder => {
                self.request_import(crate::services::ImportKind::Folder);
            }
            PaletteAction::FocusSearch => {
                self.request_search_focus();
            }
            PaletteAction::ClosePalette => {
                self.app_state.command_palette_open = false;
            }
            PaletteAction::PlayTrack(id) => {
                if let Some(track) = self
                    .library_state
                    .all_tracks
                    .iter()
                    .find(|t| t.id == id)
                    .cloned()
                {
                    self.coordinator.play_track(track, None);
                }
            }
        }
    }
}

impl ShadowPlayerApp {
    pub(super) fn render_command_palette(&mut self, ctx: &egui::Context) {
        // The command palette. It is a modal, so it traps the keyboard and
        // the background stays inert while it is open.
        if self.app_state.command_palette_open {
            let entries = self.command_palette_entries();
            if let Some(action) = crate::components::command_palette::show_command_palette(
                ctx,
                &mut self.app_state.command_palette,
                &entries,
                &self.theme,
            ) {
                self.app_state.command_palette_open = false;
                self.app_state.command_palette.query.clear();
                self.app_state.command_palette.selected = 0;
                self.run_palette_action(action);
            }
        }
    }
}

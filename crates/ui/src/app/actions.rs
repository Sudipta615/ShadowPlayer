use super::*;

impl ShadowPlayerApp {
    /// Close whatever dialog is in front, innermost first.
    ///
    /// Escape used to close only four of the thirteen overlays (search, help,
    /// new playlist, track properties, EQ) and left the rest — bit-perfect,
    /// AutoEq, the playlist and smart-playlist editors, duplicates, organize,
    /// the plugin rack — open with no keyboard way out. One press closes one
    /// layer, so Escape never tears down the whole UI at once.
    pub(super) fn dismiss_foreground_overlay(&mut self) {
        // Editors and dialogs that were opened deliberately, newest concept
        // first.
        if self.dsp_state.autoeq.is_open {
            self.dsp_state.autoeq.is_open = false;
        } else if self.app_state.bit_perfect_modal_open {
            self.app_state.bit_perfect_modal_open = false;
        } else if self.duplicates.is_open {
            self.duplicates.is_open = false;
        } else if self.organize.is_open {
            self.organize.is_open = false;
        } else if self.app_state.edit_track.is_open {
            self.app_state.edit_track.is_open = false;
        } else if self.smart_playlist_editor.is_open {
            self.smart_playlist_editor.is_open = false;
        } else if self.app_state.playlist_editor.is_open {
            self.app_state.playlist_editor.is_open = false;
        } else if self.app_state.track_properties_open {
            self.app_state.track_properties_open = false;
        } else if self.app_state.open_playlist_editor {
            self.app_state.open_playlist_editor = false;
        } else if self.dsp_state.clap_rack.is_open {
            self.dsp_state.clap_rack.is_open = false;
        } else if self.dsp_state.eq_panel.is_open {
            self.dsp_state.eq_panel.is_open = false;
        } else if self.app_state.right_overlay_open {
            self.app_state.right_overlay_open = false;
        } else if self.app_state.command_palette_open {
            self.app_state.command_palette_open = false;
        } else if !self.app_state.search_query.is_empty() {
            self.app_state.search_query.clear();
            self.refresh_view();
        }
    }

    /// The tracks a rating shortcut applies to: the multi-selection when one
    /// exists, otherwise the track that is playing.
    pub(super) fn rating_targets(&self) -> Vec<domain::ids::TrackId> {
        let selected: Vec<domain::ids::TrackId> = self
            .library_state
            .selected_tracks_in_order()
            .into_iter()
            .map(|t| t.id)
            .collect();
        if !selected.is_empty() {
            return selected;
        }
        self.coordinator
            .current_track()
            .map(|t| vec![t.id])
            .unwrap_or_default()
    }

    /// Set (or clear, with 0) the rating of the selection or the playing track.
    pub(super) fn rate_targets(&mut self, rating: u8) {
        let ids = self.rating_targets();
        if ids.is_empty() {
            return;
        }
        match self.track_repo.rate_many(&ids, rating) {
            Ok(()) => {
                for id in ids {
                    self.coordinator.update_track_rating(id, rating.min(5));
                }
                // Reload the view so a Rating sort or the Rating filter sees
                // the new value immediately.
                self.refresh_after_rating();
            }
            Err(e) => log::warn!("Could not save the rating: {e}"),
        }
    }

    /// Step the selection's rating up or down, clamping at 0 and 5.
    pub(super) fn step_target_rating(&mut self, delta: i8) {
        let ids = self.rating_targets();
        if ids.is_empty() {
            return;
        }
        for id in ids {
            let current = self
                .library_state
                .all_tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.rating)
                .or_else(|| {
                    self.track_repo
                        .get_by_id(&id)
                        .ok()
                        .flatten()
                        .map(|t| t.rating)
                })
                .unwrap_or(0);
            let next = (current as i16 + delta as i16).clamp(0, 5) as u8;
            if next != current {
                match self.track_repo.set_rating(&id, next) {
                    Ok(()) => self.coordinator.update_track_rating(id, next),
                    Err(e) => log::warn!("Could not save the rating for {id}: {e}"),
                }
            }
        }
        self.refresh_after_rating();
    }

    /// Refresh the library after a rating change.
    ///
    /// Ratings affect the visible sort, the Rating quick filter and saved
    /// smart playlists, and they are also part of the in-memory track model
    /// shown by the properties modal. Reloading from the database keeps all of
    /// those in step with the value that was just persisted.
    pub(super) fn refresh_after_rating(&mut self) {
        self.reload_library();
    }
    /// Apply the current track's own DSP profile, if it has one.
    ///
    /// A profile is a **diff** against the user's global chain, not a copy of
    /// it: the track names the parameters it changes and inherits the rest.
    /// The global chain is captured before the first profile takes over and
    /// put back the moment a track without one plays.
    pub(super) fn apply_track_dsp_profile(&mut self) {
        let Some(track) = self.coordinator.current_track().cloned() else {
            return;
        };
        if self.dsp_profile_track == Some(track.id) {
            return;
        }

        let profile = match self.track_dsp_repo.get(&track.id) {
            Ok(Some(p)) if !p.is_neutral() => p,
            Ok(_) => {
                // No profile, or one that changes nothing: the track plays
                // with the user's own settings. Restoring them matters — the
                // previous track's profile is still applied otherwise.
                self.dsp_profile_track = Some(track.id);
                self.restore_global_chain();
                // The base a future override for this track is authored
                // against: the chain before the user starts adjusting it.
                self.dsp_track_base = Some(crate::state::ChainPreset::capture(
                    "Track base",
                    "",
                    &self.dsp_state.eq_panel,
                    &self.dsp_state.settings_tab,
                ));
                return;
            }
            Err(e) => {
                log::warn!("Could not read the DSP profile for {}: {e}", track.title);
                return;
            }
        };
        self.dsp_profile_track = Some(track.id);

        // Save the user's own chain before the first profile takes over, so
        // it can be put back when a track without one plays.
        if self.app_state.dsp_global_snapshot.is_none() {
            self.app_state.dsp_global_snapshot = Some(crate::state::ChainPreset::capture(
                "Global",
                "",
                &self.dsp_state.eq_panel,
                &self.dsp_state.settings_tab,
            ));
        }
        let global = self
            .app_state
            .dsp_global_snapshot
            .clone()
            .expect("the snapshot was just captured");

        let patch = match crate::state::chain_override::legacy_patch(&profile, &global) {
            Some(patch) => patch,
            // A profile that only carries ReplayGain has no chain diff; an
            // empty patch means "global chain plus the gain below".
            None if profile.replaygain_override_db.is_some() => serde_json::json!({}),
            None => {
                self.restore_global_chain();
                return;
            }
        };
        let Some(mut merged) = crate::state::chain_override::merge_chain(&global, &patch) else {
            log::warn!(
                "The DSP profile for {} does not fit the current chain; leaving the global chain",
                track.title
            );
            self.restore_global_chain();
            return;
        };
        // Apply the track's own gain on top of the chain's preamp. The panels
        // therefore show the level actually reaching the engine, and the
        // global chain is restored untouched when the track ends.
        if let Some(gain_db) = profile.replaygain_override_db {
            merged.eq.add_track_gain(gain_db);
        }

        merged.apply(
            &mut self.dsp_state.eq_panel,
            &mut self.dsp_state.settings_tab,
        );
        self.dsp_state.sync_chain_to_session(&self.session);
        self.toasts.info(format!(
            "Using this track's own chain for \u{201c}{}\u{201d}",
            track.title
        ));
    }

    /// Put the user's own chain back.
    ///
    /// The snapshot exists only while a track's own profile is applied, so
    /// this is a no-op when the user has never had a profile take over.
    pub(super) fn restore_global_chain(&mut self) {
        let Some(global) = self.app_state.dsp_global_snapshot.take() else {
            return;
        };
        global.apply(
            &mut self.dsp_state.eq_panel,
            &mut self.dsp_state.settings_tab,
        );
        self.dsp_state.sync_chain_to_session(&self.session);
    }

    /// Open the loudness inspector for the current track.
    ///
    /// The album's other tracks are captured here rather than per frame, so
    /// the inspector can show how the track sits inside its album without
    /// scanning the library on every redraw.
    pub(super) fn open_loudness_inspector(&mut self) {
        let Some(current) = self.coordinator.current_track().cloned() else {
            return;
        };
        let album = current.album.clone();
        let artist = current.artist.clone();
        self.app_state.loudness_modal.album_tracks = if album.is_empty() {
            Vec::new()
        } else {
            self.library_state
                .all_tracks
                .iter()
                .filter(|t| t.album == album && t.artist == artist)
                .cloned()
                .collect()
        };
        self.app_state.loudness_modal.is_open = true;
    }

    /// Open the per-track DSP editor for a track.
    pub(super) fn open_track_dsp(&mut self, track: &Track) {
        self.app_state.track_dsp_modal.track_id = Some(track.id);
        self.app_state.track_dsp_modal.is_open = true;
    }
    /// Open the import-from-other-players dialog.
    ///
    /// The playlists are found and matched against the library here, so the
    /// dialog does not walk the library on every frame it is open.
    pub(super) fn open_import_playlists(&mut self) {
        let found = library::find_playlists();
        let tracks = &self.library_state.all_tracks;
        let matched: Vec<usize> = found
            .iter()
            .map(|p| crate::modals::import_playlists_modal::match_count(p, tracks))
            .collect();
        let state = &mut self.app_state.import_playlists_modal;
        state.found = found;
        state.matched = matched;
        state.ticked = vec![true; state.found.len()];
        state.is_open = true;
    }

    /// Open the library statistics.
    ///
    /// The summary is computed here rather than per frame, so opening the
    /// view does not walk the whole library on every redraw.
    pub(super) fn open_library_stats(&mut self) {
        self.app_state.library_stats_modal.stats =
            crate::modals::library_stats_modal::LibraryStats::from_tracks(
                &self.library_state.all_tracks,
            );
        self.app_state.library_stats_modal.is_open = true;
    }

    /// Move the keyboard focus to the search field.
    ///
    /// The Settings and Help views have no search box, so the shortcut returns
    /// to the library first and focuses the field on the next frame.
    pub(super) fn request_search_focus(&mut self) {
        if matches!(self.app_state.active_nav, NavItem::Help | NavItem::Settings) {
            self.app_state.active_nav = NavItem::Home;
        }
        self.app_state.focus_search = true;
    }
    /// Switch between the normal library shell and the compact mini player.
    /// The viewport resize itself is applied in the frame loop so it can be
    /// undone cleanly when the user restores the normal window.
    /// Enter or leave the full-screen Now Playing mode.
    ///
    /// The window goes genuinely full screen: the mode is about looking at
    /// the music, and a large window inside a desktop is not that.
    pub(super) fn set_now_playing_full(&mut self, ctx: &egui::Context, enabled: bool) {
        if self.app_state.now_playing_full == enabled {
            return;
        }
        self.app_state.now_playing_full = enabled;
        // The two modes are exclusive: the mini player and the full-screen
        // view are both the whole window. Leaving the mini player means
        // putting the normal geometry back before going full screen.
        if enabled && self.app_state.mini_player {
            self.app_state.mini_player = false;
            self.mini_applied = false;
            let (w, h) = self.normal_window_size;
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::new(
                1080.0, 640.0,
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                w.max(1080.0),
                h.max(640.0),
            )));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(enabled));
    }

    pub(super) fn set_mini_player(&mut self, enabled: bool) {
        self.app_state.mini_player = enabled;
    }

    /// Opens the native OS picker on a worker thread for files or a folder.
    pub fn request_import(&mut self, kind: ImportKind) {
        match kind {
            ImportKind::Files => self.import_service.request_files(),
            ImportKind::Folder => self.import_service.request_folder(),
            // SaveM3u is driven by `export_playlist_as_m3u`, and the organize
            // destination by the organizer, never from here.
            ImportKind::Playlist => self.import_service.request_playlist(),
            ImportKind::SaveM3u => {}
            ImportKind::OrganizeDestination => {}
            ImportKind::ClapLibrary => self.import_service.request_clap_library(),
        }
    }
}

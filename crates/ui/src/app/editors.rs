use super::*;

impl ShadowPlayerApp {
    pub(super) fn render_editor_modals(&mut self, ctx: &egui::Context) {
        // Smart playlist editor: rule-based, self-updating playlists. Re-evaluate
        // the draft only when it changed, so typing never re-queries the library
        // more than once per frame.
        if self.smart_playlist_editor.is_open && self.smart_playlist_editor.draft_dirty {
            let draft = self.smart_playlist_editor.draft.clone();
            self.library_state
                .evaluate_smart(&self.smart_playlist_repo, &draft);
            self.smart_playlist_editor.draft_dirty = false;
        }
        let sp_actions = show_smart_playlist_editor(
            ctx,
            &mut self.smart_playlist_editor,
            &self.library_state.smart_playlists,
            &self.library_state.smart_playlist_tracks,
            &self.theme,
        );
        let mut sp_changed = false;
        if let Some(sp) = sp_actions.save {
            match self.smart_playlist_repo.save(&sp) {
                Ok(saved) => {
                    self.app_state.active_nav = NavItem::SmartPlaylist(saved.id);
                    self.smart_playlist_editor.is_new = false;
                    self.smart_playlist_editor.draft = saved;
                    sp_changed = true;
                }
                Err(e) => log::warn!("Could not save smart playlist: {}", e),
            }
        }
        if let Some(id) = sp_actions.delete {
            if let Err(e) = self.smart_playlist_repo.delete(&id) {
                log::warn!("Could not delete the smart playlist: {e}");
            }
            if self.app_state.active_nav == NavItem::SmartPlaylist(id) {
                self.app_state.active_nav = NavItem::Home;
            }
            if !self.smart_playlist_editor.is_new && self.smart_playlist_editor.draft.id == id {
                self.smart_playlist_editor.is_open = false;
            }
            sp_changed = true;
        }
        if let Some(id) = sp_actions.open {
            if let Some(sp) = self
                .library_state
                .smart_playlists
                .iter()
                .find(|s| s.id == id)
                .cloned()
            {
                self.smart_playlist_editor.open_existing(&sp);
            }
        }
        if sp_changed {
            // The definition changed, so every cached answer is out of date.
            self.library_state.request_smart_refresh();
            self.reload_library();
        }

        // Playlist manager: create / rename / delete / add songs.
        if self.app_state.open_playlist_editor {
            self.app_state.open_playlist_editor = false;
            self.app_state.playlist_editor.open(None);
        }
        let pl_actions = show_playlist_editor(
            ctx,
            &mut self.app_state.playlist_editor,
            &self.library_state.playlists,
            &self.library_state.all_tracks,
            &self.playlist_editor_tracks,
            &self.theme,
        );
        let mut pl_changed = false;
        if let Some(name) = pl_actions.create {
            match self.playlist_repo.create(&name, None) {
                Ok(new_pl) => {
                    self.app_state.playlist_editor.selected = Some(new_pl.id);
                    self.playlist_editor_tracks.clear();
                    pl_changed = true;
                }
                Err(e) => log::warn!("Could not create playlist: {}", e),
            }
        }
        if let Some((id, name)) = pl_actions.rename {
            if let Err(e) = self.playlist_repo.rename(&id, &name) {
                log::warn!("Could not rename the playlist: {e}");
            }
            pl_changed = true;
        }
        if let Some(id) = pl_actions.delete {
            if let Err(e) = self.playlist_repo.delete(&id) {
                log::warn!("Could not delete the playlist: {e}");
            }
            if self.app_state.playlist_editor.selected == Some(id) {
                self.app_state.playlist_editor.selected = None;
            }
            self.playlist_editor_tracks.clear();
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.add_track {
            if let Err(e) = self.playlist_repo.add_track(&pl, &tr) {
                log::warn!("Could not add the track to the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some((pl, linked)) = pl_actions.link_playlist {
            match self.playlist_repo.add_link(&pl, &linked) {
                Ok(()) => self.toasts.success(tr!(Msg::S087398277D809E0D)),
                Err(e) => self
                    .toasts
                    .warning(format!("Could not link the playlist: {e}")),
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some((pl, tr)) = pl_actions.remove_track {
            if let Err(e) = self.playlist_repo.remove_track(&pl, &tr) {
                log::warn!("Could not remove the track from the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some((pl, from, to)) = pl_actions.move_track {
            if let Err(e) = self.playlist_repo.move_track(&pl, from, to) {
                log::warn!("Could not reorder the playlist: {e}");
            }
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &pl);
            pl_changed = true;
        }
        if let Some(id) = pl_actions.select_changed {
            self.playlist_editor_tracks = load_playlist_tracks(&self.playlist_repo, &id);
        }
        if let Some(format) = pl_actions.export {
            if let Some(id) = self.app_state.playlist_editor.selected {
                let name = self
                    .library_state
                    .playlists
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "playlist".to_string());
                let tracks = self.playlist_editor_tracks.clone();
                self.export_playlist(name, tracks, format);
            }
        }
        if pl_changed {
            self.reload_library();
        }

        // Duplicate-track finder.
        let dupe_actions = show_duplicates_modal(ctx, &mut self.duplicates, &self.theme);
        if dupe_actions.rescan {
            self.scan_duplicates();
        }
        if dupe_actions.remove_selected {
            let delete_from_disk = self.duplicates.delete_from_disk;
            let ids: Vec<domain::ids::TrackId> = self.duplicates.selected.iter().copied().collect();
            let mut removed = 0usize;
            for id in ids {
                // Deleting the file first means a failed unlink leaves the
                // library row intact (rather than orphaning a file on disk).
                if delete_from_disk {
                    match self.track_repo.get_by_id(&id) {
                        Ok(Some(t)) if t.path.exists() => {
                            if let Err(e) = std::fs::remove_file(&t.path) {
                                log::warn!("Could not delete {}: {e}", t.path.display());
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                if self.track_repo.delete(&id).is_ok() {
                    removed += 1;
                }
            }
            log::info!("Removed {removed} duplicate track(s)");
            if removed > 0 {
                let verb = if delete_from_disk {
                    "deleted"
                } else {
                    "removed"
                };
                self.toasts
                    .success(format!("{removed} duplicate track(s) {verb}"));
            }
            self.reload_library();
            // Refresh so the list reflects what is left.
            self.scan_duplicates();
        }

        // Batch organizer.
        let organize_actions = show_organize_modal(ctx, &mut self.organize, &self.theme);
        if organize_actions.reset_pattern {
            self.organize.reset_pattern();
        }
        if organize_actions.clear_destination {
            self.organize.set_destination(None);
        }
        if organize_actions.pick_destination {
            self.import_service.request_organize_destination();
        }
        if organize_actions.apply {
            self.apply_organize_plan();
        }

        // Edit Song Info dialog.
        let edit_actions = show_edit_track_modal(ctx, &mut self.app_state.edit_track, &self.theme);
        if let Some(track) = edit_actions.save {
            if let Err(e) = self.track_repo.update_metadata(&track) {
                log::warn!("Could not save track metadata: {}", e);
            }
            // The file is the source of truth: an edit that only lives in the
            // database is lost the moment the file moves or the library is
            // rebuilt from disk, so the tags are written back too.
            if edit_actions.write_to_file {
                let tags = engine::decode::EditorialTags {
                    title: Some(track.title.clone()),
                    artist: Some(track.artist.clone()),
                    album: Some(track.album.clone()),
                    genre: track.genre.clone(),
                    year: track.year,
                    track_number: track.track_number,
                    disc_number: track.disc_number,
                };
                match engine::decode::write_editorial_tags(&track.path, &tags) {
                    Ok(()) => self.toasts.success(format!(
                        "Updated the tags in {}",
                        track
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| track.path.display().to_string())
                    )),
                    Err(e) => self.toasts.warning(format!(
                        "Saved in the library, but the file's tags could not be written: {e}"
                    )),
                }
            }
            self.reload_library();
        }

        // The per-track DSP editor.
        {
            // The editor works on whichever track it was opened for, not
            // only the one that happens to be playing.
            let dsp_track = self
                .app_state
                .track_dsp_modal
                .track_id
                .and_then(|id| {
                    self.library_state
                        .all_tracks
                        .iter()
                        .find(|t| t.id == id)
                        .cloned()
                })
                .or_else(|| self.coordinator.current_track().cloned());
            let is_current_track = dsp_track.as_ref().is_some_and(|track| {
                self.coordinator
                    .current_track()
                    .is_some_and(|current| current.id == track.id)
            });

            // The chain as the panels hold it, and the base a save is diffed
            // against. While a profile is applied the base is the user's
            // global chain; on an unprofiled track it is the chain as it was
            // when the track started, so adjustments made while auditioning
            // this track do not silently rewrite the global chain.
            let current_chain = crate::state::ChainPreset::capture(
                "Current",
                "",
                &self.dsp_state.eq_panel,
                &self.dsp_state.settings_tab,
            );
            let base_chain = self
                .app_state
                .dsp_global_snapshot
                .clone()
                .or_else(|| {
                    if is_current_track {
                        self.dsp_track_base.clone()
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| current_chain.clone());

            let dsp_actions = crate::modals::track_dsp_modal::show_track_dsp_modal(
                ctx,
                &mut self.app_state.track_dsp_modal,
                &self.track_dsp_repo,
                dsp_track.as_ref(),
                crate::modals::track_dsp_modal::TrackDspContext {
                    current_chain: &current_chain,
                    base_chain: &base_chain,
                    is_current_track,
                },
                &self.theme,
            );
            if let Some(replaygain) = dsp_actions.replaygain_override {
                if let Some(track) = dsp_track.clone() {
                    let mut profile = self
                        .track_dsp_repo
                        .get(&track.id)
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    profile.replaygain_override_db = replaygain;
                    let result = if profile.is_neutral() {
                        self.track_dsp_repo.clear(&track.id)
                    } else {
                        self.track_dsp_repo.set(&track.id, &profile)
                    };
                    match result {
                        Ok(()) => {
                            if is_current_track {
                                self.dsp_profile_track = None;
                                self.apply_track_dsp_profile();
                            }
                            self.toasts.info(match replaygain {
                                Some(db) => format!(
                                    "{} now plays with a {:+.1} dB ReplayGain override",
                                    track.title, db
                                ),
                                None => format!("{} uses the library gain again", track.title),
                            });
                        }
                        Err(e) => self
                            .toasts
                            .warning(format!("Could not save the ReplayGain override: {e}")),
                    }
                }
            }
            if dsp_actions.save_profile {
                if let Some(track) = dsp_track.clone() {
                    // A chain save must not discard the independently edited
                    // ReplayGain field, and the applied gain must not leak
                    // into the chain diff (it would then be applied twice).
                    let replaygain = self
                        .track_dsp_repo
                        .get(&track.id)
                        .ok()
                        .flatten()
                        .and_then(|p| p.replaygain_override_db);
                    let mut chain_for_diff = current_chain.clone();
                    if let Some(gain_db) = replaygain {
                        chain_for_diff.eq.preamp_db -= gain_db;
                    }
                    let mut profile = crate::modals::track_dsp_modal::profile_from_chain(
                        &base_chain,
                        &chain_for_diff,
                    );
                    profile.replaygain_override_db = replaygain;
                    match self.track_dsp_repo.set(&track.id, &profile) {
                        Ok(()) => {
                            let count = profile
                                .overrides
                                .as_ref()
                                .map(|patch| {
                                    crate::state::chain_override::describe_overrides(patch).len()
                                })
                                .unwrap_or(0);
                            self.toasts.success(format!(
                                "Saved {count} chain parameter{} as {}'s own override",
                                if count == 1 { "" } else { "s" },
                                track.title
                            ));
                            if is_current_track && self.app_state.dsp_global_snapshot.is_none() {
                                // The chain in the panels was authored for
                                // this track. Keep the chain it started with
                                // as the global, and let the profile apply on
                                // top of it from now on.
                                self.app_state.dsp_global_snapshot = Some(base_chain.clone());
                                self.dsp_profile_track = None;
                                self.dsp_track_base = None;
                                self.apply_track_dsp_profile();
                            }
                        }
                        Err(e) => self
                            .toasts
                            .warning(format!("Could not save the profile: {e}")),
                    }
                }
            }
            if dsp_actions.clear_profile {
                if let Some(track) = dsp_track.clone() {
                    match self.track_dsp_repo.clear(&track.id) {
                        Ok(()) => {
                            self.toasts
                                .info(format!("{} uses the global settings again", track.title));
                            if is_current_track {
                                self.dsp_profile_track = None;
                                self.restore_global_chain();
                                self.dsp_track_base = None;
                                self.apply_track_dsp_profile();
                            }
                        }
                        Err(e) => self
                            .toasts
                            .warning(format!("Could not remove the profile: {e}")),
                    }
                }
            }
        }
    }
}

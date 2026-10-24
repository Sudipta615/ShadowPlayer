use super::*;

impl ShadowPlayerApp {
    pub fn reload_library(&mut self) {
        // The refresh policy lives in the settings; the state that consults
        // it needs the current value.
        self.sync_smart_refresh_policy();
        self.library_state.reload(
            &self.track_repo,
            &self.favorites_repo,
            &self.playlist_repo,
            &self.smart_playlist_repo,
            &self.album_repo,
            &self.artist_repo,
            &self.genre_repo,
            &self.scan_roots_repo,
            &self.history_repo,
            &self.search_engine,
            self.app_state.active_nav,
            &self.app_state.search_query,
            self.app_state.current_sort,
            self.app_state.current_sort_order,
        );
    }

    /// Re-evaluate the smart playlists now.
    ///
    /// The user asked for it, so the refresh policy does not get a say: the
    /// cached results are dropped and the next view of a smart playlist runs
    /// its query again.
    pub fn refresh_smart_playlists(&mut self) {
        self.library_state.request_smart_refresh();
        self.refresh_view();
    }

    /// Push the saved refresh policy into the library state.
    ///
    /// The policy is a preference the user edits in Settings, and the state
    /// that consults it lives with the library. A change of policy drops the
    /// cached results, because the new policy may want a different answer.
    pub(super) fn sync_smart_refresh_policy(&mut self) {
        let wanted = self.dsp_state.settings_tab.smart_refresh.clamped();
        if self.library_state.smart_refresh != wanted {
            self.library_state.smart_refresh = wanted;
            self.library_state.clear_smart_cache();
        }
    }

    /// Re-evaluate the smart playlists when the timer policy says so.
    ///
    /// Nothing happens under the other two policies: the library-change
    /// policy is driven by the scanner, and the manual policy by the user.
    pub(super) fn tick_smart_refresh(&mut self) {
        self.sync_smart_refresh_policy();
        if !self.library_state.smart_refresh_due() {
            return;
        }
        self.library_state.request_smart_refresh();
        self.refresh_view();
    }

    /// Rebuild only the visible track list for the current tab/search/sort.
    /// Much cheaper than [`reload_library`] because it does not re-query the
    /// whole 10k-track table or the collection metadata.
    pub fn refresh_view(&mut self) {
        // Settings/Help have no track table; skip the pointless 10k clone.
        if matches!(self.app_state.active_nav, NavItem::Settings | NavItem::Help) {
            return;
        }
        self.library_state.update_displayed_tracks(
            &self.search_engine,
            &self.history_repo,
            &self.playlist_repo,
            &self.smart_playlist_repo,
            &self.album_repo,
            &self.artist_repo,
            &self.genre_repo,
            &self.track_repo,
            self.app_state.active_nav,
            &self.app_state.search_query,
            self.app_state.current_sort,
            self.app_state.current_sort_order,
        );
    }

    /// Scan the library for byte-identical duplicates and load the finder.
    pub(super) fn scan_duplicates(&mut self) {
        let groups = match self.track_repo.find_duplicates() {
            Ok(groups) => groups,
            Err(e) => {
                log::warn!("Could not scan for duplicates: {e}");
                return;
            }
        };
        let groups: Vec<DuplicateGroup> = groups
            .into_iter()
            .map(|tracks| DuplicateGroup {
                // The library has known the lowest-id copy longest, so keep it.
                keep: tracks.first().map(|t| t.id),
                tracks,
            })
            .collect();
        log::info!("Duplicate scan: {} group(s)", groups.len());
        self.duplicates.set_groups(groups);
        self.duplicates.is_open = true;
    }

    /// Open the batch organizer for a set of tracks.
    pub(super) fn open_organize(
        &mut self,
        tracks: Vec<domain::models::Track>,
        scope_label: String,
    ) {
        if tracks.is_empty() {
            self.toasts.info(tr!(Msg::SE08BEDBD4E069BB0).to_string());
            return;
        }
        let label = format!(
            "{} · {} track{}",
            scope_label,
            tracks.len(),
            if tracks.len() == 1 { "" } else { "s" }
        );
        self.organize.open(tracks, label);
    }

    /// Move the files in the reviewed plan and relink the library.
    ///
    /// The plan is rebuilt from the board immediately before applying, so what
    /// moves is exactly what is on screen — not what was on screen a keystroke
    /// ago — and any destination that appeared meanwhile is caught as a
    /// conflict rather than silently clobbered.
    pub(super) fn apply_organize_plan(&mut self) {
        self.organize.plan = library::organize::build_plan(
            &self.organize.tracks,
            &self.organize.pattern,
            self.organize.destination.as_deref(),
        );
        self.organize.plan_dirty = false;

        let plan = self.organize.plan.clone();
        let outcome = library::organize::apply_plan(&plan);

        // Relink each moved row in place: the id, play counts, playlists and
        // artwork all survive a rename.
        let mut relinked = 0usize;
        for (id, new_path, content_hash) in &outcome.moved {
            match self
                .track_repo
                .relink_path(id, new_path, content_hash.as_deref())
            {
                Ok(true) => relinked += 1,
                Ok(false) => log::warn!(
                    "Organizer moved {} but found no library row",
                    new_path.display()
                ),
                Err(e) => log::warn!("Could not relink {}: {e}", new_path.display()),
            }
        }

        // The queue holds resolved paths, so repoint it too.
        let moved: Vec<(domain::ids::TrackId, PathBuf)> = outcome
            .moved
            .iter()
            .map(|(id, path, _)| (*id, path.clone()))
            .collect();
        self.coordinator.remap_paths(&moved);

        // The modal's scope still describes the pre-move world. Point it at the
        // new locations and rebuild, so the preview afterwards says "already in
        // place" instead of claiming the files have vanished.
        for track in self.organize.tracks.iter_mut() {
            if let Some((_, new_path, hash)) =
                outcome.moved.iter().find(|(id, _, _)| *id == track.id)
            {
                track.path = new_path.clone();
                track.content_hash = hash.clone();
            }
        }
        self.organize.force_rebuild();

        // Our own moves must not look like external edits.
        self.watch_suppress_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(5));

        let failed = outcome.failures.len();
        if failed > 0 {
            let detail = outcome
                .failures
                .iter()
                .take(3)
                .map(|(path, reason)| format!("{}: {reason}", path.display()))
                .collect::<Vec<_>>()
                .join("; ");
            log::warn!("Organizer: {relinked} moved, {failed} failed — {detail}");
        } else {
            log::info!("Organizer: moved {relinked} file(s)");
        }

        self.organize.result = Some(if failed == 0 {
            format!("Moved {relinked} file(s).")
        } else {
            format!("Moved {relinked}, {failed} left in place — see the log.")
        });
        self.organize.applied = true;

        if relinked > 0 {
            self.toasts.success(format!(
                "Organized {relinked} file(s){}",
                if failed > 0 {
                    format!(" ({failed} skipped)")
                } else {
                    String::new()
                }
            ));
        }
        self.reload_library();
    }

    /// Stage an M3U8 export and ask the user where to save it.
    pub fn export_playlist_as_m3u(&mut self, name: String, tracks: Vec<domain::models::Track>) {
        self.export_playlist(name, tracks, library::PlaylistFormat::M3u8);
    }

    /// Stage a playlist export in the chosen format.
    pub fn export_playlist(
        &mut self,
        name: String,
        tracks: Vec<domain::models::Track>,
        format: library::PlaylistFormat,
    ) {
        let entries: Vec<library::M3uEntry> = tracks
            .iter()
            .map(|t| library::M3uEntry {
                path: t.path.clone(),
                duration_secs: Some(t.duration_secs.max(0.0).round() as u64),
                artist: Some(t.artist.clone()).filter(|a| !a.trim().is_empty()),
                title: Some(t.title.clone()).filter(|t| !t.trim().is_empty()),
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        self.pending_playlist_export = Some((name.clone(), entries, format));
        self.import_service.request_save_playlist(name, format);
    }

    pub fn start_scan(&mut self, path: PathBuf) {
        if let Err(e) = self.scan_roots_repo.add(&path) {
            log::warn!("Could not add the scan root {}: {e}", path.display());
        }
        self.library_state.scan_roots = match self.scan_roots_repo.list() {
            Ok(roots) => roots,
            Err(e) => {
                log::warn!("Could not list the scan roots: {e}");
                Vec::new()
            }
        };
        // Watch the new root from now on, and scan it once the scanner is free.
        self.rebuild_watchers();
        if !self.scan_queue.contains(&path) {
            self.scan_queue.push_back(path);
        }
        self.scan_toast_wanted = true;
        self.pump_scan_queue();
    }
}

/// Load a playlist's tracks for the editor.
///
/// `unwrap_or_default()` made a failed read look like an empty playlist, which
/// reads as "this playlist has no songs" rather than "the database refused".
pub(super) fn load_playlist_tracks(
    repo: &PlaylistRepo,
    id: &domain::ids::PlaylistId,
) -> Vec<domain::models::Track> {
    match repo.get_tracks(id) {
        Ok(tracks) => tracks,
        Err(e) => {
            log::warn!("Could not load the playlist's tracks: {e}");
            Vec::new()
        }
    }
}

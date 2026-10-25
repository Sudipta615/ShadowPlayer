use super::*;

impl ShadowPlayerApp {
    /// Runs on the UI thread when the native picker returns.
    pub(super) fn handle_import_result(&mut self, result: ImportResult) {
        match result {
            ImportResult::Cancelled => {}
            ImportResult::Folder(path) => {
                // Folder imports become monitored roots (visible in Settings and
                // the Folders tab) and trigger a background scan.
                self.start_scan(path);
            }
            ImportResult::Files(paths) => {
                let mut imported = 0usize;
                let mut dirs: Vec<PathBuf> = Vec::new();

                for path in paths {
                    let is_cue = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("cue"))
                        .unwrap_or(false);

                    if is_cue {
                        if let Ok(cue_tracks) = metadata::extract_cue_metadata(&path) {
                            for meta in &cue_tracks {
                                if self.track_repo.upsert(&meta.track).is_ok() {
                                    imported += 1;
                                }
                            }
                        }
                        continue;
                    }

                    if self.import_file(&path).is_some() {
                        imported += 1;
                        if let Some(parent) = path.parent() {
                            dirs.push(parent.to_path_buf());
                        }
                    }
                }

                if imported > 0 {
                    for dir in dirs {
                        if !self.app_state.imported_dirs.contains(&dir) {
                            self.app_state.imported_dirs.insert(0, dir);
                        }
                    }
                    self.app_state.imported_dirs.truncate(24);
                    self.settings_dirty = true;
                    self.reload_library();
                    log::info!("Imported {} track(s) into the library", imported);
                    if imported > 0 {
                        self.toasts
                            .success(format!("Imported {imported} track(s) into the library"));
                    } else {
                        self.toasts.warning(tr!(Msg::SF20A9D80B386ACFE));
                    }
                }
            }
            ImportResult::Playlist(path) => {
                self.import_m3u_playlist(&path);
            }
            ImportResult::SavePath(path) => {
                self.write_pending_playlist_export(&path);
            }
            ImportResult::OrganizeDestination(path) => {
                self.organize.set_destination(Some(path));
            }
            ImportResult::ClapLibrary(path) => {
                self.open_clap_library(path);
            }
        }
    }

    /// Runs on the UI thread when the EQ curve worker returns.
    ///
    /// The curve is installed into the panel first and pushed to the engine
    /// with the targeted `push_imported_eq`, not `sync_chain_to_session`: an
    /// import must not reopen the output device or renegotiate the resampler.
    /// A failed parse leaves the previously active curve untouched and says
    /// why in a warning toast.
    pub(super) fn handle_preset_io_result(&mut self, event: PresetIoEvent) {
        match event {
            PresetIoEvent::Imported { path, curve } => {
                let graphic = curve.is_graphic();
                let points = curve.point_count();
                self.dsp_state.eq_panel.apply_imported_curve(&curve);
                self.dsp_state.push_imported_eq(&self.session);
                self.settings_dirty = true;
                let file = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("the selected file");
                self.toasts.success(tr_args!(
                    "Imported a {kind} {points}-point curve from {file}",
                    "kind" => if graphic { "graphic" } else { "parametric" },
                    "points" => points,
                    "file" => file
                ));
            }
            PresetIoEvent::Exported(path) => {
                self.toasts.success(tr_args!(
                    "Exported the EQ curve to {file}",
                    "file" => path.display().to_string()
                ));
            }
            PresetIoEvent::Cancelled => {}
            PresetIoEvent::Error(message) => {
                log::warn!("EQ curve import/export failed: {message}");
                self.toasts.warning(message);
            }
        }
    }

    /// Import one audio file into the library, resolving stable identity so a
    /// moved-but-already-known file keeps its id, play counts and favorites.
    /// Returns the track id on success.
    pub(super) fn import_file(&mut self, path: &std::path::Path) -> Option<domain::ids::TrackId> {
        if !library::scanner::is_supported_audio_path(path) {
            log::debug!("Skipping unsupported import: {:?}", path);
            return None;
        }

        match metadata::extract_metadata(path) {
            Ok(mut meta) => {
                // Imported files bypass the scanner, so resolve their identity
                // here too.
                let already_indexed = self.track_repo.get_by_path(path).ok().flatten().is_some();
                let path_derived_id = meta.track.id;
                if let Some(stable_id) = library::resolve_identity(
                    &self.track_repo,
                    path,
                    path_derived_id,
                    already_indexed,
                    &mut meta.track.content_hash,
                ) {
                    meta.track.id = stable_id;
                }
                if let Some((bytes, _mime)) = meta.embedded_artwork.as_ref() {
                    let _ = self
                        .artwork_service
                        .manager()
                        .save_thumbnail(&meta.track.id, bytes);
                }
                let id = meta.track.id;
                match self.track_repo.upsert(&meta.track) {
                    Ok(()) => Some(id),
                    Err(e) => {
                        log::warn!("Could not index {:?}: {e}", path);
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Could not read metadata for {:?}: {}", path, e);
                None
            }
        }
    }

    /// Open an M3U playlist: every referenced file is matched against the
    /// library, importing any that are missing, and the result becomes a new
    /// playlist (named after the file) in playlist order.
    pub(super) fn import_m3u_playlist(&mut self, path: &std::path::Path) {
        let mut parsed = match library::parse_playlist(path) {
            Ok(parsed) => parsed,
            Err(e) => {
                log::warn!("Could not read playlist {}: {e}", path.display());
                return;
            }
        };
        let Some(playlist_file) = parsed.drain(..).next() else {
            log::info!("Playlist {} had no entries", path.display());
            return;
        };
        let entries = playlist_file.entries;
        if entries.is_empty() {
            log::info!("Playlist {} had no entries", path.display());
            return;
        }

        let name = playlist_file
            .title
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .filter(|s| !s.trim().is_empty())
            })
            .unwrap_or_else(|| "Imported Playlist".to_string());

        let playlist = match self.playlist_repo.create(&name, Some("Imported playlist")) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Could not create playlist '{name}': {e}");
                return;
            }
        };

        let mut added = 0usize;
        for entry in &entries {
            let track_id = match self.track_repo.get_by_path(&entry.path) {
                Ok(Some(t)) => Some(t.id),
                // Not in the library yet — index it now (this is what makes
                // "open a playlist from a friend" work for files you already
                // own but have not scanned).
                _ => self.import_file(&entry.path),
            };
            if let Some(id) = track_id {
                if self.playlist_repo.add_track(&playlist.id, &id).is_ok() {
                    added += 1;
                }
            }
        }

        log::info!(
            "Imported playlist '{name}': {added}/{} entries",
            entries.len()
        );
        if added == 0 {
            self.toasts
                .warning(format!("'{name}' matched no files in the library"));
        } else {
            self.toasts.success(format!(
                "Opened playlist '{name}' — {added} of {} tracks",
                entries.len()
            ));
        }
        self.reload_library();
    }

    /// Write the queued export to the path the user chose in the save dialog.
    pub(super) fn write_pending_playlist_export(&mut self, path: &std::path::Path) {
        let Some((_name, entries, format)) = self.pending_playlist_export.take() else {
            return;
        };
        // Prefer relative paths when the playlist lands on the same volume as
        // the music, so it survives being copied alongside the library.
        let text = library::write_playlist(format, &entries, path.parent());
        match std::fs::write(path, text) {
            Ok(()) => {
                log::info!(
                    "Exported {} track(s) as {} to {}",
                    entries.len(),
                    format.label(),
                    path.display()
                );
                self.toasts.success(format!(
                    "Exported {} track(s) as {} to {}",
                    entries.len(),
                    format.label(),
                    path.display()
                ));
            }
            Err(e) => {
                log::warn!("Could not write playlist {}: {e}", path.display());
                self.toasts
                    .warning(format!("Could not write playlist: {e}"));
            }
        }
    }
}

impl ShadowPlayerApp {
    pub(super) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        // 5. Drag & Drop File / Folder Ingestion
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                let path = file.path();
                if path.is_dir() {
                    self.start_scan(path.to_path_buf());
                } else if path.is_file() {
                    let is_cue = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("cue"))
                        .unwrap_or(false);
                    if is_cue {
                        if let Ok(cue_tracks) = metadata::extract_cue_metadata(path) {
                            for meta in &cue_tracks {
                                if let Err(e) = self.track_repo.upsert(&meta.track) {
                                    log::warn!(
                                        "Could not import {}: {e}",
                                        meta.track.path.display()
                                    );
                                }
                            }
                            if let Some(first) = cue_tracks.first().map(|m| m.track.clone()) {
                                let all_t = cue_tracks.into_iter().map(|m| m.track).collect();
                                self.coordinator.play_track(first, Some(all_t));
                            }
                            self.reload_library();
                        }
                    } else if let Ok(meta) = metadata::extract_metadata(path) {
                        if let Err(e) = self.track_repo.upsert(&meta.track) {
                            log::warn!("Could not import {}: {e}", meta.track.path.display());
                        }
                        self.coordinator.play_track(meta.track.clone(), None);
                        self.reload_library();
                    }
                }
            }
        });
    }
}

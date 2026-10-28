use super::super::*;
use super::PlaybackView;

impl ShadowPlayerApp {
    pub(super) fn render_center_panel(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        window_w: f32,
        playback: &PlaybackView<'_>,
    ) {
        // 9. Render Central Panel (Main Views + Right Drawer)
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(self.theme.content_panel_fill())
                    .inner_margin(Margin {
                        left: 10,
                        right: 18,
                        top: 14,
                        bottom: 14,
                    }),
            )
            .show(ui, |root_ui| {
                let full_rect = root_ui.available_rect_before_wrap();
                let avail_w = full_rect.width();
                let avail_h = full_rect.height();
                let is_help_view = self.app_state.active_nav == NavItem::Help;
                let show_right_panel = !is_help_view
                    && self.app_state.active_nav != NavItem::Settings
                    && window_w >= 1100.0
                    && self.app_state.right_panel_visible;
                let right_w = if show_right_panel {
                    if window_w >= 1280.0 {
                        300.0
                    } else {
                        272.0
                    }
                } else {
                    0.0
                };
                let spacing = if show_right_panel { 16.0 } else { 0.0 };
                let center_w = (avail_w - right_w - spacing).max(0.0);

                let center_rect = Rect::from_min_size(full_rect.min, Vec2::new(center_w, avail_h));
                let right_rect = Rect::from_min_size(
                    Pos2::new(full_rect.max.x - right_w, full_rect.min.y),
                    Vec2::new(right_w, avail_h),
                );

                // A. Center View Column
                let mut center_ui = root_ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(center_rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                center_ui.set_clip_rect(center_rect);
                center_ui.set_width(center_w);
                center_ui.set_max_width(center_w);
                let ui = &mut center_ui;
                ui.spacing_mut().item_spacing = Vec2::new(0.0, self.theme.space_lg);

                // Top bar: search plus queue/theme/mini actions. On
                // Settings and Help the search is replaced by the
                // page title, but the actions stay reachable.
                if !is_help_view && self.app_state.active_nav != NavItem::Settings {
                    let mut search_buf = self.app_state.search_query.clone();
                    let mut search_changed = false;
                    ui.horizontal(|ui| {
                        let total_w = ui.available_width();
                        let search_w = total_w.min(500.0);

                        if !show_right_panel {
                            let actions_w = 34.0 * 2.0 + 8.0;
                            let search_pad = ((total_w - actions_w - search_w) * 0.5).max(0.0);
                            ui.add_space(search_pad);
                            ui.allocate_ui_with_layout(
                                Vec2::new(search_w, 38.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    search_changed = crate::components::topbar::render_search_input(
                                        ui,
                                        &mut search_buf,
                                        &self.theme,
                                    );
                                },
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    self.render_topbar_actions(ui, window_w);
                                },
                            );
                        } else {
                            let pad = ((total_w - search_w) * 0.5).max(0.0);
                            ui.add_space(pad);
                            ui.allocate_ui_with_layout(
                                Vec2::new(search_w, 38.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    search_changed = crate::components::topbar::render_search_input(
                                        ui,
                                        &mut search_buf,
                                        &self.theme,
                                    );
                                },
                            );
                        }
                    });
                    if search_changed {
                        self.app_state.search_query = search_buf;
                        self.refresh_view();
                    }
                    // The `/` and `Ctrl+F` shortcuts park the caret in the
                    // search field. The focus has to be requested after the
                    // field has been laid out, otherwise egui has no widget
                    // with that id to focus yet.
                    if self.app_state.focus_search {
                        self.app_state.focus_search = false;
                        ctx.memory_mut(|m| {
                            m.request_focus(crate::components::topbar::search_input_id())
                        });
                    }
                } else {
                    let title = if is_help_view {
                        "Help & Feedback"
                    } else {
                        "Settings"
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .font(crate::theme::font(18.0))
                                .strong()
                                .color(self.theme.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.render_topbar_actions(ui, window_w);

                            // The engine's state belongs with the page's
                            // title, not in a second heading row below it.
                            // Right-to-left reads back to front, so these
                            // land to the left of the actions.
                            if !is_help_view {
                                let bit_perfect = self.dsp_state.settings_tab.bit_perfect;
                                crate::components::page::status_pill(
                                    ui,
                                    &self.theme,
                                    if bit_perfect {
                                        "Bit-Perfect"
                                    } else {
                                        "DSP Active"
                                    },
                                    if bit_perfect {
                                        self.theme.bit_perfect_green
                                    } else {
                                        self.theme.accent_highlight
                                    },
                                    if bit_perfect {
                                        self.theme.bit_perfect_green
                                    } else {
                                        self.theme.text_secondary
                                    },
                                );
                                crate::components::page::status_pill(
                                    ui,
                                    &self.theme,
                                    &format!("Engine {}", engine::VERSION),
                                    self.theme.accent_warm,
                                    self.theme.text_secondary,
                                );
                            }
                        });
                    });
                }

                // View Switcher
                match self.app_state.active_nav {
                    NavItem::Albums => {
                        render_albums_view(
                            ui,
                            &mut self.library_state,
                            &mut self.coordinator,
                            &mut self.artwork_service,
                            &mut self.biography_service,
                            &mut self.app_state.biography_editor,
                            playback.current_track,
                            playback.is_playing,
                            &self.theme,
                            &self.album_repo,
                            &self.album_notes_repo,
                            &self.favorites_repo,
                        );
                    }
                    NavItem::Artists => {
                        render_artists_view(
                            ui,
                            &mut self.library_state,
                            &mut self.coordinator,
                            &mut self.artwork_service,
                            &mut self.biography_service,
                            &mut self.app_state.biography_editor,
                            playback.current_track,
                            playback.is_playing,
                            &self.theme,
                            &self.artist_repo,
                            &self.favorites_repo,
                        );
                    }
                    NavItem::Genres => {
                        render_genres_view(
                            ui,
                            &mut self.library_state,
                            &mut self.coordinator,
                            playback.current_track,
                            playback.is_playing,
                            &self.theme,
                            &self.genre_repo,
                            &self.favorites_repo,
                        );
                    }
                    NavItem::Folders => {
                        let mut scan_to_start = None;
                        let mut import_req = None;
                        let import_active = self.import_service.is_active();
                        let show_stats = render_folders_view(
                            ui,
                            &mut self.library_state,
                            &mut self.scanner_service,
                            &mut self.coordinator,
                            playback.current_track,
                            &self.theme,
                            &self.scan_roots_repo,
                            &self.track_repo,
                            &self.favorites_repo,
                            import_active,
                            |path| scan_to_start = Some(path),
                            |kind| import_req = Some(kind),
                        );
                        if let Some(path) = scan_to_start {
                            self.start_scan(path);
                        }
                        if let Some(kind) = import_req {
                            self.request_import(kind);
                        }
                        if show_stats {
                            self.open_library_stats();
                        }
                    }
                    NavItem::Settings => {
                        let prev_light = self.theme.is_light;
                        let mut scan_to_start = None;
                        let mut import_req = None;
                        let is_scanning = self.scanner_service.is_scanning();
                        let import_active = self.import_service.is_active();
                        let sv_actions = render_settings_view(
                            ui,
                            &mut self.dsp_state,
                            &self.session,
                            &mut self.theme,
                            &self.theme_manager,
                            &self.scan_roots_repo,
                            &self.track_repo,
                            is_scanning,
                            import_active,
                            &self.app_state.imported_dirs,
                            self.loudness_service.progress(),
                            &self.locale,
                            crate::components::settings_tab::CrossfadePreviewContext {
                                playing: self.crossfade_preview.is_playing(),
                                peaks: self.crossfade_preview.peaks(),
                                progress: self.crossfade_preview.progress(),
                                error: self.crossfade_preview.error(),
                            },
                            self.media_tools_service.is_busy(),
                            self.media_tools.export_progress.clone(),
                            self.media_tools_service.activity(),
                            |path| scan_to_start = Some(path),
                            |kind| import_req = Some(kind),
                        );
                        if self.theme.is_light != prev_light {
                            self.settings_dirty = true;
                        }
                        // Settings view can mutate settings_tab directly
                        self.settings_dirty = true;
                        let allow_multiple = self.dsp_state.settings_tab.allow_multiple_instances;
                        if allow_multiple != self.multi_instance_allowed {
                            self.multi_instance_allowed = allow_multiple;
                            if let Err(e) = crate::services::set_multi_instance_enabled(
                                &self.data_dir,
                                allow_multiple,
                            ) {
                                log::warn!("Could not update the multi-instance marker: {e}");
                                self.toasts.warning(format!(
                                    "Could not save the multi-instance preference: {e}"
                                ));
                            }
                        }
                        if let Some(path) = scan_to_start {
                            self.start_scan(path);
                        }
                        if let Some(kind) = import_req {
                            self.request_import(kind);
                        }
                        if sv_actions.toggle_crossfade_preview {
                            let curve = self.dsp_state.settings_tab.crossfade_curve;
                            let duration = self.dsp_state.settings_tab.crossfade_duration_secs;
                            self.crossfade_preview.toggle(curve, duration);
                        }
                        if sv_actions.crossfade_config_changed {
                            let curve = self.dsp_state.settings_tab.crossfade_curve;
                            let duration = self.dsp_state.settings_tab.crossfade_duration_secs;
                            if self.crossfade_preview.is_playing() {
                                self.crossfade_preview.start(curve, duration);
                            } else {
                                self.crossfade_preview.set_preview(curve, duration);
                            }
                        }
                        if let Some(dir) = sv_actions.biography_source_dir_changed.clone() {
                            self.biography_service.set_source_dir(&dir);
                            self.settings_dirty = true;
                        }
                        if sv_actions.start_loudness_scan {
                            let write_tags = self.dsp_state.settings_tab.loudness_write_tags;
                            self.loudness_service.start(write_tags);
                        }
                        if sv_actions.cancel_loudness_scan {
                            self.loudness_service.cancel();
                        }
                        if sv_actions.find_duplicates {
                            self.scan_duplicates();
                        }
                        if let Some(locale) = sv_actions.locale_changed {
                            self.set_locale(locale);
                        }
                        if let Some(preview_id) = sv_actions.preview_theme {
                            self.theme_manager.stage_theme(&preview_id);
                            self.sync_effective_theme();
                        }
                        if let Some(apply_id) = sv_actions.apply_theme {
                            self.theme_manager.stage_theme(&apply_id);
                            if let Some(committed) = self.theme_manager.commit_staged() {
                                let _ = self.settings_repo.set_value("active_theme", &committed);
                            }
                            self.sync_effective_theme();
                            self.settings_dirty = true;
                            self.toasts.success("Theme applied");
                        }
                        if sv_actions.cancel_theme_preview {
                            self.theme_manager.unstage_theme();
                            self.sync_effective_theme();
                        }
                        if sv_actions.reload_themes {
                            self.theme_manager.load_all();
                            self.sync_effective_theme();
                            self.toasts.info("Themes reloaded");
                        }
                        if sv_actions.frosted_glass_changed.is_some() {
                            self.sync_effective_theme();
                            self.settings_dirty = true;
                        }
                        if sv_actions.reduce_motion_changed.is_some() {
                            self.sync_effective_theme();
                            self.settings_dirty = true;
                        }
                        if sv_actions.import_theme {
                            if let Some(paths) = rfd::FileDialog::new()
                                .set_title("Import ShadowPlayer Theme")
                                .add_filter(
                                    "ShadowPlayer Theme (*.shadowtheme, *.json)",
                                    &["shadowtheme", "json"],
                                )
                                .pick_files()
                            {
                                let mut imported_count = 0;
                                let mut last_theme_id = None;
                                for path in paths {
                                    match self.theme_manager.import_theme_from_path(&path) {
                                        Ok(theme_id) => {
                                            imported_count += 1;
                                            last_theme_id = Some(theme_id);
                                        }
                                        Err(err) => {
                                            self.toasts.warning(format!(
                                                "Failed to import {}: {err}",
                                                path.display()
                                            ));
                                        }
                                    }
                                }
                                if let Some(theme_id) = last_theme_id {
                                    self.theme_manager.stage_theme(&theme_id);
                                    if let Some(committed) = self.theme_manager.commit_staged() {
                                        let _ = self
                                            .settings_repo
                                            .set_value("active_theme", &committed);
                                    }
                                    self.sync_effective_theme();
                                    self.settings_dirty = true;
                                    self.toasts.success(format!(
                                        "Imported {imported_count} theme(s) into custom-theme/"
                                    ));
                                }
                            }
                        }
                        if sv_actions.open_artwork_fetch {
                            let missing_count = self
                                .library_state
                                .all_tracks
                                .iter()
                                .filter(|t| !self.artwork_service.has_artwork(&t.id, &t.path))
                                .count();
                            self.artwork_fetch_modal.open(missing_count);
                        }
                        if sv_actions.open_room_measurement {
                            self.room_measurement.open();
                        }
                        if sv_actions.open_speaker_setup {
                            self.app_state.speaker_setup_modal.open();
                        }
                        if sv_actions.open_cd_ripper {
                            self.media_tools.open(MediaToolsTab::Cd);
                        }
                        if sv_actions.open_transcode_export {
                            self.media_tools.open(MediaToolsTab::Export);
                        }
                        if sv_actions.open_update_check {
                            self.media_tools.update_enabled =
                                self.dsp_state.settings_tab.update_check_enabled;
                            self.media_tools.open(MediaToolsTab::Update);
                        }
                        if sv_actions.pick_converter_dir {
                            if let Some(folder) = rfd::FileDialog::new()
                                .set_title("Select Destination Folder")
                                .pick_folder()
                            {
                                self.dsp_state.settings_tab.converter_output_dir = folder;
                                self.settings_dirty = true;
                            }
                        }
                        if sv_actions.pick_converter_files {
                            if let Some(files) = rfd::FileDialog::new()
                                .set_title("Select Audio Files to Convert")
                                .add_filter(
                                    "Audio Files",
                                    &[
                                        "flac", "wav", "mp3", "m4a", "aac", "ogg", "opus", "aiff",
                                        "alac", "wv", "dsf", "dff", "caf",
                                    ],
                                )
                                .pick_files()
                            {
                                for file in files {
                                    if !self
                                        .dsp_state
                                        .settings_tab
                                        .converter_input_files
                                        .contains(&file)
                                    {
                                        self.dsp_state
                                            .settings_tab
                                            .converter_input_files
                                            .push(file);
                                    }
                                }
                            }
                        }
                        if sv_actions.clear_converter_queue {
                            self.dsp_state.settings_tab.converter_input_files.clear();
                        }
                        if sv_actions.start_file_conversion {
                            if self.dsp_state.settings_tab.converter_input_files.is_empty() {
                                self.toasts
                                    .warning("Add audio files to the queue before converting");
                            } else if self
                                .dsp_state
                                .settings_tab
                                .converter_output_dir
                                .as_os_str()
                                .is_empty()
                            {
                                self.toasts
                                    .warning("Select a destination folder before converting");
                            } else {
                                let options = media_tools::transcode::TranscodeOptions {
                                    format: self.dsp_state.settings_tab.converter_format,
                                    sample_rate: if self
                                        .dsp_state
                                        .settings_tab
                                        .converter_sample_rate
                                        > 0
                                    {
                                        Some(self.dsp_state.settings_tab.converter_sample_rate)
                                    } else {
                                        None
                                    },
                                    channels: if self.dsp_state.settings_tab.converter_channels > 0
                                    {
                                        Some(
                                            self.dsp_state.settings_tab.converter_channels as usize,
                                        )
                                    } else {
                                        None
                                    },
                                    copy_tags: self.dsp_state.settings_tab.converter_copy_tags,
                                };
                                let inputs =
                                    self.dsp_state.settings_tab.converter_input_files.clone();
                                let output_dir =
                                    self.dsp_state.settings_tab.converter_output_dir.clone();
                                let pattern = self
                                    .dsp_state
                                    .settings_tab
                                    .converter_naming_pattern
                                    .trim()
                                    .to_string();
                                let naming_pattern = if pattern.is_empty() {
                                    None
                                } else {
                                    Some(pattern)
                                };
                                let overwrite = self.dsp_state.settings_tab.converter_overwrite;
                                self.dsp_state.settings_tab.converter_status_message =
                                    Some("Converting files…".to_string());
                                let started =
                                    self.media_tools_service.transcode_batch_with_options(
                                        inputs,
                                        output_dir,
                                        options,
                                        naming_pattern,
                                        overwrite,
                                    );
                                if started {
                                    self.toasts.info("Batch conversion started");
                                }
                            }
                        }
                        if sv_actions.cancel_file_conversion {
                            self.media_tools_service.cancel();
                            self.dsp_state.settings_tab.converter_status_message =
                                Some("Conversion cancelled".to_string());
                            self.toasts.info("Conversion cancelled");
                        }
                    }
                    NavItem::Podcasts => {
                        let action = crate::components::podcasts_view::PodcastsView {
                            state: &mut self.podcast_state,
                            busy: self.podcast_service.is_busy(),
                            status: self.podcast_service.status(),
                            theme: &self.theme,
                        }
                        .show(ui);

                        if let Some(url) = action.add_feed {
                            match self.podcast_service.add_feed(&url) {
                                Ok(()) => {
                                    self.podcast_state.new_feed_url.clear();
                                    self.podcast_state.error = None;
                                }
                                Err(e) => self.podcast_state.error = Some(e),
                            }
                        }
                        if let Some(id) = action.select_feed {
                            self.podcast_state.select_feed(&self.podcast_repo, id);
                        }
                        if let Some(id) = action.refresh_feed {
                            if let Some(feed) = self
                                .podcast_state
                                .feeds
                                .iter()
                                .find(|feed| feed.id == id)
                                .cloned()
                            {
                                if let Err(e) = self.podcast_service.refresh_feed(&feed) {
                                    self.podcast_state.error = Some(e);
                                }
                            }
                        }
                        if let Some(id) = action.delete_feed {
                            // If this feed owns the loaded episode, stop the
                            // transport before the rows disappear.
                            let playing_here = self
                                .podcast_state
                                .playing_episode
                                .and_then(|episode| {
                                    self.podcast_repo.get_episode(&episode).ok().flatten()
                                })
                                .is_some_and(|episode| episode.feed_id == id);
                            if playing_here {
                                self.session.stop();
                                self.podcast_state.is_playing = false;
                                self.podcast_state.playing_episode = None;
                            }
                            if let Err(e) = self.podcast_repo.delete_feed(&id) {
                                self.podcast_state.error = Some(tr_args!(
                                    "Could not remove the podcast feed: {error}",
                                    "error" => e
                                ));
                            } else {
                                self.podcast_state.selected_feed = None;
                                self.podcast_state.reload_feeds(&self.podcast_repo);
                            }
                        }
                        if let Some(id) = action.download_episode {
                            if let Some(episode) = self
                                .podcast_state
                                .episodes
                                .iter()
                                .find(|episode| episode.id == id)
                                .cloned()
                            {
                                if let Err(e) = self.podcast_service.download_episode(&episode) {
                                    self.podcast_state.error = Some(e);
                                }
                            }
                        }
                        if let Some(id) = action.play_episode {
                            self.play_podcast_episode(id);
                        }
                        if action.toggle_playback {
                            self.toggle_podcast_playback();
                        }
                        if let Some(position) = action.seek_to {
                            if let Some(id) = self.podcast_state.playing_episode {
                                self.session.seek(position);
                                self.podcast_state.position_secs = position;
                                let _ = self.podcast_repo.save_progress(&id, position, false);
                            }
                        }
                    }
                    NavItem::Help => {
                        render_help_view(
                            ui,
                            &mut self.help_state,
                            &self.theme,
                            &mut self.app_state,
                            &mut self.dsp_state,
                        );
                    }
                    _ => {
                        let mut view_action = render_main_playback_view(
                            ui,
                            &mut self.app_state,
                            &mut self.library_state,
                            &mut self.coordinator,
                            &self.session,
                            &mut self.artwork_service,
                            playback.telemetry,
                            playback
                                .current_track
                                .and_then(|t| self.waveform_service.peaks(&t.id)),
                            playback.current_track,
                            &self.theme,
                            &self.track_repo,
                            &self.playlist_repo,
                            &self.favorites_repo,
                            self.dsp_state.eq_panel.is_open,
                        );
                        if let Some(q) = view_action.search_query.take() {
                            self.app_state.search_query = q;
                            self.refresh_view();
                        }
                        if let Some(nav) = view_action.switch_nav {
                            self.app_state.active_nav = nav;
                            self.library_state.selected_album = None;
                            self.library_state.selected_artist = None;
                            self.library_state.selected_genre = None;
                            self.library_state.selected_folder = None;
                            self.refresh_view();
                        }
                        if view_action.create_playlist {
                            self.app_state.open_playlist_editor = true;
                        }
                        if view_action.open_eq_panel {
                            self.dsp_state.eq_panel.is_open = !self.dsp_state.eq_panel.is_open;
                            self.settings_dirty = true;
                        }
                        if view_action.settings_dirty {
                            self.settings_dirty = true;
                        }
                        if view_action.open_loudness {
                            self.open_loudness_inspector();
                        }
                        if view_action.needs_reload {
                            self.refresh_view();
                        }
                        if view_action.removed_track {
                            self.reload_library();
                        }
                        if let Some(sp_id) = view_action.edit_smart_playlist {
                            if let Some(sp) = self
                                .library_state
                                .smart_playlists
                                .iter()
                                .find(|s| s.id == sp_id)
                                .cloned()
                            {
                                self.smart_playlist_editor.open_existing(&sp);
                            }
                        }
                        if view_action.refresh_smart_playlist {
                            self.refresh_smart_playlists();
                        }
                        if let Some(seed) = view_action.start_radio {
                            self.start_radio(seed);
                        }
                        if view_action.refresh_radio {
                            self.refresh_radio();
                        }
                        if let Some(tracks) = view_action.organize_tracks {
                            self.open_organize(tracks, "Selection".to_string());
                        }
                        if let Some(tracks) = view_action.autotag_tracks {
                            let pairs: Vec<(domain::ids::TrackId, PathBuf)> =
                                tracks.iter().map(|t| (t.id, t.path.clone())).collect();
                            self.autotag_service.enqueue(pairs, false);
                            self.autotag_modal
                                .open_progress(self.autotag_service.progress().clone());
                        }
                        if let Some(tracks) = view_action.fetch_artwork_tracks {
                            let missing_count = tracks
                                .iter()
                                .filter(|t| !self.artwork_service.has_artwork(&t.id, &t.path))
                                .count();
                            self.artwork_fetch_modal.open(missing_count);
                            self.artwork_fetch_service
                                .start(tracks, self.artwork_service.manager());
                        }
                    }
                }
                // B. Right Column Drawer
                self.render_drawer(
                    root_ui,
                    ctx,
                    window_w,
                    super::drawer::DrawerLayout {
                        full_rect,
                        avail_w,
                        avail_h,
                        show_right_panel,
                        right_w,
                        right_rect,
                    },
                    playback,
                );
                root_ui.allocate_rect(full_rect, Sense::hover());
            });
    }
}

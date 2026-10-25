use super::*;

impl ShadowPlayerApp {
    /// (Re)create one recursive filesystem watcher per scan root. A changed file
    /// only signals the UI thread; the actual rescan is debounced and serialised
    /// with the scanner so a slow disk never gets hammered.
    pub(super) fn rebuild_watchers(&mut self) {
        self.watchers.clear();
        let roots: Vec<PathBuf> = self
            .library_state
            .scan_roots
            .iter()
            .filter(|r| r.watch_enabled)
            .map(|r| r.path.clone())
            .collect();
        for root in roots {
            let tx = self.watch_tx.clone();
            let wake = self.wake_ctx.clone();
            let signal_root = root.clone();
            match library::LibraryWatcher::watch(&root, move |_paths| {
                // Coalesce: the drain side debounces anyway.
                let _ = tx.send(signal_root.clone());
                if let Some(ctx) = &wake {
                    ctx.request_repaint();
                }
            }) {
                Ok(w) => self.watchers.push(w),
                Err(e) => log::warn!("Could not watch {}: {e}", root.display()),
            }
        }
    }

    /// Queue every known scan root once, so a launch after some downtime
    /// reconciles files that appeared while the app was not running.
    pub(super) fn schedule_startup_scan(&mut self) {
        let roots: Vec<PathBuf> = self
            .library_state
            .scan_roots
            .iter()
            .map(|r| r.path.clone())
            .collect();
        for root in roots {
            if !self.scan_queue.contains(&root) {
                self.scan_queue.push_back(root);
            }
        }
        self.pump_scan_queue();
    }

    /// Start the next queued scan if the scanner is idle.
    pub(super) fn pump_scan_queue(&mut self) {
        if self.scanner_service.is_scanning() {
            return;
        }
        if let Some(next) = self.scan_queue.pop_front() {
            self.scanner_service.start_scan(next);
        }
    }

    /// Drain watcher signals and, after a short quiet period, enqueue a rescan.
    pub(super) fn poll_watchers(&mut self) {
        // A batch organize moves files itself. Those changes are already
        // reflected in the library database, so re-scanning would be pure
        // duplicate work — and the events trail the move by a moment, so a
        // window is needed rather than a single drain.
        if let Some(until) = self.watch_suppress_until {
            if std::time::Instant::now() < until {
                while self.watch_rx.try_recv().is_ok() {}
                self.watch_pending = None;
                return;
            }
            self.watch_suppress_until = None;
        }

        while let Ok(root) = self.watch_rx.try_recv() {
            self.watch_pending = Some((root, std::time::Instant::now()));
        }

        const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(2500);
        if let Some((root, at)) = self.watch_pending.clone() {
            if at.elapsed() < DEBOUNCE {
                return;
            }
            // Never race the loudness scanner: with tag write-back enabled it
            // modifies files, which would otherwise retrigger the watcher.
            if self.loudness_service.is_running() {
                return;
            }
            self.watch_pending = None;
            if !self.scan_queue.contains(&root) {
                self.scan_queue.push_back(root);
            }
            self.scan_toast_wanted = true;
            self.pump_scan_queue();
        }
    }

    /// Apply a finished podcast fetch/download on the UI thread.
    pub(super) fn poll_podcast_service(&mut self) {
        let Some(event) = self.podcast_service.poll() else {
            return;
        };
        match event {
            PodcastEvent::FeedUpdated { feed_id } => {
                self.podcast_state.reload_feeds(&self.podcast_repo);
                self.podcast_state.selected_feed = Some(feed_id);
                self.podcast_state.reload_episodes(&self.podcast_repo);
                self.podcast_state.error = None;
                self.toasts.success(tr!("Podcast feed updated"));
            }
            PodcastEvent::Downloaded { episode_id } => {
                self.podcast_state.reload_episodes(&self.podcast_repo);
                self.podcast_state.error = None;
                if let Some(episode) = self.podcast_state.episode(episode_id) {
                    self.toasts.success(tr_args!(
                        "Downloaded “{title}”",
                        "title" => episode.title
                    ));
                }
            }
            PodcastEvent::Error(message) => {
                self.podcast_state.error = Some(message.clone());
                self.toasts.warning(message);
            }
        }
    }

    /// Persist podcast resume state and notice the end of an episode.
    pub(super) fn poll_podcast_playback(&mut self, ctx: &egui::Context) {
        // Starting a music track from the library takes the output back. Do
        // not keep crediting the podcast transport in that case.
        if self.coordinator.current_track().is_some() && self.session.is_playing() {
            self.podcast_state.is_playing = false;
            self.podcast_state.playing_episode = None;
        }

        let Some(id) = self.podcast_state.playing_episode else {
            return;
        };
        let position = self.session.position_secs();
        // Prefer the decoder's duration once the episode is open: feed
        // metadata is often approximate, and a duration that is a minute too
        // long would keep the episode from ever reaching its completion
        // threshold. Fall back to the stored duration while the file is still
        // being probed.
        let live_duration = self.session.duration_secs();
        let duration = if live_duration > 0.0 {
            live_duration
        } else {
            self.podcast_state.duration_secs
        };
        self.podcast_state.position_secs = position;
        self.podcast_state.duration_secs = duration;

        let finished = self.podcast_state.is_playing
            && !self.session.is_playing()
            && duration > 0.0
            && position >= duration - 0.75;
        if finished {
            let completed_at = duration;
            let _ = self.podcast_repo.save_progress(&id, completed_at, true);
            self.podcast_state.is_playing = false;
            self.podcast_state.position_secs = completed_at;
            self.podcast_state.last_saved_secs = completed_at;
            self.podcast_state.reload_episodes(&self.podcast_repo);
            self.toasts.info(tr!("Podcast episode finished"));
            return;
        }

        if self.podcast_state.is_playing
            && (position - self.podcast_state.last_saved_secs).abs() >= 5.0
        {
            let _ = self.podcast_repo.save_progress(&id, position, false);
            self.podcast_state.last_saved_secs = position;
            if let Some(episode) = self
                .podcast_state
                .episodes
                .iter_mut()
                .find(|episode| episode.id == id)
            {
                episode.position_secs = position;
            }
            ctx.request_repaint();
        }
    }

    /// Build a local radio station around `seed_id` and start it.
    ///
    /// The station is a pure query over the user's own library: same genre,
    /// nearby BPM, similar rating. A fresh salt gives a new shuffle without
    /// making the query non-deterministic.
    pub(super) fn start_radio(&mut self, seed_id: domain::ids::TrackId) {
        let Some(seed) = self.track_repo.get_by_id(&seed_id).ok().flatten() else {
            self.toasts
                .warning(tr!("That track is no longer in the library"));
            return;
        };
        let salt = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let mut tracks = vec![seed.clone()];
        match self.track_repo.radio_station(&seed_id, 100, salt) {
            Ok(next) => tracks.extend(next),
            Err(e) => {
                log::warn!("Could not build a radio station: {e}");
                self.toasts.warning(tr!("Could not build a radio station"));
            }
        }

        self.library_state.set_radio_station(
            seed_id,
            tr_args!("{artist} Radio", "artist" => seed.artist),
            tracks.clone(),
        );
        // The station is its own view. A search left over from the songs
        // table would filter the station down to whatever matched, which is
        // not what "Start Radio" promises.
        self.app_state.search_query.clear();
        self.library_state.selected_album = None;
        self.library_state.selected_artist = None;
        self.library_state.selected_genre = None;
        self.library_state.selected_folder = None;
        self.library_state.clear_selection();
        self.app_state.active_nav = NavItem::Radio;
        self.refresh_view();

        if let Some(first) = tracks.first().cloned() {
            self.coordinator.play_track(first, Some(tracks));
        }
    }

    /// Regenerate the current station with a new shuffle.
    pub(super) fn refresh_radio(&mut self) {
        if let Some(seed) = self.library_state.radio_seed {
            self.start_radio(seed);
        } else {
            self.toasts
                .info(tr!("Start a station from any song's context menu first"));
        }
    }

    /// Start (or resume) an episode through the main output.
    pub(super) fn play_podcast_episode(&mut self, id: domain::ids::PodcastEpisodeId) {
        let Some(episode) = self
            .podcast_repo
            .get_episode(&id)
            .ok()
            .flatten()
            .or_else(|| self.podcast_state.episode(id).cloned())
        else {
            return;
        };
        let Some(path) = episode.local_path.clone() else {
            self.toasts
                .warning(tr!("Download this episode before playing it"));
            return;
        };
        if !path.exists() {
            self.toasts
                .warning(tr!("The downloaded episode is missing; download it again"));
            return;
        }

        // The two transports share the output. Stop the music queue first so
        // its end-of-track handler cannot advance over the podcast.
        self.session.stop();
        self.coordinator.clear_queue();

        self.session.open_file(&path);
        let resume_at = if episode.completed {
            0.0
        } else {
            episode.position_secs.max(0.0)
        };
        if resume_at > 0.0 {
            self.session.seek(resume_at);
        }
        self.session.play();

        self.podcast_state.playing_episode = Some(id);
        self.podcast_state.is_playing = true;
        self.podcast_state.position_secs = resume_at;
        self.podcast_state.duration_secs = episode.duration_secs.unwrap_or(0.0);
        self.podcast_state.last_saved_secs = resume_at;
        self.podcast_state.error = None;
    }

    pub(super) fn toggle_podcast_playback(&mut self) {
        if self.podcast_state.is_playing {
            self.session.pause();
            self.podcast_state.is_playing = false;
            if let Some(id) = self.podcast_state.playing_episode {
                let _ =
                    self.podcast_repo
                        .save_progress(&id, self.podcast_state.position_secs, false);
            }
            return;
        }
        if self.podcast_state.playing_episode.is_some() {
            // Reaching the end leaves the source stopped at its duration; a
            // resume from there would replay a single silent frame.
            let at_end = self.podcast_state.duration_secs > 0.0
                && self.podcast_state.position_secs >= self.podcast_state.duration_secs - 0.75;
            if at_end {
                self.session.seek(0.0);
                self.podcast_state.position_secs = 0.0;
            }
            self.session.play();
            self.podcast_state.is_playing = true;
        }
    }

    /// Drain background media-tool jobs into the Tools modal.
    pub(super) fn poll_media_tools(&mut self) {
        for event in self.media_tools_service.poll() {
            match event {
                MediaToolEvent::DiscProbed(result) => match result {
                    Ok(disc) => self.media_tools.apply_disc(disc),
                    Err(error) => {
                        self.media_tools.cd_error = Some(error);
                        self.media_tools.disc = None;
                    }
                },
                MediaToolEvent::RipProgress(progress) => {
                    self.media_tools.cd_progress = Some((
                        progress.track_number,
                        progress.track_count,
                        progress.track_title,
                    ));
                }
                MediaToolEvent::RipFinished(result) => {
                    self.media_tools.cd_progress = None;
                    match result {
                        Ok(report) => {
                            self.media_tools.cd_message = Some(format!(
                                "Ripped {} file(s){}",
                                report.files.len(),
                                if report.warnings.is_empty() {
                                    String::new()
                                } else {
                                    format!(" · {} warning(s)", report.warnings.len())
                                }
                            ));
                            self.media_tools.cd_error = None;
                            if !report.warnings.is_empty() {
                                self.toasts.warning(report.warnings.join("; "));
                            } else {
                                self.toasts.info(format!(
                                    "Ripped {} track(s) from CD",
                                    report.files.len()
                                ));
                            }
                        }
                        Err(error) => {
                            self.media_tools.cd_error = Some(error);
                        }
                    }
                }
                MediaToolEvent::TranscodeProgress {
                    done,
                    total,
                    current,
                } => {
                    self.media_tools.export_progress = Some((done, total, current));
                }
                MediaToolEvent::TranscodeFinished(result) => {
                    self.media_tools.export_progress = None;
                    match result {
                        Ok(reports) => {
                            self.media_tools.export_message = Some(format!(
                                "Exported {} file(s) to {}",
                                reports.len(),
                                self.media_tools.export_output_dir.display()
                            ));
                            self.media_tools.export_error = None;
                            self.toasts
                                .info(format!("Exported {} file(s)", reports.len()));
                        }
                        Err(error) => self.media_tools.export_error = Some(error),
                    }
                }
                MediaToolEvent::UpdateChecked(result) => match result {
                    Ok(check) => {
                        self.media_tools.update_message = Some(if check.newer_available {
                            format!("Version {} is available", check.release.version)
                        } else {
                            "You are running the newest release".to_string()
                        });
                        self.media_tools.update_error = None;
                        self.media_tools.update_check = Some(check);
                    }
                    Err(error) => {
                        self.media_tools.update_error = Some(error);
                        self.media_tools.update_check = None;
                    }
                },
                MediaToolEvent::UpdateDownloaded(result) => match result {
                    Ok(download) => {
                        self.media_tools.update_message =
                            Some(format!("Verified {} · SHA-256 matched", download.version));
                        self.media_tools.update_error = None;
                        self.media_tools.update_download = Some(download);
                    }
                    Err(error) => self.media_tools.update_error = Some(error),
                },
            }
        }
        self.media_tools.busy = self.media_tools_service.is_busy();
        self.media_tools.activity = self.media_tools_service.activity().to_string();
    }
}

impl ShadowPlayerApp {
    pub(super) fn poll_services(&mut self, ctx: &egui::Context) -> bool {
        // 1. Poll Coordinator & Session events
        self.coordinator.poll_events();
        for event in self.coordinator.take_room_measurement_events() {
            self.room_measurement.apply_event(event);
        }
        if self.room_measurement.needs_devices {
            self.room_measurement.needs_devices = false;
            self.coordinator.enumerate_measurement_inputs();
        }
        if self.room_measurement.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        for notice in self.coordinator.take_engine_notices() {
            self.toasts.warning(notice);
        }

        // A track's own DSP profile is applied when the track starts, so it
        // plays with its own curve rather than the global one.
        self.apply_track_dsp_profile();

        // The coordinator drains the bounded output-event channel and
        // republishes device arrivals/changes. A preset bound to that device
        // is applied here, after track overrides had their chance.
        if let Some(device) = self.coordinator.take_output_device_change() {
            self.apply_device_bound_preset(&device);
        }

        // 2. Poll Library Scanner progress
        if self.scanner_service.poll() {
            // The library changed, so the smart playlists may have new
            // answers. Under the library-change policy that is the trigger;
            // under the manual policy it changes nothing. The note comes
            // first, so the rebuild below already sees it.
            self.library_state.note_library_change();
            self.reload_library();
            // A finished scan may unblock the next queued root.
            self.pump_scan_queue();
            // Only announce scans the user (or a watched folder) asked for;
            // the quiet startup catch-up should not pop a toast on every launch.
            if self.scan_toast_wanted {
                self.scan_toast_wanted = false;
                let msg = self.scanner_service.progress_text().to_string();
                self.toasts.info(msg);
            }
        }

        // 2a. Folders that changed on disk (debounced) feed the scan queue.
        self.poll_watchers();

        // 2b. Poll the native import picker (runs on a worker thread)
        if let Some(result) = self.import_service.poll() {
            self.handle_import_result(result);
        }

        // 2b2. Poll the EQ curve worker (file dialogs and parsing off-thread).
        if let Some(event) = self.preset_io.poll() {
            self.handle_preset_io_result(event);
        }

        // 2c. Sleep timer (countdown or end-of-track).
        self.update_sleep_timer(ctx);

        // 2c2. Smart playlists: the timer policy re-evaluates them on its own.
        self.tick_smart_refresh();

        // 2d. Desktop integration: apply media-key transport, publish state.
        self.handle_mpris(ctx);

        // 2e. Library-wide loudness scan progress.
        if self.loudness_service.poll() {
            self.reload_library();
            let summary = self.loudness_service.progress().current.clone();
            if !summary.is_empty() {
                self.toasts.info(summary);
            }
        }

        // 2f. Embedded lyric tags are read on a worker thread; pick up the
        // result as soon as it lands so the lyrics panel fills in.
        if self.lyrics_service.poll() {
            ctx.request_repaint();
        }

        // 2f0. Waveform decodes and podcast network work also happen off the
        // UI thread. A finished waveform is only a repaint; a finished podcast
        // job changes rows the view is showing.
        if self.waveform_service.poll() {
            ctx.request_repaint();
        }
        self.poll_podcast_service();
        self.poll_podcast_playback(ctx);
        if self.crossfade_preview.is_playing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        self.poll_media_tools();
        if self.media_tools_service.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        // 2f1. Auto-tag service polling.
        if self.autotag_service.poll() {
            ctx.request_repaint();
        }
        self.autotag_modal
            .mark_progress(self.autotag_service.progress().clone());
        if !self.autotag_service.pending.is_empty() {
            let pending = std::mem::take(&mut self.autotag_service.pending);
            for res in pending {
                if let Some(track) = self
                    .library_state
                    .all_tracks
                    .iter()
                    .find(|t| t.id == res.track_id)
                    .cloned()
                {
                    self.autotag_modal.push_result(res, track);
                }
            }
            ctx.request_repaint();
        }

        // 2f2. Cover art fetch service polling.
        let saved_art_tracks = self.artwork_fetch_service.poll();
        self.artwork_fetch_modal.progress = self.artwork_fetch_service.progress().clone();
        if !self.artwork_fetch_service.is_running()
            && self.artwork_fetch_modal.progress.total > 0
            && self.artwork_fetch_modal.progress.done >= self.artwork_fetch_modal.progress.total
        {
            self.artwork_fetch_modal.finished = true;
        }
        if !saved_art_tracks.is_empty() {
            for track_id in &saved_art_tracks {
                self.artwork_service.invalidate_track(track_id);
            }
            self.reload_library();
            ctx.request_repaint();
        }

        // 2g. Chain-preset feedback. Saving or applying a profile also marks
        // the settings dirty so it persists now rather than at the next
        // periodic save.
        if let Some(msg) = self.dsp_state.settings_tab.chain_preset_notice.take() {
            self.toasts.success(msg);
            self.settings_dirty = true;
        }

        // 2h. The Settings > Library card asks for the batch organizer. Raised
        // here because neither settings surface owns app-level modals.
        if std::mem::take(&mut self.dsp_state.settings_tab.request_organize) {
            let tracks = self.library_state.all_tracks.clone();
            self.open_organize(tracks, "Whole library".to_string());
        }
        if std::mem::take(&mut self.dsp_state.settings_tab.request_clap_library) {
            self.request_import(ImportKind::ClapLibrary);
        }

        // 2i. CLAP plugin rack. Three things happen every frame:
        // - a preset save waiting on the plugin's state is completed or timed
        //   out (the engine answers off the frame loop),
        // - the settings card gets the plugin's current description,
        // - a library probe that has answered is acted on: one plugin in the
        //   file loads itself, several wait for the user to choose.
        self.dsp_state.poll_clap_state_capture(&self.session);
        self.dsp_state.refresh_clap_summary(&self.session);
        self.autoload_probed_clap_plugin();
        if self.clap_refresh_frames > 0 {
            self.clap_refresh_frames -= 1;
            self.session.refresh_clap_plugin();
            ctx.request_repaint();
        }
        if let Some(msg) = self.dsp_state.clap_rack.pending_toast.take() {
            self.toasts.info(msg);
        }

        // 3. Reactive Repaints (capped at 30fps during playback for low CPU; 0% idle when paused)
        let is_playing = self.session.is_playing();
        if is_playing || self.scanner_service.is_scanning() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        } else if self.loudness_service.is_running() {
            // Progress bar only needs a couple of updates per second.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        } else if self.import_service.is_active() {
            // Keep polling briskly while the OS picker is open, cheaply when idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.app_state.sleep_timer.is_some() {
            // Tick the timer display roughly once per second while idle.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else if self.watch_pending.is_some() || !self.scan_queue.is_empty() {
            // A watched folder changed: keep ticking until the debounce elapses
            // and the queued rescan starts.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        } else if !self.toasts.is_empty() {
            // Keep the fade-out animating while a notification is on screen.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        } else if self.lyrics_service.is_loading() || self.waveform_service.has_pending() {
            // Waiting on an embedded-lyrics or waveform read: tick until it
            // lands, even while paused.
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        // 3b. Toast notifications ride above everything else.
        self.toasts.prune();
        self.toasts.show(ctx, &self.theme);
        is_playing
    }
}

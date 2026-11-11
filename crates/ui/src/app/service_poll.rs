use super::repaint_policy::{wake_after, RepaintSignals};
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
                            let count = reports.len();
                            self.media_tools.export_message = Some(format!(
                                "Exported {} file(s) to {}",
                                count,
                                self.media_tools.export_output_dir.display()
                            ));
                            self.dsp_state.settings_tab.converter_status_message =
                                Some(format!("Successfully converted {count} file(s)"));
                            self.media_tools.export_error = None;
                            self.toasts
                                .info(format!("Converted/Exported {} file(s)", count));
                        }
                        Err(error) => {
                            self.dsp_state.settings_tab.converter_status_message =
                                Some(format!("Conversion failed: {error}"));
                            self.media_tools.export_error = Some(error);
                        }
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
    /// 1. Poll Coordinator, audio session, DSP events, and room measurement.
    fn poll_audio_dsp_subsystems(&mut self, ctx: &egui::Context) {
        // Coordinator & Session events
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

        // Evaluate ZoneSwitch routing rules when a track starts or transitions,
        // redirecting playback to the designated zone and audio output device.
        self.apply_track_zone_switch();

        // The coordinator drains the bounded output-event channel and
        // republishes device arrivals/changes. A preset bound to that device
        // is applied here, after track overrides had their chance.
        if let Some(device) = self.coordinator.take_output_device_change() {
            self.apply_device_bound_preset(&device);
        }

        // Decode the embedded AutoEQ corpus once, when its dialog is first
        // opened. The worker owns the allocation; this loop only moves the
        // finished catalog into the modal state.
        if self.dsp_state.autoeq.is_open {
            if let Err(error) = self.autoeq_catalog.ensure_started() {
                self.dsp_state.autoeq.set_catalog_error(error);
            }
            if let Some(result) = self.autoeq_catalog.poll() {
                match result {
                    Ok(catalog) => self.dsp_state.autoeq.set_catalog(catalog),
                    Err(error) => self.dsp_state.autoeq.set_catalog_error(error),
                }
                ctx.request_repaint();
            } else if self.autoeq_catalog.is_loading() {
                // Loading is an active task, not a heartbeat: stop requesting
                // repaints as soon as the worker reports or the dialog closes.
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }

        // Chain-preset feedback. Saving or applying a profile also marks
        // the settings dirty so it persists now rather than at the next
        // periodic save.
        if let Some(msg) = self.dsp_state.settings_tab.chain_preset_notice.take() {
            self.toasts.success(msg);
            self.settings_dirty = true;
        }

        // CLAP plugin rack. Three things happen every frame:
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
        if std::mem::take(&mut self.dsp_state.settings_tab.request_clap_library) {
            self.request_import(ImportKind::ClapLibrary);
        }
    }

    /// 2. Poll library scanner, watchers, imports, file tasks, and tagger.
    fn poll_library_file_subsystems(&mut self, ctx: &egui::Context) {
        // Poll Library Scanner progress
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

        // Folders that changed on disk (debounced) feed the scan queue.
        self.poll_watchers();

        // Poll the native import picker (runs on a worker thread)
        if let Some(result) = self.import_service.poll() {
            self.handle_import_result(result);
        }

        // Disk work issued from modals: tag write-back, duplicate
        // deletion, the organizer, playlist and history exports.
        self.poll_file_tasks();

        // Library-wide loudness scan progress.
        if self.loudness_service.poll() {
            self.reload_library();
            let summary = self.loudness_service.progress().current.clone();
            if !summary.is_empty() {
                self.toasts.info(summary);
            }
        }

        // Auto-tag service polling: single-pass hash lookup eliminates O(N*M)
        // scans over the in-memory library on dual-core CPUs.
        if self.autotag_service.poll() {
            ctx.request_repaint();
        }
        self.autotag_modal
            .mark_progress(self.autotag_service.progress().clone());
        if !self.autotag_service.pending.is_empty() {
            let pending = std::mem::take(&mut self.autotag_service.pending);
            let mut track_map: std::collections::HashMap<
                domain::ids::TrackId,
                domain::models::Track,
            > = std::collections::HashMap::with_capacity(pending.len());
            let target_ids: std::collections::HashSet<domain::ids::TrackId> =
                pending.iter().map(|r| r.track_id).collect();
            for t in &self.library_state.all_tracks {
                if target_ids.contains(&t.id) {
                    track_map.insert(t.id, t.clone());
                    if track_map.len() == target_ids.len() {
                        break;
                    }
                }
            }
            for res in pending {
                if let Some(track) = track_map.remove(&res.track_id) {
                    self.autotag_modal.push_result(res, track);
                }
            }
            ctx.request_repaint();
        }

        // Cover art fetch service polling.
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

        // The Settings > Library card asks for the batch organizer. Raised
        // here because neither settings surface owns app-level modals.
        if std::mem::take(&mut self.dsp_state.settings_tab.request_organize) {
            let tracks = self.library_state.all_tracks.clone();
            self.open_organize(tracks, "Whole library".to_string());
        }
    }

    /// 3. Poll media content services: lyrics, waveforms, presets, tools.
    fn poll_media_content_subsystems(&mut self, ctx: &egui::Context) {
        // Poll the EQ curve worker (file dialogs and parsing off-thread).
        if let Some(event) = self.preset_io.poll() {
            self.handle_preset_io_result(event);
        }

        // Embedded lyric tags are read on a worker thread; pick up the
        // result as soon as it lands so the lyrics panel fills in.
        if self.lyrics_service.poll() {
            ctx.request_repaint();
        }

        // Waveform decodes also happen off the UI thread.
        // A finished waveform requests a repaint.
        if self.waveform_service.poll() {
            ctx.request_repaint();
        }
        if self.crossfade_preview.is_playing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
        self.poll_media_tools();
        if self.media_tools_service.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
    }

    /// 4. Poll desktop and system services: sleep timer, smart refresh, MPRIS, recommendations.
    fn poll_desktop_system_subsystems(&mut self, ctx: &egui::Context) {
        // Sleep timer (countdown or end-of-track).
        self.update_sleep_timer(ctx);

        // Smart playlists: the timer policy re-evaluates them on its own.
        self.tick_smart_refresh();

        // Desktop integration: apply media-key transport, publish state.
        self.handle_mpris(ctx);

        // Recommendation ranking, analysis progress, and playback edges.
        // Both workers are polled without blocking; the tracker observes the
        // transport once per frame so a skip is reported on the real change.
        self.poll_recommendation_services(ctx);
        self.track_recommendation_playback();
    }

    pub(super) fn poll_services(&mut self, ctx: &egui::Context) -> bool {
        self.poll_audio_dsp_subsystems(ctx);
        self.poll_library_file_subsystems(ctx);
        self.poll_media_content_subsystems(ctx);
        self.poll_desktop_system_subsystems(ctx);

        // 3. Reactive Repaints. The policy is a pure function so the idle case
        // is a test: no active signal means no wake-up at all (0% idle CPU).
        let is_playing = self.session.is_playing();
        let signals = RepaintSignals {
            playing: is_playing,
            minimized: ctx.input(|i| i.viewport().minimized == Some(true)),
            scanning: self.scanner_service.is_scanning(),
            loudness_running: self.loudness_service.is_running(),
            import_active: self.import_service.is_active(),
            sleep_timer_armed: self.app_state.sleep_timer.is_some(),
            watch_pending: self.watch_pending.is_some(),
            scan_queued: !self.scan_queue.is_empty(),
            toasts_visible: !self.toasts.is_empty(),
            background_read_pending: self.lyrics_service.is_loading()
                || self.waveform_service.has_pending(),
        };
        if let Some(delay) = wake_after(signals) {
            ctx.request_repaint_after(delay);
        }

        // 3b. Toast notifications ride above everything else.
        self.toasts.prune();
        self.toasts.show(ctx, &self.theme);
        is_playing
    }
}

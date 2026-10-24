use super::*;

impl ShadowPlayerApp {
    /// Apply one transport command from any controller (MPRIS, media keys,
    /// the loopback socket, or a future input source).
    pub(super) fn apply_transport_command(&mut self, ctx: &egui::Context, cmd: MprisCommand) {
        match cmd {
            MprisCommand::Play => self.session.play(),
            MprisCommand::Pause => self.session.pause(),
            MprisCommand::PlayPause => self.coordinator.toggle_play_pause(),
            MprisCommand::Stop => {
                self.session.pause();
                self.session.seek(0.0);
            }
            MprisCommand::Next => self.coordinator.next(),
            MprisCommand::Previous => self.coordinator.previous(),
            MprisCommand::SeekRelative(delta) => {
                let cur = self.session.position_secs_compensated();
                let dur = self
                    .coordinator
                    .current_track()
                    .map(|t| t.duration_secs)
                    .unwrap_or(f32::MAX);
                self.session.seek((cur + delta as f32).clamp(0.0, dur));
            }
            MprisCommand::SetPosition(pos) => {
                self.session.seek(pos.max(0.0) as f32);
            }
            MprisCommand::SetVolume(v) => {
                self.app_state.volume = v.clamp(0.0, 1.0);
                self.app_state.is_muted = false;
                self.session.set_volume(self.app_state.volume);
                self.settings_dirty = true;
            }
            MprisCommand::Raise => {
                // A minimized window must be restored before focus means
                // anything; both commands are harmless on a visible window.
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            MprisCommand::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Bridge for the desktop and loopback controllers: drain transport
    /// requests and publish the current player state to both surfaces.
    pub(super) fn handle_mpris(&mut self, ctx: &egui::Context) {
        let track = self.coordinator.current_track().cloned();
        let art_url = track.as_ref().and_then(|t| {
            let p = self
                .artwork_service
                .manager()
                .get_cached_thumbnail_path(&t.id);
            if p.exists() {
                Some(crate::services::mpris_service::artwork_file_url(&p))
            } else {
                None
            }
        });
        let snapshot = MprisSnapshot {
            loaded: track.is_some(),
            playing: self.session.is_playing(),
            title: track.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
            artist: track.as_ref().map(|t| t.artist.clone()).unwrap_or_default(),
            album: track.as_ref().map(|t| t.album.clone()).unwrap_or_default(),
            length_secs: track
                .as_ref()
                .map(|t| t.duration_secs as f64)
                .unwrap_or(0.0),
            position_secs: self.session.position_secs_compensated() as f64,
            volume: self.app_state.volume,
            art_url,
        };
        self.mpris_service.publish(snapshot.clone());
        if let Some(control) = &self.control_service {
            control.attach_context(ctx);
            control.publish(snapshot);
        }

        if self.mpris_service.is_active() {
            self.mpris_service.attach_context(ctx);
            while let Some(cmd) = self.mpris_service.try_recv() {
                self.apply_transport_command(ctx, cmd);
            }
        }
        while let Some(cmd) = self
            .control_service
            .as_ref()
            .and_then(|control| control.try_recv())
        {
            self.apply_transport_command(ctx, cmd);
        }
    }

    /// Advance the sleep timer. In `Duration` mode this counts down and pauses
    /// playback when it reaches zero; in `EndOfTrack` mode it pauses when the
    /// current track changes (or playback stops on its own).
    pub(super) fn update_sleep_timer(&mut self, ctx: &egui::Context) {
        let current_id = self.coordinator.current_track().map(|t| t.id);

        let Some(mut timer) = self.app_state.sleep_timer.clone() else {
            self.prev_track_id = current_id;
            self.prev_playing = self.session.is_playing();
            return;
        };

        match timer.mode {
            SleepTimerMode::Duration => {
                let dt = ctx.input(|i| i.stable_dt).min(0.5) as f64;
                // Only count down while something is actually playing.
                if self.session.is_playing() {
                    timer.remaining -= dt;
                }
                if timer.remaining <= 0.0 {
                    // Restore the user's volume before pausing: the next
                    // playback must not inherit a faded level.
                    self.session.set_volume(if self.app_state.is_muted {
                        0.0
                    } else {
                        self.app_state.volume
                    });
                    self.session.pause();
                    self.app_state.sleep_timer = None;
                    log::info!("Sleep timer elapsed — playback paused");
                } else {
                    if timer.fade_secs > 0.0 {
                        let factor = (timer.remaining / timer.fade_secs).clamp(0.0, 1.0) as f32;
                        let target = if self.app_state.is_muted {
                            0.0
                        } else {
                            self.app_state.volume * factor
                        };
                        self.session.set_volume(target);
                    }
                    self.app_state.sleep_timer = Some(timer);
                }
            }
            SleepTimerMode::EndOfTrack => {
                // We deliberately initialise on the first frame after enabling.
                let track_changed =
                    self.prev_track_id.is_some() && current_id != self.prev_track_id;
                if track_changed {
                    self.session.pause();
                    self.app_state.sleep_timer = None;
                    log::info!("Sleep timer reached end of track — playback paused");
                } else if self.prev_playing && !self.session.is_playing() {
                    // Playback ended by itself; timer has done its job.
                    self.app_state.sleep_timer = None;
                }
            }
        }

        self.prev_track_id = current_id;
        self.prev_playing = self.session.is_playing();
    }
}

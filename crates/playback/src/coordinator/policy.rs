use audio_session::{EngineEvent, OutputEvent};
use domain::ids::TrackId;
use domain::models::{RepeatModeChoice, Track};
use log::{info, warn};
use rand::seq::SliceRandom;

use super::{
    credited_listen_secs, MeasurementInputDevice, PlaybackCoordinator, RoomMeasurementEvent,
    PLAY_THRESHOLD_SECS, SCROBBLE_FRACTION, SCROBBLE_MIN_SECS,
};

impl PlaybackCoordinator {
    pub fn add_to_queue_next(&mut self, track: Track) {
        if let Some(curr) = self.current_index {
            self.queue.insert(curr + 1, track);
        } else {
            self.queue.push(track);
        }
        if self.shuffle {
            self.unshuffled_queue = None;
        }
        self.persist_queue();
        self.preload_next();
    }

    /// Move an upcoming queue item by one slot. Moving the currently playing
    /// item is deliberately rejected: it would desynchronise the engine's
    /// active playlist from the coordinator's display index.
    pub fn move_in_queue(&mut self, from: usize, to: usize) {
        if from == to || from >= self.queue.len() || to >= self.queue.len() {
            return;
        }
        let first_upcoming = self.current_index.map(|i| i + 1).unwrap_or(0);
        if from < first_upcoming || to < first_upcoming {
            return;
        }
        let item = self.queue.remove(from);
        self.queue.insert(to, item);
        if self.shuffle {
            self.unshuffled_queue = None;
        }
        self.persist_queue();
        self.preload_next();
    }

    pub fn remove_from_queue(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
            if self.shuffle {
                self.unshuffled_queue = None;
            }
            if let Some(curr) = self.current_index {
                if index < curr {
                    self.current_index = Some(curr.saturating_sub(1));
                } else if index == curr {
                    if self.queue.is_empty() {
                        self.current_index = None;
                        self.current_track = None;
                        self.session.stop();
                    } else {
                        let new_idx = curr.min(self.queue.len() - 1);
                        self.play_index(new_idx);
                    }
                }
            }
            self.persist_queue();
            self.preload_next();
        }
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        if enabled == self.shuffle {
            self.session.set_shuffle(enabled);
            return;
        }

        if enabled {
            // Remember the original order, then shuffle everything from the
            // current track onwards (played items stay put).
            self.unshuffled_queue = Some(self.queue.clone());
            self.shuffle_upcoming();
        } else if let Some(original) = self.unshuffled_queue.take() {
            // Restore the original order and re-locate the current track.
            let current_id = self.current_track.as_ref().map(|t| t.id);
            self.queue = original;
            self.current_index =
                current_id.and_then(|id| self.queue.iter().position(|t| t.id == id));
        }

        self.shuffle = enabled;
        self.session.set_shuffle(enabled);
        self.persist_queue();
        self.preload_next();
    }

    /// Shuffle the queue from just after the current track to the end.
    pub(crate) fn shuffle_upcoming(&mut self) {
        let from = self
            .current_index
            .map(|i| i + 1)
            .unwrap_or(0)
            .min(self.queue.len());
        self.shuffle_upcoming_from(from);
    }

    pub(crate) fn shuffle_upcoming_from(&mut self, from: usize) {
        let from = from.min(self.queue.len());
        if self.queue.len() > from + 1 {
            let mut rng = rand::rng();
            self.queue[from..].shuffle(&mut rng);
        }
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatModeChoice) {
        self.repeat_mode = mode;
        let engine_repeat = match mode {
            RepeatModeChoice::Off => audio_session::RepeatMode::Off,
            RepeatModeChoice::All => audio_session::RepeatMode::All,
            RepeatModeChoice::One => audio_session::RepeatMode::One,
        };
        self.session.set_repeat_mode(engine_repeat);
    }

    pub fn poll_events(&mut self) {
        // Accumulate listening time for the current track (drives the
        // recently/most played rules).
        self.sample_listening();

        while let Some(event) = self.session.try_recv_event() {
            match event {
                EngineEvent::SourceFinished { .. } => {
                    // A track that reaches its natural end always counts.
                    if !self.play_registered {
                        if let Some(id) = self.current_track.as_ref().map(|t| t.id) {
                            let threshold = self
                                .current_track
                                .as_ref()
                                .map(|t| PLAY_THRESHOLD_SECS.min(t.duration_secs))
                                .unwrap_or(PLAY_THRESHOLD_SECS);
                            self.register_play(id, threshold, true);
                        }
                    }

                    if self.repeat_mode == RepeatModeChoice::One {
                        // The engine already restarts the source at 0:00 for
                        // Repeat-One, so do not reopen the file: reopening is a
                        // second restart, and for a CUE virtual track it played
                        // the whole album image instead of the track. Only the
                        // listening accounting needs resetting.
                        self.begin_tracking();
                        continue;
                    }

                    self.next();
                }
                EngineEvent::Error(message) => {
                    warn!("Engine error: {message}");
                    self.push_notice(format!("Audio engine error: {message}"));
                }
                EngineEvent::PlaylistLoadFailed { path, message } => {
                    warn!("Playlist load failed for {}: {message}", path.display());
                    self.push_notice(format!(
                        "Could not load playlist {}: {message}",
                        path.display()
                    ));
                }
                EngineEvent::InputDeviceList { devices } => {
                    let devices = devices
                        .into_iter()
                        .map(|device| {
                            let measurement_capable = device.is_measurement_capable();
                            MeasurementInputDevice {
                                name: device.name,
                                is_default: device.is_default,
                                channels: device.channels,
                                sample_rates: device.sample_rates,
                                measurement_capable,
                            }
                        })
                        .collect();
                    self.push_room_event(RoomMeasurementEvent::InputDevices(devices));
                }
                EngineEvent::MeasurementProgress { stage } => {
                    self.push_room_event(RoomMeasurementEvent::Progress(stage));
                }
                EngineEvent::MeasurementComplete { path, snr_db } => {
                    info!(
                        "room measurement complete: {} (SNR {snr_db:.1} dB)",
                        path.display()
                    );
                    self.push_room_event(RoomMeasurementEvent::Complete { path, snr_db });
                }
                EngineEvent::MeasurementFailed(message) => {
                    warn!("Room measurement failed: {message}");
                    self.push_room_event(RoomMeasurementEvent::Failed(message));
                }
                EngineEvent::MeasurementCancelled => {
                    self.push_room_event(RoomMeasurementEvent::Cancelled);
                }
                // Opened/paused/stopped/seek/format/telemetry events carry no
                // action for the coordinator.
                _ => {}
            }
        }

        // Output-device events are not just decoration: a disconnected DAC
        // stops playback, and the user needs to be told rather than left
        // wondering why the transport is dead.
        while let Some(event) = self.session.try_recv_output_event() {
            match event {
                OutputEvent::DeviceDisconnected { device } => {
                    warn!("Audio device disconnected: {device}");
                    self.push_notice(format!("Audio device disconnected: {device}"));
                }
                OutputEvent::EndpointError { endpoint, message } => {
                    warn!("Audio endpoint {endpoint} failed: {message}");
                    self.push_notice(format!("Audio endpoint {endpoint} failed: {message}"));
                }
                // Arrivals, list changes and the negotiated device are already
                // reflected by the device picker.
                OutputEvent::DeviceConnected { device } => {
                    self.pending_output_device = Some(device);
                }
                OutputEvent::OutputDeviceChanged { device } => {
                    if let Some(device) = device {
                        self.pending_output_device = Some(device);
                    }
                }
                OutputEvent::DeviceListChanged { .. } => {}
            }
        }
    }

    /// Take the most recent device arrival/negotiated output, if any.
    ///
    /// This is deliberately a *take*: a binding is applied once per event,
    /// not once per frame, and the shell is the only consumer.
    pub fn take_output_device_change(&mut self) -> Option<String> {
        self.pending_output_device.take()
    }

    /// Record a user-facing engine problem, keeping the list bounded so a
    /// misbehaving engine cannot grow it without limit.
    fn push_notice(&mut self, message: String) {
        const MAX_NOTICES: usize = 16;
        if self.engine_notices.len() == MAX_NOTICES {
            self.engine_notices.remove(0);
        }
        self.engine_notices.push(message);
    }

    /// Take the engine/device problems seen since the last call, for the shell
    /// to present (toast, status line).
    pub fn take_engine_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.engine_notices)
    }

    fn push_room_event(&mut self, event: RoomMeasurementEvent) {
        const MAX_ROOM_EVENTS: usize = 64;
        if self.room_measurement_events.len() == MAX_ROOM_EVENTS {
            self.room_measurement_events.remove(0);
        }
        self.room_measurement_events.push(event);
    }

    /// Take measurement events since the last call. The wizard drains this
    /// every frame; nobody else needs to know about progress stages.
    pub fn take_room_measurement_events(&mut self) -> Vec<RoomMeasurementEvent> {
        std::mem::take(&mut self.room_measurement_events)
    }

    /// Ask the engine for its input endpoints. The reply arrives as
    /// [`RoomMeasurementEvent::InputDevices`].
    pub fn enumerate_measurement_inputs(&self) {
        self.session.enumerate_input_devices();
    }

    /// Start a measurement through `input_device`, or through the engine's
    /// preferred system mix/default input when it is `None`.
    pub fn measure_room(&self, seconds: f32, pre_emphasis: f32, input_device: Option<String>) {
        self.session
            .measure_room_with_input(seconds, pre_emphasis, input_device);
    }

    /// Stop an in-flight measurement. The reply is
    /// [`RoomMeasurementEvent::Cancelled`] (or `Failed` when nothing was
    /// running).
    pub fn cancel_room_measurement(&self) {
        self.session.cancel_room_measurement();
    }

    /// Reset per-track listening accounting.
    pub(crate) fn begin_tracking(&mut self) {
        self.listen_secs = 0.0;
        self.last_position = 0.0;
        self.play_registered = false;
    }

    /// Add the delta in playback position to the listened total, and register
    /// the play once the threshold is crossed.
    fn sample_listening(&mut self) {
        if self.play_registered {
            return;
        }
        let Some(track_id) = self.current_track.as_ref().map(|t| t.id) else {
            return;
        };
        if !self.session.is_playing() {
            self.last_position = self.session.position_secs_compensated();
            return;
        }

        let pos = self.session.position_secs_compensated();
        self.listen_secs += credited_listen_secs(self.last_position, pos);
        self.last_position = pos;

        let duration = self
            .current_track
            .as_ref()
            .map(|t| t.duration_secs)
            .unwrap_or(0.0);
        let threshold = PLAY_THRESHOLD_SECS.min(duration);
        if self.listen_secs >= threshold {
            self.register_play(track_id, threshold, false);
        }
    }

    /// Whether a play of this length counts as a scrobble.
    pub fn is_scrobble(listened: f32, duration: f32) -> bool {
        if duration <= 0.0 {
            return listened >= SCROBBLE_MIN_SECS;
        }
        listened >= SCROBBLE_MIN_SECS || listened >= duration * SCROBBLE_FRACTION
    }

    /// Note that the current track was skipped before it was heard.
    ///
    /// A skip is a jump to another track before the current one had a chance
    /// to be heard. It is what a "skip this track" smart playlist needs, and
    /// it is the signal that a track is not worth keeping.
    pub(crate) fn register_skip(&mut self) {
        if self.play_registered {
            return;
        }
        let Some(track) = self.current_track.clone() else {
            return;
        };
        if let Err(e) = self.track_repo.increment_skip_count(&track.id) {
            warn!(
                "Could not increment the skip count for {}: {e}",
                track.title
            );
        }
    }

    fn register_play(&mut self, track_id: TrackId, threshold: f32, completed: bool) {
        // Reported rather than swallowed: silently failing here means a track
        // that was heard never appears in "recently played" or the play counts.
        if let Err(e) = self.track_repo.increment_play_count(&track_id) {
            warn!("Could not increment play count for {track_id}: {e}");
        }
        let listened = self.listen_secs.max(threshold);
        let duration = self
            .current_track
            .as_ref()
            .map(|t| t.duration_secs)
            .unwrap_or(0.0);
        let is_scrobble = completed || Self::is_scrobble(listened, duration);
        if let Err(e) = self
            .history_repo
            .record_play(&track_id, listened, completed, is_scrobble)
        {
            warn!("Could not record play history for {track_id}: {e}");
        }
        // The scrobble log is the permanent record; `play_history` above is
        // only the seven-day window the "Recently Played" view reads. Copy
        // the track's text fields in now, so the scrobble still says what was
        // heard after the file is gone.
        if is_scrobble {
            if let Some(track) = self.current_track.as_ref() {
                if let Err(e) = self.scrobble_repo.record(track, listened) {
                    warn!("Could not record the scrobble for {track_id}: {e}");
                }
            }
        }
        self.play_registered = true;
    }

    pub(crate) fn preload_next(&self) {
        if let Some(curr) = self.current_index {
            let next_idx = curr + 1;
            if next_idx < self.queue.len() {
                let next_path = &self.queue[next_idx].path;
                self.session.prepare_next_file(next_path);
            }
        }
    }

    pub(crate) fn persist_queue(&self) {
        let ids: Vec<_> = self.queue.iter().map(|t| t.id).collect();
        // A failure here means the queue will not survive a restart, which is
        // worth a log line: it used to fail completely silently.
        if let Err(e) = self.queue_repo.save_queue(&ids) {
            warn!("Could not persist the queue ({} entries): {e}", ids.len());
        }
    }
}

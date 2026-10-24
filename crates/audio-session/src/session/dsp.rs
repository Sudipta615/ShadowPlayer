//! DSP, mixer, and spatial configuration.
//!
//! Every method here is a thin, typed forwarder: the command crosses the
//! channel and the engine thread does the work. The UI never touches DSP state
//! directly.

use super::AudioSession;
use domain::models::AudioProfileChoice;
use engine::{EngineCommand, EngineConfig};
use std::path::Path;

impl AudioSession {
    pub fn apply_profile(&self, choice: AudioProfileChoice) {
        let preset = match choice {
            AudioProfileChoice::Normal => config::EnginePreset::Consumer,
            AudioProfileChoice::Fidelity => config::EnginePreset::Fidelity,
            AudioProfileChoice::LegacyLowPower => config::EnginePreset::LegacyLowPower,
        };
        let cfg = EngineConfig::from_preset(preset);

        self.handle.set_resampler_quality(cfg.resampler_quality);
        self.handle.set_dither_enabled(cfg.dither_enabled);
        self.send(EngineCommand::SetPrecisionMode(cfg.precision_mode));
        self.handle.set_eq_enabled(cfg.eq.enabled);
        self.handle.set_crossfeed_enabled(cfg.crossfeed.enabled);
        self.send(EngineCommand::SetLimiterEnabled(cfg.limiter.enabled));
        self.send(EngineCommand::SetCompressorEnabled(
            cfg.multiband_compressor.enabled,
        ));
        self.handle.set_spatial_enabled(cfg.spatial.enabled);
    }

    pub fn set_eq_enabled(&self, enabled: bool) {
        self.handle.set_eq_enabled(enabled);
    }

    pub fn set_eq_band(&self, index: usize, freq: f32, gain_db: f32, q: f32, enabled: bool) {
        self.handle.set_eq_band(index, freq, gain_db, q, enabled);
    }

    pub fn set_eq_preset(&self, preset: config::EqPreset) {
        self.handle.set_eq_preset(preset);
    }

    pub fn set_graphic_eq_layout(&self, layout: config::GraphicEqLayout) {
        self.handle.set_graphic_eq_layout(layout);
    }

    pub fn set_graphic_eq_slider(&self, band: usize, gain_db: f32) {
        self.handle.set_graphic_eq_slider(band, gain_db);
    }

    pub fn set_graphic_eq_enabled(&self, enabled: bool) {
        self.handle.set_graphic_eq_enabled(enabled);
    }

    pub fn set_preamp(&self, db: f32) {
        self.handle.set_preamp(db);
    }

    pub fn set_bass_shelf(&self, db: f32) {
        self.send(EngineCommand::SetBassShelf(db));
    }

    pub fn set_treble_shelf(&self, db: f32) {
        self.send(EngineCommand::SetTrebleShelf(db));
    }

    pub fn set_stereo_width(&self, width: f32) {
        self.handle.set_stereo_width(width);
    }

    pub fn set_balance(&self, balance: f32) {
        self.handle.set_balance(balance);
    }

    pub fn set_crossfeed_enabled(&self, enabled: bool) {
        self.handle.set_crossfeed_enabled(enabled);
    }

    pub fn set_crossfeed_profile(&self, profile: config::CrossfeedProfile) {
        self.handle.set_crossfeed_profile(profile);
    }

    /// Apply the creative-effects insert's settings.
    ///
    /// One call carries every effect (chorus, flanger, phaser, delay,
    /// saturation), so the insert is always in a state the UI can describe.
    pub fn set_fx_config(&self, config: config::FxConfig) {
        self.handle.set_fx_config(config);
    }

    pub fn set_compressor_enabled(&self, enabled: bool) {
        self.send(EngineCommand::SetCompressorEnabled(enabled));
    }

    pub fn set_limiter_enabled(&self, enabled: bool) {
        self.send(EngineCommand::SetLimiterEnabled(enabled));
    }

    pub fn set_spatial_enabled(&self, enabled: bool) {
        self.handle.set_spatial_enabled(enabled);
    }

    pub fn set_spatial_listener(&self, yaw_deg: f32, pitch_deg: f32, roll_deg: f32) {
        self.handle
            .set_spatial_listener(yaw_deg, pitch_deg, roll_deg);
    }

    pub fn set_hrtf_profile(&self, profile_id: impl Into<String>) {
        self.handle.set_hrtf_profile(profile_id);
    }

    // ── Speed & Pitch ────────────────────────────────────────────────────

    /// Set playback speed multiplier (e.g. 1.0 = normal, 0.5 = half speed, 2.0 = double).
    pub fn set_speed(&self, speed: f32) {
        self.handle.set_speed(speed);
    }

    /// Set the speed processing mode (Varispeed, TimeStretch, or PitchShift).
    pub fn set_speed_mode(&self, mode: config::SpeedMode) {
        self.handle.set_speed_mode(mode);
    }

    /// Set pitch shift in semitones [-24.0, +24.0]. Effective in TimeStretch/PitchShift mode.
    pub fn set_pitch(&self, semitones: f32) {
        self.handle.set_pitch(semitones);
    }

    // ── Crossfade & Transitions ──────────────────────────────────────────

    /// Set the full crossfade configuration (duration, curve, etc.).
    pub fn set_crossfade_config(&self, config: config::CrossfadeConfig) {
        self.send(EngineCommand::SetCrossfadeConfig(config));
    }

    /// Set the crossfade curve shape (e.g. EqualPower, Linear, SCurve).
    pub fn set_crossfade_curve(&self, curve: config::CrossfadeCurve) {
        self.send(EngineCommand::SetCrossfadeCurve(curve));
    }

    /// Set the track-to-track transition mode (Gapless, Crossfade, Fade, Stop).
    pub fn set_transition_mode(&self, mode: config::TransitionMode) {
        self.send(EngineCommand::SetTransitionMode(mode));
    }

    /// Set custom crossfeed filter parameters (frequency, Q, ITD delay in ms).
    pub fn set_crossfeed_custom_params(&self, frequency_hz: f32, q: f32, delay_ms: f32) {
        self.handle
            .set_crossfeed_custom_params(frequency_hz, q, delay_ms);
    }

    // ── Loudness ─────────────────────────────────────────────────────────

    /// Set loudness normalization mode (Off, ReplayGain, EBU R128, etc.).
    pub fn set_loudness_mode(&self, mode: config::LoudnessMode) {
        self.handle.set_loudness_mode(mode);
    }

    /// Scan a file for EBU R128 / ReplayGain loudness and write the result into its tags.
    /// Requires the `tag-write` feature. Emits a `LoudnessScanComplete` event when done.
    pub fn write_loudness_tags(&self, path: impl Into<std::path::PathBuf>) {
        self.handle.write_loudness_tags(path);
    }

    // ── Limiter ───────────────────────────────────────────────────────────

    /// Set the limiter post-gain mode (Transparent or Saturate).
    pub fn set_limiter_mode(&self, mode: engine::dsp::limiter::LimiterMode) {
        self.handle.set_limiter_mode(mode);
    }

    /// Enable or disable true-peak FIR oversampling on the limiter.
    pub fn set_limiter_true_peak(&self, enabled: bool) {
        self.handle.set_limiter_true_peak(enabled);
    }

    /// Configure the full limiter parameter set (lookahead, attack, release, ceiling, soft-clip).
    pub fn set_limiter_params(
        &self,
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    ) {
        self.handle
            .set_limiter_params(lookahead_ms, attack_ms, release_ms, ceiling_db, soft_clip);
    }

    // ── Multiband Compressor ──────────────────────────────────────────────

    /// Configure per-band compressor parameters (threshold, ratio, attack, release, makeup gain).
    /// `band`: 0=Low, 1=Mid, 2=High.
    pub fn set_compressor_band_params(
        &self,
        band: usize,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    ) {
        self.send(EngineCommand::SetCompressorBandParams {
            band,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
        });
    }

    /// Configure advanced compressor band features (knee, detector mode, stereo link).
    /// `band`: 0=Low, 1=Mid, 2=High.
    pub fn set_compressor_band_features(
        &self,
        band: usize,
        knee_db: f32,
        detector: config::CompressorDetector,
        stereo_link: bool,
    ) {
        self.handle
            .set_compressor_band_features(band, knee_db, detector, stereo_link);
    }

    // ── Room Correction ───────────────────────────────────────────────────

    /// Enable or disable the room/headphone correction stage (loaded IR stays in memory).
    pub fn set_correction_enabled(&self, enabled: bool) {
        self.handle.set_correction_enabled(enabled);
    }

    /// Set the wet/dry correction depth [0.0 = bypass, 1.0 = fully corrected].
    pub fn set_correction_depth(&self, depth: f32) {
        self.handle.set_correction_depth(depth.clamp(0.0, 1.0));
    }

    /// Load a measured IR file and derive a correction from it, then enable correction.
    /// A missing or unreadable file keeps the previous correction — never a failure state.
    pub fn load_correction_ir(&self, path: impl Into<std::path::PathBuf>) {
        self.handle.load_correction_ir(path);
    }

    /// Run a room measurement sweep (ESS) and auto-derive a correction from the captured IR.
    /// Progress surfaces as `MeasurementProgress` events; completion as `MeasurementComplete`.
    /// `seconds`: sweep duration [0.25, 120]; `pre_emphasis`: HF SNR boost in dB.
    pub fn measure_room(&self, seconds: f32, pre_emphasis: f32) {
        self.handle.measure_room(seconds, pre_emphasis);
    }

    /// Run a room measurement through a chosen input device. `None` keeps the
    /// engine's preferred system-mix/default-input behaviour.
    pub fn measure_room_with_input(
        &self,
        seconds: f32,
        pre_emphasis: f32,
        input_device: Option<String>,
    ) {
        self.handle
            .measure_room_with_input(seconds, pre_emphasis, input_device);
    }

    /// Stop an in-flight room measurement and discard its partial recording.
    pub fn cancel_room_measurement(&self) {
        self.handle.cancel_room_measurement();
    }

    // ── CLAP native editor ───────────────────────────────────────────────

    /// Open the loaded CLAP plugin's floating native editor.
    pub fn open_clap_gui(&self) {
        self.handle.open_clap_gui();
    }

    /// Destroy the native editor; the plugin keeps processing.
    pub fn close_clap_gui(&self) {
        self.handle.close_clap_gui();
    }

    /// Ask the plugin to resize its editor.
    pub fn resize_clap_gui(&self, width: u32, height: u32) {
        self.handle.resize_clap_gui(width, height);
    }

    // ── Stereo Enhancer ───────────────────────────────────────────────────

    /// Enable or disable the stereo enhancer stage.
    pub fn set_stereo_enhancer_enabled(&self, enabled: bool) {
        self.handle.set_stereo_enhancer_enabled(enabled);
    }

    // ── EQ Extras ────────────────────────────────────────────────────────

    /// Set an EQ band with full filter type control (more expressive than `set_eq_band`).
    pub fn set_eq_band_params(
        &self,
        index: usize,
        frequency: f32,
        gain_db: f32,
        q: f32,
        filter_type: engine::dsp::equalizer::EqFilterType,
        enabled: bool,
    ) {
        self.send(EngineCommand::SetEqBandParams {
            index,
            frequency,
            gain_db,
            q,
            filter_type,
            enabled,
        });
    }

    /// Enable or disable automatic EQ headroom management.
    /// When enabled, the engine reserves the curve's peak boost as pre-EQ attenuation.
    pub fn set_eq_auto_headroom(&self, enabled: bool) {
        self.send(EngineCommand::SetEqAutoHeadroom(enabled));
    }

    /// Set the graphic EQ preamp gain in dB.
    pub fn set_graphic_eq_preamp(&self, gain_db: f32) {
        self.send(EngineCommand::SetGraphicEqPreamp(gain_db));
    }

    /// Enable or disable Mid/Side EQ mode.
    pub fn set_midside_eq(&self, enabled: bool) {
        self.send(EngineCommand::SetMidsideEq(enabled));
    }

    // ── Playback Queue ───────────────────────────────────────────────────

    /// Remove the queue entry at `index`. Has no effect on the currently playing track.
    pub fn remove_from_playlist(&self, index: usize) {
        self.handle.remove_from_playlist(index);
    }

    // ── Multi-Track Lane Mix Bus ──────────────────────────────────────────

    /// Add an audio source as an independent mix-bus lane (playing alongside primary stream).
    pub fn add_track(&self, source: engine::AudioSource) {
        self.send(EngineCommand::AddTrack(source));
    }

    /// Remove the lane on the given mix-bus slot and silence it.
    pub fn remove_track(&self, slot: u8) {
        self.send(EngineCommand::RemoveTrack(slot));
    }

    /// Set a lane's linear gain [0.0, 1.0].
    pub fn set_track_gain(&self, slot: u8, gain: f32) {
        self.send(EngineCommand::SetTrackGain { slot, gain });
    }

    /// Set a lane's pan [-1.0 (Left), 1.0 (Right)].
    pub fn set_track_pan(&self, slot: u8, pan: f32) {
        self.send(EngineCommand::SetTrackPan { slot, pan });
    }

    /// Set a lane's post-fader master-send gain [0.0, 1.0].
    pub fn set_track_master_gain(&self, slot: u8, gain: f32) {
        self.send(EngineCommand::SetTrackMasterGain { slot, gain });
    }

    /// Set a lane's post-fader aux-send gain [0.0, 1.0].
    pub fn set_track_send(&self, slot: u8, gain: f32) {
        self.send(EngineCommand::SetTrackSend { slot, gain });
    }

    /// Configure program-gated ducking across lanes.
    /// When `source_slot`'s peak exceeds `threshold_db`, `targets` are attenuated by `depth_db`.
    /// Pass an empty `targets` list to disable ducking.
    pub fn duck_tracks(
        &self,
        source_slot: u8,
        targets: Vec<u8>,
        threshold_db: f32,
        depth_db: f32,
        attack_ms: f32,
        release_ms: f32,
    ) {
        self.send(EngineCommand::DuckTracks {
            source_slot,
            targets,
            threshold_db,
            depth_db,
            attack_ms,
            release_ms,
        });
    }

    /// Set per-channel trim gain and polarity for a mix slot.
    pub fn set_slot_trim(&self, slot: u8, channel: usize, gain_db: f32, invert_polarity: bool) {
        self.handle
            .set_slot_trim(slot, channel, gain_db, invert_polarity);
    }

    /// Configure the aux bus return (enable/disable and return gain).
    pub fn set_aux(&self, enabled: bool, return_gain: f32) {
        self.handle.set_aux(enabled, return_gain);
    }

    /// Mute or unmute an input mix lane.
    pub fn set_input_mute(&self, slot: u8, muted: bool) {
        self.handle.set_input_mute(slot, muted);
    }

    /// Activate or detach an input mix lane.
    pub fn set_input_active(&self, slot: u8, active: bool) {
        self.handle.set_input_active(slot, active);
    }

    /// Attach or clear a parameter automation curve for a mix slot.
    pub fn set_slot_automation(
        &self,
        slot: u8,
        kind: u8,
        curve: Option<std::sync::Arc<engine::spatial::CurveScalar>>,
        time_secs: f32,
    ) {
        self.handle
            .set_slot_automation(slot, kind, curve, time_secs);
    }

    pub fn set_spatial_quality(&self, quality: config::SpatialQuality) {
        self.handle.set_spatial_quality(quality);
    }

    /// Set the spatial voice budget.
    /// `enabled = false` clears the budget (full admission).
    pub fn set_spatial_voice(
        &self,
        enabled: bool,
        capacity: usize,
        full_quality_capacity: usize,
        policy: config::VoicePriority,
    ) {
        self.handle
            .set_spatial_voice(enabled, capacity, full_quality_capacity, policy);
    }

    /// Attach a scalar automation curve (gain or spread) to a spatial program object.
    /// Pass `None` for `curve` to clear. `object`: 0 = L, 1 = R.
    pub fn set_spatial_automation(
        &self,
        object: u8,
        kind: u8,
        curve: Option<std::sync::Arc<engine::spatial::CurveScalar>>,
        time_secs: f32,
    ) {
        self.handle
            .set_spatial_automation(object, kind, curve, time_secs);
    }

    /// Drive the spatial automation clock to `seconds`.
    pub fn set_spatial_automation_time(&self, seconds: f32) {
        self.handle.set_spatial_automation_time(seconds);
    }

    /// Set the listener pose (world-space quaternion orientation + position in metres).
    /// The spatial master glides toward it every block per the tracking policy.
    pub fn set_spatial_listener_pose(
        &self,
        orientation: engine::spatial::Quat,
        position: engine::spatial::Vec3,
    ) {
        self.handle.set_spatial_listener_pose(orientation, position);
    }

    /// Configure listener glide smoothing (one-pole time constant ms; angular rate limit deg/s).
    /// `smoothing_ms = 0` snaps; `max_angular_rate_deg_s = 0` is unlimited.
    pub fn set_spatial_listener_tracking(&self, smoothing_ms: f32, max_angular_rate_deg_s: f32) {
        self.handle
            .set_spatial_listener_tracking(smoothing_ms, max_angular_rate_deg_s);
    }

    /// Configure the virtual screen geometry and gain.
    pub fn set_spatial_screen(
        &self,
        center_azimuth_deg: f32,
        half_width_deg: f32,
        elevation_deg: f32,
        gain: f32,
    ) {
        self.handle
            .set_spatial_screen(center_azimuth_deg, half_width_deg, elevation_deg, gain);
    }

    /// Configure acoustic room reflections and reverberation.
    #[allow(clippy::too_many_arguments)]
    pub fn set_spatial_room(
        &self,
        enabled: bool,
        width: f32,
        depth: f32,
        height: f32,
        absorption: f32,
        reflection_order: u8,
        rt60_ms: f32,
        late_mix: f32,
        late_distance: bool,
        wet: f32,
    ) {
        self.handle.set_spatial_room(
            enabled,
            width,
            depth,
            height,
            absorption,
            reflection_order,
            rt60_ms,
            late_mix,
            late_distance,
            wet,
        );
    }

    /// Configure atmospheric air absorption simulation.
    pub fn set_spatial_air(&self, air: engine::spatial::AirAbsorption) {
        self.handle.set_spatial_air(air);
    }

    /// Switch distance-dependent air absorption on or off, keeping the engine's
    /// own coefficients for everything else.
    pub fn set_spatial_air_absorption(&self, enabled: bool) {
        self.handle.set_spatial_air(engine::spatial::AirAbsorption {
            enabled,
            ..Default::default()
        });
    }

    /// Rotate the listener's head to `yaw_deg`, leaving pitch and roll level.
    pub fn set_spatial_listener_orientation(&self, yaw_deg: f32) {
        self.handle.set_spatial_listener(yaw_deg, 0.0, 0.0);
    }

    /// Replace the spatial master's cue bank from the scene-file model.
    pub fn set_spatial_cues(&self, cues: Vec<config::SpatialCueConfig>) {
        self.handle.set_spatial_cues(cues);
    }

    /// Fire the named spatial cue at the next block boundary.
    pub fn trigger_spatial_cue(&self, name: &str) {
        self.handle.trigger_spatial_cue(name);
    }

    /// Stop the active cue on `target` (0 = L, 1 = R).
    pub fn stop_spatial_cue(&self, target: usize) {
        self.handle.stop_spatial_cue(target);
    }

    /// Stop every active spatial cue.
    pub fn stop_all_spatial_cues(&self) {
        self.handle.stop_all_spatial_cues();
    }

    // ── Multichannel Routing ──────────────────────────────────────────────

    /// Set the upmix/downmix channel mix template or custom matrix.
    pub fn set_channel_mix(&self, config: config::ChannelMixConfig) {
        self.handle.set_channel_mix(config);
    }

    /// Set the multichannel preservation policy (StereoDownmix, PassThrough, MaxChannels).
    pub fn set_channel_policy(&self, policy: config::ChannelPolicy) {
        self.handle.set_channel_policy(policy);
    }

    /// Set per-channel trim (gain, fractional delay, polarity) for multichannel output.
    pub fn set_channel_trim(&self, config: config::ChannelTrimConfig) {
        self.handle.set_channel_trim(config);
    }

    /// Set the multichannel source→destination routing matrix.
    pub fn set_channel_routing(&self, config: config::ChannelRoutingConfig) {
        self.handle.set_channel_routing(config);
    }

    /// Set per-channel parametric EQ for multichannel output.
    pub fn set_channel_eq(&self, config: config::ChannelEqConfig) {
        self.handle.set_channel_eq(config);
    }

    /// Set LFE/subwoofer management configuration (gain and low-pass crossover).
    pub fn set_lfe_config(&self, config: config::LfeConfig) {
        self.handle.set_lfe_config(config);
    }

    /// Set bass management configuration (mains high-pass and shared crossover).
    pub fn set_bass_management(&self, config: config::BassManagementConfig) {
        self.handle.set_bass_management(config);
    }

    // ── CLAP Plugin Rack ──────────────────────────────────────────────────

    /// The hosted CLAP plugin, plus whatever the last library probe found.
    ///
    /// Read straight out of shared memory rather than asked for, so a UI can
    /// call it on every frame without a round trip through the command channel.
    pub fn clap_view(&self) -> engine::dsp::clap_rack::ClapRackView {
        self.handle.clap_view()
    }

    /// Ask the engine to re-read the loaded plugin and republish
    /// [`Self::clap_view`]. Needed after something the engine did not initiate,
    /// such as a graph rebuilt from persisted config.
    pub fn refresh_clap_plugin(&self) {
        self.handle.refresh_clap_plugin();
    }

    /// List the plugins a library file contains *without* loading one, so the
    /// user can choose. The result arrives in [`Self::clap_view`].
    pub fn probe_clap_library(&self, path: impl AsRef<Path>) {
        self.handle.probe_clap_library(path);
    }

    /// Load a CLAP plugin from a library path, optionally naming which plugin
    /// inside that library to instantiate.
    pub fn load_clap_plugin(&self, path: impl AsRef<Path>, plugin_id: Option<&str>) {
        self.handle.load_clap_plugin(path, plugin_id);
    }

    /// Unload the hosted plugin. The insert goes back to pass-through.
    pub fn unload_clap_plugin(&self) {
        self.handle.unload_clap_plugin();
    }

    /// Bypass or un-bypass the hosted plugin without unloading it, so its
    /// parameters and state survive the trip back.
    pub fn set_clap_plugin_enabled(&self, enabled: bool) {
        self.handle.set_clap_plugin_enabled(enabled);
    }

    /// Move one plugin parameter, by the id the plugin declared.
    pub fn set_clap_plugin_param(&self, id: u32, value: f64) {
        self.handle.set_clap_plugin_param(id, value);
    }

    /// Capture the plugin's full state. The bytes arrive through
    /// [`Self::take_clap_state`], not as an event: a plugin's state can be
    /// megabytes and only the caller wants it.
    pub fn save_clap_state(&self) {
        self.handle.save_clap_state();
    }

    /// Take the state blob captured by [`Self::save_clap_state`], if it has
    /// arrived. The slot is emptied, so a stale blob is never mistaken for a
    /// fresh one.
    pub fn take_clap_state(&self) -> Option<Vec<u8>> {
        self.handle.take_clap_state()
    }

    /// Restore a state blob captured earlier.
    pub fn load_clap_state(&self, bytes: Vec<u8>) {
        self.handle.load_clap_state(bytes);
    }
}

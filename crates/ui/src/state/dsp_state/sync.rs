//! Pure Direct, output-device refresh, and pushing state into the audio session.

use super::*;

impl DspState {
    pub(super) fn capture_pure_direct(&mut self, volume: f32) {
        if self.pure_direct_saved.is_none() {
            self.pure_direct_saved = Some(PureDirectSaved {
                eq_enabled: self.eq_panel.eq_enabled,
                eq_mode: self.eq_panel.eq_mode,
                preamp_db: self.eq_panel.preamp_db,
                speed: self.settings_tab.speed,
                volume,
                limiter_enabled: self.eq_panel.limiter_enabled,
                compressor_enabled: self.eq_panel.compressor_enabled,
                spatial_enabled: self.settings_tab.spatial_enabled,
                stereo_enhancer_enabled: self.settings_tab.stereo_enhancer_enabled,
            });
        }
    }

    pub(super) fn restore_pure_direct_state(&mut self) -> Option<PureDirectSaved> {
        let saved = self.pure_direct_saved.take()?;
        self.eq_panel.eq_enabled = saved.eq_enabled;
        self.eq_panel.set_eq_mode(saved.eq_mode);
        self.eq_panel.set_preamp(saved.preamp_db);
        self.settings_tab.speed = saved.speed;
        self.eq_panel.limiter_enabled = saved.limiter_enabled;
        self.eq_panel.compressor_enabled = saved.compressor_enabled;
        self.settings_tab.spatial_enabled = saved.spatial_enabled;
        self.settings_tab.stereo_enhancer_enabled = saved.stereo_enhancer_enabled;
        Some(saved)
    }

    /// Enter Pure Direct and neutralise every stage it bypasses.
    ///
    /// The previous state is captured once. Saving it is what lets the next
    /// toggle-off restore the chain instead of leaving the user with a flat,
    /// limiter-free, spatial-free setup.
    pub fn enter_pure_direct(&mut self, session: &AudioSession) {
        self.capture_pure_direct(session.volume());
        session.set_volume(1.0);
        session.set_preamp(0.0);
        session.set_speed(1.0);
        session.set_eq_enabled(false);
        session.set_graphic_eq_enabled(false);
        session.set_limiter_enabled(false);
        session.set_compressor_enabled(false);
        session.set_spatial_enabled(false);
        session.set_stereo_enhancer_enabled(false);
        session.set_bit_perfect(true);

        // Mirror the engine so the studio does not keep drawing stages as
        // active while Pure Direct bypasses them. Speed is neutralised too, so
        // its control must show the neutral value until the bypass ends.
        self.settings_tab.speed = 1.0;
        self.eq_panel.eq_enabled = false;
        self.eq_panel.graphic_enabled = false;
        self.eq_panel.limiter_enabled = false;
        self.eq_panel.compressor_enabled = false;
        self.eq_panel.set_preamp(0.0);
        self.settings_tab.spatial_enabled = false;
        self.settings_tab.stereo_enhancer_enabled = false;
        self.settings_tab.bit_perfect = true;
    }

    /// True while the Pure Direct bypass is engaged.
    ///
    /// Automatic behaviour (a device-bound EQ preset, for example) must not
    /// quietly re-enable a stage this bypass turned off.
    pub fn is_pure_direct(&self) -> bool {
        self.pure_direct_saved.is_some()
    }

    /// Leave Pure Direct and re-assert the stages it neutralised.
    pub fn exit_pure_direct(&mut self, session: &AudioSession) {
        session.set_bit_perfect(false);
        self.settings_tab.bit_perfect = false;
        let Some(saved) = self.restore_pure_direct_state() else {
            return;
        };
        session.set_volume(saved.volume);
        session.set_speed(saved.speed);
        session.set_eq_mode(saved.eq_mode);
        session.set_eq_enabled(saved.eq_enabled);
        session.set_preamp(saved.preamp_db);
        session.set_limiter_enabled(saved.limiter_enabled);
        session.set_compressor_enabled(saved.compressor_enabled);
        session.set_spatial_enabled(saved.spatial_enabled);
        session.set_stereo_enhancer_enabled(saved.stereo_enhancer_enabled);
    }

    /// Push the complete graphic-EQ model as one engine command.
    pub(super) fn push_graphic_eq_state(&self, session: &AudioSession) {
        let eq = &self.eq_panel;
        let num = eq.graphic_layout.num_bands().min(eq.graphic_gains.len());
        session.set_graphic_eq_state(
            eq.graphic_layout.clone(),
            eq.graphic_gains[..num].to_vec(),
            eq.preamp_db,
            eq.eq_enabled,
            eq.auto_headroom,
        );
    }

    /// Push a curve that was just imported from a text file.
    ///
    /// Kept separate from [`Self::sync_chain_to_session`]: that function also
    /// re-asserts the output device and resampler policy, and reopening the
    /// device because somebody imported an EQ curve would glitch playback for
    /// no reason.
    pub fn push_imported_eq(&self, session: &AudioSession) {
        let eq = &self.eq_panel;
        if eq.eq_mode == config::EqMode::Graphic {
            session.set_eq_mode(config::EqMode::Graphic);
            self.push_graphic_eq_state(session);
            return;
        }

        session.set_eq_mode(config::EqMode::Parametric);
        session.set_eq_enabled(eq.eq_enabled);
        let bands: Vec<config::EqBandConfig> = eq
            .bands
            .iter()
            .map(|band| config::EqBandConfig {
                enabled: band.enabled,
                filter_type: band.filter_type.into(),
                frequency: band.freq,
                gain_db: band.gain_db,
                q: band.q,
            })
            .collect();
        session.set_eq_bands(bands);
        session.set_preamp(eq.preamp_db);
        // A text curve carries its own preamp. Static auto-headroom would
        // attenuate it a second time; the AutoEQ dialog makes the same call.
        session.set_eq_auto_headroom(false);
    }

    /// Refresh the cached device list for the selected backend.
    ///
    /// A no-op when the list is already for the current backend, so it is safe
    /// to call every frame — the expensive part only happens when the backend
    /// actually changed.
    pub fn refresh_output_devices(&mut self, session: &AudioSession, force: bool) {
        let backend = self.settings_tab.output_backend;
        if !force && self.output_devices_backend == Some(backend) {
            return;
        }
        self.output_devices = session.available_devices_for(backend);
        self.output_devices_backend = Some(backend);
    }

    /// The display name of the currently selected output device.
    ///
    /// `SettingsTabState.output_device` holds a backend id (WASAPI endpoint
    /// id, CoreAudio UID, …), which is useless for AutoEQ device matching.
    /// Resolves it through the cached device list; an empty selection maps to
    /// the backend's default device. Falls back to the configured id when the
    /// device enumeration has not populated yet.
    pub fn resolve_autoeq_device_name(&self) -> String {
        let selection = self.settings_tab.output_device.trim().to_string();
        let default_name = self
            .output_devices
            .iter()
            .filter(|d| d.is_default)
            .map(|d| d.name.clone())
            .next();
        if selection.is_empty() {
            return default_name.unwrap_or_default();
        }
        self.output_devices
            .iter()
            .find(|d| d.id == selection)
            .map(|d| d.name.clone())
            .unwrap_or(selection)
    }

    /// Sync all restored persistent DSP and EQ parameters to the active audio engine session
    pub fn sync_all_to_session(&self, session: &AudioSession) {
        // 1. The signal chain itself (EQ, tone, dynamics, filters, mix bus) is
        // pushed by `sync_chain_to_session` at the end of this function, so
        // startup and "apply a chain preset" restore exactly the same fields.
        // (Before this, the parametric bands, limiter/compressor parameters and
        // correction settings were persisted but never restored.)

        // 2. Settings Tab & Audio Engine parameters
        let st = &self.settings_tab;
        session.set_speed(st.speed);
        session.set_pitch(st.pitch_semitones);
        session.set_speed_mode(st.speed_mode);
        session.set_transition_mode(st.transition_mode);
        session.set_crossfade_config(CrossfadeConfig {
            // Only the crossfade transition actually crossfades. This used to
            // be `true` unconditionally, so choosing Gapless, Fade or Stop
            // still overlapped the tracks.
            enabled: st.transition_mode == TransitionMode::Crossfade,
            duration_ms: (st.crossfade_duration_secs * 1000.0) as u64,
            curve: st.crossfade_curve,
        });
        session.set_crossfade_curve(st.crossfade_curve);
        session.set_loudness_mode(st.loudness_mode);
        session.apply_profile(st.audio_profile);
        if !session.set_output_backend(st.output_backend) {
            log::warn!("The saved output backend could not be applied: the engine is not running");
        }
        if !st.output_device.is_empty()
            && !session.set_output_device(Some(st.output_device.clone()))
        {
            log::warn!("The saved output device could not be selected: the engine is not running");
        }
        session.set_sample_rate_policy(st.sample_rate_policy.clone());
        session.set_volume_mode(st.volume_mode);
        session.set_bit_perfect(st.bit_perfect);
        session.set_dither_enabled(st.dither_enabled);
        session.set_resampler_quality(st.resampler_quality);
        session.set_fallback_policy(st.fallback_policy);
        session.set_resampler_filter(st.resampler_filter);
        session.set_resampler_cutoff_ratio(st.resampler_cutoff_ratio);
        session.set_allowed_sample_rates(st.allowed_sample_rates.clone());
        session.set_dac_warmup_ms(st.dac_warmup_ms);
        session.set_noise_shaper(st.noise_shaper);
        session.set_dsd_output(st.dsd_output);
        session.set_dsd_noise_filter(st.dsd_noise_filter);
        session.set_ultrasonic_filter(st.ultrasonic_filter);
        session.set_sdm_config(st.sdm.clone());
        session.set_channel_routing(st.channel_routing.clone());
        session.set_dac_correction(st.dac_correction.clone());
        // Only push precision mode once the user has actually chosen one, so the
        // engine's own persisted value survives an upgrade untouched.
        if let Some(mode) = st.precision_mode {
            session.set_precision_mode(mode);
        }
        session.set_spatial_enabled(st.spatial_enabled);
        session.set_spatial_quality(st.spatial_quality);
        if !st.hrtf_profile.is_empty() {
            session.set_hrtf_profile(st.hrtf_profile.clone());
        }
        session.set_spatial_listener_tracking(st.listener_smoothing_ms, st.listener_max_rate);
        session.set_spatial_room(
            st.room_enabled,
            st.room_width,
            st.room_depth,
            st.room_height,
            st.room_absorption,
            2,
            st.room_rt60_ms,
            0.8,
            false,
            st.room_wet,
        );
        session.set_spatial_screen(
            st.screen_center_az,
            st.screen_half_w,
            st.screen_elevation,
            st.screen_gain,
        );
        session.set_spatial_listener_orientation(st.listener_yaw_deg);
        session.set_spatial_air_absorption(st.air_absorption_enabled);
        session.set_spatial_voice(
            st.voice_budget_enabled,
            st.voice_capacity,
            st.voice_capacity / 2,
            config::VoicePriority::DistanceWeighted,
        );
        session.set_stereo_enhancer_enabled(st.stereo_enhancer_enabled);

        // 3. The signal chain, last so this is the single place that defines
        // what "restore the chain" means.
        self.sync_chain_to_session(session);
    }

    /// Push only the signal-chain parameters (EQ, tone, dynamics, filters and
    /// the mix bus) to the session.
    ///
    /// This is deliberately narrower than [`Self::sync_all_to_session`], which
    /// also re-asserts output device, backend and resampler policy: those
    /// describe the hardware and re-opening the device mid-track would glitch
    /// playback. Used when applying a chain preset.
    pub fn sync_chain_to_session(&self, session: &AudioSession) {
        let eq = &self.eq_panel;

        // Equalizer: the explicit mode selects the curve source. Both models
        // are restored so switching modes does not lose the other curve.
        session.set_eq_mode(eq.eq_mode);
        session.set_eq_enabled(eq.eq_enabled);
        // One atomic bank: a chain restore may contain a 64-band AutoEQ curve,
        // and a shorter chain must not leave the previous curve's tail active.
        let parametric_bands = eq
            .bands
            .iter()
            .map(|band| config::EqBandConfig {
                enabled: band.enabled,
                filter_type: band.filter_type.into(),
                frequency: band.freq,
                gain_db: band.gain_db,
                q: band.q,
            })
            .collect();
        session.set_eq_bands(parametric_bands);
        session.set_preamp(eq.preamp_db);
        session.set_eq_auto_headroom(eq.auto_headroom);
        self.push_graphic_eq_state(session);

        // Tone.
        session.set_bass_shelf(eq.bass_shelf_db);
        session.set_treble_shelf(eq.treble_shelf_db);
        session.set_stereo_width(eq.stereo_width);
        session.set_balance(eq.balance);
        session.set_midside_eq(eq.midside_eq);
        session.set_dither_enabled(eq.dither_enabled);

        // Dynamics.
        session.set_limiter_enabled(eq.limiter_enabled);
        session.set_limiter_mode(eq.limiter_mode);
        session.set_limiter_true_peak(eq.limiter_true_peak);
        session.set_limiter_params(
            eq.limiter_lookahead_ms,
            eq.limiter_attack_ms,
            eq.limiter_release_ms,
            eq.limiter_ceiling_db,
            eq.limiter_soft_clip,
        );
        session.set_compressor_enabled(eq.compressor_enabled);
        for band in 0..3 {
            session.set_compressor_band_params(
                band,
                eq.comp_threshold[band],
                eq.comp_ratio[band],
                eq.comp_attack_ms[band],
                eq.comp_release_ms[band],
                eq.comp_makeup_db[band],
            );
            session.set_compressor_band_features(
                band,
                eq.comp_knee_db[band],
                config::CompressorDetector::Rms,
                eq.comp_stereo_link[band],
            );
        }

        // Filters & correction.
        session.set_crossfeed_enabled(eq.crossfeed_enabled);
        session.set_crossfeed_profile(eq.crossfeed_profile);
        session.set_crossfeed_custom_params(
            eq.crossfeed_freq,
            eq.crossfeed_q,
            eq.crossfeed_delay_ms,
        );
        session.set_correction_enabled(eq.correction_enabled);
        session.set_correction_depth(eq.correction_depth);
        if !eq.correction_ir_path.is_empty() {
            session.load_correction_ir(std::path::PathBuf::from(&eq.correction_ir_path));
        }

        // Creative effects: the whole insert in one call.
        session.set_fx_config(eq.fx);

        // Mix bus.
        let st = &self.settings_tab;
        session.set_bass_management(st.bass_management.clone());
        session.set_lfe_config(st.lfe.clone());
        session.set_channel_mix(st.channel_mix.clone());
    }
}

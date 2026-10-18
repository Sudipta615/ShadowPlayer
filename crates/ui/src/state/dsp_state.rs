use audio_session::AudioSession;
use config::{CrossfadeConfig, EnginePreset};
use crate::components::advanced_audio::{AdvancedAudioActions, AdvancedAudioState};
use crate::components::eq_panel::{EqPanelActions, EqPanelState};
use crate::components::settings_tab::{SettingsTabActions, SettingsTabState};

/// Central DSP state container and command dispatcher
pub struct DspState {
    pub advanced_audio: AdvancedAudioState,
    pub eq_panel: EqPanelState,
    pub settings_tab: SettingsTabState,
    pub autoeq: crate::modals::AutoEqModalState,
}

impl Default for DspState {
    fn default() -> Self {
        Self {
            advanced_audio: AdvancedAudioState::default(),
            eq_panel: EqPanelState::default(),
            settings_tab: SettingsTabState::default(),
            autoeq: crate::modals::AutoEqModalState::default(),
        }
    }
}

impl DspState {
    /// Sync all restored persistent DSP and EQ parameters to the active audio engine session
    pub fn sync_all_to_session(&self, session: &AudioSession) {
        // 1. Equalizer Studio & Tone Controls
        session.set_eq_enabled(self.eq_panel.eq_enabled);
        session.set_graphic_eq_enabled(self.eq_panel.graphic_enabled);
        session.set_graphic_eq_layout(self.eq_panel.graphic_layout.clone());
        let num = self.eq_panel.graphic_layout.num_bands();
        for b in 0..num {
            session.set_graphic_eq_slider(b, self.eq_panel.graphic_gains[b]);
        }
        session.set_graphic_eq_preamp(self.eq_panel.graphic_preamp_db);
        session.set_preamp(self.eq_panel.preamp_db);
        session.set_eq_auto_headroom(self.eq_panel.auto_headroom);

        session.set_bass_shelf(self.eq_panel.bass_shelf_db);
        session.set_treble_shelf(self.eq_panel.treble_shelf_db);
        session.set_stereo_width(self.eq_panel.stereo_width);
        session.set_balance(self.eq_panel.balance);
        session.set_midside_eq(self.eq_panel.midside_eq);
        session.set_dither_enabled(self.eq_panel.dither_enabled);

        session.set_limiter_enabled(self.eq_panel.limiter_enabled);
        session.set_limiter_mode(self.eq_panel.limiter_mode);
        session.set_compressor_enabled(self.eq_panel.compressor_enabled);
        session.set_crossfeed_enabled(self.eq_panel.crossfeed_enabled);
        session.set_crossfeed_profile(self.eq_panel.crossfeed_profile);

        // 2. Settings Tab & Audio Engine parameters
        let st = &self.settings_tab;
        session.set_speed(st.speed);
        session.set_pitch(st.pitch_semitones);
        session.set_speed_mode(st.speed_mode);
        session.set_transition_mode(st.transition_mode);
        session.set_crossfade_config(CrossfadeConfig {
            enabled: true,
            duration_ms: (st.crossfade_duration_secs * 1000.0) as u64,
            curve: st.crossfade_curve,
        });
        session.set_crossfade_curve(st.crossfade_curve);
        session.set_loudness_mode(st.loudness_mode);
        session.set_output_backend(st.output_backend);
        if !st.output_device.is_empty() {
            session.set_output_device(Some(st.output_device.clone()));
        }
        session.set_sample_rate_policy(st.sample_rate_policy.clone());
        session.set_volume_mode(st.volume_mode);
        session.set_bit_perfect(st.bit_perfect);
        session.set_dither_enabled(st.dither_enabled);
        session.set_resampler_quality(st.resampler_quality);
        session.set_fallback_policy(st.fallback_policy);
        // Only push precision mode once the user has actually chosen one, so the
        // engine's own persisted value survives an upgrade untouched.
        if let Some(mode) = st.precision_mode {
            session.set_precision_mode(mode);
        }
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
        session.set_spatial_screen(st.screen_center_az, st.screen_half_w, st.screen_elevation, st.screen_gain);
        session.set_spatial_voice(
            st.voice_budget_enabled,
            st.voice_capacity,
            st.voice_capacity / 2,
            config::VoicePriority::DistanceWeighted,
        );
        session.set_stereo_enhancer_enabled(st.stereo_enhancer_enabled);
    }

    /// Dispatch settings actions to the audio session
    pub fn handle_settings_actions(&mut self, session: &AudioSession, s: &SettingsTabActions) {
        if let Some(v) = s.speed_changed { session.set_speed(v); }
        if let Some(v) = s.pitch_changed { session.set_pitch(v); }
        if let Some(m) = s.speed_mode_changed { session.set_speed_mode(m); }
        if let Some(m) = s.transition_mode_changed { session.set_transition_mode(m); }
        if s.crossfade_config_changed {
            session.set_crossfade_config(CrossfadeConfig {
                enabled: true,
                duration_ms: (self.settings_tab.crossfade_duration_secs * 1000.0) as u64,
                curve: self.settings_tab.crossfade_curve,
            });
        }
        if let Some(c) = s.crossfade_curve_changed { session.set_crossfade_curve(c); }
        if let Some(m) = s.loudness_mode_changed { session.set_loudness_mode(m); }
        if let Some(b) = s.backend_changed { session.set_output_backend(b); }
        if let Some(ref d) = s.device_changed { session.set_output_device(if d.is_empty() { None } else { Some(d.clone()) }); }
        if let Some(ref p) = s.sample_rate_policy_changed { session.set_sample_rate_policy(p.clone()); }
        if let Some(m) = s.volume_mode_changed { session.set_volume_mode(m); }
        if let Some(b) = s.bit_perfect_changed { session.set_bit_perfect(b); }
        if let Some(b) = s.dither_changed { session.set_dither_enabled(b); }
        if let Some(q) = s.resampler_quality_changed { session.set_resampler_quality(q); }
        if let Some(p) = s.fallback_policy_changed { session.set_fallback_policy(p); }
        if let Some(m) = s.precision_mode_changed { session.set_precision_mode(m); }
        if s.open_asio_panel { session.open_asio_control_panel(); }
        if s.clear_output_profile { session.clear_output_profile(); }
        if let Some(q) = s.spatial_quality_changed { session.set_spatial_quality(q); }
        if let Some(ref id) = s.hrtf_profile_changed { session.set_hrtf_profile(id.clone()); }
        if s.listener_tracking_changed {
            session.set_spatial_listener_tracking(self.settings_tab.listener_smoothing_ms, self.settings_tab.listener_max_rate);
        }
        if s.spatial_room_changed {
            let st = &self.settings_tab;
            session.set_spatial_room(st.room_enabled, st.room_width, st.room_depth, st.room_height, st.room_absorption, 2, st.room_rt60_ms, 0.8, false, st.room_wet);
        }
        if s.spatial_screen_changed {
            let st = &self.settings_tab;
            session.set_spatial_screen(st.screen_center_az, st.screen_half_w, st.screen_elevation, st.screen_gain);
        }
        if s.voice_budget_changed {
            let st = &self.settings_tab;
            session.set_spatial_voice(st.voice_budget_enabled, st.voice_capacity, st.voice_capacity / 2, config::VoicePriority::DistanceWeighted);
        }
        if let Some(src) = s.add_track.clone() { session.add_track(src); }
        if let Some(slot) = s.remove_track { session.remove_track(slot); }
        if let Some((slot, g)) = s.track_gain_changed { session.set_track_gain(slot, g); }
        if let Some((slot, p)) = s.track_pan_changed { session.set_track_pan(slot, p); }
        if let Some((slot, m)) = s.track_mute_changed { session.set_input_mute(slot, m); }
        if let Some((slot, a)) = s.track_active_changed { session.set_input_active(slot, a); }
        if s.aux_changed {
            let st = &self.settings_tab;
            session.set_aux(st.aux_enabled, st.aux_return_gain);
        }
        if s.duck_tracks {
            let st = &self.settings_tab;
            session.duck_tracks(st.duck_source_slot, vec![], st.duck_threshold_db, st.duck_depth_db, st.duck_attack_ms, st.duck_release_ms);
        }
        if s.start_capture {
            let st = &self.settings_tab;
            let p = if st.capture_path.is_empty() { None } else { Some(std::path::PathBuf::from(&st.capture_path)) };
            let d = if st.capture_device.is_empty() { None } else { Some(st.capture_device.clone()) };
            session.start_capture(p, d);
        }
        if s.stop_capture { session.stop_capture(); }
        if let Some(b) = s.stereo_enhancer_changed { session.set_stereo_enhancer_enabled(b); }
        if let Some(ref path) = s.write_loudness_tags { session.write_loudness_tags(std::path::PathBuf::from(path)); }
        if s.recover_stream { session.recover_stream(); }
    }

    /// Dispatch quick advanced audio modal actions
    pub fn handle_audio_modal_actions(&mut self, session: &AudioSession, audio_actions: AdvancedAudioActions) {
        if let Some(preset) = audio_actions.preset_changed {
            let choice = match preset {
                EnginePreset::Consumer => domain::models::AudioProfileChoice::Normal,
                EnginePreset::Fidelity => domain::models::AudioProfileChoice::Fidelity,
                EnginePreset::LegacyLowPower => domain::models::AudioProfileChoice::LegacyLowPower,
            };
            session.apply_profile(choice);
        }
        if let Some((band, gain)) = audio_actions.eq_band_changed {
            session.set_graphic_eq_slider(band, gain);
        }
        if audio_actions.eq_reset {
            for b in 0..10 {
                session.set_graphic_eq_slider(b, 0.0);
            }
        }
        if let Some(eq_on) = audio_actions.eq_enabled_changed {
            session.set_graphic_eq_enabled(eq_on);
        }
        if let Some(sp_on) = audio_actions.spatial_enabled_changed {
            session.set_spatial_enabled(sp_on);
        }
        if let Some(sp_w) = audio_actions.spatial_width_changed {
            session.set_stereo_width(sp_w);
        }
        if let Some(bb) = audio_actions.bass_boost_changed {
            session.set_bass_shelf(bb);
        }
    }

    /// Dispatch equalizer studio and dynamics actions
    pub fn handle_eq_panel_actions(&mut self, session: &AudioSession, eq_actions: EqPanelActions) {
        if let Some(b) = eq_actions.eq_enabled_changed { session.set_eq_enabled(b); }
        if let Some(b) = eq_actions.eq_band_changed {
            let band = &self.eq_panel.bands[b];
            session.set_eq_band_params(b, band.freq, band.gain_db, band.q, band.filter_type, band.enabled);
        }
        if let Some(v) = eq_actions.eq_preamp_changed { session.set_preamp(v); }
        if let Some(b) = eq_actions.eq_auto_headroom_changed { session.set_eq_auto_headroom(b); }
        if eq_actions.eq_reset {
            for b in 0..10 { session.set_graphic_eq_slider(b, 0.0); }
        }
        if eq_actions.open_autoeq_dialog {
            self.autoeq.is_open = true;
        }

        if let Some(b) = eq_actions.graphic_enabled_changed { session.set_graphic_eq_enabled(b); }
        if let Some(layout) = eq_actions.graphic_layout_changed {
            let num = layout.num_bands();
            session.set_graphic_eq_layout(layout);
            for b in 0..num {
                session.set_graphic_eq_slider(b, self.eq_panel.graphic_gains[b]);
            }
        }
        if let Some(b) = eq_actions.graphic_slider_changed {
            session.set_graphic_eq_slider(b, self.eq_panel.graphic_gains[b]);
        }
        if let Some(v) = eq_actions.graphic_preamp_changed { session.set_graphic_eq_preamp(v); }
        if eq_actions.graphic_reset {
            for b in 0..64 { session.set_graphic_eq_slider(b, 0.0); }
        }

        if let Some(v) = eq_actions.bass_shelf_changed { session.set_bass_shelf(v); }
        if let Some(v) = eq_actions.treble_shelf_changed { session.set_treble_shelf(v); }
        if let Some(b) = eq_actions.midside_eq_changed { session.set_midside_eq(b); }
        if let Some(v) = eq_actions.stereo_width_changed { session.set_stereo_width(v); }
        if let Some(v) = eq_actions.balance_changed { session.set_balance(v); }
        if let Some(d) = eq_actions.dither_enabled_changed { session.set_dither_enabled(d); }

        if let Some(b) = eq_actions.limiter_enabled_changed { session.set_limiter_enabled(b); }
        if let Some(m) = eq_actions.limiter_mode_changed { session.set_limiter_mode(m); }
        if let Some(b) = eq_actions.limiter_true_peak_changed { session.set_limiter_true_peak(b); }
        if eq_actions.limiter_params_changed {
            let s = &self.eq_panel;
            session.set_limiter_params(s.limiter_lookahead_ms, s.limiter_attack_ms, s.limiter_release_ms, s.limiter_ceiling_db, s.limiter_soft_clip);
        }

        if let Some(b) = eq_actions.compressor_enabled_changed { session.set_compressor_enabled(b); }
        if let Some(band) = eq_actions.comp_band_params_changed {
            let s = &self.eq_panel;
            session.set_compressor_band_params(band, s.comp_threshold[band], s.comp_ratio[band], s.comp_attack_ms[band], s.comp_release_ms[band], s.comp_makeup_db[band]);
        }
        if let Some(band) = eq_actions.comp_band_features_changed {
            let s = &self.eq_panel;
            session.set_compressor_band_features(band, s.comp_knee_db[band], config::CompressorDetector::Rms, s.comp_stereo_link[band]);
        }

        if let Some(b) = eq_actions.crossfeed_enabled_changed { session.set_crossfeed_enabled(b); }
        if let Some(p) = eq_actions.crossfeed_profile_changed { session.set_crossfeed_profile(p); }
        if eq_actions.crossfeed_custom_changed {
            let s = &self.eq_panel;
            session.set_crossfeed_custom_params(s.crossfeed_freq, s.crossfeed_q, s.crossfeed_delay_ms);
        }

        if let Some(b) = eq_actions.correction_enabled_changed { session.set_correction_enabled(b); }
        if let Some(v) = eq_actions.correction_depth_changed { session.set_correction_depth(v); }
        if eq_actions.load_correction_ir && !self.eq_panel.correction_ir_path.is_empty() {
            session.load_correction_ir(std::path::PathBuf::from(&self.eq_panel.correction_ir_path));
        }
    }
}

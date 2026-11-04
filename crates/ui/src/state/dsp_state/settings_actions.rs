//! Dispatch for the typed actions emitted by the Settings panel.

use super::*;

impl DspState {
    /// Dispatch settings actions to the audio session
    pub fn handle_settings_actions(&mut self, session: &AudioSession, s: &SettingsTabActions) {
        if let Some(v) = s.speed_changed {
            session.set_speed(v);
        }
        if let Some(v) = s.pitch_changed {
            session.set_pitch(v);
        }
        if let Some(m) = s.speed_mode_changed {
            session.set_speed_mode(m);
        }
        if let Some(m) = s.transition_mode_changed {
            session.set_transition_mode(m);
        }
        if s.crossfade_config_changed {
            session.set_crossfade_config(CrossfadeConfig {
                // Same rule as `sync_all_to_session`: only the crossfade
                // transition crossfades.
                enabled: self.settings_tab.transition_mode == TransitionMode::Crossfade,
                duration_ms: (self.settings_tab.crossfade_duration_secs * 1000.0) as u64,
                curve: self.settings_tab.crossfade_curve,
            });
        }
        if let Some(c) = s.crossfade_curve_changed {
            session.set_crossfade_curve(c);
        }
        if let Some(m) = s.loudness_mode_changed {
            session.set_loudness_mode(m);
        }
        if let Some(b) = s.backend_changed {
            if !session.set_output_backend(b) {
                log::warn!("The output backend could not be changed: the engine is not running");
            }
        }
        if let Some(ref d) = s.device_changed {
            let device = if d.is_empty() { None } else { Some(d.clone()) };
            if !session.set_output_device(device) {
                log::warn!("The output device could not be changed: the engine is not running");
            }
        }
        if let Some(ref p) = s.sample_rate_policy_changed {
            session.set_sample_rate_policy(p.clone());
        }
        if let Some(m) = s.volume_mode_changed {
            session.set_volume_mode(m);
        }
        if let Some(b) = s.bit_perfect_changed {
            // The persistent Output switch normally controls the engine-level
            // bypass without touching the chain. If the user leaves Pure
            // Direct here, though, the engine must also get the captured chain
            // back; otherwise the studio stays flat while bit-perfect is off.
            if !b && self.pure_direct_saved.is_some() {
                self.exit_pure_direct(session);
            } else {
                session.set_bit_perfect(b);
            }
        }
        if let Some(b) = s.dither_changed {
            session.set_dither_enabled(b);
        }
        if let Some(q) = s.resampler_quality_changed {
            session.set_resampler_quality(q);
        }
        if let Some(f) = s.resampler_filter_changed {
            session.set_resampler_filter(f);
        }
        if let Some(r) = s.resampler_cutoff_ratio_changed {
            session.set_resampler_cutoff_ratio(r);
        }
        if let Some(ref rates) = s.allowed_sample_rates_changed {
            session.set_allowed_sample_rates(rates.clone());
        }
        if let Some(ms) = s.dac_warmup_ms_changed {
            session.set_dac_warmup_ms(ms);
        }
        if let Some(n) = s.noise_shaper_changed {
            session.set_noise_shaper(n);
        }
        if let Some(o) = s.dsd_output_changed {
            session.set_dsd_output(o);
        }
        if let Some(f) = s.dsd_noise_filter_changed {
            session.set_dsd_noise_filter(f);
        }
        if let Some(f) = s.ultrasonic_filter_changed {
            session.set_ultrasonic_filter(f);
        }
        if let Some(ref c) = s.sdm_config_changed {
            session.set_sdm_config(c.clone());
        }
        if let Some(ref c) = s.channel_routing_changed {
            session.set_channel_routing(c.clone());
        }
        if let Some(ref c) = s.dac_correction_changed {
            session.set_dac_correction(c.clone());
        }
        if let Some(p) = s.fallback_policy_changed {
            session.set_fallback_policy(p);
        }
        if let Some(m) = s.precision_mode_changed {
            session.set_precision_mode(m);
        }
        if let Some(ref c) = s.bass_management_changed {
            session.set_bass_management(c.clone());
        }
        if let Some(ref c) = s.lfe_changed {
            session.set_lfe_config(c.clone());
        }
        if let Some(ref c) = s.channel_mix_changed {
            session.set_channel_mix(c.clone());
        }
        if s.open_asio_panel {
            session.open_asio_control_panel();
        }
        if s.clear_output_profile {
            session.clear_output_profile();
        }
        if let Some(enabled) = s.spatial_enabled_changed {
            session.set_spatial_enabled(enabled);
        }
        if let Some(q) = s.spatial_quality_changed {
            session.set_spatial_quality(q);
        }
        if let Some(ref id) = s.hrtf_profile_changed {
            session.set_hrtf_profile(id.clone());
        }
        if s.listener_tracking_changed {
            session.set_spatial_listener_tracking(
                self.settings_tab.listener_smoothing_ms,
                self.settings_tab.listener_max_rate,
            );
        }
        if s.spatial_room_changed {
            let st = &self.settings_tab;
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
        }
        if s.spatial_screen_changed {
            let st = &self.settings_tab;
            session.set_spatial_screen(
                st.screen_center_az,
                st.screen_half_w,
                st.screen_elevation,
                st.screen_gain,
            );
        }
        if s.spatial_listener_changed {
            session.set_spatial_listener_orientation(self.settings_tab.listener_yaw_deg);
        }
        if let Some(enabled) = s.air_absorption_changed {
            session.set_spatial_air_absorption(enabled);
        }
        if let Some(profile) = s.audio_profile_changed {
            session.apply_profile(profile);
        }
        if s.voice_budget_changed {
            let st = &self.settings_tab;
            session.set_spatial_voice(
                st.voice_budget_enabled,
                st.voice_capacity,
                st.voice_capacity / 2,
                config::VoicePriority::DistanceWeighted,
            );
        }
        if let Some(src) = s.add_track.clone() {
            session.add_track(src);
        }
        if let Some(slot) = s.remove_track {
            session.remove_track(slot);
        }
        if let Some((slot, g)) = s.track_gain_changed {
            session.set_track_gain(slot, g);
        }
        if let Some((slot, p)) = s.track_pan_changed {
            session.set_track_pan(slot, p);
        }
        if let Some((slot, m)) = s.track_mute_changed {
            session.set_input_mute(slot, m);
        }
        if let Some((slot, a)) = s.track_active_changed {
            session.set_input_active(slot, a);
        }
        if s.aux_changed {
            let st = &self.settings_tab;
            session.set_aux(st.aux_enabled, st.aux_return_gain);
        }
        if s.duck_tracks {
            let st = &self.settings_tab;
            session.duck_tracks(
                st.duck_source_slot,
                vec![],
                st.duck_threshold_db,
                st.duck_depth_db,
                st.duck_attack_ms,
                st.duck_release_ms,
            );
        }

        // Chain presets. Saving captures the *current* chain; applying writes a
        // whole chain back and pushes it in one pass, since the individual
        // "changed" actions above are per-control.
        if let Some(name) = s.chain_preset_save.clone() {
            let note = self.settings_tab.chain_preset_draft_note.clone();
            let preset =
                ChainPreset::capture(name.clone(), note, &self.eq_panel, &self.settings_tab);
            chain_preset::upsert(&mut self.settings_tab.chain_presets, preset);
            self.settings_tab.chain_preset_notice =
                Some(format!("Saved chain preset \"{}\"", name.trim()));
        }
        if let Some(idx) = s.chain_preset_apply {
            if let Some(preset) = self.settings_tab.chain_presets.get(idx).cloned() {
                preset.apply(&mut self.eq_panel, &mut self.settings_tab);
                self.sync_chain_to_session(session);
                self.settings_tab.chain_preset_notice =
                    Some(format!("Applied chain preset \"{}\"", preset.name));
            }
        }
        if let Some((idx, name)) = s.chain_preset_rename.clone() {
            let name = name.trim().to_string();
            if !name.is_empty() {
                if let Some(preset) = self.settings_tab.chain_presets.get_mut(idx) {
                    preset.name = name;
                }
            }
        }
        if let Some(idx) = s.chain_preset_delete {
            if idx < self.settings_tab.chain_presets.len() {
                self.settings_tab.chain_presets.remove(idx);
                self.settings_tab.chain_preset_renaming = None;
            }
        }
        if s.start_capture {
            let st = &self.settings_tab;
            let p = if st.capture_path.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(&st.capture_path))
            };
            let d = if st.capture_device.is_empty() {
                None
            } else {
                Some(st.capture_device.clone())
            };
            session.start_capture(p, d);
        }
        if s.stop_capture {
            session.stop_capture();
        }
        if let Some(b) = s.stereo_enhancer_changed {
            session.set_stereo_enhancer_enabled(b);
        }
        if let Some(ref path) = s.write_loudness_tags {
            session.write_loudness_tags(std::path::PathBuf::from(path));
        }
        if s.recover_stream {
            session.recover_stream();
        }

        // CLAP plugin insert. The card routes; the rack window does the work.
        if s.clap_open_rack {
            self.clap_rack.is_open = true;
            // The engine rebuilds its graph when the config changes, and only
            // then does a plugin restored from that config become visible. Ask
            // for a fresh view so the rack opens on what is actually loaded
            // rather than on the last publish.
            session.refresh_clap_plugin();
        }
        if s.clap_unload {
            session.unload_clap_plugin();
            self.clap_rack.announce("Plugin unloaded".to_string());
        }
        if s.clap_choose_library {
            // Only the app owns the native picker, and both settings surfaces
            // can raise this, so it goes out as a flag the app clears.
            self.settings_tab.request_clap_library = true;
        }
    }
}

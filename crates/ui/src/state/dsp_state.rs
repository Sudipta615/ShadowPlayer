use crate::components::eq_panel::{EqPanelActions, EqPanelState};
use crate::components::settings_tab::{SettingsTabActions, SettingsTabState};
use crate::state::chain_preset::{self, ChainPreset};
use crate::state::clap_rack::{ClapRackActions, ClapRackState};
use audio_session::AudioSession;
use config::{CrossfadeConfig, TransitionMode};

/// Central DSP state container and command dispatcher
#[derive(Default)]
pub struct DspState {
    pub eq_panel: EqPanelState,
    pub settings_tab: SettingsTabState,
    pub autoeq: crate::modals::AutoEqModalState,
    /// The CLAP plugin rack: its window state and the named preset library.
    pub clap_rack: ClapRackState,
}

impl DspState {
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

        // Equalizer: parametric bands first, then the graphic layer that
        // takes precedence when enabled.
        session.set_eq_enabled(eq.eq_enabled);
        for (i, band) in eq.bands.iter().enumerate() {
            session.set_eq_band_params(
                i,
                band.freq,
                band.gain_db,
                band.q,
                band.filter_type,
                band.enabled,
            );
        }
        session.set_preamp(eq.preamp_db);
        session.set_eq_auto_headroom(eq.auto_headroom);

        session.set_graphic_eq_enabled(eq.graphic_enabled);
        session.set_graphic_eq_layout(eq.graphic_layout.clone());
        for b in 0..eq.graphic_layout.num_bands() {
            session.set_graphic_eq_slider(b, eq.graphic_gains[b]);
        }
        session.set_graphic_eq_preamp(eq.graphic_preamp_db);

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

        // Mix bus.
        let st = &self.settings_tab;
        session.set_bass_management(st.bass_management.clone());
        session.set_lfe_config(st.lfe.clone());
        session.set_channel_mix(st.channel_mix.clone());
    }

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
            session.set_bit_perfect(b);
        }
        if let Some(b) = s.dither_changed {
            session.set_dither_enabled(b);
        }
        if let Some(q) = s.resampler_quality_changed {
            session.set_resampler_quality(q);
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

    /// Mirror the engine's published rack view into the settings card.
    ///
    /// Called once a frame by the app rather than read by the card itself: the
    /// summary is also wanted while the settings page is closed, and one place
    /// fetching it keeps the two settings surfaces (drawer and full page)
    /// describing the same plugin.
    pub fn refresh_clap_summary(&mut self, session: &AudioSession) {
        self.settings_tab.clap =
            crate::state::clap_rack::ClapCardSummary::from_view(&session.clap_view());
    }

    /// Dispatch the plugin rack's window actions.
    pub fn handle_clap_rack_actions(&mut self, session: &AudioSession, a: ClapRackActions) {
        if a.close {
            self.clap_rack.is_open = false;
        }
        if let Some(path) = a.probe_library {
            session.probe_clap_library(&path);
            self.clap_rack.probe_path = Some(path);
        }
        if let Some((path, id)) = a.load_plugin {
            session.load_clap_plugin(&path, Some(&id));
            self.clap_rack.last_library = path;
            self.clap_rack.notice = None;
        }
        if a.unload {
            session.unload_clap_plugin();
            self.clap_rack.announce("Plugin unloaded".to_string());
        }
        if let Some(enabled) = a.set_enabled {
            session.set_clap_plugin_enabled(enabled);
        }
        if let Some((id, value)) = a.set_param {
            session.set_clap_plugin_param(id, value);
        }
        if a.refresh {
            session.refresh_clap_plugin();
        }

        if a.save_preset {
            let view = session.clap_view();
            if self.clap_rack.begin_save(&view) {
                if view.rack.has_state {
                    // The plugin's state only the engine can fetch. Arm the
                    // save and let `poll_clap_state_capture` complete it.
                    session.save_clap_state();
                } else {
                    self.clap_rack.save_without_state(&view);
                }
            }
        }

        if let Some(index) = a.apply_preset {
            self.apply_clap_preset(session, index);
        }
        if let Some(index) = a.delete_preset {
            if index < self.clap_rack.presets.len() {
                let removed = self.clap_rack.presets.remove(index);
                self.clap_rack.selected_preset = None;
                self.clap_rack.notice =
                    Some(format!("Deleted preset \u{201c}{}\u{201d}", removed.name));
            }
        }
    }

    /// Push a preset's values and state into the live plugin.
    ///
    /// A preset is only meaningful against the plugin it came from: parameter
    /// ids are the plugin's own numbering, so recalling one into a different
    /// plugin would set arbitrary controls.
    pub fn apply_clap_preset(&mut self, session: &AudioSession, index: usize) {
        let Some(preset) = self.clap_rack.presets.get(index).cloned() else {
            return;
        };
        let view = session.clap_view();

        if !view.rack.is_loaded() {
            self.clap_rack.announce(format!(
                "Load {} before recalling \u{201c}{}\u{201d}",
                if preset.plugin_name.is_empty() {
                    "the plugin"
                } else {
                    &preset.plugin_name
                },
                preset.name
            ));
            return;
        }

        let loaded_id = view
            .rack
            .plugin
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        if !preset.matches_plugin(&loaded_id) {
            self.clap_rack.announce(format!(
                "\u{201c}{}\u{201d} was captured from {}, not {}",
                preset.name,
                preset.plugin_name,
                view.rack.display_name()
            ));
            return;
        }

        // The plugin's own state first, then the explicit values: loading a
        // state blob may reset parameters, and the values the user set are the
        // ones that should win.
        let restored_state = match preset.state_bytes() {
            Some(bytes) => {
                session.load_clap_state(bytes);
                true
            }
            None => false,
        };
        for (id, value) in &preset.params {
            session.set_clap_plugin_param(*id, *value);
        }

        self.clap_rack.selected_preset = Some(index);
        self.clap_rack.announce(format!(
            "Recalled \u{201c}{}\u{201d}{}",
            preset.name,
            if restored_state {
                ""
            } else {
                " (parameter values only)"
            }
        ));
    }

    /// Complete a preset save once the engine hands over the plugin's state.
    ///
    /// The deadline matters: a plugin that refuses `state.save` leaves the save
    /// half-armed, and storing the parameter values alone is a better outcome
    /// than storing nothing at all.
    pub fn poll_clap_state_capture(&mut self, session: &AudioSession) {
        if self.clap_rack.pending_save.is_none() {
            // A blob nobody asked for must not be mistaken for the next save's.
            let _ = session.take_clap_state();
            return;
        }
        let view = session.clap_view();
        if let Some(bytes) = session.take_clap_state() {
            self.clap_rack.finish_save(&view, Some(bytes));
        } else if self
            .clap_rack
            .pending_save
            .as_ref()
            .is_some_and(|p| p.expired())
        {
            self.clap_rack.save_without_state(&view);
        }
    }

    /// Dispatch equalizer studio and dynamics actions
    pub fn handle_eq_panel_actions(&mut self, session: &AudioSession, eq_actions: EqPanelActions) {
        if let Some(b) = eq_actions.eq_enabled_changed {
            session.set_eq_enabled(b);
        }
        if let Some(b) = eq_actions.eq_band_changed {
            let band = &self.eq_panel.bands[b];
            session.set_eq_band_params(
                b,
                band.freq,
                band.gain_db,
                band.q,
                band.filter_type,
                band.enabled,
            );
        }
        if let Some(v) = eq_actions.eq_preamp_changed {
            session.set_preamp(v);
        }
        if let Some(b) = eq_actions.eq_auto_headroom_changed {
            session.set_eq_auto_headroom(b);
        }
        if eq_actions.eq_reset {
            for b in 0..10 {
                session.set_graphic_eq_slider(b, 0.0);
            }
        }
        if eq_actions.open_autoeq_dialog {
            self.autoeq.is_open = true;
        }

        if let Some(b) = eq_actions.graphic_enabled_changed {
            session.set_graphic_eq_enabled(b);
        }
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
        if let Some(v) = eq_actions.graphic_preamp_changed {
            session.set_graphic_eq_preamp(v);
        }
        if eq_actions.graphic_reset {
            for b in 0..64 {
                session.set_graphic_eq_slider(b, 0.0);
            }
        }

        if let Some(v) = eq_actions.bass_shelf_changed {
            session.set_bass_shelf(v);
        }
        if let Some(v) = eq_actions.treble_shelf_changed {
            session.set_treble_shelf(v);
        }
        if let Some(b) = eq_actions.midside_eq_changed {
            session.set_midside_eq(b);
        }
        if let Some(v) = eq_actions.stereo_width_changed {
            session.set_stereo_width(v);
        }
        if let Some(v) = eq_actions.balance_changed {
            session.set_balance(v);
        }
        if let Some(d) = eq_actions.dither_enabled_changed {
            session.set_dither_enabled(d);
        }

        if let Some(b) = eq_actions.limiter_enabled_changed {
            session.set_limiter_enabled(b);
        }
        if let Some(m) = eq_actions.limiter_mode_changed {
            session.set_limiter_mode(m);
        }
        if let Some(b) = eq_actions.limiter_true_peak_changed {
            session.set_limiter_true_peak(b);
        }
        if eq_actions.limiter_params_changed {
            let s = &self.eq_panel;
            session.set_limiter_params(
                s.limiter_lookahead_ms,
                s.limiter_attack_ms,
                s.limiter_release_ms,
                s.limiter_ceiling_db,
                s.limiter_soft_clip,
            );
        }

        if let Some(b) = eq_actions.compressor_enabled_changed {
            session.set_compressor_enabled(b);
        }
        if let Some(band) = eq_actions.comp_band_params_changed {
            let s = &self.eq_panel;
            session.set_compressor_band_params(
                band,
                s.comp_threshold[band],
                s.comp_ratio[band],
                s.comp_attack_ms[band],
                s.comp_release_ms[band],
                s.comp_makeup_db[band],
            );
        }
        if let Some(band) = eq_actions.comp_band_features_changed {
            let s = &self.eq_panel;
            session.set_compressor_band_features(
                band,
                s.comp_knee_db[band],
                config::CompressorDetector::Rms,
                s.comp_stereo_link[band],
            );
        }

        if let Some(b) = eq_actions.crossfeed_enabled_changed {
            session.set_crossfeed_enabled(b);
        }
        if let Some(p) = eq_actions.crossfeed_profile_changed {
            session.set_crossfeed_profile(p);
        }
        if eq_actions.crossfeed_custom_changed {
            let s = &self.eq_panel;
            session.set_crossfeed_custom_params(
                s.crossfeed_freq,
                s.crossfeed_q,
                s.crossfeed_delay_ms,
            );
        }

        if let Some(b) = eq_actions.correction_enabled_changed {
            session.set_correction_enabled(b);
        }
        if let Some(v) = eq_actions.correction_depth_changed {
            session.set_correction_depth(v);
        }
        if eq_actions.load_correction_ir && !self.eq_panel.correction_ir_path.is_empty() {
            session.load_correction_ir(std::path::PathBuf::from(&self.eq_panel.correction_ir_path));
        }
    }
}

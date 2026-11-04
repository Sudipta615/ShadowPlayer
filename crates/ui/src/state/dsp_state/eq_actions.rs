//! Dispatch for the typed actions emitted by the EQ panel.

use super::*;

impl DspState {
    /// Dispatch equalizer studio and dynamics actions
    pub fn handle_eq_panel_actions(&mut self, session: &AudioSession, eq_actions: EqPanelActions) {
        if eq_actions.fx_changed {
            session.set_fx_config(self.eq_panel.fx);
        }
        if eq_actions.eq_mode_changed {
            session.set_eq_mode(self.eq_panel.eq_mode);
            // Send the curve with the mode so the newly selected source starts
            // from the state the panel is showing, not from an older command.
            self.push_graphic_eq_state(session);
        }
        if let Some(enabled) = eq_actions.eq_enabled_changed {
            if self.eq_panel.eq_mode == config::EqMode::Graphic {
                // The atomic snapshot carries the enable state, so this cannot
                // race a layout/slider update from the same frame.
                self.push_graphic_eq_state(session);
            } else {
                session.set_eq_enabled(enabled);
            }
        }
        if let Some(b) = eq_actions.eq_band_changed {
            if let Some(band) = self.eq_panel.bands.get(b) {
                session.set_eq_band_params(
                    b,
                    band.freq,
                    band.gain_db,
                    band.q,
                    band.filter_type,
                    band.enabled,
                );
            }
        }
        if eq_actions.eq_bands_changed {
            // Replace the entire bank atomically. The panel keeps all 64 bands
            // an AutoEQ curve can contain, and a reset must flatten every one
            // rather than leaving a stale tail audible.
            let bands = self
                .eq_panel
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
        }
        if let Some(b) = eq_actions.eq_auto_headroom_changed {
            session.set_eq_auto_headroom(b);
        }
        if eq_actions.open_autoeq_dialog {
            self.autoeq.is_open = true;
        }

        // One command for every graphic edit: layout change, preset load,
        // reset, slider drag or preamp move. This is what makes a preset
        // atomic and immune to the engine's 64-commands-per-tick budget.
        if eq_actions.graphic_eq_changed {
            self.push_graphic_eq_state(session);
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
        if eq_actions.comp_params_reset {
            let s = &self.eq_panel;
            for band in 0..3 {
                session.set_compressor_band_params(
                    band,
                    s.comp_threshold[band],
                    s.comp_ratio[band],
                    s.comp_attack_ms[band],
                    s.comp_release_ms[band],
                    s.comp_makeup_db[band],
                );
                session.set_compressor_band_features(
                    band,
                    s.comp_knee_db[band],
                    config::CompressorDetector::Rms,
                    s.comp_stereo_link[band],
                );
            }
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

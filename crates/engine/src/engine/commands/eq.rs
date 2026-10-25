//! EQ command handlers — parametric EQ, graphic EQ, bass/treble shelves,
//! preamp, mid-side mode.

use log::info;

use super::super::AudioEngine;
use crate::dsp::equalizer::{EqBandParams, EqFilterType, MAX_EQ_BANDS};
use config::EqMode;

impl AudioEngine {
    pub(super) fn handle_set_eq_enabled(&mut self, enabled: bool) {
        self.config.eq.enabled = enabled;
        // The model's own flag records whether the graphic *layer* is active.
        // In parametric mode it stays off even while the master EQ is on, so a
        // saved config can tell the two modes apart on the next launch.
        match self.config.eq.mode {
            EqMode::Graphic => {
                self.graphic_eq.set_enabled(enabled);
                self.sync_graphic_eq();
            }
            EqMode::Parametric => {
                self.graphic_eq.set_enabled(false);
                // Keep the persisted mirror false in the explicit parametric
                // mode. Leaving a stale true here would make a later build read
                // the file as a legacy graphic config and flip the mode.
                self.config.graphic_eq.enabled = false;
                self.graph.set_eq_enabled(enabled);
            }
        }
    }

    /// Switch which curve source owns the single pipeline EQ.
    ///
    /// Both models are retained. Switching to parametric replays the
    /// serialized `EqConfig` bands (including an AutoEQ preset) into the stage;
    /// switching to graphic compiles the slider model. Neither direction
    /// destroys the other curve.
    pub(super) fn handle_set_eq_mode(&mut self, mode: EqMode) {
        if self.config.eq.mode == mode {
            return;
        }
        self.config.eq.mode = mode;
        match mode {
            EqMode::Graphic => {
                self.graphic_eq.set_enabled(self.config.eq.enabled);
                self.graphic_eq.set_preamp_db(self.config.eq.preamp_db);
                self.sync_graphic_eq();
            }
            EqMode::Parametric => {
                self.graphic_eq.set_enabled(false);
                self.config.graphic_eq.enabled = false;
                self.sync_parametric_eq();
            }
        }
        info!("EQ mode: {:?}", mode);
    }

    /// Replay the parametric configuration into the pipeline stage.
    fn sync_parametric_eq(&mut self) {
        let config = self.config.eq.clone();
        let mut params: Vec<EqBandParams> = config
            .bands
            .iter()
            .take(MAX_EQ_BANDS)
            .map(|band| EqBandParams {
                frequency: band.frequency,
                gain_db: band.gain_db,
                q: band.q,
                filter_type: EqFilterType::from(band.filter_type),
                enabled: band.enabled,
            })
            .collect();
        if params.is_empty() {
            // A preset whose every band was rejected must still replace the
            // stage, otherwise the previous mode's filters keep processing
            // behind an "enabled" parametric layer.
            params.push(EqBandParams::default());
        }
        self.graph.with_graph(|graph| {
            let eq = &mut graph.eq_mut().eq;
            eq.set_band_layout(&params);
            eq.set_preamp_db(config.preamp_db);
            eq.set_auto_headroom(config.auto_headroom);
            eq.set_enabled(config.enabled);
        });
    }

    pub(super) fn handle_set_eq_auto_headroom(&mut self, enabled: bool) {
        self.config.eq.auto_headroom = enabled;
        self.graph.set_eq_auto_headroom(enabled);
        info!(
            "EQ auto headroom: {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Enable or disable the dynamic-EQ corrective layer.
    ///
    /// The config mirror is what carries the setting across a generation
    /// rebuild — `apply_config` re-applies `config.eq.dynamic_eq` when the
    /// arena is rebuilt on a track or sample-rate change, so without the
    /// write-back a live toggle would silently revert.
    pub(super) fn handle_set_dynamic_eq_enabled(&mut self, enabled: bool) {
        self.config.eq.dynamic_eq.enabled = enabled;
        self.graph.set_dynamic_eq_enabled(enabled);
        info!(
            "Dynamic EQ: {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Set one dynamic-EQ band's parameters.
    ///
    /// Mirrors into `config.eq.dynamic_eq.bands` when the index is in range
    /// so a generation rebuild preserves it, matching the static EQ's
    /// user-state replay. An out-of-range index is logged and dropped — the
    /// static `SetEqBand` does the same, so the two agree on the contract.
    pub(super) fn handle_set_dynamic_eq_band(
        &mut self,
        index: usize,
        params: crate::dsp::equalizer::DynamicEqBandParams,
    ) {
        if index >= crate::dsp::equalizer::MAX_DYNAMIC_EQ_BANDS {
            log::warn!(
                "Dynamic EQ band index {} is outside range (max {})",
                index,
                crate::dsp::equalizer::MAX_DYNAMIC_EQ_BANDS
            );
            return;
        }
        let bands = &mut self.config.eq.dynamic_eq.bands;
        if bands.len() <= index {
            bands.resize_with(index + 1, Default::default);
        }
        bands[index] = config::DynamicEqBandConfig::from(&params);
        self.graph.set_dynamic_eq_band(index, params);
    }

    pub(super) fn handle_set_eq_band(
        &mut self,
        index: usize,
        frequency: f32,
        gain_db: f32,
        q: f32,
        enabled: bool,
    ) {
        let num_bands = self.graph.eq_num_bands();
        let filter_type = if index == 0 {
            EqFilterType::LowShelf
        } else if num_bands > 1 && index == num_bands - 1 {
            EqFilterType::HighShelf
        } else {
            EqFilterType::Peaking
        };
        self.handle_set_eq_band_params(index, frequency, gain_db, q, filter_type, enabled);
    }

    pub(super) fn handle_set_eq_band_params(
        &mut self,
        index: usize,
        frequency: f32,
        gain_db: f32,
        q: f32,
        filter_type: crate::dsp::equalizer::EqFilterType,
        enabled: bool,
    ) {
        if index >= MAX_EQ_BANDS {
            return;
        }
        let config_band = config::EqBandConfig {
            enabled,
            filter_type: filter_type.into(),
            frequency,
            gain_db,
            q,
        };
        if self.config.eq.bands.len() <= index {
            self.config.eq.bands.resize(index + 1, Default::default());
        }
        self.config.eq.bands[index] = config_band;

        // Only the active mode may hear the edit. Mirroring the band into the
        // config while Graphic is active is deliberate: switching to Parametric
        // then restores exactly what the host last sent, rather than the curve
        // that happened to be in the stage when the mode changed.
        if self.config.eq.mode != EqMode::Parametric {
            return;
        }
        self.graph.with_graph(|graph| {
            let eq = &mut graph.eq_mut().eq;
            if eq.num_bands() <= index {
                eq.set_band_count(index + 1);
            }
            eq.set_band(
                index,
                EqBandParams {
                    frequency,
                    gain_db,
                    q,
                    filter_type,
                    enabled,
                },
            );
        });
    }

    /// Replace the whole parametric bank without changing the master enable,
    /// preamp, or auto-headroom preference.
    ///
    /// The UI models ten editable bands, while an AutoEQ curve can carry up to
    /// 64. A reset therefore has to replace the bank atomically instead of
    /// sending ten per-band updates, which used to leave bands 10–63 of the
    /// previous curve audible.
    pub(super) fn handle_set_eq_bands(&mut self, bands: Vec<config::EqBandConfig>) {
        self.config.eq.bands = bands
            .into_iter()
            .filter(|band| {
                band.frequency.is_finite()
                    && band.frequency > 0.0
                    && band.gain_db.is_finite()
                    && band.q.is_finite()
                    && band.q > 0.0
            })
            .take(MAX_EQ_BANDS)
            .collect();
        // The bank is part of the parametric source, not a request to select
        // it. The caller owns the mode; compiling here only when parametric is
        // active keeps a graphic snapshot free to own the stage afterwards.
        if self.config.eq.mode == EqMode::Parametric {
            self.sync_parametric_eq();
        }
        info!(
            "Parametric EQ: {} bands replaced",
            self.config.eq.bands.len()
        );
    }

    pub(super) fn handle_set_eq_preset(&mut self, preset: config::EqPreset) {
        self.config.eq.bands = preset
            .bands
            .iter()
            .filter_map(|band| {
                let config_band = config::EqBandConfig {
                    enabled: band.enabled,
                    filter_type: band.filter_type,
                    frequency: band.frequency,
                    gain_db: band.gain_db,
                    q: band.q,
                };
                (config_band.frequency.is_finite()
                    && config_band.frequency > 0.0
                    && config_band.gain_db.is_finite()
                    && config_band.q.is_finite()
                    && config_band.q > 0.0)
                    .then_some(config_band)
            })
            .collect();
        // The DSP range is the contract. Keeping a larger value in the
        // config would reload at the same number while the stage silently
        // played the clamped one.
        self.config.eq.preamp_db = if preset.preamp_db.is_finite() {
            preset.preamp_db.clamp(-30.0, 30.0)
        } else {
            log::warn!(
                "SetEqPreset: non-finite preamp {}; using unity",
                preset.preamp_db
            );
            0.0
        };
        self.config.eq.mode = EqMode::Parametric;
        self.config.eq.enabled = true;
        self.graphic_eq.set_enabled(false);
        self.config.graphic_eq.enabled = false;
        self.sync_parametric_eq();
        info!(
            "EQ preset '{}' applied ({} bands, preamp {:.1} dB)",
            preset.name,
            self.config.eq.bands.len(),
            self.config.eq.preamp_db
        );
    }

    /// Adopt the shared master state when an old host switches to graphic
    /// through a per-layer command instead of `SetEqMode`.
    fn enter_graphic_mode_for_legacy_command(&mut self) {
        if self.config.eq.mode == EqMode::Graphic {
            return;
        }
        self.config.eq.mode = EqMode::Graphic;
        self.graphic_eq.set_enabled(self.config.eq.enabled);
        self.graphic_eq.set_preamp_db(self.config.eq.preamp_db);
    }

    pub(super) fn handle_set_graphic_eq_layout(&mut self, layout: config::GraphicEqLayout) {
        // Legacy per-layer commands imply the graphic source. Switching mode
        // here keeps an older host's layout/slider calls audible instead of
        // silently writing a curve the parametric mode will never play.
        self.enter_graphic_mode_for_legacy_command();
        self.graphic_eq.set_layout(layout);
        self.sync_graphic_eq();
        info!(
            "Graphic EQ: layout {:?} selected ({} bands)",
            self.graphic_eq.layout(),
            self.graphic_eq.num_bands()
        );
    }

    pub(super) fn handle_set_graphic_eq_slider(&mut self, band: usize, gain_db: f32) {
        self.enter_graphic_mode_for_legacy_command();
        self.graphic_eq.set_slider(band, gain_db);
        self.sync_graphic_eq();
    }

    pub(super) fn handle_set_graphic_eq_preamp(&mut self, db: f32) {
        self.enter_graphic_mode_for_legacy_command();
        self.graphic_eq.set_preamp_db(db);
        self.config.eq.preamp_db = self.graphic_eq.preamp_db();
        self.sync_graphic_eq();
    }

    /// Install the whole graphic-EQ state atomically.
    pub(super) fn handle_set_graphic_eq_state(
        &mut self,
        layout: config::GraphicEqLayout,
        gains_db: Vec<f32>,
        preamp_db: f32,
        enabled: bool,
        auto_headroom: bool,
    ) {
        let graphic_mode = self.config.eq.mode == EqMode::Graphic;
        self.graphic_eq
            .set_state(layout, &gains_db, preamp_db, enabled && graphic_mode);
        self.config.eq.enabled = enabled;
        self.config.eq.preamp_db = self.graphic_eq.preamp_db();
        self.config.eq.auto_headroom = auto_headroom;
        self.graph.set_eq_auto_headroom(auto_headroom);
        // Mirror the model into config in both modes; only Graphic compiles it
        // into the live stage. In Parametric the layer flag stays false, which
        // is what lets a reloaded config distinguish an inactive layer from a
        // deliberately selected parametric curve.
        self.sync_graphic_eq();
        if !graphic_mode {
            // The snapshot's enabled/preamp/auto-headroom fields are the
            // shared master state even while Parametric owns the curve. Apply
            // all three so a host cannot write an atomic state whose enable
            // bit disagrees with the live stage.
            self.graph.set_eq_enabled(enabled);
            self.graph.set_preamp_db(self.config.eq.preamp_db);
        }
    }

    pub(super) fn handle_set_graphic_eq_enabled(&mut self, enabled: bool) {
        let switched = enabled && self.config.eq.mode != EqMode::Graphic;
        if enabled {
            self.config.eq.mode = EqMode::Graphic;
        }
        self.graphic_eq.set_enabled(enabled);
        if switched {
            // The shared preamp belongs to the master, not to whichever curve
            // happened to be active last.
            self.graphic_eq.set_preamp_db(self.config.eq.preamp_db);
        }
        if self.config.eq.mode == EqMode::Graphic {
            self.config.eq.enabled = enabled;
            self.sync_graphic_eq();
        }
        info!(
            "Graphic EQ {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub(super) fn handle_set_bass_shelf(&mut self, gain_db: f32) {
        self.graph.set_bass_shelf(gain_db);
    }

    pub(super) fn handle_set_treble_shelf(&mut self, gain_db: f32) {
        self.graph.set_treble_shelf(gain_db);
    }

    pub(super) fn handle_set_preamp(&mut self, db: f32) {
        if !db.is_finite() {
            log::warn!("SetPreamp: non-finite value {db}; ignoring");
            return;
        }
        // Keep the config mirror identical to what the DSP can represent. A
        // host sending +48 dB must not leave a value that reloads as +48 and
        // silently becomes +30 only after the next sync.
        let db = db.clamp(-30.0, 30.0);
        self.config.eq.preamp_db = db;
        match self.config.eq.mode {
            EqMode::Graphic => {
                self.graphic_eq.set_preamp_db(db);
                self.config.eq.preamp_db = self.graphic_eq.preamp_db();
                self.sync_graphic_eq();
            }
            EqMode::Parametric => self.graph.set_preamp_db(db),
        }
    }

    pub(super) fn handle_set_midside_eq(&mut self, enabled: bool) {
        let was_enabled = self.graph.is_midside_eq();
        if was_enabled != enabled {
            self.graph.set_midside_eq(enabled);
            self.graph.with_graph(|g| g.eq_mut().eq.reset());
        }
    }
}

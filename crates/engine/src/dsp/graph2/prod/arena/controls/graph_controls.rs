use super::*;

impl DspGraph {
    pub fn set_volume(&self, volume: f32) {
        self.control_handle().set_volume(volume);
    }

    /// Convert dB ([-60.0, 0.0]) to a linear scalar ([0.0, 1.0]).
    #[inline]
    pub fn volume_db_to_linear(db: f32) -> f32 {
        if !db.is_finite() || db <= -60.0 {
            0.0
        } else {
            10.0_f32.powf(db.clamp(-60.0, 0.0) / 20.0).clamp(0.0, 1.0)
        }
    }

    /// Convert linear scalar ([0.0, 1.0]) to dB ([-60.0, 0.0]).
    #[inline]
    pub fn volume_linear_to_db(linear: f32) -> f32 {
        if !linear.is_finite() || linear <= 1e-3 {
            -60.0
        } else {
            (20.0 * linear.clamp(0.0, 1.0).log10()).clamp(-60.0, 0.0)
        }
    }

    pub fn set_volume_db(&self, db: f32) {
        self.control_handle().set_volume_db(db);
    }

    /// Current volume as dB. Reads the active generation (post-drain truth).
    pub fn volume_db(&self) -> f32 {
        Self::volume_linear_to_db(self.volume().processor.current_gain())
    }

    pub fn set_balance(&self, balance: f32) {
        self.control_handle().set_balance(balance);
    }

    pub fn begin_seek_fadeout(&self) {
        self.control_handle().begin_seek_fadeout();
    }

    pub fn begin_seek_fadein(&self) {
        self.control_handle().begin_seek_fadein();
    }

    pub fn is_seek_fadeout_complete(&self) -> bool {
        self.seek_fade().fade.is_faded_out()
    }

    pub fn apply_loudness_metadata_outgoing(&self, metadata: Option<LoudnessMetadata>) {
        self.control_handle()
            .apply_loudness_metadata_outgoing(metadata);
    }

    pub fn apply_loudness_metadata_incoming(&self, metadata: Option<LoudnessMetadata>) {
        self.control_handle()
            .apply_loudness_metadata_incoming(metadata);
    }

    pub fn set_limiter_enabled(&self, enabled: bool) {
        self.control_handle().set_limiter_enabled(enabled);
    }

    pub fn set_limiter_mode(&self, mode: LimiterMode) {
        self.control_handle().set_limiter_mode(mode);
    }

    pub fn set_limiter_params(
        &self,
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    ) {
        self.control_handle().set_limiter_params(
            lookahead_ms,
            attack_ms,
            release_ms,
            ceiling_db,
            soft_clip,
        );
    }

    pub fn set_limiter_true_peak(&self, enabled: bool) {
        self.control_handle().set_limiter_true_peak(enabled);
    }

    pub fn limiter_true_peak_enabled(&self) -> bool {
        self.limiter().limiter.true_peak_enabled()
    }

    pub fn limiter_gain_reduction_db(&self) -> f32 {
        self.limiter().limiter.gain_reduction_db()
    }

    pub fn limiter_max_true_peak_dbtp(&self) -> f32 {
        self.limiter().limiter.max_true_peak_dbtp()
    }

    pub fn set_preamp_db(&self, db: f32) {
        self.control_handle().set_preamp_db(db);
    }

    pub fn set_bass_shelf(&self, gain_db: f32) {
        self.control_handle().set_bass_shelf(gain_db);
    }

    pub fn set_treble_shelf(&self, gain_db: f32) {
        self.control_handle().set_treble_shelf(gain_db);
    }

    pub fn set_eq_enabled(&self, enabled: bool) {
        self.control_handle().set_eq_enabled(enabled);
    }

    pub fn set_eq_auto_headroom(&self, enabled: bool) {
        self.control_handle().set_eq_auto_headroom(enabled);
    }

    pub fn set_eq_band(&self, index: usize, params: EqBandParams) {
        self.control_handle().set_eq_band(index, params);
    }

    pub fn eq_num_bands(&self) -> usize {
        self.eq().eq.num_bands()
    }

    pub fn set_midside_eq(&self, enabled: bool) {
        self.control_handle().set_midside_eq(enabled);
    }

    pub fn is_midside_eq(&self) -> bool {
        self.eq().midside_enabled
    }

    /// Enable or disable the dynamic-EQ corrective layer.
    pub fn set_dynamic_eq_enabled(&self, enabled: bool) {
        self.control_handle().set_dynamic_eq_enabled(enabled);
    }

    /// Whether the dynamic-EQ layer is currently processing.
    pub fn is_dynamic_eq_enabled(&self) -> bool {
        self.eq().is_dynamic_enabled()
    }

    /// Set one dynamic-EQ band's full parameter set.
    pub fn set_dynamic_eq_band(
        &self,
        index: usize,
        params: crate::dsp::equalizer::DynamicEqBandParams,
    ) {
        self.control_handle().set_dynamic_eq_band(index, params);
    }

    /// Number of dynamic-EQ bands currently configured (0 when the layer is
    /// structurally absent, which is what makes "enabled" meaningless).
    pub fn dynamic_eq_band_count(&self) -> usize {
        self.eq().dynamic_band_count
    }

    /// One dynamic-EQ band's live parameters, for settings read-back.
    pub fn dynamic_eq_band(
        &self,
        index: usize,
    ) -> Option<crate::dsp::equalizer::DynamicEqBandParams> {
        self.eq().dynamic_band(index).copied()
    }

    pub fn set_convolution_wet_mix(&self, mix: f32) {
        self.control_handle().set_convolution_wet_mix(mix);
    }

    pub fn set_stereo_width(&self, width: f32) {
        self.control_handle().set_stereo_width(width);
    }

    pub fn set_stereo_enhancer_enabled(&self, enabled: bool) {
        self.control_handle().set_stereo_enhancer_enabled(enabled);
    }

    pub fn set_crossfeed_enabled(&self, enabled: bool) {
        self.control_handle().set_crossfeed_enabled(enabled);
    }

    pub fn set_crossfeed_profile(&self, profile: config::CrossfeedProfile) {
        self.control_handle().set_crossfeed_profile(profile);
    }

    pub fn set_crossfeed_custom_params(&self, frequency_hz: f32, q: f32, delay_ms: f32) {
        self.control_handle()
            .set_crossfeed_custom_params(frequency_hz, q, delay_ms);
    }

    /// Apply the creative-effects insert's settings.
    pub fn set_fx_config(&self, config: config::FxConfig) {
        self.control_handle().set_fx_config(config);
    }

    pub fn set_compressor_enabled(&self, enabled: bool) {
        self.control_handle().set_compressor_enabled(enabled);
    }

    pub fn set_compressor_band_params(
        &self,
        band: usize,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        makeup_gain_db: f32,
    ) {
        self.control_handle().set_compressor_band_params(
            band,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
        );
    }

    pub fn set_compressor_band_features(
        &self,
        band: usize,
        knee_db: f32,
        detector: config::CompressorDetector,
        stereo_link: bool,
    ) {
        self.control_handle()
            .set_compressor_band_features(band, knee_db, detector, stereo_link);
    }

    pub fn set_loudness_mode(&self, mode: LoudnessMode) {
        self.control_handle().set_loudness_mode(mode);
    }

    // ── Mix bus: per-input control + transitions ─────────────

    pub fn set_input_gain(&self, input: u8, gain: f32) {
        self.control_handle().set_input_gain(input, gain);
    }

    pub fn set_input_gain_db(&self, input: u8, db: f32) {
        self.control_handle().set_input_gain_db(input, db);
    }

    pub fn set_input_balance(&self, input: u8, balance: f32) {
        self.control_handle().set_input_balance(input, balance);
    }

    pub fn set_input_pan(&self, input: u8, pan: f32) {
        self.control_handle().set_input_pan(input, pan);
    }

    pub fn set_input_pan_law(&self, input: u8, law: PanLaw) {
        self.control_handle().set_input_pan_law(input, law);
    }

    /// Set one channel's trim: gain in dB + polarity.
    pub fn set_slot_trim(&self, input: u8, channel: usize, gain_db: f32, invert: bool) {
        self.control_handle()
            .set_slot_trim(input, channel, gain_db, invert);
    }

    /// Set the slot's send levels: master-send + aux tap.
    pub fn set_slot_send(&self, input: u8, master_gain: f32, aux_gain: f32) {
        self.control_handle()
            .set_slot_send(input, master_gain, aux_gain);
    }

    /// Configure the aux bus: enabled + return gain.
    pub fn set_aux(&self, enabled: bool, return_gain: f32) {
        self.control_handle().set_aux(enabled, return_gain);
    }

    /// Runtime toggle of the Aux insert (enabled / wet only).
    pub fn set_aux_insert(&self, enabled: bool, wet_mix: f32) {
        self.control_handle().set_aux_insert(enabled, wet_mix);
    }

    // ── Correction ──────────────────────────────────────────

    /// Live toggle of the correction stage (enabled only; the loaded IR
    /// stays). Disabled = the plan step is skipped, bit-exact.
    pub fn set_correction_enabled(&self, enabled: bool) {
        self.control_handle().set_correction_enabled(enabled);
    }

    /// Live wet/dry depth in [0, 1] (1.0 = fully corrected).
    pub fn set_correction_depth(&self, depth: f32) {
        self.control_handle().set_correction_depth(depth);
    }

    // ── Spatial master ────────────────────────────────────────

    /// Live toggle of the spatial master output stage.
    pub fn set_spatial_enabled(&self, enabled: bool) {
        self.control_handle().set_spatial_enabled(enabled);
    }

    /// Set the virtual screen (see [`GraphControlHandle::set_spatial_screen`]).
    pub fn set_spatial_screen(
        &self,
        center_azimuth_deg: f32,
        half_width_deg: f32,
        elevation_deg: f32,
        gain: f32,
    ) {
        self.control_handle().set_spatial_screen(
            center_azimuth_deg,
            half_width_deg,
            elevation_deg,
            gain,
        );
    }

    /// Configure the room (see [`GraphControlHandle::set_spatial_room`]).
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
        self.control_handle().set_spatial_room(
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

    /// Configure the scene-wide air-absorption model (see
    /// [`GraphControlHandle::set_spatial_air`]).
    pub fn set_spatial_air(&self, air: crate::spatial::level::AirAbsorption) {
        self.control_handle().set_spatial_air(air);
    }

    /// Set the listener orientation (see
    /// [`GraphControlHandle::set_spatial_listener`]).
    pub fn set_spatial_listener(&self, yaw_deg: f32, pitch_deg: f32, roll_deg: f32) {
        self.control_handle()
            .set_spatial_listener(yaw_deg, pitch_deg, roll_deg);
    }

    /// Listener motion: set the target listener pose (see
    /// [`GraphControlHandle::set_spatial_listener_pose`]).
    pub fn set_spatial_listener_pose(
        &self,
        orientation: crate::spatial::math::Quat,
        position: crate::spatial::math::Vec3,
    ) {
        self.control_handle()
            .set_spatial_listener_pose(orientation, position);
    }

    /// Listener motion: set the listener glide's smoothing policy
    /// (see [`GraphControlHandle::set_spatial_listener_tracking`]).
    pub fn set_spatial_listener_tracking(&self, smoothing_ms: f32, max_rate_deg_s: f32) {
        self.control_handle()
            .set_spatial_listener_tracking(smoothing_ms, max_rate_deg_s);
    }

    /// Scene animation (v4.4.0): fire the named cue on the
    /// spatial master at the block boundary — resolves the name against
    /// the active bank and enqueues the index. Returns `false` when the
    /// bank holds no cue of that name (the trigger then never reaches
    /// the queue).
    pub fn trigger_spatial_cue(&self, name: &str) -> bool {
        let Some(index) = self.spatial().cue_index_of(name) else {
            return false;
        };
        self.control_handle().trigger_spatial_cue(index);
        true
    }

    /// Scene animation: stop the active cue on `target` (see
    /// [`GraphControlHandle::stop_spatial_cue`]).
    pub fn stop_spatial_cue(&self, target: usize) {
        self.control_handle().stop_spatial_cue(target);
    }

    /// Scene animation: stop every active cue (see
    /// [`GraphControlHandle::stop_all_spatial_cues`]).
    pub fn stop_all_spatial_cues(&self) {
        self.control_handle().stop_all_spatial_cues();
    }

    /// Scene animation: replace the spatial master's cue bank
    /// from the scene-file model (direct-`DspGraph` control path — the
    /// mutation twin of the queued trigger surface). The bank is also
    /// mirrored onto the sticky user state so a live swap survives a
    /// generation rebuild.
    pub fn set_cue_bank(&mut self, cues: &[config::SpatialCueConfig]) {
        let arc = Arc::new(cues.to_vec());
        self.bus.set_cue_bank_user_state(Some(arc.clone()));
        self.spatial_mut().set_cues(&arc);
    }

    /// Scene animation: fire cue `cue_index` (already resolved
    /// by name) immediately (direct-`DspGraph` hosts/tests). Mirrors the
    /// active-cue state onto the sticky user state.
    pub fn trigger_spatial_cue_by_index(&mut self, cue_index: usize) {
        let n = self.spatial_mut().trigger_cue(cue_index);
        if n {
            for target in 0..crate::spatial::cue::MAX_ACTIVE_CUES {
                let idx = self.spatial().cue_active_index(target);
                self.bus.set_active_cue_user_state(target, idx);
            }
        }
    }

    /// Scene animation: stop every active cue immediately
    /// (direct-`DspGraph` hosts/tests). Mirrors the cleared state.
    pub fn stop_all_cue_bank(&mut self) {
        self.spatial_mut().stop_all_cues();
        for target in 0..crate::spatial::cue::MAX_ACTIVE_CUES {
            self.bus.set_active_cue_user_state(target, None);
        }
    }

    /// The spatial master's enable flag (mirrored at drain).
    pub fn spatial_enabled(&self) -> bool {
        self.control_handle().spatial_enabled()
    }

    /// Set the active HRTF profile on the spatial node.
    pub fn set_hrtf_profile(&mut self, profile_id: &str) -> bool {
        self.spatial_mut().set_hrtf_profile(profile_id)
    }

    /// Register a custom HRTF profile on the spatial node.
    pub fn register_hrtf_profile(&mut self, profile: crate::spatial::hrtf::HrtfProfile) {
        self.spatial_mut().register_hrtf_profile(profile);
    }

    /// Register a custom HRTF dataset on the spatial node.
    pub fn register_hrtf_dataset(
        &mut self,
        key: impl Into<String>,
        dataset: Arc<crate::spatial::hrtf::HrtfDataset>,
    ) {
        self.spatial_mut().register_hrtf_dataset(key, dataset);
    }

    // ── Plugin host ────────────────────────────────────────

    /// Live toggle of the plugin host insert (all slots). Disabled = the
    /// plan step is skipped, bit-exact.
    pub fn set_plugin_enabled(&self, enabled: bool) {
        self.control_handle().set_plugin_enabled(enabled);
    }

    /// Live plugin parameter batch: plain data over the per-node SPSC
    /// queue, applied at the next block boundary.
    pub fn set_plugin_params(&self, batch: plugin_abi::PluginParams) {
        self.control_handle().set_plugin_params(batch);
    }

    /// The plugin host's enable flag (mirrored at drain).
    pub fn plugin_enabled(&self) -> bool {
        self.control_handle().plugin_enabled()
    }

    /// Live toggle of the CLAP plugin host.
    pub fn set_clap_plugin_enabled(&self, enabled: bool) {
        self.control_handle().set_clap_plugin_enabled(enabled);
    }

    /// Live CLAP plugin parameter change: plain data over the per-node SPSC
    /// queue, applied at the next block boundary.
    pub fn set_clap_plugin_param(&self, id: u32, value: f64) {
        self.control_handle().set_clap_plugin_param(id, value);
    }

    /// Load a rendered correction IR set into the ACTIVE node and mirror it
    /// onto the sticky bus state so a later generation rebuild replays it.
    /// Single-threaded control path (the engine's tick thread); cross-thread
    /// hosts must publish a fresh generation instead (which inherits the
    /// sticky set through `build_with_state`).
    pub fn load_correction_ir(&mut self, set: Arc<CorrectionIrSet>) {
        let rate = self.sample_rate;
        let _ = self.correction_mut().load_set(&set, rate);
        self.control_handle().load_correction_ir(set);
    }

    pub fn set_input_mute(&self, input: u8, mute: bool) {
        self.control_handle().set_input_mute(input, mute);
    }

    pub fn set_input_active(&self, input: u8, active: bool) {
        self.control_handle().set_input_active(input, active);
    }

    /// Configure program-gated ducking.
    pub fn set_duck(&self, cfg: Option<DuckState>) {
        self.control_handle().set_duck(cfg);
    }

    /// Replace a slot's automation track starting at `initial_frame`.
    pub fn set_slot_automation_at_frame(
        &self,
        input: u8,
        target: AutomationTarget,
        points: &[AutomationPoint],
        initial_frame: usize,
    ) {
        self.control_handle()
            .set_slot_automation_at_frame(input, target, points, initial_frame);
    }

    /// Replace a slot's automation track.
    pub fn set_slot_automation(
        &self,
        input: u8,
        target: AutomationTarget,
        points: &[AutomationPoint],
    ) {
        self.control_handle()
            .set_slot_automation(input, target, points);
    }

    /// Remove a slot's automation track.
    pub fn clear_slot_automation(&self, input: u8) {
        self.control_handle().clear_slot_automation(input);
    }

    /// Begin a crossfade from the outgoing to the incoming bus input over
    /// `duration_ms`, converting to frames at the graph's sample rate.
    pub fn begin_crossfade(&self, duration_ms: u64) {
        let frames = (duration_ms as f32 * 0.001 * self.sample_rate) as usize;
        self.control_handle().begin_crossfade_frames(frames);
    }

    /// Begin a sequential fade over `duration_ms` (same frame conversion).
    pub fn begin_fade(&self, duration_ms: u64) {
        let frames = (duration_ms as f32 * 0.001 * self.sample_rate) as usize;
        self.control_handle().begin_fade_frames(frames);
    }

    /// Return the bus to single-stream playback (input 0 at unity).
    pub fn begin_playing(&self) {
        self.control_handle().begin_playing();
    }

    pub fn set_crossfade_curve(&self, curve: config::CrossfadeCurve) {
        self.control_handle().set_crossfade_curve(curve);
    }

    pub fn set_crossfade_enabled(&self, enabled: bool) {
        self.control_handle().set_crossfade_enabled(enabled);
    }

    pub fn set_crossfade_duration_ms(&self, duration_ms: u64) {
        let frames = (duration_ms as f32 * 0.001 * self.sample_rate) as usize;
        self.control_handle().set_crossfade_duration_frames(frames);
    }

    /// Current transition state of the bus (introspection).
    pub fn mixer_state(&self) -> MixerState {
        self.mix().state
    }
}

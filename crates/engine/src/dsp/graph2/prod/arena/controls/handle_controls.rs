use super::*;

impl GraphControlHandle {
    // ── Control surface (enqueue) ─────────────────────────────────────────

    pub fn set_volume(&self, volume: f32) {
        self.enqueue(
            node_id::VOLUME,
            NodeCmd::SetVolumeTarget(volume.clamp(0.0, 1.0)),
        );
    }

    pub fn set_volume_db(&self, db: f32) {
        if !db.is_finite() {
            log::warn!("DspGraph::set_volume_db: non-finite value {}; ignoring", db);
            return;
        }
        let linear = DspGraph::volume_db_to_linear(db);
        self.enqueue(node_id::VOLUME, NodeCmd::SetVolumeTarget(linear));
    }

    pub fn set_balance(&self, balance: f32) {
        self.enqueue(node_id::BALANCE, NodeCmd::SetBalance(balance));
    }

    pub fn begin_seek_fadeout(&self) {
        self.enqueue(node_id::SEEK_FADE, NodeCmd::BeginSeekFadeout);
    }

    pub fn begin_seek_fadein(&self) {
        self.enqueue(node_id::SEEK_FADE, NodeCmd::BeginSeekFadein);
    }

    pub fn apply_loudness_metadata_outgoing(&self, metadata: Option<LoudnessMetadata>) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input: 0,
                cmd: MixInputCmd::ApplyLoudnessMetadata(metadata.unwrap_or_default()),
            },
        );
    }

    pub fn apply_loudness_metadata_incoming(&self, metadata: Option<LoudnessMetadata>) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input: 1,
                cmd: MixInputCmd::ApplyLoudnessMetadata(metadata.unwrap_or_default()),
            },
        );
    }

    pub fn set_limiter_enabled(&self, enabled: bool) {
        self.enqueue(node_id::LIMITER, NodeCmd::SetLimiterEnabled(enabled));
    }

    pub fn set_limiter_mode(&self, mode: LimiterMode) {
        self.enqueue(node_id::LIMITER, NodeCmd::SetLimiterMode(mode));
    }

    pub fn set_limiter_params(
        &self,
        lookahead_ms: f32,
        attack_ms: f32,
        release_ms: f32,
        ceiling_db: f32,
        soft_clip: bool,
    ) {
        self.enqueue(
            node_id::LIMITER,
            NodeCmd::SetLimiterParams {
                lookahead_ms,
                attack_ms,
                release_ms,
                ceiling_db,
                soft_clip,
            },
        );
    }

    pub fn set_limiter_true_peak(&self, enabled: bool) {
        self.enqueue(node_id::LIMITER, NodeCmd::SetLimiterTruePeak(enabled));
    }

    pub fn set_preamp_db(&self, db: f32) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqPreampDb(db));
    }

    pub fn set_bass_shelf(&self, gain_db: f32) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqBassShelf(gain_db));
    }

    pub fn set_treble_shelf(&self, gain_db: f32) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqTrebleShelf(gain_db));
    }

    pub fn set_eq_enabled(&self, enabled: bool) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqEnabled(enabled));
    }

    pub fn set_eq_auto_headroom(&self, enabled: bool) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqAutoHeadroom(enabled));
    }

    pub fn set_eq_band(&self, index: usize, params: EqBandParams) {
        self.enqueue(node_id::EQ, NodeCmd::SetEqBand { index, params });
    }

    pub fn set_midside_eq(&self, enabled: bool) {
        self.enqueue(node_id::EQ, NodeCmd::SetMidsideEq(enabled));
    }

    /// Enable or disable the dynamic-EQ corrective layer.
    pub fn set_dynamic_eq_enabled(&self, enabled: bool) {
        self.enqueue(node_id::EQ, NodeCmd::SetDynamicEqEnabled(enabled));
    }

    /// Set one dynamic-EQ band's full parameter set.
    pub fn set_dynamic_eq_band(
        &self,
        index: usize,
        params: crate::dsp::equalizer::DynamicEqBandParams,
    ) {
        self.enqueue(node_id::EQ, NodeCmd::SetDynamicEqBand { index, params });
    }

    pub fn set_convolution_wet_mix(&self, mix: f32) {
        self.enqueue(node_id::CONVOLUTION, NodeCmd::SetConvolutionWetMix(mix));
    }

    pub fn set_stereo_width(&self, width: f32) {
        self.enqueue(node_id::STEREO, NodeCmd::SetStereoWidth(width));
    }

    pub fn set_stereo_enhancer_enabled(&self, enabled: bool) {
        self.enqueue(node_id::STEREO, NodeCmd::SetStereoEnhancerEnabled(enabled));
    }

    pub fn set_crossfeed_enabled(&self, enabled: bool) {
        self.enqueue(node_id::CROSSFEED, NodeCmd::SetCrossfeedEnabled(enabled));
    }

    pub fn set_crossfeed_profile(&self, profile: config::CrossfeedProfile) {
        self.enqueue(node_id::CROSSFEED, NodeCmd::SetCrossfeedProfile(profile));
    }

    pub fn set_crossfeed_custom_params(&self, frequency_hz: f32, q: f32, delay_ms: f32) {
        self.enqueue(
            node_id::CROSSFEED,
            NodeCmd::SetCrossfeedCustom(frequency_hz, q, delay_ms),
        );
    }

    /// Apply the creative-effects insert's settings.
    pub fn set_fx_config(&self, config: config::FxConfig) {
        // The bus keeps a copy so a generation rebuild can inherit it: the
        // fx settings are user state, like the volume.
        {
            let mut guard = self
                .bus
                .user_fx_config
                .lock()
                .expect("fx config mutex poisoned");
            *guard = config;
        }
        self.enqueue(node_id::FX, NodeCmd::SetFxConfig(config));
    }

    pub fn set_compressor_enabled(&self, enabled: bool) {
        self.enqueue(node_id::DYNAMICS, NodeCmd::SetCompressorEnabled(enabled));
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
        self.enqueue(
            node_id::DYNAMICS,
            NodeCmd::SetCompressorBand {
                band,
                threshold_db,
                ratio,
                attack_ms,
                release_ms,
                makeup_gain_db,
            },
        );
    }

    pub fn set_compressor_band_features(
        &self,
        band: usize,
        knee_db: f32,
        detector: config::CompressorDetector,
        stereo_link: bool,
    ) {
        self.enqueue(
            node_id::DYNAMICS,
            NodeCmd::SetCompressorBandFeatures {
                band,
                knee_db,
                detector,
                stereo_link,
            },
        );
    }

    pub fn set_loudness_mode(&self, mode: LoudnessMode) {
        let cmd = MixInputCmd::SetLoudnessMode(mode);
        self.enqueue(node_id::MIX, NodeCmd::MixInput { input: 0, cmd });
        self.enqueue(node_id::MIX, NodeCmd::MixInput { input: 1, cmd });
    }

    // ── Mix bus: per-input control ───────────────────────────

    pub fn set_input_gain(&self, input: u8, gain: f32) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetGain(gain),
            },
        );
    }

    pub fn set_input_gain_db(&self, input: u8, db: f32) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetGainDb(db),
            },
        );
    }

    pub fn set_input_balance(&self, input: u8, balance: f32) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetBalance(balance),
            },
        );
    }

    /// Set the per-input pan in [-1, 1]. Shapes the front L/R
    /// pair through the slot's pan law; channels >= 2 pass at unity.
    pub fn set_input_pan(&self, input: u8, pan: f32) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetPan(pan),
            },
        );
    }

    /// Set the per-input pan law.
    pub fn set_input_pan_law(&self, input: u8, law: PanLaw) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetPanLaw(law),
            },
        );
    }

    /// Set one channel's trim: gain in dB + polarity inversion.
    /// Applied on the slot's own planes between its pre-mix chains and the
    /// sum; all-unity = inactive = bit-exact.
    pub fn set_slot_trim(&self, input: u8, channel: usize, gain_db: f32, invert: bool) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetSlotTrim {
                    channel,
                    gain_db,
                    invert,
                },
            },
        );
    }

    /// Set the slot's send levels: post-fader master-send and
    /// aux-send gains in [0, 1]. The master-send scales the slot's
    /// contribution to the master sum; the aux-send taps the post-fader
    /// signal into the aux accumulator.
    pub fn set_slot_send(&self, input: u8, master_gain: f32, aux_gain: f32) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetSend {
                    master_gain,
                    aux_gain,
                },
            },
        );
    }

    /// Configure the aux bus: `enabled` routes the aux
    /// return into the master before the post-mix chain; `return_gain` in
    /// [0, 1] scales the return. Disabled = bit-exact.: applied to
    /// the aux bus node, whose send taps the mix node gates on the shared
    /// bus's `enabled`.
    pub fn set_aux(&self, enabled: bool, return_gain: f32) {
        self.enqueue(
            node_id::AUX,
            NodeCmd::SetAux {
                enabled,
                return_gain,
            },
        );
    }

    /// Runtime toggle of the Aux insert (global convolution):
    /// `enabled` + `wet_mix` only; the IR stays as configured. No-op when
    /// no IR engine exists yet.
    pub fn set_aux_insert(&self, enabled: bool, wet_mix: f32) {
        self.enqueue(node_id::AUX, NodeCmd::SetAuxInsert { enabled, wet_mix });
    }

    /// Control-side read of the mirrored aux state (enabled, return gain).
    pub fn aux_state(&self) -> (bool, f32) {
        self.bus.user_aux()
    }

    /// Control-side read of the mirrored Aux insert state
    /// (enabled, wet mix).
    pub fn aux_insert_state(&self) -> (bool, f32) {
        self.bus.user_aux_insert()
    }

    // ── Correction ─────────────────────────────────────────

    /// Live toggle of the correction stage (enabled only; the loaded IR
    /// stays). Disabled = the plan step is skipped, bit-exact.
    pub fn set_correction_enabled(&self, enabled: bool) {
        self.enqueue(node_id::CORRECTION, NodeCmd::SetCorrectionEnabled(enabled));
    }

    /// Live wet/dry depth in [0, 1] (1.0 = fully corrected).
    pub fn set_correction_depth(&self, depth: f32) {
        self.enqueue(node_id::CORRECTION, NodeCmd::SetCorrectionDepth(depth));
    }

    /// Store a rendered correction IR set (control side) so a generation
    /// swap replays it into the new node. The set is immutable after load;
    /// the audio thread never reads it.
    pub fn load_correction_ir(&self, set: Arc<CorrectionIrSet>) {
        self.bus.set_correction_ir(set);
    }

    /// Control-side read of the mirrored correction toggle / depth.
    pub fn correction_state(&self) -> (bool, f32) {
        self.bus.user_correction()
    }

    // ── Spatial master ────────────────────────────────────────

    /// Live toggle of the spatial master output stage. Disabled = the plan
    /// step is skipped, bit-exact.
    pub fn set_spatial_enabled(&self, enabled: bool) {
        self.enqueue(node_id::SPATIAL, NodeCmd::SetSpatialEnabled(enabled));
    }

    /// Live toggle of the plugin host insert. Disabled = the
    /// plan step is skipped, bit-exact (attached instances stay).
    pub fn set_plugin_enabled(&self, enabled: bool) {
        self.enqueue(node_id::PLUGIN, NodeCmd::SetPluginEnabled(enabled));
    }

    /// Live plugin parameter batch: plain data over the SPSC
    /// queue, applied at the next block boundary.
    pub fn set_plugin_params(&self, batch: plugin_abi::PluginParams) {
        self.enqueue(node_id::PLUGIN, NodeCmd::SetPluginParams(batch));
    }

    /// Live toggle of the CLAP plugin host.
    pub fn set_clap_plugin_enabled(&self, enabled: bool) {
        self.enqueue(node_id::PLUGIN, NodeCmd::SetClapPluginEnabled(enabled));
    }

    /// Live CLAP plugin parameter change: plain data over the SPSC
    /// queue, applied at the next block boundary.
    pub fn set_clap_plugin_param(&self, id: u32, value: f64) {
        self.enqueue(node_id::PLUGIN, NodeCmd::SetClapPluginParam { id, value });
    }

    /// Set the virtual screen: `center_azimuth_deg` (screen center),
    /// `half_width_deg` (L/R spread, clamped to 90°), `elevation_deg`
    /// (clamped to ±90°), `gain` (linear, clamped to 4).
    pub fn set_spatial_screen(
        &self,
        center_azimuth_deg: f32,
        half_width_deg: f32,
        elevation_deg: f32,
        gain: f32,
    ) {
        self.enqueue(
            node_id::SPATIAL,
            NodeCmd::SetSpatialScreen {
                center_azimuth_deg,
                half_width_deg,
                elevation_deg,
                gain,
            },
        );
    }

    /// Configure the room: geometry (metres), wall absorption, reflection
    /// order (1 or 2), late-field RT60 (ms) / late mix / distance
    /// roll-off, and the program's reflection send `wet` (all clamped).
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
        self.enqueue(
            node_id::SPATIAL,
            NodeCmd::SetSpatialRoom {
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
            },
        );
    }

    /// Configure the scene-wide air-absorption model applied to live room
    /// Reflections. Mirrors `DspGraph::set_spatial_air`.
    pub fn set_spatial_air(&self, air: crate::spatial::level::AirAbsorption) {
        self.enqueue(node_id::SPATIAL, NodeCmd::SetSpatialAir(air));
    }

    /// Set the listener orientation (yaw / pitch / roll, degrees) applied
    /// to the rendered scene.
    pub fn set_spatial_listener(&self, yaw_deg: f32, pitch_deg: f32, roll_deg: f32) {
        self.enqueue(
            node_id::SPATIAL,
            NodeCmd::SetSpatialListener {
                yaw_deg,
                pitch_deg,
                roll_deg,
            },
        );
    }

    /// Listener motion: set the target listener pose (world-space
    /// orientation quaternion + position). The spatial node glides toward
    /// it every processed block (nlerp + one-pole per its tracking policy),
    /// so a moving listener sweeps the image smoothly instead of snapping.
    pub fn set_spatial_listener_pose(
        &self,
        orientation: crate::spatial::math::Quat,
        position: crate::spatial::math::Vec3,
    ) {
        self.enqueue(
            node_id::SPATIAL,
            NodeCmd::SetSpatialListenerPose {
                qx: orientation.x,
                qy: orientation.y,
                qz: orientation.z,
                qw: orientation.w,
                px: position.x,
                py: position.y,
                pz: position.z,
            },
        );
    }

    /// Listener motion: set the listener glide's smoothing policy
    /// (one-pole time constant ms; `0` snaps; angular rate limit deg/s,
    /// `0` unlimited).
    pub fn set_spatial_listener_tracking(&self, smoothing_ms: f32, max_angular_rate_deg_s: f32) {
        self.enqueue(
            node_id::SPATIAL,
            NodeCmd::SetSpatialListenerTracking {
                smoothing_ms,
                max_angular_rate_deg_s,
            },
        );
    }

    /// Scene animation: fire cue `cue_index` (resolved by name
    /// against the active bank — see [`DspGraph::trigger_spatial_cue`])
    /// at the block boundary. Out-of-range indices no-op on drain.
    pub fn trigger_spatial_cue(&self, cue_index: usize) {
        self.enqueue(node_id::SPATIAL, NodeCmd::TriggerSpatialCue(cue_index));
    }

    /// Scene animation: stop the active cue on `target` (0 = L,
    /// 1 = R) at the block boundary.
    pub fn stop_spatial_cue(&self, target: usize) {
        self.enqueue(node_id::SPATIAL, NodeCmd::StopSpatialCue(target));
    }

    /// Scene animation: stop every active cue at the block
    /// boundary.
    pub fn stop_all_spatial_cues(&self) {
        self.enqueue(node_id::SPATIAL, NodeCmd::StopAllSpatialCues);
    }

    /// Control-side read of the mirrored spatial enable flag.
    pub fn spatial_enabled(&self) -> bool {
        self.bus.user_spatial()
    }

    /// Control-side read of the mirrored plugin host enable flag
    /// .
    pub fn plugin_enabled(&self) -> bool {
        self.bus.user_plugin()
    }

    /// Control-side read of the aux meters (peak_db, rms_db), published once
    /// Per audio block.
    pub fn aux_meters(&self) -> (f32, f32) {
        self.bus.aux_meters()
    }

    /// Control-side read of one mix slot's aux-send peak (dBFS) — the
    /// Independent per-send metering of the aux bus node.
    pub fn aux_send_peak(&self, slot: usize) -> f32 {
        self.bus.aux_send_peak(slot)
    }

    pub fn set_input_mute(&self, input: u8, mute: bool) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetMute(mute),
            },
        );
    }

    /// Detach / re-attach a mix-bus slot (stream slots). A
    /// detached slot contributes nothing and its chains do not advance.
    /// Slot 0 (the primary stream) cannot be detached.
    pub fn set_input_active(&self, input: u8, active: bool) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetActive(active),
            },
        );
    }

    /// Configure program-gated ducking. `None` disables and the
    /// sum returns to bit-exact. `Some` config rides the queue as `Copy` data
    /// and is applied atomically on the audio side.
    pub fn set_duck(&self, cfg: Option<DuckState>) {
        self.enqueue(node_id::MIX, NodeCmd::SetDuck(cfg));
    }

    /// Replace a slot's automation track starting at an initial timeline frame.
    pub fn set_slot_automation_at_frame(
        &self,
        input: u8,
        target: AutomationTarget,
        points: &[AutomationPoint],
        initial_frame: usize,
    ) {
        let mut buf = [AutomationPoint {
            frame: 0,
            value: 0.0,
        }; MAX_AUTOMATION_POINTS];
        let count = points.len().min(MAX_AUTOMATION_POINTS);
        buf[..count].copy_from_slice(&points[..count]);
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::SetAutomation {
                    target,
                    points: buf,
                    count,
                    initial_frame,
                },
            },
        );
    }

    /// Replace a slot's automation track. `points` are clamped
    /// to [`MAX_AUTOMATION_POINTS`]; an empty slice clears the track. The
    /// points must be monotonically non-decreasing in `frame`; values are
    /// linearly interpolated on the audio side.
    pub fn set_slot_automation(
        &self,
        input: u8,
        target: AutomationTarget,
        points: &[AutomationPoint],
    ) {
        self.set_slot_automation_at_frame(input, target, points, 0);
    }

    /// Remove a slot's automation track.
    pub fn clear_slot_automation(&self, input: u8) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixInput {
                input,
                cmd: MixInputCmd::ClearAutomation,
            },
        );
    }

    /// Begin a crossfade from input 0 to input 1 over `duration_frames`.
    /// Gated by the graph's crossfade config (mirrors `TrackMixer`).
    pub fn begin_crossfade_frames(&self, duration_frames: usize) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixTransition(MixTransitionCmd::StartCrossfade { duration_frames }),
        );
    }

    /// Begin a sequential fade (fade-out → gap → fade-in) over
    /// `duration_frames`.
    pub fn begin_fade_frames(&self, duration_frames: usize) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixTransition(MixTransitionCmd::StartFade { duration_frames }),
        );
    }

    /// Return the bus to ordinary single-stream playback (input 0 at unity).
    pub fn begin_playing(&self) {
        self.enqueue(
            node_id::MIX,
            NodeCmd::MixTransition(MixTransitionCmd::StartPlaying),
        );
    }

    /// Set the transition curve at runtime (mirrors
    /// `TrackMixer::set_curve`).
    pub fn set_crossfade_curve(&self, curve: config::CrossfadeCurve) {
        self.enqueue(node_id::MIX, NodeCmd::SetMixCurve(curve));
    }

    /// Enable / disable crossfade transitions (mirrors
    /// `TrackMixer::set_enabled`).
    pub fn set_crossfade_enabled(&self, enabled: bool) {
        self.enqueue(node_id::MIX, NodeCmd::SetMixEnabled(enabled));
    }

    /// Set the transition duration in output frames (mirrors
    /// `TrackMixer::set_duration_ms` converted at the graph's sample rate).
    pub fn set_crossfade_duration_frames(&self, frames: usize) {
        self.enqueue(node_id::MIX, NodeCmd::SetMixDurationFrames(frames));
    }
}

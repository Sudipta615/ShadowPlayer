use super::*;

impl SpatialNode {
    pub fn new(sample_rate: f32) -> Self {
        let mut scene = SpatialScene::new(sample_rate.max(1.0) as u32);
        let obj_l = scene
            .create_audio_object(Vec3::new(-1.0, SCREEN_RADIUS, 0.0))
            .expect("invariant: a fresh scene accepts its first program object");
        let obj_r = scene
            .create_audio_object(Vec3::new(1.0, SCREEN_RADIUS, 0.0))
            .expect("invariant: a fresh scene accepts its second program object");
        for id in [obj_l, obj_r] {
            let obj = scene
                .object_mut(id)
                .expect("invariant: program objects created above exist");
            // The program is the already-mixed master: no distance
            // attenuation, unity gain (screen gain is applied separately).
            obj.distance_model = DistanceModel::Linear;
            obj.gain = 1.0;
        }

        let mut hrtf_manager = HrtfProfileManager::new();
        let sphere_profile = HrtfProfile::spherical_head_model(sample_rate.max(1.0) as u32);
        hrtf_manager.register_profile(sphere_profile);

        let mut node = Self {
            enabled: false,
            scene,
            cue_clock: 0.0,
            last_cost_units: 0.0,
            last_cost_utilization: 0.0,
            tail_budget_blocks: f32::INFINITY,
            binaural: BinauralRenderer::new(10.0),
            mc_renderer: None,
            mc_speakers: None,
            mc_layout: None,
            mc_bed: None,
            mc_prepared: false,
            mc_planes: Vec::new(),
            prog_l: vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
            prog_r: vec![0.0; MAX_AUDIO_BLOCK_FRAMES],
            out: vec![0.0; MAX_CHANNELS * MAX_AUDIO_BLOCK_FRAMES],
            center_azimuth_deg: 0.0,
            half_width_deg: 30.0,
            elevation_deg: 0.0,
            screen_gain: 1.0,
            listener_yaw_deg: 0.0,
            listener_pitch_deg: 0.0,
            listener_roll_deg: 0.0,
            obj_l,
            obj_r,
            sample_rate: sample_rate.max(1.0),
            voice: None,
            voice_active: false,
            voice_full: 0,
            voice_degraded: 0,
            voice_dropped: 0,
            listener_tracking: TrackingConfig::default(),
            listener_target_quat: Quat::IDENTITY,
            listener_target_pos: Vec3::ZERO,
            listener_motion_active: false,
            hrtf_manager,
            prepared_rate: -1.0,
            prepared: false,
        };
        // The graph only calls `DspNode::prepare` on rate changes, so the
        // node prepares its renderer eagerly at construction (control path).
        node.prepare(sample_rate.max(1.0), 2);
        node
    }

    /// Tell the node what channel layout the master will carry.
    ///
    /// This is the piece that was missing for multichannel support, and it is
    /// *control path* only — the audio thread never consults it, it reads the
    /// pre-resolved `Self::mc_prepared` flag.
    ///
    /// Given a layout, the node derives the matching speaker geometry and
    /// prepares `Self::mc_renderer` against it, so a >2-plane block can be
    /// routed role-to-speaker instead of being declined. Given nothing (or a
    /// layout whose geometry is degenerate), `mc_prepared` stays `false` and
    /// multichannel blocks keep passing through **bit-exact** — an unknown
    /// width is not guessed at.
    ///
    /// Returns `true` when a layout-driven renderer is now active.
    pub fn set_multichannel_layout(&mut self, layout: &ChannelLayout) -> bool {
        let rate = self.sample_rate.max(1.0) as u32;
        self.mc_layout = Some(layout.clone());

        // A layout of two or fewer channels is the *stereo* case, and the
        // stereo path is the binaural head model — which never consults the
        // multichannel renderer. So the renderer is neither built nor kept
        // alive for it.
        //
        // That is a memory decision, not a cosmetic one. A `VbapRenderer` owns
        // early-reflection taps, a room late-field tail, and a diffuse-field
        // decorrelator, and constructing one unconditionally added roughly
        // 1.6 MB to *every* graph build — including the overwhelmingly common
        // stereo one — which pushed the preparation estimate out of the band
        // `prepare::tests` requires an admission check to stay inside.
        if layout.channel_count() <= 2 {
            self.mc_prepared = false;
            self.mc_speakers = None;
            self.mc_bed = None;
            self.mc_renderer = None;
            return false;
        }

        let speakers = SpeakerLayout::custom_from_roles(layout.channel_ids());
        // `validate` rejects an empty or fully-degenerate layout (e.g. LFE
        // only), which is exactly the set of layouts that cannot be panned to.
        if speakers.validate().is_err() {
            self.mc_prepared = false;
            self.mc_speakers = None;
            self.mc_bed = None;
            self.mc_renderer = None;
            return false;
        }

        // Retarget the master bed rather than creating a new one.
        //
        // `SpatialBedStore` is a fixed 16-slot pool and `create_bed` consumes a
        // slot permanently, so allocating a fresh bed on every call would
        // exhaust the store after sixteen reconfigurations and silently drop MC
        // routing — a failure with no symptom until the user changed layout
        // sixteen times. `SpatialBed::set_layout` exists for exactly this.
        match self.mc_bed.and_then(|id| self.scene.bed_mut(id)) {
            Some(bed) => bed.set_layout(layout.clone()),
            None => self.mc_bed = self.scene.create_bed(layout.clone()),
        }

        // Built on first real use and reused across layout changes: a fresh
        // `prepare` re-points it at the new geometry, which is cheaper than
        // rebuilding and keeps the allocation count off the reconfig path.
        if self.mc_renderer.is_none() {
            self.mc_renderer = Some(Box::new(VbapRenderer::new()));
        }
        let ok = self
            .mc_renderer
            .as_mut()
            .expect("just constructed above")
            .prepare(&speakers, rate)
            .is_ok();
        self.mc_speakers = if ok { Some(speakers) } else { None };
        // The f64 multichannel path demotes into a channel-major scratch before
        // rendering. Size it HERE, on the control path, rather than lazily in
        // `render_block_multichannel_f64`: a lazy growth put up to
        // `MAX_CHANNELS * MAX_AUDIO_BLOCK_FRAMES` f32 (256 KiB) of `malloc`
        // on the first multichannel block, on the audio thread, which is the
        // one place this crate forbids it.
        //
        // `self.out` is already allocated to exactly this width at
        // construction, so a multichannel node now costs two such planes and a
        // stereo node still costs neither (both early-returns above run before
        // this line).
        if ok && self.mc_planes.len() < MAX_CHANNELS * MAX_AUDIO_BLOCK_FRAMES {
            self.mc_planes
                .resize(MAX_CHANNELS * MAX_AUDIO_BLOCK_FRAMES, 0.0);
        }
        self.mc_prepared = ok && self.mc_bed.is_some();
        self.mc_prepared
    }

    /// Whether multichannel blocks are routed rather than passed through.
    pub fn multichannel_rendering_active(&self) -> bool {
        self.mc_prepared
    }

    /// The layout the node was last told about, if any.
    pub fn multichannel_layout(&self) -> Option<&ChannelLayout> {
        self.mc_layout.as_ref()
    }

    /// Route a multichannel master through the layout-driven renderer.
    ///
    /// The master's own channels are handed to the renderer as the bed that
    /// [`Self::set_multichannel_layout`] created, so `render_beds` places each
    /// channel on the speaker carrying its semantic role — FL to front left,
    /// LFE to the effects speaker, and so on. That is the transparent round
    /// trip an already-spatial master wants; the scene's program objects, cues,
    /// and voice budget are untouched by this path.
    ///
    /// Guards, in order, each a bit-exact-passthrough case:
    /// - no layout was configured, or its geometry was rejected;
    /// - the block's width disagrees with the configured layout (the node
    ///   refuses to guess which is right);
    /// - the renderer's out-scratch is too small for `channels × frames`.
    ///
    /// Allocation-free: the bed reads the input planes in place and the result
    /// goes back into them through the preallocated interleaved scratch.
    pub(super) fn render_block_multichannel(&mut self, planes: &mut [&mut [f32]], frames: usize) {
        if !self.mc_prepared || frames == 0 || frames > MAX_AUDIO_BLOCK_FRAMES {
            return;
        }
        let channels = planes.len();
        // `need > self.out.len()` does NOT imply this: a 64-channel block of
        // 100 frames fits the out-scratch while still exceeding MAX_CHANNELS.
        // The bed list is a fixed-size stack array, so bound the width here.
        if channels > MAX_CHANNELS {
            return;
        }
        let Some(layout) = self.mc_layout.as_ref() else {
            return;
        };
        if layout.channel_count() != channels {
            return;
        }
        let need = channels * frames;
        if need > self.out.len() {
            return;
        }

        let block_secs = frames as f32 / self.sample_rate;
        self.glide_listener(block_secs);
        self.step_cues(block_secs);
        let snap = self.apply_cues_for_render();
        self.apply_voice_budget();

        // `render_beds` *adds* into `out`, so the scratch must start silent —
        // otherwise the previous block's samples leak into this one.
        self.out[..need].fill(0.0);

        // A fixed-size stack array, not a `collect()`: this runs once per block
        // on the audio thread, and a `Vec` here put one malloc/free pair per
        // block on the hottest path in the spatial renderer.
        let mut bed_storage: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
        {
            let bed_inputs = &mut bed_storage[..channels];
            for (dst, plane) in bed_inputs.iter_mut().zip(planes.iter()) {
                *dst = &plane[..frames];
            }
            let inputs = HybridBlockInputs {
                objects: &[],
                beds: bed_inputs,
                fields: &[],
            };
            let Some(renderer) = self.mc_renderer.as_mut() else {
                return;
            };
            let result =
                renderer.process_hybrid_block(&self.scene, &inputs, frames, &mut self.out[..need]);
            self.restore_program(&snap);
            if result.is_err() {
                return;
            }
        }

        for (ch, plane) in planes.iter_mut().enumerate() {
            for f in 0..frames {
                plane[f] = self.out[f * channels + ch];
            }
        }
    }

    /// f64 twin of [`Self::render_block_multichannel`]: demote, render in
    /// f32, promote back.
    ///
    /// Needs a deinterleaving step the f32 path does not: `render_beds` reads
    /// each bed channel as a *contiguous* `&[f32]`, and the f64 planes are
    /// neither contiguous nor `f32`. So the block is demoted into the
    /// preallocated `mc_planes` scratch (channel-major) before the render and
    /// promoted back after.
    pub(super) fn render_block_multichannel_f64(
        &mut self,
        planes: &mut [&mut [f64]],
        frames: usize,
    ) {
        if !self.mc_prepared || frames == 0 || frames > MAX_AUDIO_BLOCK_FRAMES {
            return;
        }
        let channels = planes.len();
        if channels > MAX_CHANNELS {
            return;
        }
        let Some(layout) = self.mc_layout.as_ref() else {
            return;
        };
        if layout.channel_count() != channels {
            return;
        }
        let need = channels * frames;
        if need > self.out.len() {
            return;
        }
        // Sized on the control path in `set_multichannel_layout`. This used to
        // grow lazily, which put a 256 KiB `malloc` on the audio thread on the
        // first multichannel f64 block. A short scratch is now a refuse, not a
        // grow — the same policy `arena/process.rs` applies to under-allocation.
        if self.mc_planes.len() < need {
            return;
        }

        let block_secs = frames as f32 / self.sample_rate;
        self.glide_listener(block_secs);
        self.step_cues(block_secs);
        let snap = self.apply_cues_for_render();
        self.apply_voice_budget();

        // Demote into channel-major scratch.
        for (ch, plane) in planes.iter_mut().enumerate() {
            let dst = &mut self.mc_planes[ch * frames..(ch + 1) * frames];
            for f in 0..frames {
                dst[f] = plane[f] as f32;
            }
        }

        self.out[..need].fill(0.0);
        let mut bed_storage: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
        {
            let bed_inputs = &mut bed_storage[..channels];
            for (ch, dst) in bed_inputs.iter_mut().enumerate() {
                *dst = &self.mc_planes[ch * frames..(ch + 1) * frames];
            }
            let inputs = HybridBlockInputs {
                objects: &[],
                beds: bed_inputs,
                fields: &[],
            };
            let Some(renderer) = self.mc_renderer.as_mut() else {
                return;
            };
            let result =
                renderer.process_hybrid_block(&self.scene, &inputs, frames, &mut self.out[..need]);
            self.restore_program(&snap);
            if result.is_err() {
                return;
            }
        }

        for (ch, plane) in planes.iter_mut().enumerate() {
            for f in 0..frames {
                plane[f] = self.out[f * channels + ch] as f64;
            }
        }
    }

    /// Set the active HRTF profile and sync head model / dataset into the renderer (§4.6, Item 21).
    pub fn set_hrtf_profile(&mut self, profile_id: &str) -> bool {
        if self.hrtf_manager.set_active_profile(profile_id) {
            if let Some(profile) = self.hrtf_manager.active_profile().cloned() {
                let radius =
                    profile.anthropometry.head_radius_m * profile.personalization.itd_scaling;
                self.binaural
                    .set_head_parameters(radius, DEFAULT_SPEED_OF_SOUND);
                match self
                    .hrtf_manager
                    .resolve_dataset(&profile, self.sample_rate.max(1.0) as u32)
                {
                    Ok(ds_opt) => {
                        self.binaural.use_dataset(ds_opt);
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to resolve HRTF dataset for profile '{}': {}",
                            profile.id,
                            e
                        );
                    }
                }
                return true;
            }
        }
        false
    }

    /// Register a custom HRTF profile into the node's profile manager.
    pub fn register_hrtf_profile(&mut self, profile: HrtfProfile) {
        self.hrtf_manager.register_profile(profile);
    }

    /// Register a custom HRTF dataset into the node's profile manager.
    pub fn register_hrtf_dataset(&mut self, key: impl Into<String>, dataset: Arc<HrtfDataset>) {
        self.hrtf_manager.register_dataset(key, dataset);
    }

    /// Access active HRTF profile (if any).
    pub fn active_hrtf_profile(&self) -> Option<&HrtfProfile> {
        self.hrtf_manager.active_profile()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Screen geometry `(center_azimuth_deg, half_width_deg,
    /// elevation_deg, gain)`.
    pub fn screen(&self) -> (f32, f32, f32, f32) {
        (
            self.center_azimuth_deg,
            self.half_width_deg,
            self.elevation_deg,
            self.screen_gain,
        )
    }

    /// Room params `(enabled, width, depth, height, absorption,
    /// reflection_order, rt60_ms, late_mix, late_distance, wet)`.
    pub fn room(&self) -> (bool, f32, f32, f32, f32, u8, f32, f32, bool, f32) {
        let r = &self.scene.room;
        let wet = self
            .scene
            .object(self.obj_l)
            .map(|o| o.room_send)
            .unwrap_or(0.0);
        (
            r.enabled,
            r.width,
            r.depth,
            r.height,
            r.absorption,
            r.reflection_order,
            r.rt60_ms,
            r.late_mix,
            r.late_distance,
            wet,
        )
    }

    /// Listener orientation `(yaw, pitch, roll)` in degrees.
    pub fn listener(&self) -> (f32, f32, f32) {
        (
            self.listener_yaw_deg,
            self.listener_pitch_deg,
            self.listener_roll_deg,
        )
    }

    /// Apply the config surface (construction / reconfig / `apply_config`).
    ///
    /// `layout` is the graph's configured master channel layout. It is threaded
    /// through here (rather than only via `set_multichannel_layout`) so a
    /// *rebuilt* generation carries it too — without it, a graph rebuild would
    /// silently drop the multichannel renderer back to passthrough.
    pub fn apply_config(
        &mut self,
        cfg: &config::SpatialConfig,
        sample_rate: f32,
        layout: &ChannelLayout,
    ) {
        self.sample_rate = sample_rate.max(1.0);
        self.set_multichannel_layout(layout);
        self.enabled = cfg.enabled;
        // Render knobs (spec §86, §76, §70): quality tier, metering enable,
        // and the voice budget.
        self.binaural.set_quality(match cfg.quality {
            config::SpatialQuality::Low => SpatialQuality::Low,
            config::SpatialQuality::Medium => SpatialQuality::Medium,
            config::SpatialQuality::High => SpatialQuality::High,
            config::SpatialQuality::Ultra => SpatialQuality::Ultra,
        });
        self.binaural.set_metering_enabled(cfg.metering.enabled);
        self.voice = if cfg.voice.enabled {
            Some(VoiceBudget {
                capacity: cfg.voice.capacity,
                full_quality_capacity: cfg.voice.full_quality_capacity,
                policy: match cfg.voice.policy {
                    config::VoicePriority::Fixed => VoicePriority::Fixed,
                    config::VoicePriority::DistanceWeighted => VoicePriority::DistanceWeighted,
                    config::VoicePriority::GainWeighted => VoicePriority::GainWeighted,
                    config::VoicePriority::UserDefined => VoicePriority::UserDefined,
                    config::VoicePriority::AdaptiveAudibility => VoicePriority::AdaptiveAudibility,
                },
            })
        } else {
            None
        };
        self.apply_screen(
            cfg.center_azimuth_deg,
            cfg.half_width_deg,
            cfg.elevation_deg,
            cfg.gain,
        );
        let r = &cfg.room;
        self.apply_room(
            r.enabled,
            r.width,
            r.depth,
            r.height,
            r.absorption,
            r.reflection_order,
            r.rt60_ms,
            r.late_mix,
            r.late_distance,
            r.wet,
        );
        self.apply_listener(
            cfg.listener_yaw_deg,
            cfg.listener_pitch_deg,
            cfg.listener_roll_deg,
        );
        // The declarative cue bank (control path — the runtime
        // curves are built here, then only read).
        self.set_cues(&cfg.cues);
        // Refresh the modeled cost / tail budget for the new
        // configuration.
        self.refresh_cost_diagnostics();
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set the renderer quality tier at runtime (spec §86).
    pub fn set_quality(&mut self, q: SpatialQuality) {
        self.binaural.set_quality(q);
    }

    /// The active quality tier (spec §86).
    pub fn quality(&self) -> SpatialQuality {
        self.binaural.quality()
    }

    /// Set / replace the voice budget at runtime (spec §76). `enabled ==
    /// false` clears it (full admission). Control path — the budget is read
    /// allocation-free at block rate.
    pub fn set_voice(
        &mut self,
        enabled: bool,
        capacity: usize,
        full_quality_capacity: usize,
        policy: VoicePriority,
    ) {
        self.voice = if enabled {
            Some(VoiceBudget {
                capacity,
                full_quality_capacity,
                policy,
            })
        } else {
            None
        };
    }

    /// Attach a scalar automation curve (gain or spread) to one program
    /// object (`object` 0 = L, 1 = R). `None` clears it. Control path: the
    /// curve is built / moved off the audio thread and then evaluated
    /// allocation-free at block rate. `kind` 0 = gain, 1 = spread.
    pub fn set_program_automation(
        &mut self,
        object: usize,
        kind: u8,
        curve: Option<Arc<CurveScalar>>,
    ) {
        let id = match object {
            0 => self.obj_l,
            1 => self.obj_r,
            _ => return,
        };
        let Some(obj) = self.scene.object_mut(id) else {
            return;
        };
        let curved = curve.map(|c| (*c).clone());
        match kind {
            0 => obj.automation.gain = curved,
            1 => obj.automation.spread = curved,
            _ => {}
        }
    }

    /// Drive program-object automation at `seconds` (spec §47).
    pub fn set_automation_time(&mut self, seconds: f32) {
        self.binaural.set_automation_time(seconds);
    }

    // ── Scene animation cues (v4.4.0) ──

    /// Replace the node's cue bank from the scene-file model (control
    /// path — allocates on the caller's thread, then only reads).
    pub fn set_cues(&mut self, cues: &[config::SpatialCueConfig]) {
        self.scene.set_cues(cues);
    }

    /// Fire the named cue at the block boundary (audio drain; the trigger
    /// resolves the name to the bank index here — the queued command
    /// carries the index). Returns `false` when the name is unknown.
    pub fn trigger_cue(&mut self, cue_index: usize) -> bool {
        let now = self.cue_clock;
        if cue_index >= self.scene.cue_bank.len() {
            return false;
        }
        self.scene.cue_bank.trigger(cue_index, now);
        true
    }
    /// Stop the active cue on a program object (audio drain).
    pub fn stop_cue(&mut self, target: usize) {
        self.scene.cue_bank.stop(target);
    }

    /// Stop all active cues (audio drain).
    pub fn stop_all_cues(&mut self) {
        self.scene.cue_bank.stop_all();
    }

    /// The number of cues in the bank (control introspection).
    pub fn cue_count(&self) -> usize {
        self.scene.cue_bank.len()
    }

    /// The cue bank as the scene-file model (control path — the
    /// persistence / round-trip surface).
    pub fn scene_cues(&self) -> Vec<config::SpatialCueConfig> {
        self.scene.cues()
    }

    /// Whether a target currently has an active cue (control
    /// introspection / tests).
    pub fn cue_active(&self, target: usize) -> bool {
        self.scene.cue_bank.is_active(target)
    }

    /// The active cue's bank index on a target (control introspection /
    /// the sticky mirror).
    pub fn cue_active_index(&self, target: usize) -> Option<usize> {
        self.scene.cue_bank.active_index(target)
    }

    /// Resolve a cue name to its bank index (control path).
    pub fn cue_index_of(&self, name: &str) -> Option<usize> {
        self.scene.cue_bank.index_of(name)
    }

    /// Advance the cue clock one block and retire finished cues (audio
    /// path, allocation-free). Returns the block duration in seconds.
    pub(super) fn step_cues(&mut self, block_secs: f32) -> f32 {
        self.cue_clock += block_secs;
        self.scene.cue_bank.step(self.cue_clock);
        block_secs
    }

    /// Apply the active cue overlays onto the two program objects'
    /// effective parameters for this block's render, snapshotting the
    /// authored values for [`Self::restore_program`] (audio path,
    /// allocation-free — plain stack data; a no-op when no cue is
    /// Active, so the legacy render stays bit-exact).
    pub(super) fn apply_cues_for_render(&mut self) -> ProgramSnapshot {
        let mut snap = ProgramSnapshot::default();
        for target in 0..2 {
            let mut overlay = crate::spatial::cue::CueOverlay::none();
            self.scene
                .cue_bank
                .evaluate(target, self.cue_clock, &mut overlay);
            if overlay.is_none() {
                continue;
            }
            let id = if target == 0 { self.obj_l } else { self.obj_r };
            let Some(obj) = self.scene.object_mut(id) else {
                continue;
            };
            snap.applied = true;
            snap.position[target] = obj.position;
            snap.gain[target] = obj.gain;
            snap.spread[target] = obj.spread;
            if let Some(p) = overlay.position {
                obj.position = p;
            }
            if let Some(g) = overlay.gain {
                obj.gain = g;
            }
            if let Some(s) = overlay.spread {
                obj.spread = s;
            }
        }
        snap
    }

    /// Restore the program objects' authored parameters after a render
    /// that carried a cue overlay (audio path, allocation-free).
    pub(super) fn restore_program(&mut self, snap: &ProgramSnapshot) {
        if !snap.applied {
            return;
        }
        for (target, &id) in [self.obj_l, self.obj_r].iter().enumerate() {
            let Some(obj) = self.scene.object_mut(id) else {
                continue;
            };
            obj.position = snap.position[target];
            obj.gain = snap.gain[target];
            obj.spread = snap.spread[target];
        }
    }

    /// The output meters snapshot (control thread read; spec §70).
    pub fn meters(&self) -> &SpatialMeterState {
        self.binaural.meters()
    }

    /// The voice budget honored from config, if enabled (spec §76). A host
    /// can apply it via the spatial `VoiceBudget::plan` scheduler.
    pub fn voice_budget(&self) -> Option<&VoiceBudget> {
        self.voice.as_ref()
    }

    /// Run the voice budget over the scene's objects (spec §76) and feed the
    /// resulting admission plan into the renderer's object loop. Allocation-
    /// free (stack staging + bounded copy), so it is safe on the audio path;
    /// called once per `render_block`. When no voice budget is configured the
    /// renderer is reset to full admission (bit-exact passthrough).
    pub(super) fn apply_voice_budget(&mut self) {
        let Some(budget) = &self.voice else {
            self.binaural.clear_voice_admission();
            self.voice_active = false;
            self.voice_full = 0;
            self.voice_degraded = 0;
            self.voice_dropped = 0;
            return;
        };
        let mut candidates = [BudgetCandidate {
            index: 0,
            gain: 0.0,
            distance: 0.0,
            priority: 0,
            importance: 1.0,
        }; MAX_SPATIAL_OBJECTS];
        let mut n = 0usize;
        for (slot, obj) in self.scene.objects.iter_enabled() {
            if n >= MAX_SPATIAL_OBJECTS {
                break;
            }
            candidates[n] = BudgetCandidate {
                index: slot,
                gain: obj.gain,
                distance: (obj.position - self.scene.listener.position).length(),
                priority: 0,
                importance: 1.0,
            };
            n += 1;
        }
        let n_slots = self.scene.objects.len().min(MAX_SPATIAL_OBJECTS);
        let mut scratch = [0usize; MAX_SPATIAL_OBJECTS];
        let mut admission = [VoiceAdmission::Dropped; MAX_SPATIAL_OBJECTS];
        budget.plan_into(
            &candidates[..n],
            n_slots,
            &mut scratch[..n.max(1)],
            &mut admission[..n_slots.max(1)],
        );
        self.binaural
            .set_voice_admission(&admission[..n_slots.max(1)]);
        // Record the admission plan for telemetry (spec §76).
        let (mut full, mut degraded, mut dropped) = (0usize, 0usize, 0usize);
        for &a in admission[..n_slots.max(1)].iter() {
            match a {
                VoiceAdmission::Full => full += 1,
                VoiceAdmission::Degraded => degraded += 1,
                VoiceAdmission::Dropped => dropped += 1,
            }
        }
        self.voice_active = true;
        self.voice_full = full;
        self.voice_degraded = degraded;
        self.voice_dropped = dropped;
    }

    /// Live spatial telemetry snapshot (control thread): the per-ear output
    /// meters (peak/RMS dBFS) and the voice-admission counts, published into
    /// `PlaybackInfo`. Reads the renderer's meters on the engine's single
    /// audio/control thread, then the value is atomically published via
    /// `ArcSwap<PlaybackInfo>` for lock-free host reads.
    pub fn spatial_telemetry(&self) -> crate::playback_info::SpatialTelemetry {
        use crate::playback_info::SpatialTelemetry;
        let m = self.binaural.meters().snapshot();
        let db = |lin: f32| -> f32 {
            if lin <= 1e-9 {
                -96.0
            } else {
                20.0 * lin.log10()
            }
        };
        let peak_l = m.speaker_peak.first().copied().unwrap_or(0.0);
        let peak_r = m.speaker_peak.get(1).copied().unwrap_or(0.0);
        let rms_l = m.speaker_rms.first().copied().unwrap_or(0.0);
        let rms_r = m.speaker_rms.get(1).copied().unwrap_or(0.0);
        SpatialTelemetry {
            enabled: self.enabled,
            voice_active: self.voice_active,
            voice_full_voices: self.voice_full,
            voice_degraded_voices: self.voice_degraded,
            voice_dropped_voices: self.voice_dropped,
            peak_db_l: db(peak_l),
            peak_db_r: db(peak_r),
            rms_db_l: db(rms_l),
            rms_db_r: db(rms_r),
            listener_yaw_deg: self.listener_yaw_deg,
            listener_pitch_deg: self.listener_pitch_deg,
            listener_roll_deg: self.listener_roll_deg,
            listener_position: self.scene.listener.position,
            render_cost_units: self.last_cost_units,
            cost_utilization: self.last_cost_utilization,
            tail_blocks_remaining: self.tail_budget_blocks,
        }
    }

    /// Recompute the modeled render cost + tail budget (control
    /// path — pure model math from the diagnostics module; call after
    /// config/scene changes). The values mirror into telemetry.
    pub fn refresh_cost_diagnostics(&mut self) {
        let budget = self.voice.as_ref().map(|v| v.capacity as f32);
        let report = crate::spatial::diagnostics::build_scene_cost_report(
            &self.scene,
            self.enabled,
            self.binaural.quality(),
            budget,
        );
        self.last_cost_units = report.total_cost;
        self.last_cost_utilization = report.utilization;
        // Tail budget: the remaining budget expressed in equivalent
        // blocks at the current per-block cost (`∞` when idle, 0 when at
        // or over budget).
        self.tail_budget_blocks = if report.total_cost <= 0.0 {
            f32::INFINITY
        } else if report.utilization >= 1.0 {
            0.0
        } else {
            (report.block_budget - report.total_cost) / report.total_cost
        };
    }

    /// The modeled render-cost report (deterministic). The
    /// budget defaults to the voice budget's capacity when configured.
    pub fn scene_cost_report(
        &self,
        block_budget: Option<f32>,
    ) -> crate::spatial::diagnostics::SceneCostReport {
        let budget = block_budget.or_else(|| self.voice.as_ref().map(|v| v.capacity as f32));
        crate::spatial::diagnostics::build_scene_cost_report(
            &self.scene,
            self.enabled,
            self.binaural.quality(),
            budget,
        )
    }

    /// The configured voice budget's capacity (the cost budget
    /// when a budget exists), for the engine-level report accessor.
    pub fn voice_budget_capacity(&self) -> Option<f32> {
        self.voice.as_ref().map(|v| v.capacity as f32)
    }

    /// Whether the spatial stage is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Spatial-health snapshot (spec §103 extension): an explainable
    /// per-source status derived from the existing meters + scene + voice
    /// counts. **Control/telemetry path only** — reads the renderer's meter
    /// accumulator like [`Self::spatial_telemetry`], then the snapshot is
    /// published via `ArcSwap<PlaybackInfo>` for lock-free host reads.
    pub fn spatial_health(&self) -> crate::spatial::health::SpatialHealthSnapshot {
        use crate::spatial::health::{build_health, HrtfCoverage, SpatialHealthInputs};
        let meters = self.binaural.meters().snapshot();
        let hrtf = self
            .binaural
            .hrtf_grid()
            .map(|(a0, a1, e0, e1)| HrtfCoverage {
                azimuth_min_deg: a0,
                azimuth_max_deg: a1,
                elevation_min_deg: e0,
                elevation_max_deg: e1,
            });
        build_health(SpatialHealthInputs {
            scene: &self.scene,
            meters: &meters,
            enabled: self.enabled,
            quality: self.binaural.quality(),
            voice_active: self.voice_active,
            voice_full: self.voice_full,
            voice_degraded: self.voice_degraded,
            voice_dropped: self.voice_dropped,
            hrtf,
        })
    }
}

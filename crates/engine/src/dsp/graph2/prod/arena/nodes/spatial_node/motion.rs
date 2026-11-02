use super::*;

impl SpatialNode {
    /// Move the two program objects onto the virtual screen and set the
    /// screen gain.
    pub fn apply_screen(
        &mut self,
        center_azimuth_deg: f32,
        half_width_deg: f32,
        elevation_deg: f32,
        gain: f32,
    ) {
        self.center_azimuth_deg = center_azimuth_deg;
        self.half_width_deg = half_width_deg.clamp(0.0, 90.0);
        self.elevation_deg = elevation_deg.clamp(-90.0, 90.0);
        self.screen_gain = gain.clamp(0.0, 4.0);
        let az_l = (self.center_azimuth_deg - self.half_width_deg).to_radians();
        let az_r = (self.center_azimuth_deg + self.half_width_deg).to_radians();
        let el = self.elevation_deg.to_radians();
        let (sin_el, cos_el) = el.sin_cos();
        let pos = |az: f32| -> Vec3 {
            Vec3::new(
                SCREEN_RADIUS * cos_el * az.sin(),
                SCREEN_RADIUS * cos_el * az.cos(),
                SCREEN_RADIUS * sin_el,
            )
        };
        let pl = pos(az_l);
        let pr = pos(az_r);
        self.scene
            .object_mut(self.obj_l)
            .expect("invariant: the left program object exists")
            .position = pl;
        self.scene
            .object_mut(self.obj_r)
            .expect("invariant: the right program object exists")
            .position = pr;
        for id in [self.obj_l, self.obj_r] {
            self.scene
                .object_mut(id)
                .expect("invariant: program objects created in new exist")
                .gain = self.screen_gain;
        }
    }

    /// Configure the room: geometry, reflection order, late field, the
    /// program's reflection send (`wet`), and the late-field distance
    /// Roll-off.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_room(
        &mut self,
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
        let r = &mut self.scene.room;
        r.enabled = enabled;
        r.width = width.max(0.1);
        r.depth = depth.max(0.1);
        r.height = height.max(0.1);
        r.absorption = absorption.clamp(0.0, 0.99);
        r.reflection_order = reflection_order.clamp(1, 2);
        r.rt60_ms = rt60_ms.max(1.0);
        r.late_mix = late_mix.clamp(0.0, 1.0);
        r.late_distance = late_distance;
        let wet = wet.clamp(0.0, 1.0);
        for id in [self.obj_l, self.obj_r] {
            self.scene
                .object_mut(id)
                .expect("invariant: program objects created in new exist")
                .room_send = wet;
        }
    }

    /// Configure the scene-wide air-absorption model applied to live room
    /// Reflections: each live image's surface corner is composed
    /// with the model's distance corner so realtime reflections darken
    /// with travel distance, agreeing with the offline spectral kernels.
    pub fn set_air_absorption(&mut self, air: crate::spatial::level::AirAbsorption) {
        self.binaural.set_air_absorption(air);
    }

    /// The scene-wide air-absorption model.
    pub fn air_absorption(&self) -> crate::spatial::level::AirAbsorption {
        self.binaural.air_absorption()
    }

    /// Set the listener orientation (yaw/pitch/roll, degrees). Snaps the
    /// Listener immediately (the pre- semantic) and seeds the
    /// motion target so a later glide continues from here.
    pub fn apply_listener(&mut self, yaw_deg: f32, pitch_deg: f32, roll_deg: f32) {
        self.listener_yaw_deg = yaw_deg;
        self.listener_pitch_deg = pitch_deg;
        self.listener_roll_deg = roll_deg;
        let q = Quat::from_euler_rad(
            yaw_deg.to_radians(),
            pitch_deg.to_radians(),
            roll_deg.to_radians(),
        );
        self.scene.listener.set_orientation(q);
        self.listener_target_quat = q;
        self.listener_motion_active = true;
    }

    // ── Runtime listener motion (v4.3.0) ──────────────────────

    /// The listener-motion smoothing policy (nlerp / one-pole time
    /// constant, optional angular rate limit). Control path — the policy
    /// is plain data read allocation-free per block.
    pub fn set_listener_tracking(&mut self, cfg: TrackingConfig) {
        self.listener_tracking = cfg;
    }

    /// The active listener-motion smoothing policy.
    pub fn listener_tracking(&self) -> TrackingConfig {
        self.listener_tracking
    }

    /// Set the **target listener pose** (world-space orientation +
    /// position). The listener glides toward it every processed block
    /// (shortest-arc nlerp on orientation, one-pole on position) per the
    /// tracking conventions — the runtime-editable rotation/position
    /// Surface. With `smoothing_ms = 0` the next block snaps.
    /// The glide itself is allocation-free (audio path).
    pub fn set_listener_pose_target(&mut self, orientation: Quat, position: Vec3) {
        self.listener_target_quat = orientation;
        self.listener_target_pos = position;
        self.listener_motion_active = true;
    }

    /// Clear the motion target: the listener holds its current pose
    /// (no further gliding) until a new target arrives.
    pub fn clear_listener_motion(&mut self) {
        self.listener_motion_active = false;
        self.listener_target_quat = self.scene.listener.orientation;
        self.listener_target_pos = self.scene.listener.position;
    }

    /// The live listener pose (the post-glide state the renderers read),
    /// for telemetry and control-side introspection.
    pub fn listener_pose(&self) -> (Quat, Vec3) {
        (
            self.scene.listener.orientation,
            self.scene.listener.position,
        )
    }

    /// The target pose the listener is gliding toward (held pose when
    /// motion is inactive).
    pub fn listener_pose_target(&self) -> (Quat, Vec3) {
        (self.listener_target_quat, self.listener_target_pos)
    }

    /// Advance the listener one block-step toward its motion target
    /// (audio path, per `render_block`): shortest-arc nlerp on orientation
    /// with the optional rate limit, linear one-pole on position — the
    /// `HeadTracker` discipline inlined (the node cannot own a tracker
    /// because the tracker clocks on host timestamps; the node clocks on
    /// blocks). Allocation-free. No-op when no target is active or
    /// already converged.
    pub(super) fn glide_listener(&mut self, block_secs: f32) {
        if !self.listener_motion_active {
            return;
        }
        let cfg = self.listener_tracking;
        let l = &mut self.scene.listener;
        // Orientation: one-pole nlerp toward the target, then the
        // optional angular rate clamp.
        let alpha = if cfg.smoothing_ms <= 0.0 || block_secs <= 0.0 {
            1.0
        } else {
            1.0 - (-block_secs / (cfg.smoothing_ms / 1000.0)).exp()
        };
        let mut next_q = l.orientation.nlerp(self.listener_target_quat, alpha);
        if cfg.max_angular_rate_deg_s > 0.0 && block_secs > 0.0 {
            let max_step = cfg.max_angular_rate_deg_s.to_radians() * block_secs;
            let angle = l.orientation.angle_to(next_q);
            if angle > max_step && angle > 1e-9 {
                next_q = l.orientation.nlerp(next_q, (max_step / angle).min(1.0));
            }
        }
        l.set_orientation(next_q);
        // Position: the same one-pole factor on each axis.
        let next_p = l.position.lerp(self.listener_target_pos, alpha);
        l.set_position(next_p);
        // Mirror the introspection degrees from the live quaternion so
        // `listener()` (and persistence) reflect the gliding pose.
        let (y, p, r) = next_q.to_euler_rad();
        self.listener_yaw_deg = y.to_degrees();
        self.listener_pitch_deg = p.to_degrees();
        self.listener_roll_deg = r.to_degrees();
        // Converged: stop gliding (bit-stable when the target is held).
        if next_q.angle_to(self.listener_target_quat) < 1e-6
            && (next_p - self.listener_target_pos).length_squared() < 1e-12
        {
            self.listener_motion_active = false;
        }
    }

    /// Whether a moving listener has crossed the baked-scene relevance
    /// bound and the control thread should re-bake the acoustic scene
    /// ('s smooth re-bake seam). The bound is the bake cell
    /// size (the resolution a re-bake can meaningfully change); the
    /// engine calls this on its tick and, when due, rebuilds the baked
    /// generation on the control thread and publishes it — the audio
    /// Path is never interrupted (the swap machinery).
    pub fn listener_rebake_due(&self, cell_m: f32, last_baked_at: Vec3) -> bool {
        let cell = cell_m.max(0.05);
        let p = self.scene.listener.position;
        let d = p - last_baked_at;
        // Crossed a full cell in any axis → the baked response is stale.
        d.x.abs() >= cell || d.y.abs() >= cell || d.z.abs() >= cell
    }

    /// Render the scene into the block planes in place (f32 path).
    pub(super) fn render_block(&mut self, planes: &mut [&mut [f32]], frames: usize) {
        if !self.prepared || frames == 0 || frames > MAX_AUDIO_BLOCK_FRAMES {
            return;
        }
        if planes.len() < 2 {
            return;
        }
        // Glide the listener toward its motion target this
        // block (allocation-free; no-op when converged / inactive).
        let block_secs = frames as f32 / self.sample_rate;
        self.glide_listener(block_secs);
        // Advance the cue clock and retire finished cues, then
        // overlay any active cue onto the program objects for this block
        // (allocation-free; no-op when the bank is idle).
        self.step_cues(block_secs);
        let snap = self.apply_cues_for_render();
        // Voice budget → admission (spec §76), allocation-free, this block.
        self.apply_voice_budget();
        self.prog_l[..frames].copy_from_slice(&planes[0][..frames]);
        self.prog_r[..frames].copy_from_slice(&planes[1][..frames]);
        let need = 2 * frames;
        let obj_refs = [
            self.prog_l[..frames].as_ref(),
            self.prog_r[..frames].as_ref(),
        ];
        let inputs = HybridBlockInputs {
            objects: &obj_refs,
            beds: &[],
            fields: &[],
        };
        let result =
            self.binaural
                .process_hybrid_block(&self.scene, &inputs, frames, &mut self.out[..need]);
        // Restore the program objects' authored parameters (cue overlays
        // apply per block only).
        self.restore_program(&snap);
        // After a successful prepare the render cannot fail; any error
        // leaves the block untouched (bit-exact passthrough) rather than
        // emitting garbage.
        if result.is_err() {
            return;
        }
        for (ch, plane) in planes.iter_mut().enumerate().take(2) {
            for f in 0..frames {
                plane[f] = self.out[f * 2 + ch];
            }
        }
    }

    /// f64 twin: demote the front pair, render in f32, promote back.
    pub(super) fn render_block_f64(&mut self, planes: &mut [&mut [f64]], frames: usize) {
        if !self.prepared || frames == 0 || frames > MAX_AUDIO_BLOCK_FRAMES {
            return;
        }
        if planes.len() < 2 {
            return;
        }
        // Glide the listener toward its motion target this
        // block (allocation-free; no-op when converged / inactive).
        let block_secs = frames as f32 / self.sample_rate;
        self.glide_listener(block_secs);
        // Advance the cue clock and retire finished cues, then
        // overlay any active cue onto the program objects for this block
        // (allocation-free; no-op when the bank is idle).
        self.step_cues(block_secs);
        let snap = self.apply_cues_for_render();
        // Voice budget → admission (spec §76), allocation-free, this block.
        self.apply_voice_budget();
        for (f, (&l, &r)) in planes[0]
            .iter()
            .zip(planes[1].iter())
            .take(frames)
            .enumerate()
        {
            self.prog_l[f] = l as f32;
            self.prog_r[f] = r as f32;
        }
        let need = 2 * frames;
        let obj_refs = [
            self.prog_l[..frames].as_ref(),
            self.prog_r[..frames].as_ref(),
        ];
        let inputs = HybridBlockInputs {
            objects: &obj_refs,
            beds: &[],
            fields: &[],
        };
        let result =
            self.binaural
                .process_hybrid_block(&self.scene, &inputs, frames, &mut self.out[..need]);
        // Restore the program objects' authored parameters (cue overlays
        // apply per block only).
        self.restore_program(&snap);
        if result.is_err() {
            return;
        }
        for (ch, plane) in planes.iter_mut().enumerate().take(2) {
            for f in 0..frames {
                plane[f] = self.out[f * 2 + ch] as f64;
            }
        }
    }
}

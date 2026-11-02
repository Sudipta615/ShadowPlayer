use super::apply::apply_node_cmd;
use super::*;

impl DspGraph {
    fn apply_shell_cmd(&mut self, cmd: NodeCmd) {
        match cmd {
            NodeCmd::SetBitPerfect(enabled) => {
                self.bit_perfect = enabled;
                if enabled {
                    self.volume_mut().processor.set_gain(1.0);
                    self.volume_mut().processor.snap();
                    self.seek_fade_mut().fade.reset();
                }
            }
            NodeCmd::SetDoPBypass(enabled) => self.dop_bypass = enabled,
            NodeCmd::SetSpeed(speed) => {
                let speed = speed.clamp(0.25, 4.0);
                self.speed = speed;
                self.timestretch_mut().stretcher.set_speed(speed);
            }
            NodeCmd::SetNonFinitePolicy(policy) => self.non_finite_policy = policy,
            _ => {}
        }
    }

    /// Drain every control queue into the active generation and handle a
    /// pending generation swap. Called once per caller block, before any
    /// signal processing and before the transport-bypass early returns, so
    /// control application is never skipped by bypass contracts.
    /// Drain queued control commands and any pending generation swap NOW.
    ///
    /// The engine is single-threaded (its tick drives both command dispatch
    /// and the decode loop), so it applies commands immediately after
    /// dispatch instead of waiting for the next audio block. Safe only when
    /// no other thread is processing audio; multi-threaded hosts must rely
    /// on the block-boundary `Self::control_tick` instead.
    pub fn drain_queued_control(&mut self) {
        self.control_tick();
    }

    pub(crate) fn control_tick(&mut self) {
        // Swap a pending generation in BEFORE draining the command queues: a
        // command enqueued after a generation was published must land on the
        // NEW generation (the one it replaced is retired this block).
        // Draining first would apply it to the outgoing generation and lose
        // it across the swap. The sticky user-state mirror reads the
        // post-apply state of whichever generation is now active, so
        // snapshot seeding stays consistent.
        let pending = self
            .bus
            .pending
            .swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !pending.is_null() {
            let new_gen = unsafe { Box::from_raw(pending) };
            let prev = std::mem::replace(&mut self.active, new_gen);
            if let Some(old_retiring) = self.retiring.take() {
                self.hand_back_retired(old_retiring);
            }
            self.retiring = Some(prev);
            self.transition_fader.trigger(self.sample_rate);
            self.bus.swap_seq.fetch_add(1, Ordering::Relaxed);
        }
        if self.bus.has_pending.swap(false, Ordering::AcqRel) {
            self.drain_control();
        }
    }

    /// Audio-side helper: hand a retiring [`GraphGeneration`] back to the
    /// control thread via `ControlBus::retired`. Called from `process.rs`
    /// when a transition crossfade completes and the old generation is no
    /// longer needed. Allocation-free; uses a single atomic `Release` store.
    #[inline]
    /// Audio-thread hand-back of a finished [`GraphGeneration`] to the control
    /// thread's reclamation slot.
    ///
    /// Uses `swap`, NOT `store`. `reclaim_retired` is only called from
    /// `publish_generation`, so two hand-backs with no intervening publish means
    /// a `store` would silently overwrite the first raw pointer and leak an
    /// entire generation — each one an 18-node arena holding convolution IRs,
    /// spatial scenes and plugin instances. Two reconfigurations inside a single
    /// crossfade window reach that state.
    ///
    /// `swap` makes the collision visible instead. A non-null return means the
    /// control thread has not reclaimed the previous occupant yet. The audio
    /// thread must NOT free it (that would violate "the audio thread never
    /// frees"), so it is counted and left to the control thread, which drains
    /// any overflow on its next reclaim.
    fn hand_back_retired(&self, gen: Box<GraphGeneration>) {
        let displaced = self.bus.retired.swap(Box::into_raw(gen), Ordering::AcqRel);
        if displaced.is_null() {
            return;
        }
        // Single writer (the audio thread), so a load/store pair is enough — and it
        // is required to be a load/store pair rather than an RMW: the cursor
        // must be published AFTER the slot it counts, so that a consumer
        // seeing this value also sees the slot. See `reclaim_retired`.
        let idx = self.bus.retired_overflow_len.load(Ordering::Relaxed);
        if idx < RETIRE_OVERFLOW_SLOTS {
            self.bus.retired_overflow[idx].store(displaced, Ordering::Relaxed);
            self.bus
                .retired_overflow_len
                .store(idx + 1, Ordering::Release);
        } else {
            // Out of overflow slots. The audio thread must not free, so this
            // is counted and leaked deliberately — the one outcome strictly
            // preferable to freeing on the audio thread.
            self.bus
                .retired_overflow_dropped
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(in crate::dsp::graph2::prod::arena) fn retire_generation_to_bus(
        &mut self,
        gen: Box<GraphGeneration>,
    ) {
        self.hand_back_retired(gen);
    }

    /// FIFO-per-node drain. Shell commands first, then per-node commands in
    /// arena order (a deterministic total order). Bounded work: at most
    /// [`CONTROL_QUEUE_CAPACITY`] pops per queue.
    fn drain_control(&mut self) {
        let mut buf = [NodeCmd::default(); CONTROL_QUEUE_CAPACITY];

        // Shell queue first.
        let n = self.bus.queues[NodeId::SHELL.0].pop_block(&mut buf);
        for cmd in &buf[..n] {
            self.apply_shell_cmd(*cmd);
        }

        // Per-node queues in arena order. `bus` and `active` are disjoint
        // fields, so mirroring user state and mutating the node coexist.
        for i in 0..self.active.nodes.len() {
            let id = self.active.node_ids[i];
            let n = self.bus.queues[id.0].pop_block(&mut buf);
            for cmd in &buf[..n] {
                match cmd {
                    NodeCmd::SetVolumeTarget(v) => {
                        self.bus.user_volume.store(v.to_bits(), Ordering::Relaxed)
                    }
                    NodeCmd::SetBalance(b) => {
                        self.bus.user_balance.store(b.to_bits(), Ordering::Relaxed)
                    }
                    NodeCmd::SetSpeed(s) => {
                        self.bus.user_speed.store(s.to_bits(), Ordering::Relaxed)
                    }
                    _ => {}
                }
                apply_node_cmd(&mut self.active.nodes[i], cmd);
                // Mirror the slot's *post-apply* state so the
                // sticky snapshot carries the linear target even for
                // dB/ramped commands (the node state is the source of truth).
                if let NodeCmd::MixInput { input, .. } = cmd {
                    let input = *input as usize;
                    if let GraphNode::Mix(mix) = &self.active.nodes[i] {
                        if let Some(slot) = mix.inputs.get(input) {
                            // Mirror the *target* gain (the user's intended
                            // setting): the ramped current value may still be
                            // mid-slew with no audio processed yet.
                            self.bus.set_slot_user_state(input, slot);
                        }
                    }
                }
                // Mirror the aux bus state post-apply so a
                // Generation swap preserves enabled / return gain.:
                // the aux bus is its own plan node now.
                if let NodeCmd::SetAux { .. } = cmd {
                    if let GraphNode::Aux(aux) = &self.active.nodes[i] {
                        self.bus
                            .set_aux_user_state(aux.enabled(), aux.return_gain());
                    }
                }
                // Mirror the aux insert state post-apply (a live
                // runtime toggle survives generation swaps, like SetAux).
                if let NodeCmd::SetAuxInsert { .. } = cmd {
                    if let GraphNode::Aux(aux) = &self.active.nodes[i] {
                        let (enabled, wet) = aux.insert_state();
                        self.bus.set_aux_insert_user_state(enabled, wet);
                    }
                }
                // Room/headphone correction: mirror the correction toggle / depth post-apply
                // so a live runtime toggle survives generation swaps.
                if matches!(
                    cmd,
                    NodeCmd::SetCorrectionEnabled(_) | NodeCmd::SetCorrectionDepth(_)
                ) {
                    if let GraphNode::Correction(c) = &self.active.nodes[i] {
                        self.bus.set_correction_user_state(c.enabled(), c.depth());
                    }
                }
                // Mirror the spatial master enable post-apply so a
                // live runtime toggle survives generation swaps.
                if matches!(cmd, NodeCmd::SetSpatialEnabled(_)) {
                    if let GraphNode::Spatial(s) = &self.active.nodes[i] {
                        self.bus.set_spatial_user_state(s.enabled());
                    }
                }
                // Mirror cue triggers/stops post-apply so live
                // cue state survives generation swaps.
                if matches!(
                    cmd,
                    NodeCmd::TriggerSpatialCue(_)
                        | NodeCmd::StopSpatialCue(_)
                        | NodeCmd::StopAllSpatialCues
                ) {
                    if let GraphNode::Spatial(s) = &self.active.nodes[i] {
                        for target in 0..crate::spatial::cue::MAX_ACTIVE_CUES {
                            self.bus
                                .set_active_cue_user_state(target, s.cue_active_index(target));
                        }
                    }
                }
                // Mirror the plugin host enable + params
                // post-apply so live runtime changes survive swaps.
                if matches!(cmd, NodeCmd::SetPluginEnabled(_)) {
                    if let GraphNode::PluginHost(p) = &self.active.nodes[i] {
                        self.bus.set_plugin_user_state(p.runtime_enabled());
                    }
                }
                if let NodeCmd::SetPluginParams(batch) = cmd {
                    self.bus.set_plugin_params_user_state(*batch);
                }
            }
        }
    }

    /// Number of swaps performed so far (audio-side monotonic counter).
    pub fn generation(&self) -> u64 {
        self.bus.swap_seq.load(Ordering::Acquire)
    }

    /// A cloneable handle to this graph's control plane, for use from
    /// another thread (the engine's tick thread, or tests).
    pub fn control_handle(&self) -> GraphControlHandle {
        GraphControlHandle {
            bus: self.bus.clone(),
        }
    }
}

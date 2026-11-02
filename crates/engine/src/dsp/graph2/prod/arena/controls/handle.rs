use super::*;

impl GraphControlHandle {
    pub(in crate::dsp::graph2::prod::arena) fn enqueue(&self, slot: usize, cmd: NodeCmd) {
        self.bus.enqueue(slot, cmd);
    }

    /// Read a slot's latest metering as `(peak_db, rms_db)`.
    /// Audio-written once per block; safe from any thread.
    pub fn slot_meters(&self, slot: usize) -> (f32, f32) {
        self.bus.slot_meters(slot)
    }

    /// Set the real-time non-finite floating point containment policy for the DSP graph.
    pub fn set_non_finite_policy(&self, policy: crate::dsp::safety::NonFinitePolicy) {
        self.enqueue(NodeId::SHELL.0, NodeCmd::SetNonFinitePolicy(policy));
    }

    /// Publish a fully-built generation for the audio thread to swap in at
    /// the next block boundary. Reclaims any generation returned by the
    /// audio thread first, then coalesces: an earlier pending (not yet
    /// swapped) generation is dropped in favor of this one — config is
    /// stateful, "latest wins".
    ///
    /// This is the cross-thread control path: build with
    /// [`GraphGeneration::from_config`] (allocation is fine on the control
    /// side) and publish from any thread holding this handle. The audio
    /// thread executes the swap with zero allocation. For same-thread
    /// reconfiguration, [`DspGraph::reconfigure`] wraps build + publish and
    /// preserves live user state.
    pub fn publish_generation(&self, gen: Box<GraphGeneration>) {
        self.reclaim_retired();
        let prev = self.bus.pending.swap(Box::into_raw(gen), Ordering::AcqRel);
        if !prev.is_null() {
            // Reclaim the coalesced, never-swapped generation.
            unsafe {
                drop(Box::from_raw(prev));
            }
        }
    }

    /// Reclaim (free) a generation returned by the audio thread. Called
    /// implicitly by [`Self::publish_generation`]; exposed for tests that
    /// assert the reclamation discipline without publishing.
    pub fn reclaim_retired(&self) {
        let r = self
            .bus
            .retired
            .swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !r.is_null() {
            unsafe {
                drop(Box::from_raw(r));
            }
            self.bus.reclaimed.fetch_add(1, Ordering::Relaxed);
        }
        // Drain the overflow stack the audio thread parked displaced
        // generations in (see `hand_back_retired`).
        //
        // The producer publishes the cursor LAST, so a consumer that observes
        // `n` entries is guaranteed to see all `n` slot stores. (It used to
        // `fetch_add` the cursor *before* storing its slot: the `AcqRel` pair
        // ordered only what was sequenced before the increment, so a consumer
        // could see a non-zero cursor with a still-null slot, take nothing, and
        // reset the cursor — permanently orphaning that generation.)
        let n = self
            .bus
            .retired_overflow_len
            .load(Ordering::Acquire)
            .min(RETIRE_OVERFLOW_SLOTS);
        for i in 0..n {
            let p = self.bus.retired_overflow[i].swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !p.is_null() {
                unsafe {
                    drop(Box::from_raw(p));
                }
                self.bus.reclaimed.fetch_add(1, Ordering::Relaxed);
            }
        }
        if n > 0 {
            // Compare-and-swap, not `store(0)`: a producer may have published
            // further entries while the loop above was draining, and a blind
            // reset would discard that cursor and leak those generations. On
            // failure the entries stay queued and the next drain re-scans them —
            // every slot is taken with `swap(null)`, so a rescan is idempotent.
            let _ = self.bus.retired_overflow_len.compare_exchange(
                n,
                0,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        }
    }

    /// Retire hand-backs that exceeded the overflow stack and were leaked.
    /// Non-zero means generations are being dropped rather than reclaimed.
    pub fn retired_overflow_dropped(&self) -> u64 {
        self.bus.retired_overflow_dropped.load(Ordering::Relaxed)
    }

    /// Number of swaps the audio thread has performed (monotonic).
    pub fn generation(&self) -> u64 {
        self.bus.swap_seq.load(Ordering::Acquire)
    }

    /// Number of generations reclaimed by the control side.
    pub fn reclaimed_count(&self) -> u64 {
        self.bus.reclaimed.load(Ordering::Relaxed)
    }

    /// Number of commands dropped on full queues.
    pub fn dropped_commands(&self) -> u64 {
        self.bus.dropped.load(Ordering::Relaxed)
    }

    /// Record the cost of one generation build. Called control-side only.
    pub(crate) fn record_build_cost(&self, nanos: u64) {
        self.bus.last_build_nanos.store(nanos, Ordering::Relaxed);
        self.bus
            .total_build_nanos
            .fetch_add(nanos, Ordering::Relaxed);
        self.bus.build_count.fetch_add(1, Ordering::Relaxed);
    }

    /// How long the most recent generation build took, in milliseconds.
    pub fn last_build_ms(&self) -> f64 {
        self.bus.last_build_nanos.load(Ordering::Relaxed) as f64 / 1e6
    }

    /// Mean generation-build cost in milliseconds across the engine's life.
    ///
    /// `0.0` before the first rebuild — the initial construction is not a
    /// rebuild and is not counted, since it happens once before playback.
    pub fn mean_build_ms(&self) -> f64 {
        let count = self.bus.build_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }
        self.bus.total_build_nanos.load(Ordering::Relaxed) as f64 / count as f64 / 1e6
    }

    /// How many generation builds have happened.
    pub fn build_count(&self) -> u64 {
        self.bus.build_count.load(Ordering::Relaxed)
    }
}

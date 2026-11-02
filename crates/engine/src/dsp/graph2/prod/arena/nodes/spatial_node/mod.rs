//! The SpatialNode: the spatial master output stage in the
//! production graph.
//!
//! The graph's canonical chain ends with a spatial stage that renders the
//! mixed master through the engine's spatial layer:
//!
//! ```text
//! … → seek_fade → spatial → (limiter/dither)
//! ```
//!
//! **What it does.** The master's front pair is treated as a two-object
//! "program" (prog-L at `center − half_width`, prog-R at
//! `center + half_width`, at `elevation`, radius 2 m), rendered by the
//! [`BinauralRenderer`] (head model) with the scene's room (early
//! reflections + late field) and listener orientation. This is the
//! "spatialize stereo" output stage: the dry image sits on the configurable
//! virtual screen, and the room wraps ambience around the listener.
//!
//! **Multichannel masters** (>2-channel blocks) are routed through a
//! **layout-driven renderer** ([`VbapRenderer`], selected from the graph's
//! configured [`ChannelLayout`]) rather than declined. Until 0.9.2 this node
//! had one renderer and a `planes.len() != 2` early return, so a 5.1 master
//! passed through *wholly* unprocessed — front pair included. The fix is not
//! a second panning round over the master (that would scramble an already
//! spatial signal): the master's channels are handed to the renderer as a
//! **bed**, so each is placed on the speaker carrying its semantic role. The
//! scene's two program objects, cue machinery, and voice budget keep their
//! stereo meaning and are not redefined for N inputs.
//!
//! A multichannel block whose width disagrees with the configured layout —
//! or arriving when no layout is known at all — still passes through
//! **bit-exact**. Guessing which of the two is authoritative would be worse
//! than declining.
//!
//! **Realtime discipline.** The scene, the renderers, and every scratch
//! plane are preallocated at construction/prepare; `process_block*` runs the
//! (already zero-allocation) renderer and copies the interleaved result back
//! — no allocation, no locks. The multichannel renderer is held behind a
//! `Box` so the node's *stack* footprint stays bounded: it was sized to hold a
//! full `VbapRenderer` inline until 0.9.2, which was enough to overflow a
//! default 2 MiB test thread. Control
//! commands (enabled / screen / room / listener) are plain-data
//! [`super::super::controls::NodeCmd`]s applied at the block boundary, like
//! every other node. Disabled (`enabled = false`, the default) the node
//! returns before touching a sample — bit-exact, so the equivalence suites
//! stay pinned.
//!
//! The scene is node-private; its two program objects carry the master's
//! front pair. The [`config::SpatialConfig`] section configures it at
//! construction/reconfig; the live control surface
//! (`GraphControlHandle::set_spatial_*`) changes it at runtime. The channel
//! layout is *not* part of `SpatialConfig` — it is a graph-level property
//! ([`DspGraph::set_multichannel_layout`]) threaded into
//! [`SpatialNode::apply_config`], because it describes the master, not the
//! spatial stage.
//!
//! ## — listener motion (v4.3.0)
//!
//! The listener is **runtime-movable** on the audio path: a
//! `SetSpatialListenerPose` control command sets a target pose
//! (orientation + position), and the node glides toward it every block
//! with the head-tracking conventions — shortest-arc nlerp on
//! orientation, linear one-pole on position, a configurable smoothing
//! time (0 ms = snap) and optional angular rate limit. The glide is
//! allocation-free and per-block, so a world-fixed image sweeps smoothly
//! as the listener rotates/moves (the VR seam, host-driven). The baked
//! acoustic seam re-bakes on the control thread when the listener
//! crosses a relevance bound (see [`SpatialNode::listener_rebake_due`]).

use super::super::node::DspNode;
use crate::buffer::{MAX_AUDIO_BLOCK_FRAMES, MAX_CHANNELS};
use crate::decode::ChannelLayout;
use crate::dsp::pipeline::{DspStageCapability, StageChannelSupport, StagePrecision};
use crate::spatial::{
    automation::CurveScalar,
    binaural::BinauralRenderer,
    hrtf::{HrtfDataset, HrtfProfile, HrtfProfileManager, DEFAULT_SPEED_OF_SOUND},
    level::DistanceModel,
    math::{Quat, Vec3},
    metering::SpatialMeterState,
    object::{ObjectId, MAX_SPATIAL_OBJECTS},
    quality::SpatialQuality,
    render::{HybridBlockInputs, SpatialRenderer},
    scene::SpatialScene,
    speaker::SpeakerLayout,
    tracking::TrackingConfig,
    vbap::VbapRenderer,
    voice::{BudgetCandidate, VoiceAdmission, VoiceBudget, VoicePriority},
};
use std::sync::Arc;

/// Program radius (m): the virtual screen sits 2 m in front, matching the
/// speaker presets' nominal radius.
const SCREEN_RADIUS: f32 = 2.0;

/// The program objects' authored parameters, snapshotted for
/// the render that carries a cue overlay and restored afterwards (plain
/// stack data — allocation-free on the audio path).
#[derive(Default)]
struct ProgramSnapshot {
    applied: bool,
    position: [Vec3; 2],
    gain: [f32; 2],
    spread: [f32; 2],
}

/// The SpatialNode (see the module docs).
pub struct SpatialNode {
    enabled: bool,
    /// The node-private scene: two program objects + room + listener.
    scene: SpatialScene,
    /// The scene-clock for cue evaluation (seconds, advanced
    /// per block by `frames / sample_rate`). Owned by the node so cue
    /// triggers are independent of the automation clock (which the host
    /// may drive explicitly via `set_automation_time`).
    cue_clock: f32,
    /// Modeled per-block render cost (cost units; refreshed
    /// on the control path via `refresh_cost_diagnostics`).
    last_cost_units: f32,
    /// Modeled budget utilization fraction.
    last_cost_utilization: f32,
    /// The render tail budget (blocks of headroom).
    tail_budget_blocks: f32,
    /// The head-model renderer (2-channel path).
    binaural: BinauralRenderer,
    /// Layout-driven renderer for multichannel masters (the >2-plane path).
    ///
    /// Added in 0.9.2. Previously the node had exactly one renderer and a
    /// `planes.len() != 2` early return, so a 5.1 master was neither
    /// spatialized nor routed — it passed through untouched, *including* its
    /// front pair. `VbapRenderer` was already implemented and tested as
    /// library code; what was missing was selecting it from the channel
    /// layout, which is what [`Self::set_multichannel_layout`] does.
    mc_renderer: Option<Box<VbapRenderer>>,
    /// Speaker geometry derived from [`Self::mc_layout`]. `None` until a
    /// layout is set, which is what keeps an unknown-width block bit-exact
    /// rather than guessed at.
    mc_speakers: Option<SpeakerLayout>,
    /// The channel layout the graph is configured for, if known.
    mc_layout: Option<ChannelLayout>,
    /// Bed carrying the master itself on the multichannel path. Routing the
    /// master as a *bed* rather than as N program objects is deliberate: the
    /// master's channels already sit on physical speakers, so the correct
    /// treatment is role-to-speaker routing (`render_beds`), not a second
    /// round of panning. It also means the scene's two program objects, cue
    /// machinery, and voice budget keep their existing stereo meaning instead
    /// of being redefined for N inputs.
    mc_bed: Option<crate::spatial::bed::BedId>,
    /// Whether `mc_renderer` is prepared against [`Self::mc_speakers`].
    mc_prepared: bool,
    /// Channel-major demotion scratch for the f64 multichannel path
    /// (`channels × MAX_AUDIO_BLOCK_FRAMES`). Preallocated so the audio thread
    /// never grows it.
    mc_planes: Vec<f32>,
    /// Front-pair program scratch (f32 planes).
    prog_l: Vec<f32>,
    prog_r: Vec<f32>,
    /// Interleaved render scratch (`channels × MAX_AUDIO_BLOCK_FRAMES`).
    out: Vec<f32>,
    /// Virtual-screen geometry (degrees / linear).
    center_azimuth_deg: f32,
    half_width_deg: f32,
    elevation_deg: f32,
    screen_gain: f32,
    /// Listener orientation (degrees), kept for introspection.
    listener_yaw_deg: f32,
    listener_pitch_deg: f32,
    listener_roll_deg: f32,
    /// Program object ids (L/R).
    obj_l: ObjectId,
    obj_r: ObjectId,
    sample_rate: f32,
    /// Voice budget (spec §76) honored from the node's config; `None` =
    /// engine default budgeting (no cap from config).
    voice: Option<VoiceBudget>,
    /// Live voice admission counts from the last `apply_voice_budget` (the
    /// audio thread), published into `PlaybackInfo` telemetry.
    voice_active: bool,
    voice_full: usize,
    voice_degraded: usize,
    voice_dropped: usize,
    // ── Runtime listener motion ──
    /// Smoothing policy for the listener glide (nlerp on orientation,
    /// one-pole on position). `smoothing_ms = 0` snaps.
    listener_tracking: TrackingConfig,
    /// Target listener orientation (world space; the glide goal).
    listener_target_quat: Quat,
    /// Target listener position (world space; the glide goal).
    listener_target_pos: Vec3,
    /// Whether a runtime motion target is active (the config-applied
    /// static orientation also seeds the target, so `apply_listener`
    /// and the glide converge on the same state).
    listener_motion_active: bool,
    /// Active HRTF profile manager (spec §4.6, Item 21).
    hrtf_manager: HrtfProfileManager,
    /// Sample rate at last successful `prepare` (re-prepare on change).
    prepared_rate: f32,
    prepared: bool,
}

impl DspNode for SpatialNode {
    fn capability(&self) -> DspStageCapability {
        DspStageCapability {
            name: "spatial",
            channel_support: StageChannelSupport::AllChannels,
            position: "post-mix, spatial master output",
            stateful: true,
            realtime_safe: true,
            bit_perfect_compatible: false,
            sample_rate_sensitive: true,
            precision: StagePrecision::Any,
        }
    }

    fn is_active(&self) -> bool {
        self.enabled && self.prepared
    }

    /// The node's private scene, its renderers and its program
    /// scratch planes.
    ///
    /// This is the graph's spatial memory, measured rather than estimated: the
    /// scene's voices, the binaural IR tables, the multichannel renderer's
    /// speaker/bed state, and the interleaved render scratch, at their
    /// allocated lengths.
    fn persistent_bytes(&self) -> usize {
        std::mem::size_of_val(self)
            + self.prog_l.capacity() * std::mem::size_of::<f32>()
            + self.prog_r.capacity() * std::mem::size_of::<f32>()
            + self.out.capacity() * std::mem::size_of::<f32>()
            + self.mc_planes.capacity() * std::mem::size_of::<f32>()
            + self.scene.heap_bytes()
            + self.binaural.heap_bytes()
            + self.mc_renderer.as_ref().map_or(0, |r| {
                r.heap_bytes() + std::mem::size_of::<Box<VbapRenderer>>()
            })
    }

    fn reset(&mut self) {
        // The renderer owns per-block state; nothing to clear here (the
        // scene parameters are user state, like volume).
    }

    fn prepare(&mut self, sample_rate: f32, _max_channels: usize) {
        self.sample_rate = sample_rate.max(1.0);
        if (sample_rate - self.prepared_rate).abs() > 1.0 || !self.prepared {
            // The binaural head model (stereo/headphone path). Its output width
            // is fixed at two by the head, so this layout is not a
            // configuration choice — see `set_multichannel_layout` for the
            // >2-plane path.
            let layout = SpeakerLayout::stereo();
            self.prepared = self
                .binaural
                .prepare(&layout, sample_rate.max(1.0) as u32)
                .is_ok();
            self.prepared_rate = sample_rate.max(1.0);

            // A rate change invalidates the multichannel renderer too, so
            // re-prepare it against the same layout it was built for. Skipped
            // when no layout is known, which keeps unknown-width blocks
            // bit-exact.
            if let Some(layout) = self.mc_layout.clone() {
                self.set_multichannel_layout(&layout);
            }
        }
        // Re-apply the geometric params so a rebuilt node carries them.
        self.apply_screen(
            self.center_azimuth_deg,
            self.half_width_deg,
            self.elevation_deg,
            self.screen_gain,
        );
    }

    fn process_block_f32(&mut self, planes: &mut [&mut [f32]]) {
        if !self.enabled || !self.prepared {
            return;
        }
        let frames = planes[0].len();
        if planes.len() == 2 {
            self.render_block(planes, frames);
        } else if planes.len() > 2 {
            // Multichannel: routed through the layout-driven renderer when a
            // layout is configured, bit-exact otherwise. The two-plane case
            // stays the binaural head model — that is the headphone path and
            // its output width is fixed at two by the head itself.
            self.render_block_multichannel(planes, frames);
        }
    }

    fn process_block_f64(&mut self, planes: &mut [&mut [f64]]) {
        if !self.enabled || !self.prepared {
            return;
        }
        let frames = planes[0].len();
        if planes.len() == 2 {
            self.render_block_f64(planes, frames);
        } else if planes.len() > 2 {
            self.render_block_multichannel_f64(planes, frames);
        }
    }
}

// The inherent impl is large enough to hide its seams. `engine.rs` owns
// construction, configuration and telemetry; `motion.rs` owns the screen,
// room, listener motion and the render entry points.
mod engine;
mod motion;

#[cfg(test)]
mod tests;

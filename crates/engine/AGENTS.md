# AGENTS.md — the engine's engineering rules

This file is the contract for changing the audio engine. It exists because the
engine is a **realtime** product: a change that is correct in a test can still
be wrong on the audio thread, and a change that improves a number can still
break a guarantee somebody else depends on.

The rules below are not style preferences. Each one exists because breaking it
produces a failure that a unit test will not catch.

---

## 1. The realtime contract

The audio thread is the one place where a bug is a *sound* bug, not a crash.

1. **No heap allocation on the audio thread.** Not "few allocations", none. The
   buffers, the DSP plan and the plan's scratch space are allocated during
   setup and never again. `tests/realtime_alloc_budget.rs` installs a counting
   global allocator and fails the build if this is broken — extend it rather
   than replacing it.
2. **No locks on the audio thread.** No mutexes, no `RwLock`, no `Condvar`, no
   `RefCell` that can be borrowed at the wrong moment. The communication
   primitives are the lock-free SPSC rings in `src/buffer/`.
3. **No blocking I/O on the audio thread.** No file reads, no allocation, no
   logging that touches a file, no `println!`. Diagnostics leave the audio
   thread through a lock-free queue and are drained elsewhere.
4. **No unbounded work per block.** A block must finish inside its deadline.
   A filter that suddenly costs ten times more is a dropout; the
   `spatial_cost` qualification check exists to catch that.
5. **Panics must not cross the FFI boundary.** Every `extern "C"` entry point
   is wrapped in `catch_unwind`. The same is true of the plugin host's
   callbacks.

## 2. The honesty contract

The engine's rule is: **never a guessed claim.** It extends to everything the
project says about itself.

1. **A capability is reported by the code, not by a string.** If the engine
   can decode a format, the registry says so *because the code can*, and the
   registry has a test that holds the two together. If it cannot, the failure
   is an explicit, typed error that names the format.
2. **A limitation is written down.** The
   [Known Limitations](README.md#-known-limitations) section is not a
   disclaimer; it is a list of the boundaries a user will hit. When a
   boundary moves, the section moves with it.
3. **A fallback is announced.** When a native backend cannot be opened, the
   engine falls back to shared mode and *says* — `OutputInfo::actual_backend`
   and `OutputAccessState` carry the truth, and the user interface shows it. A
   silent fallback that keeps the original claim is the worst outcome
   available.
4. **The documentation is generated or it is tested.** A number in a README
   that describes the tree is checked by a test
   (`tests/suite_manifest.rs`). A capability in the manual is rendered from
   the live engine. If you add a claim, add the check with it.

## 3. The DSP contract

1. **The reference pipeline is the oracle.** `src/dsp/pipeline/` is the
   definition of what the engine should do. The Graph 2.0 runtime must match
   it bit-for-bit, and `tests/bit_perfect_equivalence.rs` is what holds them
   together. When they disagree, the pipeline is right until proven otherwise.
2. **Precision is a choice, not an accident.** `f32` is the default for
   throughput; `f64` is available for mastering. A stage that silently
   changes precision is a bug.
3. **Filters are minimum-phase unless they say otherwise.** A linear-phase
   design is a deliberate, documented decision with its latency accounted for.
4. **The loudness numbers are the standards.** ITU-R BS.1770-5 and ReplayGain
   2.0 are implemented to the letter, and `tests/loudness_r128.rs` and
   `tests/replaygain.rs` check them against closed-form references.

## 4. Adding things

### A codec

1. Add the `Codec` variant and its extension mapping in
   `src/decode/codecs.rs`.
2. Add the decoder, or declare the format as *named but not decodable* — with
   an explicit error. Do not leave a format that half-works.
3. Add a fixture to `tests/codec_matrix.rs` that proves the claim, and a
   decode test that proves the audio.
4. If the decoder is a new dependency, justify it in the manifest comment the
   way `opus-decoder` and `wavicle` are justified: what it is, why it is
   needed, and what upstream bug it works around.

### A backend

1. Implement the `Output` trait. The trait's `output_info()` must report the
   access mode **honestly** — shared, exclusive, or exclusive-and-verified.
2. Add the backend to `available_backends()` **behind its compile-time
   feature**, so a build never offers a backend it cannot open.
3. Enumerate your own devices. A device list borrowed from another backend is
   a device list that names the wrong thing.
4. Add the backend to `src/output/backend.rs`'s dispatch, with an honest
   fallback and a log line that says which path was taken.

### A DSP stage

1. Add it to the graph's typed-port topology, not as a special case in the
   pipeline.
2. Give it a place in the reference pipeline, or state why it is not there.
3. Extend the allocation budget test if it allocates during setup.
4. Benchmark it. `benches/` is where the cost of a stage becomes visible.

## 5. Before you commit

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo bench --no-run
```

If the change touches a claim — a README number, a capability list, a
limitation — the test that guards that claim must change in the same commit.
A claim that is true and a test that proves it are one change, not two.

## 6. Module layout: one owner per concept

The engine tree is large, and several concepts are legitimately implemented
twice (an offline path and a realtime path). The rule is that the *second*
implementation must be a documented seam, never an accident. This section is
that documentation; a change that adds a third implementation, or that moves a
boundary named here, updates this section in the same commit.

### 6.1 The two DSP execution models (reference pipeline vs Graph 2.0)

- **`src/dsp/pipeline/` is the frozen reference oracle**, not a runtime
  fallback. The audio path routes through Graph 2.0 (`src/dsp/graph2/`, lowered
  from a `ChainRequest`); the pipeline exists so
  `tests/fidelity/graph_pipeline_equivalence.rs` and
  `benches/dsp_pipeline.rs` can pin the production chain against a second,
  linear implementation. Removing it would remove the bit-exactness oracle.
- **New DSP stages go into Graph 2.0 only.** Adding a stage to the pipeline is
  allowed only to keep the oracle honest about a stage the graph already has.
- `dsp::pipeline::format` also owns the output-status vocabulary the whole
  crate reports through — `OutputSampleFormat`, `BitPerfectReport`,
  `BitPerfectResult`, `VolumePath`, `LatencyReport`, `EngineStats`. That is a
  public API surface; extracting it into its own module is an API-breaking
  change and is deliberately **not** mixed into structural refactors.
- `src/dsp/graph2/prod/` names its control modules by role, not by number:
  `control_handle.rs` (the cloneable cross-thread surface) and
  `control_mutators.rs` (the queued `set_*` mutators on `Graph2Engine`). A
  bare `control.rs` next to `controls.rs` is not allowed again.

### 6.2 Room acoustics: modelling vs measurement vs correction

Three formerly confusable modules now name their job:

- **`src/spatial/room_model/`** (was `spatial/acoustic/`) — geometry, materials,
  ray/beam solving, and the baked scene the renderer consumes.
- **`src/spatial/acoustic_metrics/`** (was `spatial/acoustics/`) — measurement
  analysis: impulse responses, RT60/EDT/ETC/clarity, transfer and phase
  response, MLS and sweep stimuli.
- **`src/dsp/correction/`** — the per-channel (1D) correction node: IR
  windowing, phase/magnitude derivation, and the FIR/IIR it feeds.
- **`src/spatial/room_correction/`** — the multi-point workflow above it:
  measurement capture, spatial averaging, target curves, filter synthesis.

`room_correction` reuses the primitives from `acoustic_metrics` and
`dsp::correction`; it must not re-derive one of them.

**Sweep generation and the matched inverse filter have exactly one
implementation:** `src/dsp/correction/sweep.rs` (`EssSweep`,
`farina_inverse_filter`, `deconvolve_direct` / `deconvolve_direct_raw`).
Everything else is a thin `f32` adapter over it:
`spatial::acoustic_metrics::sweep` (analysis metrics) and
`spatial::room_correction::measurement` (the capture wizard, which keeps a
unit-*peak* inverse filter and truncates the IR to the recording length).
Neither adapter may grow its own chirp law, inverse-filter envelope, or
convolution loop; a change belongs in the canonical kit, and each adapter's
`generation_is_the_canonical_sweep_converted_to_f32` equivalence test must
keep holding. No new sweep generator may be added anywhere in the engine.

### 6.3 Signal analysis is engine-owned

FFT, windowing, spectral/temporal descriptors, crest factor, and EBU R128
loudness have exactly one canonical implementation:
`src/dsp/analysis/` (descriptors) and `src/dsp/loudness/` (R128 meter).
`src/dsp/analyzer.rs` and `src/profile/` compose those primitives for the
profile cache; they may not grow their own FFT or loudness loop.

The outer workspace (`recommendation`, `media-tools`, `library`) cannot reach
these modules: `audio-session` is the only crate allowed to speak to the
engine, so player-side analysis must consume results through that boundary
rather than linking engine DSP code. That is why a similar-looking feature
loop exists there — it is an *application* policy layer, and it must state in
its module docs which engine feature it approximates if it does.

### 6.4 `standards/` defines specifications, never processing

`src/standards/` holds specification *vocabulary* only: standard identifiers,
compliance thresholds and windows, ADM document shape, channel-layout and
coordinate-system names. It contains no DSP and no I/O, and the implementations
elsewhere (`dsp::loudness`, `dsp::true_peak`, `spatial::adm`,
`decode::channel_layout`) import from it rather than hard-coding the numbers.
If a constant is needed by exactly one implementation and the specification
names it there, the implementation may own it — but the two copies must not
disagree, and a test must hold them together.

### 6.5 What the engine owns vs what the player owns

The engine is an independently versioned workspace with no dependency on the
player crates. This is deliberate, and it fixes ownership of the subsystems
that appear on both sides:

| Concept | Engine owner | Player owner |
| --- | --- | --- |
| File identity / acoustic fingerprint | `decode::fingerprint` (stream-side, for the CLI and duplicate detection during decode) | `library::fingerprint` + `domain::file_identity` (scanner-side identity; the only implementation that touches the library DB) |
| Playlist parsing | `playlist::io` (engine CLI interchange) | `library::m3u` / `playlist_format` (import into the library) |
| CUE sheets | `decode::cue` + `engine::cue_split` (splitting a stream for playback) | `domain::cue` (a parsed, pure model the library stores) |
| Metadata / tags | `decode::tags` (what the decoder needs mid-stream) | `metadata::extractor` (what the library stores) |
| MPRIS | `mpris/` (an engine-only CLI/test server) | `ui::services::mpris_service` (the shipped desktop integration; the engine server is not started by the app) |

The rule: a subsystem is *not* ported across the boundary because it looks
similar. It is ported only if one side can be deleted. When both are needed,
each side documents its consumer, as this table does.

### 6.6 Engine state: one file, one writer

Live control state is `EngineConfig` (control thread, in memory).
Durable engine state is narrow: `DspState` (saved EQ/limiter/output
selection), owned and written only by
`src/engine/dsp_persistence.rs` (atomic temp-file + rename, save-if-changed),
and the spatial equivalent in `spatial_persistence.rs`. The engine never
writes player settings and the player never writes the engine's state file;
`ui/src/app/settings_io.rs` owns the player's own settings rows. On startup the
player loads its rows and then hands the engine a configuration; it does not
replay a second copy of engine settings.

### 6.7 Diagnostics have one sink

`src/diagnostics/` (events + health) is the single diagnostic vocabulary.
`dsp::graph2::diagnostics`, `spatial::health`, and `spatial::diagnostics`
produce reports that are translated into it for `engine::telemetry`; none of
them may define a parallel event type that the telemetry pipeline cannot
carry.

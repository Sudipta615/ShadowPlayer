# Architecture

This document is the map: which crate owns what, how a sample gets from a file
to a DAC, and which rules hold the realtime path together. The engine's
detailed engineering contract lives in
[`crates/engine/AGENTS.md`](../crates/engine/AGENTS.md); this page is the
cross-workspace view.

## Two workspaces

The repository contains two independent Cargo workspaces.

| Workspace | Root | Members |
| --- | --- | --- |
| Player | `/` | `domain`, `audio-session`, `database`, `metadata`, `lyrics`, `search`, `library`, `playback`, `recommendation`, `ui`, `app`, `media-tools`, `clap-fixture-plugin` |
| Engine | `crates/engine` | `engine` (lib name `engine`, package `audio-engine`), `config`, `plugin-abi`, `opus-decoder` |

The engine is deliberately **not** a member of the player workspace. It is
independently versioned, publishable as a standalone realtime audio library,
and has no dependency on player crates. The player consumes it as a path
dependency, but only through `audio-session` (see the ownership table below).

```
┌──────────────────────────────────────────────────────────────────┐
│ Player workspace                                                 │
│                                                                  │
│  app ──► ui ──► audio-session ──► engine (path dependency)       │
│           │            │                                         │
│           ├──► library ──► database ──► domain                   │
│           ├──► metadata / lyrics / search                        │
│           └──► recommendation / playback / media-tools           │
└──────────────────────────────────────────────────────────────────┘
```

## Runtime shape

```
 UI thread            control thread              audio / tick thread
─────────            ──────────────              ───────────────────
 user input  ──────► EngineHandle  ──command──►  AudioEngine
                        │  queue                  │
                        │                         ├─ decode thread (file I/O)
                        │                         ├─ DSP graph (realtime)
                        │                         └─ output backend
                        ◄──── lock-free telemetry ───────┘
```

- **Control thread** owns `EngineConfig`, the playlist and the device
  lifecycle. Allocation and blocking I/O are allowed here.
- **Audio/tick thread** runs the graph and pushes blocks to the output. It is
  allocation-free, lock-free and does no file or device I/O (engine AGENTS
  §1).
- **Decode thread** reads files and fills the SPSC ring consumed by the audio
  thread.
- **UI thread** never touches engine internals directly; it sends
  `EngineCommand`s through `EngineHandle` and reads telemetry snapshots.

The communication primitives are the lock-free SPSC rings in
`crates/engine/src/buffer/`. Diagnostics leave the audio thread through a
lock-free queue and are drained on the control side.

## Signal path

```
AudioSource
   │
   ▼
Decoder (Symphonia / native DSD / APE / Opus / TTA / WavPack)
   │  PCM f32/f64, or DSD (native / DoP)
   ▼
Resampler (only when the device rate differs; rubato or polyphase)
   │
   ▼
Graph 2.0
   ├─ volume / channel trim / balance
   ├─ parametric EQ, graphic EQ, dynamic EQ
   ├─ room correction (FIR/IIR)
   ├─ crossfeed / bass enhancement / dynamics
   ├─ spatial renderer (HRTF, VBAP, ambisonic, room model)
   └─ final safety limiter
   │
   ▼
Output backend (cpal shared, WASAPI exclusive, CoreAudio hog, ASIO, null)
   │
   ▼
DAC
```

The **reference pipeline** in `crates/engine/src/dsp/pipeline/` is the oracle:
Graph 2.0 must match it bit-for-bit, and
`crates/engine/tests/fidelity/graph_pipeline_equivalence.rs` holds the two
together. New stages go into Graph 2.0 only; the pipeline changes only to stay
honest about a stage the graph already has.

## Ownership boundaries

| Concept | Engine owner | Player owner |
| --- | --- | --- |
| File identity / fingerprint | `decode::fingerprint` (stream-side) | `library::fingerprint` + `domain::file_identity` (scanner-side, DB) |
| Playlist parsing | `playlist::io` (CLI interchange) | `library::m3u` / `playlist_format` (library import) |
| CUE sheets | `decode::cue` + `engine::cue_split` | `domain::cue` (stored model) |
| Tags | `decode::tags` (mid-stream) | `metadata::extractor` (library) |
| MPRIS | `mpris/` (engine-only test server) | `ui::services::mpris_service` (shipped integration) |

A subsystem is not ported across the boundary because it looks similar. It is
ported only if one side can be deleted; otherwise both sides document their
consumer, as this table does.

`audio-session` is the only player crate allowed to speak to the engine. The
outer crates (`recommendation`, `media-tools`, `library`) consume analysis
results through that boundary rather than linking engine DSP code.

## State

| State | Owner | Storage |
| --- | --- | --- |
| Library, tracks, playlists, analysis | Player | SQLite via `database` |
| Player settings | Player | `ui::app::settings_io` rows |
| Live engine configuration | Engine control thread | in memory (`EngineConfig`) |
| Durable engine state (EQ/limiter/output, spatial) | Engine | `engine::dsp_persistence` / `spatial_persistence`, atomic temp-file + rename |

The engine never writes player settings and the player never writes the
engine's state file. On startup the player loads its rows and hands the engine
one configuration; it does not replay a second copy.

## Realtime rules

These are the invariants a test can catch and the ones it cannot:

1. No heap allocation, locks, blocking I/O or unbounded work on the audio
   thread. `crates/engine/tests/realtime_alloc_budget.rs` installs a counting
   allocator and fails the build if allocation creeps in.
2. Panics must not cross the C ABI or the plugin host's callbacks; every
   `extern "C"` entry point is wrapped in `catch_unwind`.
3. Precision is a choice, not an accident: `f32` for throughput, `f64` for
   mastering. A stage that silently changes precision is a bug.
4. A capability is reported by the code, not by a string. The codec registry
   and the decode dispatch are held together by a test.
5. A fallback is announced: `OutputInfo::actual_backend` and
   `OutputAccessState` carry the truth when an exclusive backend falls back to
   shared mode.

## Testing map

| Concern | Where |
| --- | --- |
| Player units | `crates/*/src/**` `#[cfg(test)]`, `crates/*/tests/` |
| Engine units + integration | `crates/engine/src/**`, `crates/engine/tests/` |
| Graph/pipeline bit-exactness | `crates/engine/tests/fidelity/` |
| Realtime allocation | `crates/engine/tests/realtime_alloc_budget.rs` |
| 100k-track budgets | `crates/ui/tests/library_budget.rs` |
| Documentation claims | `crates/app/tests/docs_truth.rs`, `crates/engine/tests/suite_manifest.rs` |
| Fuzz | `crates/engine/fuzz/` (scheduled) |
| Sanitizers | CI `sanitizers` job (AddressSanitizer on the unsafe DSP paths) |

## Where to change what

| Change | Start here |
| --- | --- |
| A codec | `crates/engine/src/decode/codecs.rs`, then a decoder + fixture |
| An output backend | `crates/engine/src/output/`, then `output/backend.rs` dispatch |
| A DSP stage | Graph 2.0 topology + the reference pipeline if the oracle needs it |
| A UI screen | `crates/ui/src/components/`, strings in `locales/en.json` |
| A library query | `crates/database/src/`, exposed through `crates/library/` |
| A recommendation signal | `crates/recommendation/` (on-device, no network) |

When a change moves one of these boundaries, update this file and the engine's
`AGENTS.md` §6 in the same commit.

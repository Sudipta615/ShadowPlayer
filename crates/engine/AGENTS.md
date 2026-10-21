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
4. Add the backend to `src/output/output.rs`'s dispatch, with an honest
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

<div align="center">

# Audio Engine

### Independent, Headless, Bit-Perfect Audiophile Audio & DSP Engine built with pure Rust

[![Version](https://img.shields.io/badge/version-1.10.0-blue.svg?style=flat-square&logo=rust)](Cargo.toml)
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg?style=flat-square)](LICENSE-APACHE)
[![MSRV](https://img.shields.io/badge/MSRV-1.98-orange.svg?style=flat-square&logo=rust)](Cargo.toml)
[![Platforms](https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey.svg?style=flat-square)](#-hardware-output-backends--os-integration)
[![Realtime Safety](https://img.shields.io/badge/realtime-0%20allocations%20hot%20path-brightgreen.svg?style=flat-square)](#-real-time-safety--concurrency)
[![Qualification Gate](https://img.shields.io/badge/qualification-100%25%20PASS-brightgreen.svg?style=flat-square)](#-formal-release-qualification)
[![Test Matrix](https://img.shields.io/badge/tests-24%20suites-blueviolet.svg?style=flat-square)](#-testing--quality-gates)

<p align="center">
  <b>A zero-allocation, lock-free, node-based audio playback and DSP engine engineered for bit-perfect audiophile listening, pro-audio mastering, and low-latency studio monitoring — with zero C/C++ codec SDKs.</b>
</p>

---

[Key Highlights](#-key-highlights) •
[Architecture](#-architecture-at-a-glance) •
[Signal Flow](#-dsp-signal-chain) •
[Real-Time Safety](#-real-time-safety--concurrency) •
[Live Qualification](#-formal-release-qualification) •
[Quick Start](#-quick-start) •
[Known Limitations](#-known-limitations) •
[TUI & CLI](#-interactive-terminal-ui--cli) •
[C-FFI](#-c-ffi-embedding) •
[Configuration](#-configuration-model) •
[Codecs & DSD](#-codecs-dsd--formats) •
[Backends](#-hardware-output-backends--os-integration) •
[Docs](#-documentation-index)

</div>

---

## 📖 Overview

**Shadow Desktop** is an independent, headless core audio playback and DSP engine written in Rust. It enforces strict separation of concerns: **zero UI dependencies, zero database/library ties, zero playlist policy, and zero OS-specific application assumptions**.

It is built from the ground up for critical listening environments, professional workstations (DAWs), embedded playback appliances, and desktop applications (Slint, Iced, Qt, GTK, egui, Tauri). It also ships an optional **C-ABI FFI** (`c-ffi`) with complete panic containment (`catch_unwind`) and runtime ABI version querying (`engine_abi_version()`), allowing native integration with C, C++, Python, C#, and Node.js.

> [!IMPORTANT]
> **Read Before Relying on Headline Claims**:
> The engine is designed with engineering honesty. Read [Known Limitations](#-known-limitations) to see exact hardware, codec, and spatial boundary contracts before designing against specific assumptions.

---

## 📚 Documentation Index

There is no `docs/` directory in this repository. An earlier revision of this
table linked `docs/GETTING_STARTED.md`, `docs/ENGINE_SPEC.md`,
`docs/OWNERS_GUIDE.md`, `docs/ARCHITECTURE.md`, `docs/SIGNAL_FLOW.md`,
`docs/EMBEDDING.md`, `docs/LICENSES_AND_ATTRIBUTION.md`, `docs/HISTORY.md`,
`AGENTS.md` and `CONTRIBUTING.md`; none of those files were ever committed, so
every one of those links was a dead end. Rather than keep pointing at them, this
is what actually documents the engine:

| Where | What it covers |
| :--- | :--- |
| This README | Overview, architecture, DSP chain, backends, quality gates, known limitations |
| `cargo doc -p audio-engine --no-deps --open` | Per-module and per-item contracts, invariants and examples straight from the source |
| The source itself (`src/**`) | The specification is in the module docs: precision tiers, real-time rules, ownership |
| `tests/` and `benches/` | Executable contracts: fidelity, robustness, real-time allocation budget |
| `../..` (repository root) | The desktop player built on this engine, its build steps and shortcuts |

---

## ✨ Key Highlights

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│  PURE RUST AUDIO STACK   │  GRAPH 2.0 RUNTIME CORE    │  REAL-TIME LOCK-FREE HOT PATH            │
│  No C/C++ codec SDKs     │  Typed-port topology       │  0 heap allocations in steady state      │
│  Native DSD512 + PCM     │  Live block-boundary swaps │  Cache-padded SPSC rings + CAS buffers   │
├──────────────────────────┼────────────────────────────┼──────────────────────────────────────────┤
│  BIT-PERFECT INTEGRITY   │  MULTI-ENDPOINT MATRIX     │  MASTERING-GRADE DSP                     │
│  Per-stage cause codes   │  Simultaneous physical DACs│  ITU-R BS.1770-5 loudness & true-peak    │
│  Hardware exclusive modes│  Crystal drift resamplers  │  64-band parametric EQ & AutoEQ presets  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

| Subsystem | Architectural Highlights |
| :--- | :--- |
| **Pure-Rust Decode & DSP** | Zero C/C++ codec SDKs, no external wrapper shims, and no build-time codegen. Memory-safe and auditable throughout. *(OS audio backend drivers reach system APIs: ALSA `libasound` on Linux; WASAPI COM / ASIO on Windows; CoreAudio ObjC on macOS)*. |
| **Minimal, Auditable Unsafe** | Unsafe code is strictly quarantined to `src/dsp/simd/` (`std::arch` runtime-detected intrinsics with bit-exact scalar fallbacks) and ring-buffer slice reconstruction. No unsafe FFI touches the DSP hot path. |
| **Graph 2.0 DSP Core** | Production hot path runs on an arena graph with **compiled execution plans lowered from typed-port Graph 2.0 topology**. Full reconfigurations swap atomically at block boundaries with **zero allocation and zero locks** on the audio thread. |
| **Per-Stage Bit-Perfect Auditing** | `DspPipeline::bit_perfect_report_with_format` diagnoses each stage individually and reports the exact condition invalidating bit-perfection from an ordered `BitPerfectCause` enum. Exposed across FFI as `bit_perfect_cause`. |
| **N-Input Mix Bus & Aux Bus** | Primary playback stream, crossfade partner, and independent lane tracks ride dedicated slots with per-slot trim, pan, ducking, and automation. A dedicated Aux Bus provides automatable sends with an optional convolution insert. |
| **Multi-Endpoint Output Matrix** | Simultaneously fan out the master mix across multiple physical audio interfaces. Each endpoint runs its own realtime worker, private ring buffer, and independent Rubato `Slip` drift-correction resampler. |
| **Bit-Perfect Hardware Endpoints** | Verified native exclusive backends: ALSA direct `hw:` / `plughw:`, Windows WASAPI Exclusive (`IAudioClient`), native Steinberg ASIO (`IASIO` COM in pure Rust, no SDK needed), and macOS CoreAudio Hog Mode. |
| **Dual-Precision Engine** | Selectable per-session precision: fast single-precision **f32** (Performance) or double-precision **f64** (Quality) mastering-grade computation across all filters and math stages. |
| **Comprehensive Audiophile Formats** | Native support for FLAC, ALAC, WAV, AIFF, APE, WavPack, TTA, Opus, Vorbis, AAC, MP3, and native **1-bit DSD (DSF/DFF)** up to DSD512 over Native wire and DoP (DSD-over-PCM). |
| **Spatial 3D Audio & Binaural HRTF** | World-space objects, diffuse beds, room acoustics, 3D VBAP, Ambisonics (HOA Orders 1–3), and Binaural HRTF rendering with Woodworth ITD, Duda-Martens head shadowing, and spectral SOFA dataset support. |
| **Professional Loudness & Tagging** | ITU-R BS.1770-5 / EBU R128 and ReplayGain 2.0 analysis, volume normalization, and metadata tag write-back (`tag-write` via `lofty`) across standard containers. |
| **Plugin Architecture** | C-ABI plugin spec (`crates/plugin-abi`) supporting in-process effects via `dlopen` or a static registry. Fault containment (panic boundary, watchdog, NaN scan, dry-audio failover) exists for trusted plugins but is **not** an isolation boundary — see `SECURITY.md` §2 before loading anything you do not trust. |

---

## 🏗 Architecture at a Glance

The engine is structured around strict thread boundaries, lock-free communication channels, and atomic generation publishing:

```
                      Host Application (GUI / CLI / C-ABI Host / Daemon)
                           │  EngineCommand (One-way non-blocking control)
                           │  EngineEvent / OutputEvent (Lifecycle broadcast)
                           ▼
                      EngineHandle ──────────────────────────────┐
                           │                                     │ lock-free
                           ▼                                     ▼
                    Command Channel                      ArcSwap<PlaybackInfo>
                    (Crossbeam SPSC)                      (Atomic telemetry)
                           │
                           ▼
 ┌─────────────────────────── AUDIO ENGINE CORE ───────────────────────────────────┐
 │                                                                                 │
 │   ┌─────────────────┐       ┌─────────────────┐       ┌──────────────────────┐  │
 │   │   Decode Loop   │ ────▶ │  N-Slot Mix Bus │ ────▶ │  Graph 2.0 DSP Core  │  │
 │   │ (Decoders, SPSC,│       │ (Primary track, │       │ (Compiled plan: mix, │  │
 │   │  resamplers)    │       │  lanes, ducking)│       │  aux, EQ, dynamics,  │  │
 │   └─────────────────┘       └─────────────────┘       │  spatial, limiter)   │  │
 │                                                       └──────────────────────┘  │
 │                                                                   │             │
 │                                                                   ▼             │
 │   ┌──────────────────────────────────────────────────────────────────────────┐  │
 │   │                        Multi-Endpoint Routing Matrix                     │  │
 │   │   Each endpoint: SPSC ring ──▶ Rate Resampler ──▶ Slip Drift Correction  │  │
 │   └──────────────────────────────────────────────────────────────────────────┘  │
 └───────────────────────────────────────┬─────────────────────────────────────────┘
                                         │ Lock-free fan-out
                                         ▼
                   Primary DAC & Secondary Physical Endpoints
        (ALSA Direct ─ WASAPI Exclusive ─ ASIO Native ─ CoreAudio Hog ─ CPAL)
                                         │
                                         ▼
                               Physical Audio Output
```

### Architectural Guarantees

- **No Hot-Path Locks**: Sample blocks flow through cache-line-padded single-producer single-consumer (`PcmRingBuffer`) queues. Never a mutex on the audio path.
- **Dedicated Engine Worker Thread**: A single worker thread drives `AudioEngine::tick_blocking(timeout)`, sleeping when idle and waking immediately on commands.
- **Glitch-Free Generation Swaps**: Reconfigurations build a fresh `GraphGeneration` on the control thread and publish it via an atomic pointer swap at block boundaries.
- **Isolated Secondary Endpoints**: Physical endpoints run independent worker threads, drift controllers, and SPSC rings. A slow or stalled secondary device never stalls the primary audio stream.

---

## 🔒 Real-Time Safety & Concurrency

The audio hot path conforms to strict realtime safety standards:

1. **Zero Steady-State Heap Allocations**: All scratch buffers, mix planes, node arena cells, plan sets, and filter delay lines are pre-allocated during initialization. Reconfigurations prepare memory entirely on the control thread before swapping pointers.
2. **Lock-Free Concurrency**: Audio samples move across padded SPSC queues. Control changes ride lock-free command channels applied at block boundaries. Telemetry is published lock-free via `ArcSwap<PlaybackInfo>`.
3. **Audited Intrinsics**: SIMD acceleration (AVX-512, AVX2+FMA, SSE2, ARM NEON) uses explicit target feature gates and carries bit-exact scalar reference fallbacks.
4. **Float Sanitization & Denormal Prevention**: Inline containment cleanses non-finite samples (`NaN` and `Inf`) and applies FTZ (Flush-To-Zero) / DAZ (Denormals-Are-Zero) to prevent CPU cycle spikes from denormal floats.

---

## 📊 Formal Release Qualification

The engine carries an in-process, reference-based qualification pipeline. It measures the engine against
a registry of reference vectors — not against itself — and every check states what it measured, what it
expected, and why that expectation exists.

Run it with:

```bash
cargo run --bin audio-engine-cli --features audio-output -- selftest
```

The command exits non-zero if any check fails, so it can gate a release.

### Live Qualification Benchmark

The block below is a **captured run of `selftest`, taken on 1.4.0**. It is reproduced verbatim rather
than re-labelled: the engine version in the output is the one that actually produced these numbers.

```text
Quality evaluation report — engine 1.4.0 (reference registry v1)
──────────────────────────────────────────────────────────────────
Component: DspPipeline (bit-exact passthrough) (dsp-pipeline@1)
  PASS  bit_exactness              measured 0.00e0 mismatch      expect 0.00e0 mismatch ± 0.00e0 mismatch  — full-scale passthrough must be bit-identical
  PASS  thd_plus_n                 measured 0.000 (0.0%)         expect ≤ 0.001 (0.1%)  — a pure path adds no measurable distortion

Component: Parametric EQ biquad (parametric-eq@1)
  PASS  freq_response_deviation_db measured 6.00 dB              expect 6.00 dB ± 0.35 dB  — peaking centre gain reaches +6 dB
  PASS  freq_response_deviation_db measured 0.08 dB              expect 0.00 dB ± 0.30 dB  — far-band magnitude stays at unity
  PASS  phase_deviation_deg        measured 0.00 deg             expect 0.00 deg ± 6.00 deg  — phase crosses 0 at the resonance centre

Component: Lookahead limiter (limiter@1)
  PASS  true_peak_error_db         measured -0.00 dB             expect 0.00 dB ± 0.10 dB  — true peak stays at the −1 dBFS ceiling

Component: Resampler (48→44.1 kHz) (resampler@1)
  PASS  freq_response_deviation_db measured 0.00 dB              expect 0.00 dB ± 0.50 dB  — in-band tone survives at unity gain

Component: Binaural head model (binaural@1)
  PASS  interaural_level_db        measured 0.00 dB              expect ≤ 3.00 dB  — a centered image is heard by both ears near-equally
  PASS  interaural_level_db        measured 5.04 dB              expect ≥ 4.00 dB  — a far-panned source produces a large inter-aural level difference

Component: EBU R128 loudness (loudness@1)
  PASS  loudness_error_lufs        measured -0.04 LUFS           expect -0.02 LUFS ± 0.25 LUFS  — calibration tone reads near −0.02 LUFS

Component: Convolution (partitioned FFT) (convolution@1)
  PASS  acoustic_ir_error_db       measured -60.00 dB            expect ≤ -60.00 dB  — engine matches the naive direct convolution to −60 dB or better

Component: Channel separation (channel-separation@1)
  PASS  channel_separation_db      measured -200.00 dB           expect ≤ -100.00 dB  — driving one channel produces no cross-channel leakage

Component: HRTF interpolation (hrtf@1)
  PASS  hrtf_interpolation_error_db measured 0.33 dB              expect ≤ 1.00 dB  — interpolated response stays within its bracketing grid nodes

Component: Spatial render-cost model (spatial_cost@1)
  PASS  spatial_render_cost_units  measured 2.00 units           expect ≤ 2.00 units  — the default stereo program models at 2 cost units per block
  PASS  spatial_cost_utilization   measured 0.50 frac            expect ≤ 0.60 frac  — the default program stays within the block budget

──────────────────────────────────────────────────────────────────
Summary: 10 components · 15 checks — 15 PASS, 0 FAIL (ALL PASS)
```

### What the qualification pipeline does not cover

It runs headless. It does not open a physical DAC, and it does not measure
jitter, drift or the analogue stage — see the
[Known Limitations](#-known-limitations) below, which are stated rather than
implied away.

---

## 🎛 DSP Signal Chain

The production engine executes a pre-allocated plan lowered directly from the Graph 2.0 topology. All processing operates in-place:

```text
Decoded Audio Frames
  │
  ├── Multichannel Routing & Bass Management (LFE crossover, channel delay, trim)
  │
  ├── Mix Bus Stage
  │    ├── Per-slot gain trim, balance/pan, and mute
  │    ├── Per-slot loudness normalizer (ITU-R BS.1770-5 / ReplayGain)
  │    ├── Program-gated ducking & sample-accurate automation curves
  │    └── Post-fader sends (Master Send & Aux Send)
  │
  ├── Aux Bus Node
  │    ├── Summation of all slot aux sends
  │    ├── Per-send automation & ducking
  │    └── Optional Convolution Insert (Reverb / Cabinet IR) returned to Master
  │
  ├── Acoustic Room & Headphone Correction Node (FIR/IIR calibration curves)
  ├── 64-Band Parametric Equalizer (+ AutoEQ headphone database presets)
  ├── EQ Width Models (Q · `BW Oct` · `BW Hz` · shelf slope `S`, Nyquist-exact octave bands)
  ├── Graphic Equalizer Layer (10, 15, or 31 ISO standard bands)
  ├── 3-Band Multiband Compressor (Independent crossovers, attack, release, ratios)
  ├── Partitioned FFT Convolution (Zero-latency impulse response modeling)
  ├── Headphone Crossfeed (Bauer, Chu Moy, Jan Meier, or custom profiles)
  ├── Mid-Side Stereo Enhancer & Channel Balance
  ├── WSOLA Time-Stretch & Pitch-Shift (Varispeed, TimeStretch, PitchShift)
  ├── Perceptual Logarithmic Volume (dB curve with click-free sample smoothing)
  ├── Seek & Track Transition Fader (Micro-fade suppression of discontinuities)
  │
  ├── Spatial Master Output Stage (Opt-in binaural HRTF head model & 3D room)
  │
  └── Output Domain Processing
       ├── High-Performance Sinc Resampling (Rubato FFT sinc filter tiers)
       ├── 4× Oversampling True-Peak Lookahead Limiter (Inter-sample peak guard)
       ├── Triangular Probability Density Function (TPDF) Dither
       └── Master SPSC Ring Buffer ──▶ Dispatched to Output Matrix & Physical DACs
```

### Hardware Bypass Modes

- **Bit-Perfect Direct**: Bypasses all DSP filtering, EQ, dynamics, and resampling. Only unity-gain seek fades and essential volume smoothing apply.
- **DSD-over-PCM (DoP) Bypass**: Total bit-transparent passthrough. Raw 24-bit DoP frames stream directly to the hardware DAC without attenuation or alteration.

---

## 🧩 Subsystems With No Desktop UI

The engine exports more than the ShadowPlayer desktop app drives. That is
deliberate — the crate is a library for embedders, appliances and DAWs — but it
is worth stating plainly which parts have **no** caller in this repository, so
nobody assumes a feature is wired up because the module exists:

| Subsystem | Status for the desktop app |
| :--- | :--- |
| `network_audio` | AES67 / RTP receive and send. No UI; drive it from an embedding host. |
| `mpris` (feature `mpris`) | **Not used by the app.** It is gated behind a feature the app does not enable. The desktop app implements MPRIS2 itself (`crates/ui/src/services/mpris_service.rs`) because transport state lives in the app's coordinator, not in the engine — exposing it from the engine would have two owners of the same D-Bus name. |
| `profile` | AutoEq / headphone target analysis. The app's AutoEq path applies the resulting filters through the DSP chain instead. |
| `dsp::filter_design` | Matched (Vicanek) biquad design, Butterworth biquad ladders, and the Nyquist-exact octave-width model. The desktop app does not drive these directly; the width→Q conversions are already on the EQ-file import path, and a host reaches the models through `ParametricEq::set_bandwidth_mode` / `set_band_width`. |
| `eval`, `governance`, `standards` | Engineering-process suites: qualification runs, standards conformance, release gates. Used by the engine's own test binaries. |
| `fx` | Modulation effects (chorus, flanger, phaser, delay, distortion). Built and tested, not yet surfaced in the app's DSP panel. |
| `buffer`, `track_cache`, `state` | Internal building blocks used by other engine modules; not host-facing. |

Anything in this table is engine-only until a host wires it up. Adding it to the
UI is a feature, not a fix, and this file is the place to say so.

---

## ⚠️ Known Limitations

We state limitations clearly so no architectural contract is misunderstood:

> [!NOTE]
> **Four of the six limitations documented before 0.9.2 were closed in 0.9.2.**
> What remains is listed here in full, so the current contract is in one place.

> [!WARNING]
> - **No Musepack or TAK decoding, and no plan to add it in a release**: **no pure-Rust decoder exists for either format**, in this tree or in any dependency — Symphonia 0.6.1 supports neither in any feature combination, and the alternatives are an FFI binding to `libmpcdec` (which this project deliberately does without) or writing a decoder from scratch. A from-scratch Musepack SV7/SV8 decoder is a multi-week project with a modified Rice/ANS decoder and range coding, and it cannot be validated without reference streams and conformance vectors — an unvalidated bit-exact claim is worse than an honest refusal. `.mpc`/`.mp+`/`.mpp` and `.tak` therefore fail with descriptive, typed errors rather than being mis-parsed. `Codec::Musepack`/`Codec::Tak` exist so the formats are *named* and reported rather than silently unknown, and the vestigial `codec-musepack` feature is deliberately excluded from `all-codecs` so `--all-features` cannot imply a decoder that does not exist. WavPack supports mono/stereo integer and f32 only — that boundary is upstream (`wavicle` hard-scopes to mono/stereo), not an engine choice.
> - **Talkback and duplex mode do not exist**: input capture and input enumeration work everywhere (see below), but routing a live input back into the output path as a monitor mix is a separate DSP feature with its own gain, feedback and ducking questions. It is not implemented.
> - **No test opens a physical DAC**: CI now exercises the engine's entire output path — master ring, output matrix, per-endpoint worker thread, clock-drift resampler, format converter, underrun declick — through `AudioBackend::Null`, a hardware-free sink that runs the same paced drain loop a device callback runs. Real driver negotiation, real hardware clocks, and real DACs remain unverifiable from a CI agent. That is an environment fact rather than a defect in the engine.
> - **Ambisonic bus rotation is exact only for orders 1–3.** `rotate_bus_frame_n` implements exact Wigner blocks for those orders; the order-4..8 basis blocks are not implemented, so a bus above order 3 is left **unrotated** rather than partially rotated. This is deliberate: the previous behaviour tilted ACN 1–15 and passed ACN 16+ through unchanged, which is physically incoherent and a worse error than no rotation at all. Check `spatial::ambisonic::rotation_supported(order)` before relying on a rotation transform; the renderer still encodes and decodes all orders up to 9 exactly.
> - **The N-input mix bus has no automatic summation headroom.** Slots sum at their per-input user gain, which defaults to `1.0`; eight unity-gain slots reach +18 dB coherent / +9 dB incoherent before the limiter. Two slots — the primary and its crossfade partner, the default configuration and the one the bit-exact equivalence suite pins — are unaffected, because a crossfade cross-fades rather than sums. Use per-input `gain` or `SlotSend::master_gain` when running many lanes. The law is deliberately not applied automatically because changing it alters the sound of every existing configuration and would have to be mirrored in the frozen `dsp::pipeline` oracle.
> - **`plugin-dylib` is not sandboxed.** Plugins run **in-process** with only a `catch_unwind` boundary. There is no out-of-process isolation: `PluginSandboxConfig::validate()` rejects every isolated mode as unimplemented, and the fault-containment types in `plugin_sandbox.rs` are not wired into the production graph. Do not load a plugin you do not trust — see `SECURITY.md` §2.


---

## 🚀 Quick Start

### 1. Cargo Dependency Setup

The **package** is `audio-engine`; its **library target** is `engine`.

```toml
[dependencies]
audio-engine = { path = "path/to/audio_engine" }
config       = { path = "path/to/audio_engine/crates/config" }
```

*(Note: `audio-engine` and `config` are distributed from source / GitHub releases; see [Distribution](#-distribution).)*

### 2. Basic Playback in Rust

```rust
use std::time::Duration;
use engine::{AudioEngine, EngineConfig, EngineHandle, EngineEvent};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize engine with default configuration
    let mut engine = AudioEngine::new(EngineConfig::default())?;
    let handle: EngineHandle = engine.handle();

    // 2. Drive the engine on a dedicated tick thread (sleeps when idle)
    std::thread::spawn(move || {
        while engine.is_running() {
            engine.tick_blocking(Duration::from_millis(10));
        }
        engine.stop();
    });

    // 3. Monitor engine lifecycle events
    let events = handle.clone_event_receiver();
    std::thread::spawn(move || {
        while let Ok(event) = events.recv() {
            match event {
                EngineEvent::PlaybackStarted => println!("▶ Playback started"),
                EngineEvent::PlaybackPaused  => println!("⏸ Playback paused"),
                EngineEvent::PlaybackStopped => println!("⏹ Playback stopped"),
                EngineEvent::Error(err)      => eprintln!("❌ Error: {err}"),
                _ => {}
            }
        }
    });

    // 4. Open audio, adjust volume, and start playback
    handle.open_file("music/track.flac");
    handle.set_volume_db(-6.0); // Perceptual dB (-60.0 .. 0.0 dB)
    handle.play();

    // 5. Read atomic lock-free telemetry snapshot
    let info = handle.playback_info();
    println!(
        "State: {:?} | Playhead: {:.2}s / {:.2}s | Rate: {} Hz",
        info.state, info.position_secs_compensated, info.duration_secs, info.sample_rate
    );

    std::thread::sleep(Duration::from_secs(5));
    Ok(())
}
```

---

## 🖥 Interactive Terminal UI & CLI

Three binaries ship with the engine. They are the headless face of it: the same
engine the desktop player drives, without the desktop.

### 1. Terminal UI (`engine-tui`)

An interactive, zero-overhead TUI player built with **Ratatui** and **Crossterm**. Features real-time level meters, spectrum taps, gain reduction indicators, EQ curve plots, a file browser, and routing panels:

```bash
# Launch interactive TUI player
cargo run -p audio-engine --bin engine-tui --features tui -- [path]

# The TUI needs a terminal. Without one it says so and exits, rather than
# hanging on an invisible screen.
```

The meters, the spectrum and the EQ curve are read from the running engine,
and the EQ panel writes back to it — the curve you move is the curve the
engine applies.

<details>
<summary><b>🕹️ TUI Keybindings Reference</b></summary>

| Key | Action |
| :--- | :--- |
| `Tab` / `Shift+Tab` | Cycle between UI panels |
| `↑` / `↓` | Select item / row (hold to accelerate) |
| `←` / `→` | Adjust selected value / parameter |
| `Enter` | Activate selected item / drill down |
| `Space` | Play / Pause toggle |
| `/` | Open file and directory browser |
| `f` / `F` | Equalizer: Adjust frequency |
| `w` / `W` | Equalizer: Adjust Q factor |
| `t` | Equalizer: Cycle filter type (Peaking, Low Shelf, High Shelf, Notch, HPF, LPF) |
| `x` | Equalizer: Bypass or enable selected band |
| `r` | Output Panel: Rescan hardware output devices |
| `q` | Quit application (press twice) |

</details>

### 2. Reference CLI Player (`audio-engine-cli`)

A scriptable player and inspector. It prints what it is doing in a form a
shell can read — one JSON object per line with `--json` — which is what makes
it useful in a pipeline rather than only at a terminal:

```bash
# Play an album through a specific ALSA device
cargo run -p audio-engine --bin audio-engine-cli -- -b alsa -d "hw:0,0" ~/Music

# List the devices a backend can actually open
cargo run -p audio-engine --bin audio-engine-cli -- --list-devices -b wasapi

# Report a file's format and the engine's bit-perfect verdict for it
cargo run -p audio-engine --bin audio-engine-cli -- probe ~/Music/song.flac

# Machine-readable output, one object per line
cargo run -p audio-engine --bin audio-engine-cli -- --json ~/Music/song.flac
```

`--list-devices` asks the *selected backend* for its devices, not cpal: on
Windows the exclusive WASAPI backend opens endpoints by id, and the ASIO
drivers are not devices cpal knows about at all.

`probe` opens a file, reports the decoded format, and prints the engine's
bit-perfect report — the per-stage causes, and what would have to change for
the stream to be bit-perfect. It does not open an output device, so the
transport half of the verdict is not claimed; the report says so.

### 3. Loudness scanner (`replaygain-scanner`)

Measures EBU R128 loudness and ReplayGain 2.0 gain, and can write the result
back into the files' tags:

```bash
# Measure one file
cargo run -p audio-engine --bin replaygain-scanner -- --file ~/Music/song.flac

# Measure a tree, and write the results into the files' tags
cargo run -p audio-engine --bin replaygain-scanner -- --dir ~/Music --recursive --write-tags

# A CSV the spreadsheet can open
cargo run -p audio-engine --bin replaygain-scanner -- --dir ~/Music --format csv
```

---

## 🔗 C-FFI Embedding

The optional `c-ffi` feature exposes 45+ `extern "C"` functions behind an opaque `EngineHandleFFI*` handle. All entry points are protected by `catch_unwind` and return explicit status codes.

```c
#include <stdint.h>
#include <stdio.h>

typedef struct EngineHandleFFI EngineHandleFFI;

/* Lifecycle & Transport */
EngineHandleFFI* engine_create(uint32_t backend);
void             engine_destroy(EngineHandleFFI* engine);
int32_t          engine_open_file(EngineHandleFFI* h, const char* path);
int32_t          engine_play(EngineHandleFFI* h);
int32_t          engine_set_volume_db(EngineHandleFFI* h, float db);
float            engine_position_secs(EngineHandleFFI* h);
uint32_t         engine_abi_version(void);

int main(int argc, char** argv) {
    if (engine_abi_version() < ((1 << 16) | 0)) {
        fprintf(stderr, "Incompatible ABI\n");
        return 1;
    }

    EngineHandleFFI* h = engine_create(0); // 0 = AUTO backend
    if (!h) return 1;

    engine_open_file(h, "test.flac");
    engine_set_volume_db(h, -3.0f);
    engine_play(h);

    engine_destroy(h);
    return 0;
}
```

*The full C prototypes, ownership rules and lifecycle contracts are documented on
`src/ffi.rs` and its re-exports — `cargo doc -p audio-engine --features c-ffi --no-deps --open`.*

---

## ⚙️ Configuration Model

Engine configuration is specified in **TOML** via [`EngineConfig`](crates/config/src/engine_config.rs). Every field is optional; loaded files act as a patch over built-in defaults:

```toml
# shadow.toml
output_backend = "ExclusiveAlsa"
output_device  = "hw:1,0"
precision_mode = "Quality"         # Double-precision f64 DSP path

[limiter]
enabled          = true
true_peak_guard  = true            # 4x oversampled true-peak inter-sample limiter

[eq]
enabled = true
```

### Curated Presets

```rust
use config::{EngineConfig, EnginePreset};

// Curated presets for different listening and system profiles
let config = EngineConfig::from_preset(EnginePreset::Fidelity);
```

- **`EnginePreset::Fidelity`**: `ExclusiveAlsa` backend, `Strict` fallback (never silently drop to shared mode), f64 Quality precision, `HardwarePreferred` volume, and **all DSP processing disabled** for pure bit-perfect passthrough.
- **`EnginePreset::Consumer`**: Standard desktop playback: `Auto` backend, f32 Performance mode, `SoftwareOnly` volume, `FollowTrack` rate policy, gapless transitions, and TPDF dither enabled.
- **`EnginePreset::LegacyLowPower`**: Conservative CPU profile for embedded/constrained hardware: `Fast` resampler, f32 precision, and heavy DSP stages disabled.

---

## 🎧 Codecs, DSD & Formats

| Format / Codec | Implementation | Technical Specifications |
| :--- | :--- | :--- |
| **FLAC** | Symphonia Bundle | Lossless 16/24/32-bit integer PCM, multichannel up to 7.1.4 |
| **ALAC** | Symphonia Codec | Apple Lossless 16/24-bit in M4A/CAF containers |
| **WAV / AIFF** | Symphonia Riff/Aiff | Integer PCM (8/16/24/32-bit), IEEE float (32/64-bit), RF64, BWF, BW64 |
| **DSD (DSF / DFF)** | Pure Rust (`src/decode/dsd/`) | Native wire packing, DoP (DSD64–DSD512), 1-bit multistage decimation |
| **Ogg Opus** | RFC 8251 Pure Rust (`crates/opus-decoder`) | 48 kHz float decoding, packet-loss concealment, gapless metadata |
| **True Audio (TTA)** | Pure Rust (`src/decode/tta.rs`) | Lossless v1/v2 integer decoding, sample-accurate CRC32 verification |
| **WavPack** | Pure Rust (`wavicle`) | Lossless v5 integer & 32-bit float decoding. *(Mono/stereo only)* |
| **Monkey's Audio (APE)**| Pure Rust (`ape-decoder`) | APEv2 metadata tags and lossless audio decompression |
| **MP3 / AAC / Vorbis** | Symphonia Bundles | Psychoacoustic lossy decoding with gapless delay/padding trimming |

---

## 🔊 Hardware Output Backends & OS Integration

| Backend | Feature Flag | Default | Platform | Technical Description |
| :--- | :--- | :---: | :--- | :--- |
| **CPAL Universal** | `audio-output` | ✅ | Cross-platform | Universal shared-mode audio output fallback |
| **ALSA Direct** | `audio-output` | ✅ | Linux | Kernel direct access (`hw:`, `plughw:`), MMAP, bypasses system mixers |
| **WASAPI Exclusive** | `wasapi-native` | ❌ | Windows | `IAudioClient` exclusive event-driven mode, true bit-perfect bypass |
| **Steinberg ASIO** | `asio-native` | ❌ | Windows | Pure-Rust `IASIO` COM interface (no C++ SDK), native DSD transport |
| **CoreAudio Hog** | `audio-output` | ✅ | macOS | Hardware Hog-Mode, direct HAL IO render procedures, hardware sync |
| **PipeWire Pro** | — | ❌ | Linux | **Not implemented.** The `AudioBackend::PipeWire` variant is declared for the settings format; selecting it is refused under `FallbackPolicy::Strict` and degrades to shared output otherwise. No feature flag exists. |
| **JACK Pro-Audio** | — | ❌ | Linux / Unix | **Not implemented.** See above. |

---

## 🧪 Testing & Quality Gates

The engine enforces correctness across **24 test files (0 registered `[[test]]` suites + 24 integration suites)**, on top of the unit tests that live beside the code they test. The integration suites are the ones that exercise the engine the way an embedder does — through its public API, with a real decoder and a real output path.

```bash
# 1. Run the whole matrix: the in-crate unit tests and the integration suites
cargo test --workspace

# 2. Assert zero heap allocations on the audio hot path
cargo test --test realtime_alloc_budget

# 3. Verify bit-exact equivalence between the bypassed pipeline and the input
cargo test --test bit_perfect_equivalence

# 4. Verify the per-stage bit-perfect report and its causes
cargo test --test bit_perfect

# 5. Verify EBU R128 loudness and ReplayGain against closed-form references
cargo test --test loudness_r128
cargo test --test replaygain

# 6. Verify every biquad type against an independently derived reference
cargo test --test eq_conformance

# 7. Verify the codec registry never over-claims what it can decode
cargo test --test codec_matrix

# 8. Run the engine's own conformance pipeline through the CLI
cargo run --bin audio-engine-cli --features audio-output -- selftest

# 9. Run workspace clippy and formatting checks
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# 10. Check licence allow-list and supply chain dependencies
cargo deny check
```

The suite manifest (`tests/suite_manifest.rs`) counts the tree and checks it
against the numbers stated above, so these counts cannot drift from reality
without the build failing. Set `ENGINE_MANIFEST_STRICT=1` to make the drift a
hard failure rather than a report.

### Benchmarks

The engine ships a Criterion benchmark suite covering the realtime hot paths,
the DSP stages, the codecs and the output pipeline:

```bash
# Run the whole suite
cargo bench

# Run one benchmark, and save the result as a baseline to compare against
cargo bench --bench dsp_eq -- --save-baseline ci
```

Committed baselines live in `crates/engine/benches/baselines/`, and CI
compares against them weekly so a performance regression cannot land unnoticed.

---

## 📦 Distribution

**Shadow Desktop releases ship from GitHub (source code and prebuilt binaries), not from crates.io.**

1. The companion crate `config` is already owned on crates.io by `config-rs` (~107M downloads); publishing under that name would cause conflicts.
2. `crates/opus-decoder` is an in-tree **fork** fixing an integer overflow in the upstream 0.1.1 release; `cargo publish` would rewrite this path dependency into the broken upstream registry crate.
3. Therefore, workspace crates declare `publish = false` to guarantee build correctness. `plugin-abi` is the standalone publishable component.

### Toolchain and MSRV

`rust-toolchain.toml` pins **`stable`**, and **`rust-version = "1.98"`** is declared in all manifests. CI's `msrv` job installs an explicit 1.98 toolchain and checks the workspace
under `--locked` with warnings denied (`-D warnings`) — pinning `stable` alone
would only ever test a moving target.

---

## 📁 Repository Layout

```text
├── Cargo.toml                       # Workspace manifest & engine root package
├── LICENSE-APACHE                   # The engine's licence
├── README.md                        # This document
├── crates/
│   ├── config/                      # Serde-serializable engine & DSP configuration models
│   ├── plugin-abi/                  # C-ABI plugin specification, vtables, and host loader
│   └── opus-decoder/                # Pure-Rust RFC 8251 Opus audio decoder (Edition 2024)
├── src/
│   ├── lib.rs                       # Crate root, feature guards, and prelude re-exports
│   ├── commands.rs                  # EngineCommand — complete host control enumeration
│   ├── events.rs                    # EngineEvent & OutputEvent lifecycle definitions
│   ├── playback_info.rs             # Atomic telemetry snapshot models (ArcSwap)
│   ├── ffi.rs                       # C Foreign Function Interface implementation (c-ffi)
│   ├── diagnostics.rs               # Typed diagnostics: DiagnosticKind + BitPerfectCause
│   ├── buffer/                      # Lock-free SPSC PCM ring buffers, audio frames, DSD bytes
│   ├── decode/                      # Decoders, format scanners, channel mix, tags, loudness
│   ├── dsp/                         # DSP filters, Graph 2.0 topology, arena, and limiter
│   │   ├── graph2/                  # Typed-port graph engine, lowering, and realtime execution
│   │   ├── pipeline/                # Reference chain — the bit-exact oracle
│   │   ├── simd/                    # AVX-512 / AVX2 / SSE2 / NEON intrinsics & scalar fallbacks
│   │   ├── resampler/               # Rubato-based high-performance sinc resampler
│   │   └── safety.rs                # Inline NaN/Inf containment and denormal mitigation
│   ├── spatial/                     # 3D spatial layer: VBAP, Ambisonics (HOA), Room, Binaural HRTF
│   ├── output/                      # Audio backends (ALSA, WASAPI, ASIO, CoreAudio, CPAL)
│   └── bin/                         # audio-engine-cli, replaygain-scanner, engine-tui
├── benches/                         # Criterion benchmarks (DSP, pipeline, graph, spatial, budget)
└── tests/                           # 24 integration suites (fidelity, robustness, realtime allocation)
```

---

## 🤝 Contributing & Engineering Rules

Contributions are welcome. The working rules are:

1. **Strict Versioning**: `engine`, `config` and `plugin-abi` move in lockstep.
   `opus-decoder` has its own version line, because it is a fork of an upstream
   crate.
2. **No God Files**: Strictly maintain modularity. Decompose growing structs and split large impls by concern.
3. **Real-Time Hot Path Invariants**: Strictly no heap allocations, no locks, and no blocking I/O on the audio thread.
4. **All CI Gates Green**: Format, clippy (`-D warnings`), tests, and `cargo deny check` must pass cleanly.

---

## 📄 License

Licensed under the **[Apache License, Version 2.0](LICENSE-APACHE)**.  
Third-party dependency licences are enumerated by `cargo deny check licenses` (see
`.github/workflows/ci.yml` in the repository root); the algorithm citations that a
missing `docs/LICENSES_AND_ATTRIBUTION.md` promised are in the module docs of the
code that implements them.

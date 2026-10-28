<div align="center">

# Audio Engine

### Independent, Headless, Bit-Perfect Audiophile Audio & DSP Engine built with pure Rust

[![Version](https://img.shields.io/badge/version-1.11.0-blue.svg?style=flat-square&logo=rust)](Cargo.toml)
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
  ├── Multichannel Routing & Bass Management (per-route matrix FIR,
  │   LFE crossover, channel delay, trim)
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
  ├── Device-specific DAC Correction (separate pre-rendered profile bank)
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
       ├── High-Performance Sinc Resampling (Rubato FFT tiers or the engine's
       │   rational polyphase bank: Kaiser, Blackman-Harris, minimum-phase,
       │   apodizing, Sinc-M)
       ├── 4× Oversampling True-Peak Lookahead Limiter (Inter-sample peak guard)
       ├── Noise-Shaped Dither (TPDF, Shibata, NS5, NS9, LNS15) at the
       │   integer-quantization boundary
       ├── PCM → SDM (optional: half-band cascade + 1-bit modulator, native DSD only)
       └── Master SPSC Ring Buffer ──▶ Dispatched to Output Matrix & Physical DACs
```

### Hardware Bypass Modes

- **Bit-Perfect Direct**: Bypasses all DSP filtering, EQ, dynamics, and resampling. Only unity-gain seek fades and essential volume smoothing apply.
- **DSD-over-PCM (DoP) Bypass**: Total bit-transparent passthrough. Raw 24-bit DoP frames stream directly to the hardware DAC without attenuation or alteration.

---

## 🧩 Engine Subsystems & Desktop Coverage

The engine exports more than the ShadowPlayer desktop app drives. That is
deliberate — the crate is a library for embedders, appliances and DAWs — but it
is worth stating plainly which parts a host can reach and which parts have **no**
caller in this repository, so nobody assumes a feature is wired up because the
module exists:

| Subsystem | Status for the desktop app |
| :--- | :--- |
| `network_audio` | AES67 / RTP receive and send. No UI; drive it from an embedding host. |
| `mpris` (feature `mpris`) | **Not used by the app.** It is gated behind a feature the app does not enable. The desktop app implements MPRIS2 itself (`crates/ui/src/services/mpris_service.rs`) because transport state lives in the app's coordinator, not in the engine — exposing it from the engine would have two owners of the same D-Bus name. |
| `profile` | AutoEq / headphone target analysis. The app's AutoEq path applies the resulting filters through the DSP chain instead. |
| `dsp::filter_design` | Matched (Vicanek) biquad design, Butterworth biquad ladders, and the Nyquist-exact octave-width model. The desktop app does not drive these directly; the width→Q conversions are already on the EQ-file import path, and a host reaches the models through `ParametricEq::set_bandwidth_mode` / `set_band_width`. |
| `dsp::sdm` | PCM→SDM conversion for native DSD output (`config.sdm`). The engine negotiates the rate and publishes `PlaybackInfo::sdm`; the desktop output settings expose conversion, modulator order, and rate request. |
| `dsp::dither` high-order shapers | `EngineConfig.noise_shaper` selects `Shibata`/`Ns5`/`Ns9`/`Lns15`. The desktop settings expose the family selector; a change is applied when the output stream opens. |
| `dsp::apodizing` | Near-Nyquist-burst guidance and recommendation. The result is published in `PlaybackInfo.apodizing` and the desktop settings render it read-only; it never changes a filter automatically. |
| `EngineConfig.dac_correction` | Pre-rendered device/rate-keyed DAC profiles, selected by output device and published in `PlaybackInfo.dac_correction`. The desktop output settings include a profile editor that applies through the live command path. |
| `ChannelRoutingConfig.fir_routes` | Sparse matrix FIR routes (multichannel path). The desktop Mix Bus settings include a route editor; the stage still runs only on the `>2`-channel path. |
| `eval`, `governance`, `standards` | Engineering-process suites: qualification runs, standards conformance, release gates. Used by the engine's own test binaries. |
| `fx` | Modulation effects (chorus, flanger, phaser, delay, distortion). Built and tested, not yet surfaced in the app's DSP panel. |
| `buffer`, `track_cache`, `state` | Internal building blocks used by other engine modules; not host-facing. |

Entries marked engine-only remain that way until a host wires them up. Adding a
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
> - **Several stereo-designed stages run on the front L/R pair only in a `>2`-channel stream.** The mixer, EQ, compressor, convolution, balance, crossfeed, stereo enhancement, bass enhancement, time-stretch, creative FX, and the output resampler are stereo-linked or stereo-stateful; when the stream has more than two channels they leave the centre, surrounds, and LFE untouched. The engine reports this per node (`DspNodeInfo::channel_support` and `front_pair_only_active_stages`), so a UI can say exactly which active stages do not cover every channel. Volume, seek-fade, routing/channel trim, correction, spatial, plugin host, ultrasonic, limiter, and dither cover every channel.
> - **Sparse matrix FIR is a multichannel-path feature.** `channel_routing.fir_routes` runs only in the `>2`-channel routing stage (the stereo plan deliberately has no routing stage). The desktop Mix Bus settings edit the routes, and the same configuration is available through the engine config and C-FFI surface. At most 16 routes are loaded; an IR is capped at 262,144 samples and a route's pre-convolution delay at 1000 ms. Missing IR files are logged and the route is left inactive. A FIR route and a direct scalar route for the same source/destination sum, but the FIR partition latency is not compensated against the direct path, so a FIR-only profile must leave that matrix entry at zero. The stage reports its latency in `LatencyReport::routing_latency_ms` rather than hiding it, and execution is automatically split across parked helper threads above four routes; the summed result is bit-identical to the single-threaded path. Measured costs are in the matrix-routing section above.
> - **DAC correction is a device profile, not room correction.** It is selected by a case-insensitive substring of the active output-device name and by actual output rate (a profile with `sample_rate = 0` is a wildcard). Profiles are pre-rendered FIRs; the engine does not measure or derive them, does not resample a rate-mismatched profile, and caps each IR at 32,768 samples. Missing or unmatched profiles leave the second convolution bank inactive and publish the reason in `PlaybackInfo.dac_correction.reason`; they never pass the dry signal while claiming correction. The reference pipeline is device-agnostic, so DAC correction is a production-graph feature and has no `DspPipeline` equivalent; with the default empty profile table the pipeline and graph remain identical. The separate bank adds one partition (512 samples at 48 kHz) of latency on top of the profile's declared `latency_ms`, and that total is included in the graph latency report.
> - **Matrix-FIR multicore is CPU-only and has no affinity or GPU controls.** When at least four routes are loaded, the stage runs on the audio thread plus up to `available_parallelism - 1` helpers; the audio thread always keeps a share, so a two-core host uses both cores. Helpers are parked between blocks and are woken with an atomic job sequence and `unpark`, with no allocation or mutex in the block path. There is no worker-affinity pinning, no user-selectable block size/blocks-per-cycle control, and no GPU convolution backend: the measured route work is small enough that a GPU transfer-and-sync per 512-frame block would cost more than it saves, and no portable GPU dependency is worth adding for a stage that is already below the typical deadline. The pool is a CPU realtime feature, not a claim of scalable DSP on every host; a pathological 16-route × 262,144-sample profile can still miss a 192 kHz deadline on a host whose parallel throughput does not scale. The stage reports its latency and persistent bytes but not a per-route deadline guarantee.
> - **The engine is bounded to 16 channels (`MAX_CHANNELS`).** The channel layout, routing matrix, per-channel trim/EQ, matrix FIR endpoints, and DSP scratch planes all use that bound. More than 16 output channels would be a format/buffer/FFI change rather than a new matrix entry, and is not implemented; a configuration naming a channel above 15 is rejected and logged rather than silently folded.
> - **The apodizing counter is guidance, not a converter measurement.** It counts near-Nyquist energy bursts (0.45 × source rate, clamped to 8–40 kHz, −60 dBFS, 0.5 ms minimum, 10 ms re-arm) and recommends an apodizing filter above ten incidents per track. It is independent of HQPlayer's `Apod` algorithm, runs only on the decoded source in the single-stream and outgoing-crossfade paths, and does not analyse multi-track lanes or native-DSD raw payloads. The count is published in `PlaybackInfo.apodizing`; a desktop UI that wants to show it must read that field.
> - **The output ultrasonic `2x`/`4x`/`8x` selections are referenced to the output clock family (44.1 kHz or 48 kHz).** At ordinary PCM rates their corner sits at or above Nyquist, so the stage reports itself *inactive* instead of installing a filter that cannot exist; `20/30/40/50 kHz` are the selections that do something at normal rates. The filter is bypassed in bit-perfect and DoP modes like every other sample-altering stage.
> - **Noise-shaper selection is global and applies at stream open.** `EngineConfig.noise_shaper` is one process-wide value, not a per-endpoint override, and an already-running output converter keeps the family it was built with until the next stream open (the engine logs that fact). The `Ns5`/`Ns9`/`Lns15` tables are the engine's own designs, not HQPlayer coefficients; their 1 kHz attenuation is bounded by the one-sample error-feedback delay, which the README table reports honestly rather than claiming a deeper notch.
> - **PCM→SDM is native-DSD only, and its high rates are CPU-bound.** DoP is not synthesized from a PCM source, and the conversion requires an endpoint that advertises native DSD; otherwise the engine falls back to PCM with a reported reason. DSD64 is real-time on a low-power dual-core (measured ≈0.38–0.55 ms per 1.45 ms block, stereo); DSD256 is not (≈1.6–2.1 ms), and DSD128 lies between the two. `SdmRate::Auto` still negotiates the endpoint's highest rate, so choose a fixed rate when the CPU cannot sustain the maximum; `PlaybackInfo::sdm` and the `cpu_overloads` counter make a miss visible rather than silent.
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
dither_enabled = true              # integer PCM output quantization boundary
noise_shaper   = "Tpdf"            # Tpdf | Shibata | Ns5 | Ns9 | Lns15
                                    # read at stream open; see below

resampler_quality = "Balanced"     # Fast | Balanced | HighQuality | Ultra
resampler_filter  = "Fft"          # Fft | Kaiser | BlackmanHarris | MinimumPhase
                                   #     | Apodizing | SincM

[limiter]
enabled          = true
true_peak_guard  = true            # 4x oversampled true-peak inter-sample limiter

ultrasonic_filter = "Cut20k"       # 4th-order 20 kHz output low-pass (output domain)
                                    # other values: Off, Cut30k, Cut40k, Cut50k, X2, X4, X8

[sdm]                              # PCM → native DSD (desktop output settings)
enabled   = false                  # opt-in: converts PCM to one-bit SDM (independent of dsd_output)
modulator = "FifthOrder"           # FifthOrder | SeventhOrder
rate      = "Auto"                 # Auto (highest native) | Dsd64 | Dsd128 | Dsd256 | Dsd512

[eq]
enabled = true
```

### Multichannel matrix routing and FIR

`channel_routing` is the multichannel (more than two channels) routing stage. The
scalar `matrix` is a square `src[dst]` gain matrix; an empty matrix means
identity. Sparse `fir_routes` add a partitioned-convolution path from any source
to any destination, without requiring a square matrix:

```toml
[channel_routing]
enabled = true          # master switch for the scalar matrix and FIR routes
matrix  = []            # empty = identity; a FIR-only profile is valid

[[channel_routing.fir_routes]]
source      = 0         # post-matrix, post-trim source channel (0-based)
destination = 2
ir_path     = "/etc/shadowplayer/ir/left-to-centre.wav"
gain_db     = -3.0      # applied after convolution
delay_ms    = 0.0       # applied before convolution, capped at 1000 ms
```

Each route loads its impulse response on the **control path** when the
configuration is applied; a missing or unreadable file is logged and leaves the
route inactive rather than passing the dry signal through silently. The file's
declared rate is honoured and resampled to the stream rate on that path. A
stereo IR uses its left channel; a mono IR is used as-is.

The FIR runs **after** the scalar matrix, per-channel trim/EQ, and bass
management, and its output is summed into the destination. Sources are read
before any FIR sum is added, so route order does not matter. A direct matrix
entry and a FIR route for the same source/destination sum, and the FIR path is
**not** automatically latency-aligned to the direct path: a FIR-only route
should leave that matrix entry at zero. The reported matrix-FIR latency is the
longest route's pre-delay plus one 512-sample convolution partition
(`LatencyReport::routing_latency_ms`).

Limits are explicit: at most **16** routes, an IR of at most **262,144**
samples, and a pre-convolution delay of at most **1000 ms**. The route's
convolver memory is proportional to its IR length (four 512-sample partitions
for a 2048-sample IR); the engine reports it through the routing node's
persistent-bytes figure.

The stage is **multicore above four loaded routes**. The audio thread always
keeps a share of the routes and up to `available_parallelism - 1` helpers take
the rest, so a two-core host uses both cores instead of leaving one idle while
it waits. Helpers are created on the control path, park between blocks, and
are woken through an atomic job sequence plus `unpark`; the block path performs
no allocation and takes no mutex. Each route uses the engine's mono convolution
path (one source, one IR, one destination), which halves the FFT and memory
work of the stereo convolver. Route outputs are summed in configuration order,
so the result is bit-identical to the single-threaded path; a unit test pins
that equality, including the `> 4096`-frame chunking path. If a helper cannot
be created the stage logs the failure and rebuilds single-threaded rather than
dropping routes.

Measured on the README's i3-8130U benchmark host (release, criterion, one
512-frame eight-channel block; the first three rows use a 2048-sample IR, the
last row a 65,536-sample IR):

| Routes / IR | Median block cost | Share of 10.67 ms (48 kHz) | Share of 2.67 ms (192 kHz) |
| :--- | ---: | ---: | ---: |
| 1 / 2k | 47.2 µs | 0.44% | 1.77% |
| 4 / 2k | 115.9 µs | 1.09% | 4.35% |
| 16 / 2k | 252.8 µs | 2.37% | 9.48% |
| 16 / 64k | 2.504 ms | 23.5% | 93.9% |

The last row is the stress case, not a typical profile. It fits a 192 kHz
block on this host, but it is close enough that a slower host or a host with
poor thread scaling can miss; the IR cap is 262,144 samples, and 16 routes at
that length are beyond the measured realtime envelope. That boundary is stated
in the Known Limitations rather than presented as a guaranteed configuration.

### Apodizing-need counter

`PlaybackInfo.apodizing` is a per-track source analysis: it counts bursts of
energy near the source Nyquist limit, the band an apodizing reconstruction
filter rolls off. Each burst that stays above −60 dBFS for at least 0.5 ms is
one incident, and the detector waits for the band to fall back below the
threshold before it can count another, so a continuous HF passage is one
incident rather than one per block. Above **10** incidents per track the report
recommends an apodizing filter, matching the manual's "more than ten" guidance.

The report is explicit about what it is:

- `highpass_hz` is `0.45 × source_rate`, clamped to 8–40 kHz, and
  `threshold_dbfs` is −60 dBFS; both are in the report.
- `analyzed_seconds` and `incidents_per_minute` normalise the count, so a short
  or partially played track is not presented as a full-track result.
- `recommendation` is `InsufficientData` before any source has been analysed,
  then `NotNeeded` or `Recommended`.

This is a **guidance heuristic**, not a measurement of a specific ADC or
mastering chain, and it is not a claim of algorithmic parity with HQPlayer's
`Apod` counter. It runs on the decode thread, never in the output callback. Its
state is fixed at construction and the analysis allocates nothing per block;
`tests/realtime_alloc_budget.rs` holds that. The counter follows the outgoing
track during a crossfade and starts at zero for the incoming track; the
multi-track lane path is not analysed. Measured on the i3-8130U benchmark host
(release, criterion): **6.3 µs per 512-frame stereo block at 48 kHz**, or 0.06%
of the 10.67 ms block budget.

### Device-specific DAC correction

Room/headphone correction and DAC correction are different artifacts and stay
in different banks. A room correction is derived from a measurement; a DAC
correction is a **pre-rendered profile for a particular converter and output
rate**. The engine never synthesises one from a measurement. Both can run at
the same time, and telemetry reports them as `correction` and
`dac_correction` separately.

```toml
[dac_correction]
enabled = true

[[dac_correction.profiles]]
dac           = "RME ADI-2"   # case-insensitive substring of the device name
sample_rate   = 96000         # 0 = any; a profile is rate-keyed
ir_paths      = ["/etc/shadowplayer/dac/rme-adi2-96k.wav"]
latency_ms    = 0.0           # latency already inside the profile IRs
depth         = 1.0           # wet/dry
```

The first matching profile is selected by output device and actual output
rate. A profile whose WAV rate differs from the output rate is **rejected**,
not silently resampled; if no profile matches, the bank stays bit-exact and
`DacCorrectionInfo.reason` says why. The IR budget is the same 32,768 samples
as the room correction node, and the partitioned convolver adds its own
512-sample latency to the profile's declared `latency_ms`. Profiles are loaded
on the control path at config apply, output-device change, or output-rate
change — never in `prepare` or a block.

### Curated Presets

```rust
use config::{EngineConfig, EnginePreset};

// Curated presets for different listening and system profiles
let config = EngineConfig::from_preset(EnginePreset::Fidelity);
```

- **`EnginePreset::Fidelity`**: `ExclusiveAlsa` backend, `Strict` fallback (never silently drop to shared mode), f64 Quality precision, `HardwarePreferred` volume, and **all DSP processing disabled** for pure bit-perfect passthrough.
- **`EnginePreset::Consumer`**: Standard desktop playback: `Auto` backend, f32 Performance mode, `SoftwareOnly` volume, `FollowTrack` rate policy, gapless transitions, and TPDF dither enabled.
- **`EnginePreset::LegacyLowPower`**: Conservative CPU profile for embedded/constrained hardware: `Fast` resampler, f32 precision, and heavy DSP stages disabled.

### Resampler filter families

`resampler_filter` is independent of the quality tier. `Fft` keeps the
existing rubato FFT engine; every other family builds the engine's own
**rational polyphase** bank. For an input/output pair the ratio is reduced to
`L/M` and the low-pass prototype is split into `L` branches of `T` taps, so an
output costs exactly `T` multiply-accumulates regardless of the ratio. The
prototype is capped at 2,000,000 taps; a ratio that would exceed it (for
example 44.1 kHz → 768 kHz at a very long tier) falls back to the FFT engine
with a logged reason, and `AudioResampler::effective_filter()` reports the
truth.

The following figures are **measured**, not design targets: a 100 ms stereo
44.1 → 48 kHz block, f32, i3-8130U, release profile, criterion 3 s per case.
The stopband, ripple and delay come from an FFT of the final coefficient set
(`ResamplerFilterMetadata`); the cost column is the same run's median.

| Family | Tier | Taps/phase | Phases | Stopband | Ripple | Group delay | Cost / 100 ms |
| :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| Kaiser | Fast | 320 | 160 | 128.0 dB | < 0.001 dB | 3.63 ms (174 frames) | 3.8–4.1 ms |
| Kaiser | Balanced | 640 | 160 | 127.9 dB | < 0.001 dB | 7.26 ms (348 frames) | 6.1–6.6 ms |
| Blackman-Harris | Balanced | 640 | 160 | 121.6 dB | < 0.001 dB | 7.26 ms (348 frames) | 7.0–7.6 ms |
| MinimumPhase | Balanced | 640 | 160 | 87.7 dB | 0.001 dB | 0.21 ms (10 frames) | 6.4–6.9 ms |
| Apodizing | Balanced | 1280 | 160 | 125.0 dB | < 0.001 dB | 14.51 ms (697 frames) | 13.5–14.7 ms |

- **MinimumPhase** uses the real cepstrum to convert the Kaiser passband to a
  causal, front-loaded impulse response. It has no pre-ringing and roughly
  1/35th of the linear design's latency, but its group delay varies with
  frequency and the tapered cepstral tail costs stopband depth (87.7 dB here).
  The reported latency is the measured energy centroid, not a constant group
  delay.
- **Apodizing** and **SincM** double the tier's taps; `Apodizing` uses the
  Blackman-Harris window and places its stopband just above the source
  Nyquist. `SincM` uses a deep Kaiser window (β = 14) for reference
  conversion. Both are linear phase, and their latency is included in
  `latency_samples()` / `latency_ms()` and in the graph's reported delay.
- The polyphase cost is 4–15 % of one core at 44.1 → 48 kHz for these tiers.
  It is not the cheapest path; `Fft` remains the default and `Fast` is the
  cheapest polyphase tier. At high output rates the cost scales with the
  output rate, so choose a shorter tier when the CPU cannot sustain it.

### Noise shaping and dither

`noise_shaper` selects the error-feedback filter used when the output is
quantized to integer PCM. `Tpdf` is the historical default and adds no
shaping; `Shibata` keeps the existing 9-tap F-weighted design; `Ns5`, `Ns9`
and `Lns15` are the engine's own 5th/9th/15th-order designs. They are **not**
HQPlayer's proprietary coefficient tables, and the name states the design
order, not a compatibility claim.

The following figures are **measured from the final coefficient sets** as the
realised noise transfer function `NTF(z) = 1 − z⁻¹C(z)`, including the
one-sample feedback delay the implementation actually has. They are not
design targets:

| Family | 1 kHz @ 44.1 kHz | Max boost @ 44.1 kHz | 1 kHz @ 192 kHz | Max boost @ 192 kHz |
| :--- | ---: | ---: | ---: | ---: |
| Shibata | −13.0 dB | +21.2 dB | −14.8 dB | +21.8 dB |
| Ns5 | −19.1 dB | +7.5 dB | −28.1 dB | +7.7 dB |
| Ns9 | −17.0 dB | +8.5 dB | −28.9 dB | +10.2 dB |
| Lns15 | −16.3 dB | +7.0 dB | −29.7 dB | +6.6 dB |

- **The one-sample error-feedback delay is the 1 kHz limit.** A sample-by-sample
  quantizer cannot feed back the current error, so every NTF has a first-order
  zero at DC and a roughly −17 dB ceiling at 1 kHz for a 44.1 kHz stream.
  Deeper attenuation requires multiple zeros at DC and a correspondingly
  larger high-frequency boost, which is why the families trade the two.
- **Coefficient tables exist for 44.1, 48, 88.2, 96, 176.4 and 192 kHz.**
  An unlisted rate reuses the nearest table and logs once; the selected family
  is reported by `DitherType::as_str()`. The output converter rebuilds its
  dither when the stream opens, so a config change applies on the next stream
  open, not to a stream that is already running.
- **The shapers are cheap relative to the block deadline.** Measured with
  `cargo bench --bench dsp_pipeline -- dither_high_order` on the same
  i3-8130U, a 512-frame stereo 24-bit block at 44.1 kHz costs ≈30 µs (Ns5),
  ≈42 µs (Ns9) and ≈54 µs (Lns15) — 0.3–0.5 % of the 10.67 ms block. The
  cost is dominated by the two TPDF random draws per channel, not the
  coefficient length.
- **Float output bypasses every shaper.** When the negotiated device format is
  `f32`/`f64`, no quantization happens and the shaper is explicitly disabled.
  `dither_enabled = false` (or a per-endpoint `DitherPolicy::ForceOff`) also
  leaves the shaper unused even though the family remains selected.
  Bit-perfect and DoP modes bypass it for the same reason.
- The desktop settings expose the noise-shaper family. The choice is applied
  when the output stream is opened, because the converter is constructed with
  the device format; the engine config and C-FFI surfaces select the same field.

---

## 🎧 Codecs, DSD & Formats

| Format / Codec | Implementation | Technical Specifications |
| :--- | :--- | :--- |
| **FLAC** | Symphonia Bundle | Lossless 16/24/32-bit integer PCM, multichannel up to 7.1.4 |
| **ALAC** | Symphonia Codec | Apple Lossless 16/24-bit in M4A/CAF containers |
| **WAV / AIFF** | Symphonia Riff/Aiff | Integer PCM (8/16/24/32-bit), IEEE float (32/64-bit), RF64, BWF, BW64 |
| **DSD (DSF / DFF)** | Pure Rust (`src/decode/dsd/`) | Native wire packing, DoP (DSD64–DSD512), 1-bit multistage decimation with selectable noise filters (standard, low, high-order, brickwall, slow-lp, slow-mp); PCM→SDM conversion for native DSD output |
| **Ogg Opus** | RFC 8251 Pure Rust (`crates/opus-decoder`) | 48 kHz float decoding, packet-loss concealment, gapless metadata |
| **True Audio (TTA)** | Pure Rust (`src/decode/tta.rs`) | Lossless v1/v2 integer decoding, sample-accurate CRC32 verification |
| **WavPack** | Pure Rust (`wavicle`) | Lossless v5 integer & 32-bit float decoding. *(Mono/stereo only)* |
| **Monkey's Audio (APE)**| Pure Rust (`ape-decoder`) | APEv2 metadata tags and lossless audio decompression |
| **MP3 / AAC / Vorbis** | Symphonia Bundles | Psychoacoustic lossy decoding with gapless delay/padding trimming |

### PCM → SDM (native DSD output)

When `config.sdm.enabled` is set and the endpoint advertises native DSD, the
engine modulates PCM into a one-bit stream instead of sending PCM. This is a
**conversion**, not a passthrough: the bit-perfect verdict is off and the
negotiated plan is published through `PlaybackInfo::sdm` so a host can say
exactly what happened.

- **Signal path.** PCM is resampled once to a *working rate* of
  `8 × clock family` (352.8 kHz for 44.1 kHz families, 384 kHz for 48 kHz
  families). The complete DSP graph (EQ, dynamics, limiter, …) runs there.
  A half-band cascade then interpolates ×8/×16/×32/×64 to the DSD rate, and
  a one-bit modulator quantizes each sample. The already-modulated stream is
  not resampled again, and no PCM is sent to the DAC.
- **Modulator.** 5th- and 7th-order CRFB loops synthesized with Schreier's
  `synthesizeNTF`/`realizeNTF` (optimized zeros, `H_inf = 1.5`), with the
  coefficient tables compiled in for DSD64/128/256/512. The input is
  attenuated by 0.5 (50 % modulation depth, the maximum HQPlayer documents),
  which keeps the loop stable at full scale. `SdmReport` exposes clip count,
  state recoveries, input peak, ones ratio, and the interpolation delay.
- **Rate negotiation.** `SdmRate::Auto` probes the endpoint from DSD512 down
  to DSD64 and uses the highest native rate the device accepts; a fixed
  selection is verified the same way. If native DSD is unavailable, the
  engine stays on PCM, sets `SdmReport::fallback_reason`, and plays normally
  — it never pretends to be in DSD.
- **DoP is not synthesized.** DoP output for a PCM source is not
  implemented; PCM→SDM requires a native-DSD endpoint. The DSD-over-PCM path
  remains the existing bit-transparent passthrough for DSD files.
- **Measured cost** (i3-8130U, 512-frame stereo block at 352.8 kHz, release
  profile; the block carries 1.45 ms of audio): DSD64 5th order ≈ 0.38–0.43 ms
  (≈26–30 %), DSD64 7th ≈ 0.48–0.55 ms, DSD256 5th ≈ 1.62–1.80 ms
  (≈112–124 %), DSD256 7th ≈ 1.96–2.14 ms. DSD64 is comfortably real-time;
  DSD256 is not on this low-power dual-core, and DSD512 is further out of
  reach. DSD128 sits between the two measured rates (it was not separately
  benchmarked) and is the practical upper bound here. The engine's existing
  `cpu_overloads` / underrun telemetry reports a miss; it does not silently
  drop bits. Higher-rate conversion is a CPU capability, not a correctness
  switch — choose a fixed lower rate when the machine cannot sustain it.

The desktop output settings expose the SDM controls (conversion, modulator
order, and rate request); the negotiated rate and report published by the
engine remain authoritative.

- **The polyphase resampler has a hard prototype cap.** It is capped at
  2,000,000 coefficients. A ratio/tier combination over the cap falls back to
  the rubato FFT engine at the same quality tier, logs the reason, and reports
  `effective_filter() == Fft` while `requested_filter()` remains the
  requested family. It never silently runs a different filter.
- **Minimum-phase is not free of trade-offs.** The cepstral tail is tapered to
  fit the polyphase branch; the measured stopband is 87.7 dB at Balanced
  versus 127.9 dB for the linear Kaiser design. The delay is the measured
  energy centroid (0.21 ms), not a constant group delay, so the phase response
  is frequency-dependent by construction.
- **Linear-phase families pay latency.** Kaiser/Balanced reports 7.26 ms and
  Apodizing/Balanced 14.51 ms at 44.1 → 48 kHz; the engine accounts for these
  in `latency_samples()` and the graph's delay. This is deliberate: the
  engine's default remains the existing FFT path, and `MinimumPhase` is the
  low-latency alternative.
- **Polyphase cost is CPU, not a free quality switch.** Measured at 4–15 % of
  one core for a 100 ms 44.1 → 48 kHz stereo block, depending on tier. It is
  higher than the rubato FFT path; the benchmark in `benches/resampler.rs`
  keeps the numbers visible.

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

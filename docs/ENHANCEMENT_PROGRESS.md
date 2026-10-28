# ShadowPlayer Enhancement Progress & Handoff Blueprint

> **Handoff Document**: This document records the complete reverse-engineering findings from JRiver Media Center, the implementation details of Phases 1–3, and the exact step-by-step blueprints for implementing **Phase 4** and **Phase 5**.
>
> **For the next AI Assistant**: Read this file to seamlessly resume implementation without any loss of context.

---

## 1. Project Context & Authoritative Engineering Contract

- **Repository**: ShadowPlayer (`crates/*`, desktop music player over independent realtime audio engine).
- **Architecture Contract** ([`AGENTS.md`](../AGENTS.md)):
  - **No God Files**: Every file must remain well under ~800–1000 lines. Modular decomposition with façades (`mod.rs`) is required.
  - **The Realtime Contract**: 0 heap allocations, 0 locks, 0 blocking I/O on the audio callback thread.
  - **The UI & Performance Contract**: 0% CPU when idle; 60 fps cap during playback/animations; virtualize every unbounded list; no I/O on the UI thread.
  - **The Audio Honesty Contract**: Report negotiated hardware sample rate, bit depth, channel count, and backend; never claim bit-perfect unless confirmed by the engine.
  - **i18n Catalogue**: All user-facing strings must use `tr!` and be registered in `crates/ui/locales/en.json` and `de.json` with stable FNV-1a keys validated by `crates/ui/build.rs`.

---

## 2. JRiver Media Center Reverse Engineering Summary (Key Discoveries)

Extracted from JRiver Media Center 36 (`/home/sudipta/Downloads/software`):

### 2.1 Audio Path & Inspection
- **Three-Stage Inspection Pipeline**: Input Stage (file decoding, bit depth, sample rate), Active Transformations (gain adjustments, EQ, crossfeed, DSP stages), Output Stage (ALSA/WASAPI exclusive driver negotiation).
- **Bit-Perfect Honesty**: Media Center explicitly names any stage causing bit alteration (e.g. Volume leveling, DSP, Resampling).

### 2.2 Room Correction & Bass Management
- **Distance / Arrival Delay Physics**:
  $$\Delta t_i = \frac{d_{\max} - d_i}{v_{\text{sound}}} \times 1000\quad (\text{ms})$$
  Closer speakers arrive early and are delayed by $\Delta t_i$ to synchronize arrival at the listening sweet spot.
- **Linkwitz-Riley Crossover**: Subwoofer crossover filter with selectable slopes (12 dB, 24 dB, 48 dB/oct) that sum acoustically flat at $f_c$, coupled with continuous subwoofer phase alignment ($0^\circ \dots 180^\circ$) and polarity inversion.

### 2.3 Headphone Processing & Crossfeed
- **Recovered Mathematical Parameters**:
  - **Bauer (BSM)**: $f_c = 700\text{ Hz}$, $Q = 0.50$, $\Delta t = 0.30\text{ ms}$ ($300\ \mu\text{s}$).
  - **Chu Moy**: $f_c = 700\text{ Hz}$, $Q = 0.707$, $\Delta t = 0.25\text{ ms}$ ($250\ \mu\text{s}$).
  - **Jan Meier Reference**: $f_c = 600\text{ Hz}$, $Q = 0.60$, $\Delta t = 0.35\text{ ms}$ ($350\ \mu\text{s}$).
  - **Subtle (JRiver Recovery)**: $f_c = 700\text{ Hz}$, $Q = 0.707$, $\Delta t = 0.30\text{ ms}$ ($-3.0\text{ dB}$ cross-gain).
  - **Pronounced (JRiver Recovery)**: $f_c = 850\text{ Hz}$, $Q = 0.707$, $\Delta t = 0.30\text{ ms}$ ($-2.5\text{ dB}$ cross-gain, for 1960s hard-panned stereo).
  - **Studio Monitor Simulation**: $f_c = 600\text{ Hz}$, $Q = 0.707$, $\Delta t = 0.28\text{ ms}$ ($60^\circ$ equilateral nearfield monitoring).

### 2.4 Independent Playable Audio Zones (`CPlayerZone`, `CPlayerZoneLink`, `CPlayerZoneSwitch`)
- **Autonomous Zone Instances**: Each zone owns an independent playback queue, audio engine instance, decoder, and output device binding.
- **Clock Drift Compensation**: Independent physical DACs drift due to separate quartz crystals. Master zone distributes PCM; slave zones micro-adjust sample rate ($\pm 0.05\%$) via resampler with millisecond phase alignment sliders.
- **ZoneSwitch**: Automatic metadata-based routing (e.g. route DSD $\to$ DSD DAC; 5.1 multichannel $\to$ AVR).

### 2.5 Large Library Management (100,000+ Tracks)
- **`AlbumAnalyzer` Pattern**: Pre-aggregates album metadata in-memory into `(AlbumArtist, Album)` records during scanning so grid views require $O(1)$ lookups instead of scanning all tracks.
- **Pre-folded Collation Keys**: Cached sort byte keys on tracks to eliminate string normalization allocations during sorting.

---

## 3. Completed Phases (Phases 1–3)

### Phase 1: Audio Path 2.0 & Unified DSP Studio Hub (Completed)
- **Audio Path 2.0 Modal** ([`crates/ui/src/modals/bit_perfect_modal.rs`](../crates/ui/src/modals/bit_perfect_modal.rs)):
  - Live Transformation Ledger listing every active DSP alteration (volume leveling, EQ bands, crossfeed, limiter, resampler) with exact dB values and bit-cleanness indicators.
  - Graph 2.0 latency breakdown bar reporting exact per-node buffer delays.
  - Two-way "Pure Direct Mode" toggle with snapshot restoration.
- **Now Playing Quick Action** ([`crates/ui/src/components/now_playing.rs`](../crates/ui/src/components/now_playing.rs)):
  - Dedicated Audio Path icon button opening the inspector directly.
- **DSP Studio Hub Tab Navigation** ([`crates/ui/src/components/eq_panel/controls.rs`](../crates/ui/src/components/eq_panel/controls.rs)):
  - 7 tabs (Graphic, Parametric, Tone, Dynamics, Filters, Effects, Advanced) with zero-allocation active stage indicator dots.

### Phase 2: Acoustic Alignment & Speaker Setup Suite (Completed)
- **Modular Submodule Package** ([`crates/ui/src/modals/speaker_setup_modal/`](../crates/ui/src/modals/speaker_setup_modal/)):
  - `types.rs`: `DistanceUnit`, `SpeakerRole`, `SpeakerLayout` (2.0, 2.1, 3.1, 5.1, 7.1), `SpeakerSizePreset`, and modal state.
  - `canvas.rs`: 2D interactive soundstage geometry canvas with concentric polar distance rings, directional guidelines, sweet spot listener icon, and click-to-select speaker boxes with delay badges.
  - `controls.rs`: Channel distance slider, level trim dB slider, polarity inversion toggle, Linkwitz-Riley crossover slope selector, Butterworth vs Linkwitz-Riley topology, Mains High-Pass switch, and Subwoofer phase ($0^\circ \dots 180^\circ$).
  - `mod.rs`: Façade coordinating delay physics math, channel trim config generation, and audio session synchronization.
  - `tests.rs`: Unit tests for unit conversion, layout mappings, delay math, and config generation.
- **Launchers**:
  - EQ Panel Filters tab launcher card.
  - Settings -> Mix Bus / Crossover card launcher button.
  - Command Palette command: `"Speaker setup & acoustic alignment"`.

### Phase 3: Calibrated Headphone Spatialization & Enhanced Crossfeed (Completed)
- **Modular Submodule Package** ([`crates/ui/src/components/eq_panel/crossfeed/`](../crates/ui/src/components/eq_panel/crossfeed/)):
  - `types.rs`: Preset definitions (Bauer, Chu Moy, Jan Meier, Subtle, Pronounced, Studio Monitor, Custom), physics constants, ITD delay converters, path offset formulas, and biquad transfer function math.
  - `geometry.rs`: 2D interactive soundstage geometry canvas with listener head, virtual speakers at $\pm 30^\circ$, direct sound vectors, diffracted contralateral wavefront arcs ($+\Delta t$), and cranial acoustic shadow field.
  - `spectrum.rs`: Logarithmic frequency spectrum canvas ($100\text{ Hz} \dots 20\text{ kHz}$) with direct sound $0\text{ dB}$ reference, contralateral biquad lowpass response curve with gradient fill, and $-12\text{ dB/octave}$ attenuation.
  - `telemetry.rs`: Telemetry HUD (ITD Arrival Delay $\mu\text{s}$, Cranial Path Offset $\text{cm}$, Cutoff $f_c$, Fatigue Relief %), and tuning sliders.
  - `mod.rs`: Façade coordinating header, mode switching (Geometry vs Spectrum), preset selection, and parameter sync.
  - `tests.rs`: Unit tests verifying physical ITD and cranial offset math, biquad lowpass transfer function attenuation, and preset profile detection.
- **Launchers**:
  - Command Palette command: `"Headphone crossfeed & spatializer"`.
  - Filters tab in DSP Studio.

---

## 4. Next Implementation: Phase 4 (High-Scale Library Management)

### Goal
Optimize collection performance for 100,000+ tracks by implementing in-memory pre-aggregation, caching derived sort keys, and decoupling background maintenance workers.

### Implementation Blueprint
1. **`AlbumSummary` & `AlbumAnalyzer` (`crates/library/src/index/album_analyzer.rs`)**:
   ```rust
   pub struct AlbumSummary {
       pub artist: String,
       pub album: String,
       pub track_count: usize,
       pub total_duration_secs: u64,
       pub year_min: Option<i32>,
       pub year_max: Option<i32>,
       pub max_sample_rate: u32,
       pub max_bit_depth: u8,
       pub is_hires: bool,
       pub has_lossless: bool,
   }
   ```
   - Provide an in-memory aggregation index over all tracks in `LibraryState`.
   - Precompute once during library load or track changes; make album grid rendering an $O(1)$ read.
2. **Pre-Folded Collation Sort Keys (`crates/domain/src/models/track.rs`)**:
   - Add a derived, pre-folded byte key field `sort_artist_folded: Box<[u8]>` or normalized string to `Track` during tag ingestion.
   - Sorting by artist/title directly compares pre-folded byte slices (`&[u8]`) using slice comparison, bypassing per-row `to_lowercase()` allocations.
3. **Decoupled Worker Architecture (`crates/ui/src/services/`)**:
   - Ensure `ScannerService`, `ArtworkFetchService`, and `LoudnessService` run on bounded thread pools and communicate exclusively via lock-free channels.
   - Enforce the 0% CPU idle rule: workers sleep when queues are empty.

---

## 5. Next Implementation: Phase 5 (Multi-Zone Audio Playback)

### Goal
Provide independent, concurrent audio streams across multiple output devices with precision clock drift compensation and metadata routing rules.

### Implementation Blueprint
1. **Zone Management Architecture (`crates/playback/src/zone/`)**:
   - `ZoneId(Uuid)`
   - `ZoneState`: Owns current `PlaybackQueue`, active output device preference, volume, and DSP preset link.
   - `ZoneCoordinator`: Manages active zones, routes playback actions to the selected zone.
2. **Clock Drift Management & Master/Slave Linking**:
   - Designate one zone as Master; slave zones receive decoded PCM.
   - Clock drift compensation: micro-adjust slave resampler rate by $\pm 0.05\%$ dynamically to avoid buffer underflow/overflow.
   - Add egui timing alignment slider ($-50\text{ ms} \dots +50\text{ ms}$) in UI to phase-lock sound across rooms.
3. **ZoneSwitch Rule Engine**:
   - Expression matching on track metadata: e.g. `if format == DSD => route_to(Zone::DsdDac)`.

---

## 6. Pre-Commit & Verification Status

Prior to handoff, all verification gates passed:
- `cargo fmt --all -- --check` -> **Passed** (0 formatting differences)
- `cargo check --workspace` -> **Passed** (0 errors)
- `cargo clippy --workspace --bins --lib --all-targets -- -D warnings` -> **Passed** (0 warnings)
- `cargo test -p ui --lib -- --test-threads=1` -> **Passed** (234 unit tests passed)
- `cargo test -p shadowplayer --test platform_parity --test version_lockstep` -> **Passed** (10 integration tests passed)

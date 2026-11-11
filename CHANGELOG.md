# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.31.0] - 2026-11-11

*Released with audio-engine 1.14.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Unified Player Card Across Library Views.** Extracted `render_player_card` into a dedicated UI subsystem, ensuring the now playing card and playback controls are seamlessly visible across Albums, Artists, Genres, Folders, and playlist views without duplication (while omitted from Settings and Help).
- **Independent Volume Slider.** Decoupled the volume control into a dedicated `VolumeSlider` component positioned at the bottom-right corner below the right sidebar, remaining continuously accessible whether the queue/lyrics sidebar is toggled on or off.

### Removed
- **Pruned Legacy Collections, Podcasts, and Radio Subsystems.** Removed dead podcast feed/episode models, repositories, and UI services; pruned local radio generation in favor of the active local recommendation engine; and removed legacy VU and LUFS meter widgets.

## [0.30.0] - 2026-11-07

*Released with audio-engine 1.14.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Multi-Platform Packaging & Distribution Pipeline.** Added production packaging scripts under `scripts/` supporting Linux portable tarballs (`scripts/package-tarball.sh` with `install.sh`/`uninstall.sh`), Debian packages (`scripts/package-deb.sh`), AppImage bundles (`scripts/package-appimage.sh`), macOS `.dmg` and `.zip` bundles (`scripts/package-macos.sh`), Windows portable `.zip` and Inno Setup installer (`scripts/package-windows.sh`, `scripts/shadowplayer.iss`), and a unified packaging CLI (`scripts/package.sh`). Bundled Freedesktop desktop entry (`crates/app/assets/shadowplayer.desktop`) and AppStream metadata (`crates/app/assets/io.github.shadowplayer.metainfo.xml`), and upgraded `.github/workflows/release.yml` with cross-platform packaging jobs and automated checksum generation.
- **100% Secondary Localisation & Hindi (`hi`).** Complete 100% coverage across all 1027 catalogue keys in German (`de`), French (`fr`), Spanish (`es`), Japanese (`ja`), and Simplified Chinese (`zh`). Added full Hindi translation catalogue (`crates/ui/locales/hi.json`), registered `Locale::Hindi` (`"hi"` / `"हिन्दी (Hindi)"`) in `crates/ui/src/i18n/mod.rs`, and extended compile-time catalogue validation in `crates/ui/build.rs`.
- **Streamlined `.stm` Theme Format.** Migrated theme file format from `.shadowtheme` to `.stm` (short form of ShadowTheme) across all 34 bundled themes, theme engine, schema, custom theme manager, color wheel editor, and file pickers, while preserving full backward-compatibility for importing legacy `.shadowtheme` and `.json` theme files.

### Fixed
- **Disk work no longer runs on the frame thread.** Tag write-back, duplicate
  deletion, applying a batch organizer plan (and relinking the moved rows),
  playlist and listening-history export, and saving a measurement WAV now run
  on a typed worker (`ui::services::file_task_service`). `AGENTS.md` §2.4 puts
  file I/O on a worker; a slow drive or a library-sized batch could previously
  hold a frame for seconds.
- **The idle-repaint decision is a tested function.** The frame-wake policy is
  `ui::app::repaint_policy::wake_after`, with unit tests for the idle,
  minimized, playback, scan, timer, toast and pending-read cases. "0% CPU when
  idle" is now checked rather than maintained by review.
- **`ui` no longer depends on `audio-engine` directly.** Engine types are read
  through `audio_session::engine`, keeping `audio-session` the single bridge to
  the engine workspace (`AGENTS.md` §1).
- **Opus conformance claim corrected.** The decoder passes 11 of 12 RFC 8251
  vectors, not 12: `testvector08` mismatches both reference decodes RFC 8251
  permits. The crate README reports the measured result, the harness gained a
  real vector-fetch script (`crates/engine/scripts/fetch-opus-vectors.sh`), and
  CI decodes the eleven passing vectors so a regression cannot hide behind the
  `#[ignore]` attributes.

### Changed
- `ui/src/components/settings_tab/mod.rs`, `ui/src/components/eq_panel/controls.rs`
  and `ui/src/theme/manager.rs` were split into façades with focused
  submodules. Public paths and behaviour are unchanged.
- `docs/ENHANCEMENT_PROGRESS.md` is now a feature study with stated provenance,
  not a reverse-engineering handoff document.

---

## [0.29.0] - 2026-11-06

*Released with audio-engine 1.14.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added (UI & Design)
- **Settings modes.** Settings has a `Standard` and a `Pro Studio` view. Standard
  shows playback, output device, library indexing and appearance; Pro Studio
  adds the mix bus, crossovers, spatial radar, format converter and diagnostic
  tools. A toggle switches between them.
- **Settings navigation.** Categories are grouped into `PLAYBACK & HARDWARE`,
  `STUDIO LAB & DSP`, `UTILITIES & MEDIA` and `SYSTEM & INTERFACE`, with group
  headings, an active-row indicator and `PRO` badges.
- **Compact settings navigation.** Below 640 px the category rail becomes a
  dropdown plus an inline `Std`/`Pro` toggle.
- **Settings search results.** Results carry a category icon and a
  `Category › Location` breadcrumb, and jump to the row.
- **Equalizer quick-tune profiles.** Five macro presets (Vocal Clarity, Bass
  Punch, Warm Tube, Air / Sparkle, Acoustic Live), a tabbed layout, an active
  stage indicator, a compact pre-amp slider and a biquad filter inspector.
- **Colour wheel theme editor.** An HSV wheel and saturation/value canvas,
  with `#HEX`, `rgb(...)` and `hsl(...)` input and validation.
- **Transport bar layout.** Fixed button spacing in the MiniPlayer and Now
  Playing transports, and aligned the card headers.

### Engine & DSP (audio-engine 1.14.0)
- **Non-uniform partitioned convolution (NUPC).** The head partition runs as a
  direct FIR filter and the tail partitions as frequency-domain overlap-add, so
  the stream has no input latency and the audio thread still allocates nothing.
- **Neural inference (`dsp::neural`).** A lock-free real-time inference path
  with linear projections, ReLU/LeakyReLU/Tanh/GELU/Sigmoid activations,
  residual skips and declipping models.
- **FFT sweep deconvolution.** `EssSweep::deconvolve_into` is `O(N log N)` via
  `rustfft`, replacing the time-domain correlation.
- **Delaunay HRTF interpolation.** Replaced the brute-force triangulation in
  `spatial::hrtf::interpolate` with incremental 3D convex-hull Delaunay
  triangulation.
- **SVF corrections.** Fixed the damping term, the mixer tap mapping and
  shelving frequency prewarping in `equalizer/svf.rs`.
- **FIR crossover buffers.** Power-of-two circular buffers with a bitmask
  replace the inner-loop modulo in `FirCrossover`.
- **Multiband compressor phase.** Second-order allpass compensation on the Low
  band removes the 3-way crossover recombination notch.
- **Limiter saturation.** First-order ADAA tanh soft-knee saturation in
  `limiter.rs`.
- **EQ math.** Setters convert dB to linear with `exp2`; the RBJ golden
  vectors, Vicanek notch null and headroom lobes are covered by tests.

### Release & Infrastructure
- **Security & Distribution Hardening.** Updated `SECURITY.md` supported versions, deduplicated Linux ARM64 build matrix in `release.yml`, corrected repository links in `README.md`, and updated `THEMING.md` with Smoked Glass and theme starter specifications.

---

## [0.28.0] - 2026-11-06

*Released with audio-engine 1.13.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added & Integrated
- **ZoneSwitch Metadata Routing.** Wired `ZoneCoordinator::evaluate_zone_switch` into track playback and natural queue advancement, allowing rule-based automatic redirection of playback to dedicated audio zones and output devices (e.g. surround, DSD, lossless).
- **Persistent Speaker Setup & Acoustic Alignment.** `SpeakerSetupModalState` is now serialized to durable settings storage across sessions, preserving calibrated speaker distances, arrival delay compensation, level trims, and subwoofer Linkwitz-Riley crossovers.

### Fixed & Optimized (Engine & DSP)
- **Decramped Orfanidis Shelf Filters.** Implemented true high-shelf bilinear prewarping ($\omega_g = \omega_0 / g^{0.25}$) and numerator mapping in `biquad.rs` and `filter_design.rs`, eliminating low-shelf inversion on treble shelf boosts/cuts.
- **Dynamic EQ Boost Ballistics.** Corrected attack/release envelope selection for positive dynamic gains in `DynamicEqBand::process_stereo_block`, applying attack dynamics on transient peaks.
- **Zero-Allocation 3-Way Crossover.** Preallocated scratch buffers in `Crossover3Way` eliminating audio-thread heap allocations, and added delay alignment compensation to the High band to match Mid/Low filter latency.
- **Dynamics Detector Lookahead Delay.** Corrected power-of-two lookahead buffer capacity sizing and bitmask indexing in `DynamicsDetector`, preventing delay collapse to 0 samples on exact powers of two.

### Fixed & Optimized (UI & State)
- **Contiguous KeyTable 100k Sort.** Slashed heap fragmentation and allocations during large library sorts using a two-allocation `KeyTable` (offsets + byte storage), verified by the `library_budget` performance suite.
- **Subordinate Natural Sort Order.** Maintained chronological album release and track number ordering when sorting by artist or album in descending order.
- **Favorite Synchronization.** Synchronized in-memory `Track.is_favorite` across `all_tracks` and `displayed_tracks` on favorite toggle and addition.
- **Settings & Help Layout Overhaul.** Category rail navigation, tabbed EQ studio with active stage indicators, modal backdrop dismissal, and contrast-guaranteed text styling on accent fills.
- **Frosted glass surfaces** and the artwork backdrop/blur pipeline removed in favor of opaque theme surfaces.

---

## [0.27.1] - 2026-10-31

*Released with audio-engine 1.12.1. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Changed & Refactored
- **Single Canonical Sweep & Inverse-Filter Engine.** Consolidated exponential sine sweep (ESS) generation and Farina deconvolution under `crates/engine/src/dsp/correction/sweep.rs` (`EssSweep`), sharing an exact implementation between spatial acoustic metrics and room correction capture.
- **Engine Module Reorganization.** Disambiguated module names across audio engine subsystems: `output` (`backend.rs`), `spatial` (`acoustic_metrics` vs `room_model`), `simd/peak.rs`, and graph2 control structures (`control_handle.rs`, `control_mutators.rs`).
- **UI Architecture & Screen Standardization.** Renamed and refactored UI components (`albums_panel`, `artists_panel`, `folders_panel`, `genres_panel`) adhering to the canonical four-layer UI contract (`views/` for screen policy, `components/` for widgets and panels).
- **Subsystem Ownership & Architecture Contracts.** Formally codified canonical subsystem ownership, state hierarchy, and screen implementation rules in `AGENTS.md` and `crates/engine/AGENTS.md`.
- **Documentation Parity.** Updated `README.md` known limitations covering all 15 engine-level boundaries verified by `ui_docs_truth`.
- **Asset Optimization & Lockfile Management.** Recompressed help images reducing repository asset footprint, added build asset documentation, and centralized authoritative dependency locking at workspace root.

---

## [0.27.0] - 2026-10-31

*Released with audio-engine 1.12.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **AutoEQ Headphone Auto-Detection & Quick Calibration.** Automatic matching of active audio output device display names (resolving WASAPI endpoint IDs and CoreAudio UIDs) against 8,850+ acoustic target measurement profiles with stop-word tokenization. Shows an interactive suggestion banner in the AutoEQ modal with one-click "Apply Match".
- **Polyphase Resampler Cutoff Tuning.** User-configurable anti-aliasing low-pass cutoff ratio ($0.85 \dots 0.999 \times \text{Nyquist}$, default 0.95) for Kaiser, Blackman-Harris, minimum-phase, apodizing, and Sinc-M resamplers, triggering dynamic filter rebuilds on change.
- **Allowed Sample Rates Whitelist.** User-definable whitelist for output sample rates across 44.1 kHz and 48 kHz families up to 768 kHz, filtering device negotiation to avoid jitter, PLL unlock, or hardware-specific filter degradation at unsupported rates.
- **DAC Warmup on Start.** Configurable zero-frame burst ($0 \dots 500\text{ ms}$) delivered on stream initialization to allow external DAC clock PLLs to achieve lock without startup pops or clicks.
- **Native DSD Bypass for SDM.** Dedicated `dsd_bypass` toggle ensuring native DSF/DFF bitstreams bypass delta-sigma re-modulation and play bit-perfect to the DAC while SDM modulation remains active for PCM streams, coordinated through a unified `DsdTransportPlan`.
- **Expanded Settings Index & Localization.** Added 4 new settings entries to the searchable settings index (119 total) with complete German (`de.json`) and English (`en.json`) localizations.

---

## [0.26.0] - 2026-10-29

*Released with audio-engine 1.11.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **On-device For You recommendations.** A new dependency-free `shadow-recommender` crate (audio-feature extraction, taste profile, ranking, and a stateful rolling session) is integrated as an optional feed beside the Lossless/Hi-Res chips. The first batch holds 12 tracks; plays, completions, skips, likes, ratings, and playlist adds re-rank the pending list and refill it without duplicates. Settings gain a Recommendation Engine card with enable/disable, batch size, analysis quality, library-wide background analysis with progress and cancellation, and a clear-history action.
- **Recommendation persistence.** Database migration V17 adds `track_features` and `recommendation_feedback`, with a `RecommendationRepo` that caches feature vectors per schema version and replays signed feedback. Analysis decodes through `media-tools` on a worker thread, so playback and the UI thread are never blocked.

### Fixed
- **Artist column sort is case-insensitive for lists built outside the repository.** `sort_tracks` now populates the pre-folded collation keys before comparing, so imported or freshly built lists no longer sort uppercase names before lowercase ones.

---

## [0.25.0] - 2026-10-28

*Released with audio-engine 1.11.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Multi-Zone Audio Playback & ZoneSwitch Routing (Phase 5).** Independent audio zones (`ZoneConfig`, `ZoneId`, `audio_zones`) with isolated queues, volume levels, mute controls, output device selection, and master-slave multi-room pairing.
- **Sub-Millisecond Clock Drift Compensation.** Realtime drift tracking with quartz-oscillator phase micro-adjustments ($R = 1.0 + \frac{\text{drift\_pct}}{100.0}$) clamped to $\pm 0.05\%$ ($\pm 500\text{ ppm}$) to maintain continuous phase lock without pitch warble.
- **Multi-Room Delay Alignment.** Configurable relative timing offsets ($-50.0\text{ ms} \dots +50.0\text{ ms}$) between linked zones to eliminate acoustic comb filtering across rooms.
- **Dynamic ZoneSwitch Routing Rules.** Priority-ordered rule engine (`zone_switch_rules`) that automatically routes tracks based on sample rate, bit depth, channel count, format, or genre.
- **Audio Zones & Multi-Room Hub UI.** Dedicated management modal with zone status cards, output selectors, volume sliders, delay alignment, clock drift readouts, and rule configuration. Accessible via NowPlaying transport card, center top bar status pill (`[🔊 Main Zone]`), and Command Palette (`PaletteAction::OpenZones`).
- **High-Scale 100k Library Management & In-Memory Index (Phase 4).** Added `AlbumAnalyzer` for pre-aggregated $O(1)$ album metadata and cover art resolution; zero-allocation pre-folded UTF-8 collation keys (`sort_artist_folded`, `sort_title_folded`, `sort_album_folded`); database migration `V15` adding composite indexes on `tracks`.
- **Audio Path 2.0 & Studio DSP Suite (Phases 1-3).** Enhanced parametric EQ, speaker acoustic alignment, calibrated crossfeed (Meier & Bauer), and listening environment simulation.

### Performance & Hardening
- **Optimized 100k Track Sorting.** Switched table sorting to pdqsort (`sort_unstable_by`) over pre-folded byte slice keys, reducing 100,000-track sort time from 195 ms to 39 ms and library rebuild time from 320 ms to 155 ms.

---

## [0.24.0] - 2026-10-28

*Released with audio-engine 1.11.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **DSD transport and decimation controls.** PCM conversion, DSD over PCM, and native DSD transport selection; Standard/Low/High-Order/Brickwall and slow linear/minimum-phase decimation filters. Changes apply on the next DSD stream and the negotiated fallback is reported.
- **Output ultrasonic filtering.** Live off/20/30/40/50 kHz and rate-relative 2x/4x/8x low-pass selections, with an explicit bypass and an inactive report when a rate-relative corner cannot exist.
- **Polyphase resampler families and high-order noise shaping.** FFT, Kaiser, Blackman-Harris, minimum-phase, apodizing, and Sinc-M families; TPDF, Shibata, NS5, NS9, and LNS15 shapers. Active resamplers rebuild on the control path; a new shaper applies when the output stream opens.
- **Native DSD modulation (PCM→SDM).** Fifth- and seventh-order f64 modulators, Auto or fixed DSD64/128/256/512 rate requests, and device-negotiated reporting.
- **Sparse matrix FIR routing with a worker pool.** Up to 16 multichannel routes, per-route gain/delay/IR, partition latency reporting, and bit-identical parallel summation above four routes.
- **Apodizing guidance.** Read-only near-Nyquist burst counting and a per-track recommendation, surfaced in the output settings without silently switching filters.
- **Device/rate DAC correction.** A separate FIR bank keyed by output device and session rate, with a desktop profile editor, live reload, and distinct reporting from room correction.

---

## [0.23.0] - 2026-10-27

*Released with audio-engine 1.10.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Universal Frosted Glass Surfaces.** Optional optical translucent frosted glass styling toggleable across any active theme in Settings > Themes & Appearance. Includes ~70% translucent card fills, 1px specular micro-bevel highlight edges (`border_card`), layered shadow depth, and ambient artwork bloom behind cards during playback.
- **Curated Smoked Glass Theme.** Bundled `.shadowtheme` featuring obsidian-smoked translucent cards, vibrant sky-blue accents (`#38BDF8`), and full WCAG AA contrast compliance.
- **Changelog Archiving.** Preserved historical release notes (`v0.1.0` through `v0.15.0`) into `docs/CHANGELOG-archive.md` to keep repository metadata streamlined and sustainable.

---

## Earlier Releases

Historical release notes for versions `0.1.0` through `0.22.0` are preserved in the [Changelog Archive](docs/CHANGELOG-archive.md).

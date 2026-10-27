# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.23.0] - 2026-10-27

*Released with audio-engine 1.10.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Universal Frosted Glass Surfaces.** Optional optical translucent frosted glass styling toggleable across any active theme in Settings > Themes & Appearance. Includes ~70% translucent card fills, 1px specular micro-bevel highlight edges (`border_card`), layered shadow depth, and ambient artwork bloom behind cards during playback.
- **Curated Smoked Glass Theme.** Bundled `.shadowtheme` featuring obsidian-smoked translucent cards, vibrant sky-blue accents (`#38BDF8`), and full WCAG AA contrast compliance.
- **Changelog Archiving.** Preserved historical release notes (`v0.1.0` through `v0.15.0`) into `docs/CHANGELOG-archive.md` to keep repository metadata streamlined and sustainable.

## [0.22.0] - 2026-10-26

*Released with audio-engine 1.10.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Bit-perfect uncompressed WAV transcode & batch export.** Added 16-bit and 24-bit PCM WAV encoding in `crates/media-tools` and batch transcode export options in `MediaToolsModal` for CD mastering and hardware player compatibility.
- **Audiophile file organization tokens.** Extended `library::organize` with `{format}`, `{samplerate}`, `{bitdepth}`, `{bitrate}`, and `{channels}` pattern tokens for hierarchical audio folder organization.
- **Advanced smart playlist fields.** Added `Bitrate`, `Channels`, `FileSize`, `TrackNumber`, and `DiscNumber` to smart playlist filtering for Hi-Res audio, surround sound, and multi-disc collection querying.

### Optimized & Hardened
- **High-quality JPEG thumbnail cache.** Switched thumbnail disk caching from PNG to 85% high-quality JPEG in `library::ArtworkManager`, achieving an ~85% reduction in disk cache footprint (e.g. 4 GB to 600 MB for 100k tracks) and faster read times on mechanical HDDs, with full backward compatibility for existing PNG caches.
- **Decompression bomb & DoS defense.** Configured strict memory limits (32 MB / 64 MB) and dimension bounds (4096px / 8192px) on image decoders; offloaded image decoding from the UI thread to background workers.
- **Dual-core CPU starvation prevention.** Added cooperative scheduler yields (`thread::yield_now()`) every 100 scanned files during library indexing to guarantee audio playback and UI responsiveness on 2-core machines.
- **Spinning HDD & SQLite optimization.** Single-pass directory inode preloading (`list_scan_entries_under_prefix`) eliminating thousands of sequential SQLite queries; optimized SQLite PRAGMAs (`cache_size = -8000`, `temp_store = MEMORY`, `mmap_size = 33554432`).
- **Single-pass metadata tag reading.** Shared single `lofty` parser instances across tags, embedded lyrics, and cover artwork, cutting file reads in half.
- **Minimized window throttling.** Throttled invisible window repaints to 500 ms (2 Hz) when minimized, cutting background GPU/APU power draw to ~0%.
- **Input sanitization.** Added 10 MB playlist limits and null-byte injection guards in M3U/PLS parsers, plus 1 MB text limit and 16 MB SYLT frame cap on lyrics.

## [0.21.0] - 2026-10-26

*Released with audio-engine 1.10.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Spatial DSP & room correction suite.** Comprehensive acoustics and spatial audio processing:
  - Higher-Order Ambisonics (HOA) decoding across 1st, 2nd, and 3rd orders with AmbiX (ACN/SN3D) and B-format support.
  - Analytic Woodworth/Schroeder spherical head model and measured HRTF dataset interpolation.
  - Room acoustics simulation, mode estimation, and reverberation time (RT60) calculations via Sabine and Eyring formulas.
  - Parametric room correction filter synthesis with target curve profiles (Harman listener target, B&K 1974, flat reference) and psychoacoustic 1/N octave smoothing.
  - Subwoofer bass management with configurable crossover slopes, phase alignment, and bass redirection.
- **Immersive channel downmixing & height folding.** ITU-R BS.775 5.1/7.1 downmixing expanded with 7.1.4 immersive channel folding, rendering 3D bed and height channels cleanly into stereo with phase preservation.
- **Extended DSD rate support.** DSF and DSDIFF stream readers now support 48 kHz integer-multiple rates up to DSD1024 (49.152 MHz) alongside standard 44.1 kHz DSD rates (DSD64 through DSD1024).
- **CUE sheet ReplayGain parsing.** Sheet-level and track-level `REM REPLAYGAIN_TRACK_GAIN`, `REM REPLAYGAIN_ALBUM_GAIN`, and peak metadata extraction with parent inheritance.
- **WavPack APEv2 tag extraction.** Full APEv2 tag reading for WavPack files, parsing title, artist, album, genre, year, track numbers, and embedded ReplayGain tags.
- **DSP core optimizations.** Vectorized SIMD limiter processing, zero-allocation multiband compressor envelope detectors, TPDF and noise-shaped dithering, true peak 4x oversampled detection, and smooth gain ramping.

### Testing and release
- Full engine and player test suites pass, including ambisonics, HRTF, DSD 48k rates, CUE ReplayGain inheritance, and platform parity checks. Clippy warnings are zero across all workspace targets.

## [0.20.0] - 2026-10-26

*Released with audio-engine 1.9.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Equalizer bandwidth models & filter design suite.** `crates/engine/src/dsp/filter_design.rs`
  and `crates/engine/crates/config/src/eq_width.rs` introduce comprehensive bandwidth mode
  representations: standard quality factor ($Q$), octave bandwidth (`BW Oct`), frequency
  bandwidth in Hertz (`BW Hz`), and shelf slope ($S$). Equalizer text importing now parses
  and converts `BW Oct`, `BW Hz`, and shelf slope $S$ transparently to and from $Q$.
- **Matched Vicanek biquads and Butterworth filter ladders.** Dedicated filter design
  support for Vicanek-matched digital biquads that eliminate high-frequency cramping near
  the Nyquist boundary, along with cascade generator helpers for high-order Butterworth filters.
- **Dynamic loudness correction curves.** ANSI S1.11-2014 / ISO 226 equal-loudness contour
  compensation modules (`dsp::loudness_corr`) to accurately adjust bass and treble perception
  at varying listening levels without manual tone control adjustment.

### Fixed
- **Equalizer disabled band passthrough is flat unity.** When an EQ band is disabled,
  its coefficients now target a flat Peaking filter with 0.0 dB gain ($H(z) = 1.0$)
  rather than keeping its active filter type (which previously continued cutting or filtering
  frequencies for LowPass, HighPass, Bandpass, and Notch bands).
- **Logarithmic parameter smoothing in EQ biquads.** `EqParams::lerp` now interpolates
  frequency and Q logarithmically/exponentially across parameter updates. This removes
  audible zipper noise, parameter clustering in upper octaves, and coefficient instability
  during interactive sweeps.
- **Sample-rate invariant DC blocking in Bass Enhancer.** Replaced the static sample-rate
  dependent DC pole (`0.995`, which drifted from 35 Hz at 44.1 kHz to ~153 Hz at 192 kHz)
  with a dynamic, sample-rate invariant 10 Hz cutoff filter ($e^{-2\pi \cdot 10 / f_s}$),
  preserving sub-bass content at high sample rates (88.2 kHz - 768 kHz).
- **Non-finite sample protection and anti-denormal flushing across DSP chain.**
  Added sanitization against NaN/Inf input samples, denormal flushing, and stability bounds
  across the Simper State Variable Filter (SVF), Bass Enhancer harmonics generator, and
  Dynamic Equalizer envelope detectors, preventing DSP state latching, denormal CPU spikes,
  and digital blowups.
- **Headroom estimator accounts for resonant filters and bandwidth modes.**
  The equalizer headroom estimation now evaluates active resonant filters (`LowPass`,
  `HighPass`, `Bandpass`, `Notch`) and active bandwidth modes, preventing digital clipping
  when steep resonant slopes or high Q values are active.

### Testing and release
- Comprehensive unit test suites added for SVF non-finite rejection, dynamic EQ non-finite
  rejection, sample-rate invariant bass enhancer DC pole, disabled lowpass band passthrough,
  and resonant lowpass headroom estimation. Full workspace tests, clippy with denied warnings,
  and engine test suites pass.

## [0.19.0] - 2026-10-25

*Released with audio-engine 1.8.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Text EQ curves can be imported and exported.** AutoEQ
  `ParametricEQ.txt`, `GraphicEQ.txt`, Equalizer APO, and REW text are parsed
  by one clamped grammar and written back without inventing bands. File I/O
  and the native picker run on a worker, so a slow network share cannot stall
  a frame.
- **A durable preset library with bindings.** Saved curves live in their own
  table with provenance, a searchable virtualized browser, and optional
  output-device or per-track bindings. Precedence is explicit: a full track
  profile, a track preset pointer, the longest matching device pattern, the
  global default, then the manual chain. Applying a preset pushes only the EQ
  stage; it never reopens the output device.
- **8,850 AutoEQ parametric corrections.** The AutoEQ dialog now browses
  an embedded corpus instead of re-optimizing sixteen hand-written
  measurement sketches. Every entry names its measurement source and carries
  a preamp derived from the combined response peak, so a +15 dB correction
  does not ship at 0 dB preamp. Search and the band preview are virtualized;
  decoding happens once on a worker thread. AutoEQ is MIT-licensed and the
  notice travels with the binary in Help and
  `crates/ui/assets/autoeq/ATTRIBUTION.md`.

### Fixed
- **The manual no longer promises measurement import it does not have.** Help
  and Quick Start used to claim AutoEQ accepts a custom measurement curve;
  neither the dialog nor the engine exposed one. They now describe the corpus
  and point to the text-curve import that actually exists.
- **The equalizer now has one authoritative state.** A single master enable,
  a single shared preamp, and an explicit Graphic vs Parametric / AutoEQ mode
  replace the layers that could silently disagree. Graphic edits are sent as
  one atomic snapshot, so a preset or layout change installs every band and
  cannot be truncated by the command budget.
- **Factory and saved presets apply complete curves.** Presets write all
  bands on 10, 15, 31, and 64-band layouts; custom presets remember their
  source ladder and preamp, and selecting a factory preset restores unity
  preamp. Switching modes no longer erases the inactive curve. A 64-band
  AutoEQ correction is now persisted, captured by chain presets, and restored
  as a whole instead of being truncated to the old ten-row panel.
- **Extreme EQ use is audible, not suppressed.** Graphic sliders reach ±24 dB,
  the preamp applies in full, and static auto-headroom is opt-in and labelled
  as fixed attenuation. The true-peak limiter is on by default at −1 dBFS so
  large boosts are caught only when they would actually clip.
- **Persisted-but-unreachable DSP controls are now visible.** The Dynamics tab
  exposes the limiter and three-band compressor; the Filters tab exposes
  crossfeed and impulse-response correction. A Spatial Audio Master card owns
  the spatial bypass and disables its subordinate controls when bypassed.
- **Pure Direct mirrors the stages it bypasses and restores them on exit.**
  EQ, preamp, limiter, compressor, spatial, and stereo state in the studio no
  longer claim to be active after Pure Direct disabled them, and the next sync
  cannot re-enable them behind the user's back. Playback volume and speed are
  restored too, and leaving Pure Direct replays the captured pre-bypass chain
  instead of leaving the player flat.

### Testing and release
- Engine regression tests measure +12 dB, +24 dB with an −18 dB preamp, and
  static auto-headroom cancellation through the live EQ stage; config tests
  cover legacy graphic migration and explicit-mode round trips. UI tests cover
  complete presets, custom-preset ladders, interpolation, preamp migration,
  and mode migration. Formatting, workspace tests, release performance
  budgets, and clippy with warnings denied pass.

## [0.18.0] - 2026-10-24

*Released with audio-engine 1.8.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **The bundled gallery now ships thirty-two themes.** Twelve new schema-v2
  themes join the collection: Blueprint, Blue Hour, Cherry Cola, Plum Circuit,
  Copper Patina, Peach Fuzz, Night Swim, Highlighter, Sea Glass, Citrus Press,
  Adobe Rose, and Silverprint. The set now has 21 dark/OLED and 11 light
  themes, each with a full palette, independent radius scale, and
  contrast-checked text tokens.
- **Themes can carry their own typography.** Schema-v2 `fonts` declares
  independent display, body, and mono families. The gallery warns when a
  requested family is unavailable, the editor exposes all three roles, and the
  selection persists and applies across the app.
- **Six OFL families ship with the player.** Fraunces, Instrument Serif,
  Space Grotesk, DM Sans, Bricolage Grotesque, and JetBrains Mono are embedded
  alongside Inter, with licences and a font README, so themes do not depend on
  host-installed fonts.

### Changed
- Theme font roles fall back deterministically: missing text families use
  Inter, an omitted mono role uses JetBrains Mono, and an explicitly missing
  mono family falls back to Inter.
- Amber Terminal uses Inter for body copy to keep monospaced labels from
  clipping while retaining JetBrains Mono for display and diagnostics.
- `docs/THEMING.md` documents all thirty-two bundled themes and their
  typography.

### Testing and release
- Theme validation covers every bundled file, unique IDs and names, strict
  ten-surface WCAG AA contrast, border contrast, and font availability.
- Workspace tests, integration guards, formatting, and clippy with warnings
  denied pass.

## [0.17.0] - 2026-10-24

*Released with audio-engine 1.8.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **An engineering contract at the repository root.** `AGENTS.md` defines the
  dependency topology, the audio-truth rules, the performance guarantees, and
  the decomposition protocol that guards against new god files and god objects.

### Changed
- **The application shell is now a set of focused modules.** `crates/ui/src/app.rs`
  (5,814 lines) became an `app/` facade with modules for service polling,
  settings I/O, shortcuts, the command palette, eval hooks, imports, library
  actions, desktop integration, modals, and frame rendering. `app/mod.rs` is
  428 lines and every new module is under 800 lines.
- **The remaining god files were decomposed without changing their public
  APIs.** `database::repos`, `audio-session::session`, `help_view`,
  `songs_table`, `eq_panel`, and `settings_tab` now live in domain- or
  view-specific module trees. Their facades preserve the existing import paths
  and behaviour.
- **Source-reflection tests inspect every split source file** instead of the
  deleted monoliths, so documentation and capability drift checks still cover
  the entire surface rather than only a facade.

### Documentation
- **README rewritten around the product.** It leads with verified bit-perfect
  output, local-first operation, the DSP studio, library scale, first-run steps,
  and measured performance instead of a flat feature inventory.
- **Limitations are smaller and more accurate.** True boundaries are now
  separated from deliberate product choices in "What ShadowPlayer is not":
  no streaming or telemetry, no video, no background updater, no LAN control
  server, and no GUI-less library manager are choices, not missing features.

### Testing and release
- Workspace tests, documentation-truth checks, source-reflection checks,
  platform parity, version lockstep, clippy with warnings denied, and the
  release library budget all pass. No user-facing behaviour was intended to
  change in this release.

## [0.16.0] - 2026-10-24

*Released with audio-engine 1.8.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Unified Theming Architecture.** The UI reads every colour, radius, and spacing step from
  `ui::theme::Theme`. Custom themes use the `.shadowtheme` JSON format, with declarative macros
  serving as a single source of truth across the engine, schema, and editor.
- **Theme Gallery with Real-Time Staged Previews.** A responsive gallery grid supports category
  filtering, search, author/version labels, tag chips, and active/staged badges. Themes can be
  previewed live across the app without committing, or auditioned in a sandbox playback preview.
- **In-App Theme Editor & WCAG AA Contrast Auto-Fix.** Custom themes can be created and adjusted
  in real time. The editor validates WCAG AA text contrast (4.5:1) and non-text border contrast
  (3.0:1) with multi-failure reporting, and an HSL lightness-only algorithm rescues low-contrast
  tokens while preserving original hue and saturation.
- **Automatic Schema Migration (v1 → v2).** Legacy theme files lacking `schema_version` or root
  metadata are automatically upgraded on load, deriving missing tokens from existing palette
  surfaces with contrast-safe defaults.
- **Active Theme Persistence & Fast Import.** Committed theme selections persist to `active_theme.txt`
  across app restarts. Importing themes from native file dialogs performs an in-memory cache upsert
  without expensive disk traversal.

---

## Earlier Releases

Historical release notes for versions `0.1.0` through `0.15.0` are preserved in the [Changelog Archive](docs/CHANGELOG-archive.md).

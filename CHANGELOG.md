# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.2.0] - 2026-10-18

### Added
- **Master Manual & Interactive Audio Knowledge Base**:
  - Full-page dedicated reference manual accessible directly via *Help & Feedback* in the navigation bar.
  - 8 comprehensive modules covering Quick-Start Architecture, Audio Signal Path, Equalizer & Acoustic Filters, Bit-Perfect Direct Stream & DACs, Spatial 3D & Binaural HRTF, Playback Dynamics & Normalization, Mix Bus Studio Tools, and Keyboard Shortcuts / Audiophile Glossary.
  - 4 high-resolution technical vector infographics:
    - `audio_flow.jpg`: Uncompressed PCM sample flow through 64-bit DSP rack to hardware DAC endpoints.
    - `equalizer_bands.jpg`: Acoustic spectrum map (Sub-Bass to Air) illustrating peaking bell and shelving curves.
    - `bitperfect_dac.jpg`: Degradation analysis of OS shared audio mixers vs bit-perfect direct stream.
    - `spatial_3d.jpg`: 3D binaural wave propagation, pinna reflections, and virtual room acoustics.
  - **Interactive Try-It Sandboxes**:
    - *Signal Chain Inspector*: Clickable pipeline blocks (Decoder, RingBuffer, Resampler, EQ Rack, Headroom Limiter, TPDF Dither, Driver Output, Hardware DAC) revealing plain-English functions and exact DSP mathematics.
    - *Real-Time Filter Sandbox*: Interactive sliders for Bass Shelf, Mid Peaking gain/Q, and Treble Shelf with a live logarithmic frequency response curve painter (20 Hz – 20 kHz).
    - *3D Binaural Soundstage Simulator*: Azimuth angle (-180° to +180°) and virtual distance sliders paired with a top-down radar canvas, dynamically computing Interaural Time Difference (ITD in µs), Interaural Level Difference (ILD in dB), and inverse-square attenuation.
    - *Full-Width Crossfade Oscillogram*: Real-time mathematical curve generator for Constant Power ($cos^2 + sin^2 = 1$), Linear, and S-Curve transitions.
  - **Full-Text Live Search Bar**:
    - Dedicated full-width search input in the manual's hero banner with vector search icon and instant clear (`x`) button.
    - Comprehensive search index covering all settings, shortcuts, DSP topics, and glossary terms with direct *Jump to Section >>* action buttons.
- **Contextual Settings Info Popups (`info_box.rs`)**:
  - Interactive 'i' info badges positioned next to all complex and advanced audio settings in both the Floating Equalizer panel and the Settings Studio tabs.
  - Click-to-open floating cards explaining plain-English functionality, acoustic formulas, and recommended audiophile defaults.
  - Auto-dismiss on click outside or when pressing the `Escape` key.
- **Settings Persistence**:
  - Full cross-session persistence for master volume, mute states, color themes, equalizer bands/gains/preamps, spatializer preferences, and audio driver modes saved into SQLite.

### Changed
- **Equalizer Layout & Alignment**:
  - Floating Equalizer window now opens center-aligned on screen on its initial activation.
  - Default Equalizer state set to disabled/flat bypass (`0.0 dB` across all bands) with auto-headroom enabled.
  - Fixed active band button text visibility in Graphic EQ mode (ensuring crisp white text on violet backgrounds instead of unreadable pink-on-pink).
- **Dual-Tone Slider Aesthetics**:
  - Restored signature dual-tone gradient fill (Rose Pink `#F43F5E` to Luminous Indigo `#6366F1`) on all scrubbers, seekbars, and volume sliders.
- **Help View Full-Page Layout**:
  - Right sliding drawer (Up Next queue & lyrics) is strictly disabled in the Help page, providing 100% full-screen width for documentation.
  - Redundant top music library search bar hidden when viewing Help or Settings.
  - Replaced non-standard Unicode dingbat symbols (`➔`, `✕`) with universal ASCII glyphs (`>>`, `x`) to resolve missing font warnings on Linux desktop systems.
- **Responsive Text Wrapping**:
  - Descriptions and glossary terms across all manual cards now wrap smoothly across multiple lines, eliminating text cut-off on any viewport size.

---

## [0.1.0] - 2026-10-15

### Added
- Initial release of **ShadowPlayer** audiophile desktop local music player in Rust and `egui`/`eframe`.
- Core playback engine integration supporting bit-perfect streaming, 64-bit float DSP, Symphonia decoders, and SQLite library management.
- Multi-format audio library scanner (FLAC, ALAC, WAV, AIFF, DSD, MP3, AAC, Opus).
- Floating parametric and graphic equalizer window with AutoEQ headphone profiles.
- Time-synchronized LRC lyrics viewer and drag-and-drop playback queue.
- Dark and light responsive themes with customizable accent colors.

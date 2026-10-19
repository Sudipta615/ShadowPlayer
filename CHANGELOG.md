# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **CLAP plugin hosting**: load and process standard CLAP audio plugins inside the DSP graph, with a generic parameter rack, per-plugin bypass and state save/restore.
- **Batch organizer**: build a rename/move plan from `{token}` patterns under a destination folder, review every entry (conflicts, duplicates, missing sources) before anything moves, and relink the library afterwards.
- **Named DSP chain presets**: save and recall whole-chain presets (EQ, dynamics, filters, effects, plugin rack), stored with the settings.
- **Lyrics from file tags**: embedded `USLT`/`SYLT` (and equivalents) are read and preferred over sidecar files, with a hardened sidecar lookup; timed `SYLT` lyrics render as synced LRC.
- **Recently added view and quick-filter chips**: a "Recently Added" section, plus Lossless / Hi-Res / Favourites chips above the songs table, backed by the existing filters.
- **Toast notifications**: every operation that fails (scan, import, organize, persistence, engine problems) reports it instead of logging into the void.
- **Bass management**: subwoofer/LFE routing, crossover with selectable slope, and up/down-mix controls, all saved with the user's audio profile.
- **Duplicate finder**: groups tracks by content fingerprint and offers keep-best/remove handling.
- **DSP precision mode and runtime diagnostics**: selectable processing precision, plus a diagnostics card that reports the negotiated output path and why bit-perfect is (not) active.
- **M3U import/export**: extended `.m3u`/`.m3u8` playlists, with relative paths, encoding tolerance and a portable export.
- **Scan-root watching**: roots are watched while the app runs and changes are reconciled on startup, so the library tracks the disk without a manual rescan.
- **ReplayGain tag writing**: an optional library scan that writes ReplayGain 2.0 tags (track and album) into supported files, with a dry-run mode.
- **Root `README.md`, `LICENSE` (Apache-2.0) and a CI workflow** (`fmt`, `clippy -D warnings`, tests for the player workspace and the engine).

### Fixed
- **Playback and UI honesty** (audit milestone P1): clearing the queue also clears the current track; turning shuffle off restores the pre-shuffle order from a snapshot that is no longer overwritten; listening time is credited only for contiguous playback; repeat-one no longer reopens the file; persistence failures are logged; engine and output-device problems surface as toasts.
- **Sorting** is a single enum again (domain `SortField` + `SortOrder`): the direction is selectable and persisted, favourites can be sorted, the `#` and heart headers sort, and album/track-number collation is fixed. The row menu opens, the never-reachable "Advanced Audio" modal is gone, crossfade is enabled only for the crossfade transition, the spatial radar drives real engine parameters, and the settings tab persists its own complete struct instead of a hand-maintained mirror.
- **Bad data can no longer be lost silently** (audit milestone P0): metadata extraction no longer divides by zero on sub-second files; scans do not follow symlinks and do reconcile deletions; content fingerprints probe head, middle and tail; relinking requires the old file to be genuinely absent; `.cue`-covered album images are indexed as the sheet's tracks only; malformed ids are reported instead of reinvented; `ScanRoots` add is idempotent; absent settings fields fall back to defaults; SQLite pragmas are verified; the V3 migration is guarded against a lost migration row.
- **Database correctness**: album year uses `MIN(year)`; full-text search builds a real FTS query from the tokenizer's own tokens (`AC/DC`, `don't`); playlist positions are compacted from an ordered read; smart-playlist rules that a field cannot express are repaired on load, and an unreadable rule blob matches nothing rather than everything.
- **Library and sidecars**: organizing never overwrites a file that appeared after the plan was made and works across filesystems; `.m3u` files are decoded rather than assumed UTF-8, `file://` URIs percent-decode, relative paths survive a symlinked base directory, and unrepresentable paths are skipped instead of corrupting the playlist; the watcher coalesces event bursts, logs backend errors and ignores opens; LRC parsing understands metadata tags, `[offset:ms]` and multiple timestamps, and reads non-UTF-8 sidecars.

---

## [0.4.0] - 2026-10-18

### Added
- **MPRIS2 / media-key integration (Linux)**:
  - ShadowPlayer now appears on the D-Bus session bus as `org.mpris.MediaPlayer2.ShadowPlayer`, so the desktop shell, media keys, lock screens and tools like `playerctl` can play/pause, skip, seek, set shuffle/repeat and read the current track, position, volume and artwork.
  - Transport commands are funnelled through a channel into the UI loop and the player state is published every frame, so the audio thread is never touched by D-Bus work.
- **Smart playlists**:
  - Rule-based, auto-updating playlists (field / operator / value) with *match all* or *match any*, an optional track limit and a sort order.
  - Managed from a new **Smart Playlists** sidebar section and an editor with a live match preview.
  - Stored as JSON rules in a new `smart_playlists` table (schema V2) and evaluated in SQL, so they stay fast on large libraries. Incomplete rules are ignored rather than matching nothing.
- **Library-wide ReplayGain / EBU R128 scan**:
  - **Settings → Tools → Library ReplayGain Scan** analyses every track that has no loudness data, in the background, with live progress and a Cancel button.
  - Results feed both the library and the engine's on-disk loudness cache, so the existing Track/Album ReplayGain and EBU R128 loudness modes work library-wide. A small inter-track delay keeps the audio thread responsive on dual-core machines.
- **Multi-select and bulk actions in the Songs table**:
  - Checkbox column with a select-all header box (including an indeterminate state), Ctrl/Cmd-click to toggle, Shift-click to extend a range — while a plain click still plays the track.
  - A bulk bar appears while tracks are selected: Play, Play Next, Add to Queue, Favorite, Add to Playlist, Remove and Clear. The selection survives tab switches and is pruned when tracks disappear.
- **Stable track identity**:
  - Every file now carries a cheap content fingerprint (size plus a hash of the first and last 64 KiB), stored in the new `tracks.content_hash` column (schema V3).
  - Moving or renaming a file no longer orphans it: the scan re-links the existing row, preserving play counts, favourites, playlist membership, lyrics and artwork. Applies to both folder scans and single-file imports.
  - Relinking only happens when the fingerprint matches exactly one row and the old file is gone, so duplicated copies are still indexed separately.

### Changed
- Replaced the symbol/emoji glyphs that the bundled fonts cannot render (they appeared as empty boxes in buttons, modal titles and headers) with plain text or vector-drawn equivalents, including a new funnel icon marking smart playlists.

---

## [0.3.0] - 2026-10-18

### Added
- **Real Music Import (native OS file picker)**:
  - *Import Songs…* and *Import Folder…* buttons in **Settings → Library & App**, backed by the system file chooser (`rfd`, XDG portal) running off the UI thread so the app never freezes.
  - Individual songs are tagged/inserted straight into the library; folders become monitored scan roots (also reachable from the **Folders** tab).
  - Imported file locations are listed beneath the import buttons.
- **Playlist Manager**:
  - The `+` button now opens a floating manager to create, rename, delete playlists and add/remove songs with a built-in library search.
- **Edit Song Info**:
  - Right-click a track → *Edit Song Info* to edit title, artist, album, genre, year, track and disc numbers; changes persist to the library.
- **Sleep Timer**:
  - Clock button in the Now Playing transport bar with 5/15/30/60-minute presets, "end of current track", and off. Counts down while playing and pauses playback automatically.
- **Recently / Most Played 7-day rules**:
  - A track counts as played once **>20 s** of it has been heard (or it finishes naturally).
  - *Recently Played* and *Most Played* use a sliding **7-day window**; *Most Played* requires **5+ plays**. History is pruned automatically.

### Changed
- **Playback semantics**:
  - Autoplay/auto-advance restored; clicking any row in the Songs table (or the queue) starts playback immediately.
  - **Shuffle now actually affects the queue** — enabling it permutes the upcoming tracks, and disabling it restores the original order.
- **Theming**:
  - Light/dark mode now covers every element (active rows, artwork placeholders, scan banners, modals, bit-perfect nodes, destructive actions) via new semantic theme tokens.
- **Performance (large libraries; 10k tracks on dual-core + HDD)**:
  - Songs table and queue are fully virtualized (`ScrollArea::show_rows`); only visible rows are built.
  - Artwork uses a bounded in-memory LRU (256 textures) plus downscaling to 160 px before GPU upload and persistent disk thumbnails.
  - Removed per-frame clones of the full track list and queue; navigation/search switches skip rebuilding when nothing changed; idle CPU is ~0%.

### Removed
- All bundled demo songs, thumbnails and their seeding/purge migration; the library starts empty until you import.
- Startup auto-play demo behavior.

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

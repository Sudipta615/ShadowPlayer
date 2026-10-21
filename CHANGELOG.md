# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [0.8.0] - 2026-10-21

### Added
- **A command palette** (`Ctrl+Space`): one field that reaches the sidebar, the playlists, the smart playlists, the toggles, the panels that are otherwise buried in a menu, and any track in the library. The matching is a scored subsequence match, not a plain `contains`, and its precedence is documented and tested: a candidate that starts with the query beats a phrase that merely contains the word, a whole word beats a mid-word fragment, and a shorter candidate wins a tie. Tracks are scored against their raw titles before any row is built, so a ten-thousand-track library does not allocate ten thousand rows on every keystroke.
- **ReplayGain album gain is indexed.** The engine could already read an album's gain from the file's own `REPLAYGAIN_ALBUM` tag and apply it, but the library never indexed it, so the app could neither show it nor fall back to a computed value for an album with no tag. The library now stores it (migration V6), the loudness scan computes it with the engine's own loudness-weighted definition and writes it to every track of the album, and the track properties show the track and album gains side by side.
- **A loudness inspector**: the track's integrated loudness on a scale, with the rest of the album's spread behind it, the EBU R128 and ReplayGain reference levels marked, and the true peak, the track gain and the album gain. It is reachable from the now-playing card and from the command palette.
- **Library statistics**: what the library is made of, all in one place. The headline numbers (tracks, albums, artists, genres, running time, size on disk), the format breakdown as a stacked bar, the sample-rate breakdown with the Hi-Res ones marked, how much has never been played, how much arrived this month, and the most-played tracks. The library already knew all of it; it just never said. It is reachable from the library manager and from the command palette, and the summary is computed once when it opens rather than walked on every frame.
- **A full-screen Now Playing mode** (`F`, or Escape to leave): the window gives itself over to the artwork, the visualiser and the lyrics, with the transport underneath. The mini player is a 320x110 strip, which is the right size for a corner of the screen and the wrong size for looking at.
- **Per-track DSP.** A track can carry its own graphic EQ and preamp, applied only while that track plays — the corrective curve for the one badly mastered album, the preamp for the one brick-walled single, without changing the whole library. The profile is stored in the library (migration V7), reached from the song's context menu or the command palette, and saved from the EQ as it stands. Tracks without a profile get the user's own settings back, which is the part that has to be right or the previous track's curve leaks into the next one.
- **"Edit Song Info" can write the tags back into the file.** The dialog gained a general tag writer in the engine and an opt-out checkbox. The file is the source of truth for a music library, so an edit that only lives in the database is lost the moment the file moves or the library is rebuilt from disk. The player says so when it did.

- **Scrobbling, counted locally.** A scrobble is a play that counted: thirty seconds, or half the track, whichever comes first. Plays were already recorded, but nothing distinguished a track that was listened to from one that was abandoned. The log is separate from the play count, it is what the listening statistics read, and it is exportable. The submission transport is deliberately not wired to anything: ShadowPlayer makes no network calls, and a Last.fm client would be the first one.
- **Skip counts, and smart-playlist rules over them.** A skip is a jump to another track before the current one was heard. The count lives on the track (migration V8), and "Skip count is at least N" is a smart-playlist field like any other — so the playlist that collects the tracks worth deleting is now a rule instead of a chore.
- **Importing the playlists other players left behind.** Rhythmbox's `playlists.xml`, iTunes' XSPF, and Audacious' `playlists.txt` are read from the places those players put them, the paths are matched against the library, and only the tracks that are actually here come across. A playlist whose files have all moved is shown as empty rather than imported as an empty list.
- **A "most skipped" list** in the library statistics, because the tracks you keep jumping past are the ones worth knowing about.

### Fixed
- The songs table's titles wrapped onto a second row instead of being elided, and the sort-direction arrow sat on top of the column's own label.
- A track without a DSP profile inherited the previous track's EQ instead of returning to the user's own settings.

---

## [0.7.0] - 2026-10-21

### Added
- **Quick-filter chips**: All / Lossless / Hi-Res / Favorites / Played narrow the current view without leaving it. The filtering was fully implemented and simply had no way to be reached.
- **Per-track quality tags**: every track carries a `HI-RES` or `LOSSLESS` tag in the song table, so the format of a file is visible at a glance. `Track::is_hires` is now the single definition of "Hi-Res", shared by the tag, the chip and the library filters.
- **Recently Added** in the sidebar: the nav item had a render path and a data path but no entry point.
- **Real cover art in the album and artist grids**: both painted a permanent placeholder, because artwork is cached per *track* and the grids only knew the album or artist. The library now keeps a representative track per album and per artist, and the grids load covers through the same per-frame decode budget the song table uses.
- **Empty states**: the song table, the queue panel and the grid views explain an empty list and offer a way out. An empty library offers an "Add a music folder" button, and a search that matched nothing offers to clear the search.
- **A real sort menu**: the sort pill draws a chevron, so it now opens a menu instead of cycling through the fields one click at a time. The active column also shows the sort direction, which used to be signalled by colour alone.
- **Motion**: the queue/lyrics pill slides between the tabs, the grid/list toggle fades, the navigation and collection highlights fade in, and the quick-filter chips fade to their active state. egui's animation helpers are used, so a repaint is only requested while something is moving.
- **`/` and `Ctrl+F`** jump to the search box, which could previously only be reached with the mouse.

### Fixed
- **The global keyboard shortcuts could never fire.** The guard read `if !i.focused`, but `InputState::focused` is the *native window* focus — whether the OS is delivering key presses here — not "a widget has focus". The bindings therefore only ran while the window was in the background, where no key event is delivered at all. Space/K, J, L, M, S, R and the four arrows were dead code, while the README and the in-app Help both advertised them. The guard now checks that a text field does not own the keyboard, and the plain-letter bindings ignore modifier chords so `Ctrl+M` and friends stay available to the OS.
- **The system clipboard**: eframe's `clipboard` feature was not enabled, so egui fell back to an app-internal clipboard and Ctrl+C/V never reached the OS.
- **The dialogs are real modals.** They were `egui::Window`s, which leave the library behind them live: a click behind a dialog still landed on the track table, and Tab walked straight out of it. They are `egui::Modal`s now, with a backdrop, a focus trap, and the app's own title bar. The keyboard focus moves into a dialog when it opens, so the user can start typing straight away.
- **Light-mode contrast**: `accent_highlight` was 3.74:1 and `text_dim` 2.97:1 against the page, and both are used for body text. The dark theme's `text_dim` failed too, at 3.53:1. All text tokens now clear WCAG AA in both themes, and the search field's border clears the 3:1 bar for a text input's boundary. Tests assert this, so it cannot regress.
- **The song table's header did not line up with its rows.** The header and the rows computed their positions separately, so the columns drifted apart and the header overflowed by 8 px. Both now share one column model, and artist, album and title are truncated instead of running over the duration column.
- **Three dialogs were larger than the app's own minimum window**, with `resizable(false)`, so their contents fell outside it with no way to reach them.
- **Escape closed two things at once** — the info popup and the layer behind it.
- **The queue/lyrics switcher, the queue reorder buttons and the favourite hearts** were hit-tested with raw pointer maths, so they were absent from the accessibility tree and unreachable by keyboard. They are real widgets now, and every interactive control carries an accessible name, a role and a tooltip.
- **The instrument panels ignored the theme.** The VU meters, the LUFS meter and the spatial radar hardcoded a dark navy that stayed dark in light mode.
- **The now-playing artwork jumped between three fixed sizes** instead of scaling with the card, and the transport row could overlap below about 250 px.
- **The song table read `SHADOWPLAYER_EVAL` from the environment** for every row of every frame, in release builds too. The check is behind the `eval-tools` feature now.
- The copy was a mix of "Favorites" and "Favourites" in the same interface; it is "Favorites" throughout, matching the rest of the app's US English.

### Changed
- **The Hi-Res and Lossless collections are gone.** They were unreachable nav items whose only entry point was a sidebar section that had been removed. The engine still plays those formats; the information now lives on the tracks themselves, as a per-track tag and as a quick filter.
- **One radius scale and one spacing scale.** The theme tokenised colour and nothing else: there were 240 corner-radius calls across 16 literal values, and 176 spacing overrides while `ui.spacing()` was read zero times. Every radius and every spacing now reads its value from the theme, and the egui widget radius follows the same scale.
- **One card width and one gutter** across the album, artist, genre and song grids, so the columns line up from one view to the next.
- The README's "Known limitations" no longer claims that screen-reader support is absent — accesskit was enabled all along, and the labelling is now much more complete.
- The startup log now reports both versions — `Version: ShadowPlayer v0.7.0 · audio-engine v1.3.1` — where it previously printed only the engine's, despite the README promising both.

---

## [0.6.0] - 2026-10-20

### Added
- **Obsidian & Jade visual identity**: an obsidian/graphite surface palette with jade actions, mint highlights, amber favourites and sky information accents; the purple/pink gradients and their remaining semantic names are gone. Both dark and light themes are covered.
- **Responsive player shell**: a full right-hand drawer on wide windows, a temporary overlay below 1100 px, a 200 px compact sidebar and a 60 px icon rail below 720 px. The normal window can be tiled or manually resized down to 640×360.
- **Compact mini-player**: a dedicated 320×110 always-on-top shell with transport, seek, drag-to-move and restore controls. Restoring returns to the previous normal geometry, and mini geometry is never persisted.
- **Grouped search**: artists, albums and playlists are shown as first-class results above the song table. Artist/album chips narrow the query; a playlist chip opens that playlist and clears the search.
- **Reorderable queues and playlists**: upcoming queue entries can be moved with hover controls while the current and already-played entries stay fixed; playlist rows get per-row move controls and stored positions are renumbered transactionally.
- **Windows and macOS media integration**: System Media Transport Controls and macOS Now Playing are driven through `souvlaki`, with metadata, artwork, playback state and transport commands bridged into the UI loop. Linux keeps its native MPRIS2 backend.
- **Accessibility**: AccessKit is enabled and transport controls, sliders, sidebar navigation, search chips, song and queue rows, and mini-player controls now carry accessible labels.
- **Bundled Inter typography**: Inter Regular and Medium are shipped with the app, with a 1.08 type scale and a legibility floor instead of relying on system fonts.

### Changed
- **Dependency refresh**: `rusqlite 0.40`, `lofty 0.25`, `rfd 0.17`, `zbus 6`, `dirs 7`, `rand 0.10` and `pollster 1.0`, while eframe/egui_extras remain on the current 0.37 line. Linux-only D-Bus dependencies are now target-scoped so Windows/macOS builds do not compile an unusable stack.
- **Linux MPRIS on zbus 6**: the D-Bus service moved to the async API, with workers driven by a small executor and transport commands still delivered through the UI command queue.
- **Responsive layout polish**: the Songs header hides only when there is genuinely no table height left, compact-sidebar branding and long labels no longer truncate, the queue overlay no longer shows a duplicate theme toggle, and opaque overlay backing prevents table content bleeding through.
- **Playback UI performance**: spectrum rendering repaints only during playback, artwork loading stays bounded, theme application happens only when the theme changes, queue rendering borrows the live queue, and media metadata/timeline publishing is throttled instead of running every frame.

### Fixed
- **Tag write-back with lofty 0.25**: R128 gain remains available for Vorbis-comment containers (FLAC/Ogg/Opus), while ID3v2/MP4/APE files retain standard ReplayGain 2.0 keys; the unsupported R128 peak no longer attempts to write an arbitrary vendor key.
- **Queue and playlist movement semantics**: moving a track is refused for the current/already-played part of the queue, and playlist reordering preserves a complete 0-based position sequence even across repeated moves.
- **CI coverage**: native Windows/macOS compile jobs, the engine's MPRIS feature check, and an explicit tag-write test job now cover integration paths that the default Linux test run cannot see.

---

## [0.5.0] - 2026-10-19

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
- **Failures are reported, and files are read once** (audit milestone P3): every engine-command send logs when the engine is gone, and the output device/backend setters report whether the command was delivered instead of letting a settings change appear applied; settings, scan-root, import and playlist writes surface their errors; a failed repository read logs instead of looking like an empty library. The metadata extractor switches to the engine's single `probe_metadata` pass, so a scan reads each file once (three opens and three tag parses became one), and the engine offers one probe API instead of two overlapping ones.
- **Engine honesty** (engine audit items E1-E25): the simulated PipeWire/JACK backends are deleted — an unimplemented backend is refused by name instead of "opening" silence; ASIO reports the driver's own rates, formats and channels; ALSA reads back the period/buffer size it negotiated; WASAPI pauses on the stream's idle event instead of busy-waiting on the caller thread; the true-peak meter is documented as the BS.1770-5 Annex 2 *definition* measured with the engine's own filter (not the published 48-tap table); the Shibata dither warns when an off-design rate reuses its 48 kHz curves; out-of-grid HRTF directions are reported as coverage instead of an unstated fallback; ADM capacity overflow is an error; the codec registry is reachable through the C FFI; `.oga` files are routed by content; and queue entries keep the playlist file's own metadata.
- **CI passes what it checks**: both workspaces are rustfmt- and clippy-clean under `-D warnings`, the engine's long-broken `channel_gains` doc example is fenced as a formula rather than compiled as a doctest, and the six crates whose `src/tests.rs` nested a same-named module no longer trip `module_inception`.

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

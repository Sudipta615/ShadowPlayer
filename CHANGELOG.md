# Changelog

All notable changes to **ShadowPlayer** are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

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

## [0.15.0] - 2026-10-23

*Released with audio-engine 1.8.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Auto-tagging can identify a file and show its evidence.** A selection is fingerprinted with
  AcoustID, looked up on MusicBrainz, and presented side by side with the tags already in the file,
  so the listener can apply only the fields they trust. Nothing is written until the review is
  accepted, and the lookup runs only when the action is invoked.
- **The interface speaks French, Spanish, Japanese and Chinese as well as German.** Each catalogue
  is compiled into the binary, a missing translation falls back to English, and `Auto` still
  follows the system locale. The skin, auto-tag and artwork screens are included.
- **Themes can be supplied as `.shadowtheme` files.** The JSON format is validated for WCAG AA text
  contrast before it is offered, merged over the current light or dark theme, and selectable live
  in Settings. An invalid theme is rejected with the reason, and `docs/themes/` contains a working
  example.
- **Cover art can be filled in for a whole selection.** The Cover Art Archive is queried in the
  background for albums with no artwork, results are cached locally, and an existing image is not
  replaced unless the listener asks for it.
- **Waveform seek bars show the shape of the track.** Peaks are computed once in the background
  and cached, so the now-playing bar and the full-screen view share the same waveform instead of
  decoding the file on the UI thread.
- **A crossfade can be heard before it is used.** A short local render demonstrates the selected
  curve with its waveform, without touching the queue or the current output.
- **Artists and albums can carry biographies.** The pane reads text the listener wrote, or
  `.txt`/`.md` files from a folder they choose, and caches the result in the library database. It
  never contacts the network.
- **Podcasts are subscriptions, not library tracks.** RSS and Atom feeds are fetched on a worker
  thread, episodes download into their own folder, and every episode keeps its own resume state.
  Removing a feed leaves the music library untouched.
- **`.lrc` lyrics can be edited in place.** The inline editor writes the sidecar back without
  disturbing the audio file, and the synced-lyrics panel picks up the change immediately.
- **Radio is local and seedable.** Starting from a track builds a station from genre, BPM and
  rating, and Refresh reshuffles it without contacting a streaming service.
- **The songs table has editable column presets.** Columns can be added, removed and resized, a
  preset can be copied or reset to its factory layout, and the chosen preset survives a restart.
- **Room correction has a measurement wizard.** The Settings → Tools card opens a guided flow:
  choose the microphone or interface input, choose the sweep length and pre-emphasis, start the
  measurement, and watch each engine stage. An explicit input never falls back to the system mix,
  and cancelling discards the sweep and the partial recording with a distinct cancelled event.
  A finished sweep can be saved as a WAV or loaded into the room-correction stage as an impulse
  response.
- **CD ripping through libcdio-paranoia.** The CD Ripper probes a drive, reads its table of
  contents, fetches CDDB metadata when `libcddb` is available, and extracts selected tracks to
  WAV, FLAC 16/24-bit or Opus. Native libraries are loaded at runtime, so the player still builds
  and runs without them, and the tool names the missing library instead of failing at startup.
  Existing files are never overwritten silently.
- **FLAC and Opus export.** A batch exporter decodes with the engine's own decoder and writes
  native FLAC at 16 or 24 bits, or Ogg-Opus at 48 kHz with a windowed-sinc resampler for other
  source rates. Output goes to a temporary sibling and is renamed only after the encoder closes
  successfully, so a failed or cancelled export cannot leave a truncated album file.
- **Album liner notes in Markdown.** The album detail view has a liner-notes card with an editor
  and a preview of a safe Markdown subset (headings, emphasis, lists, quotes, code and verified
  links). Notes are keyed by album identity and stored in the library database (schema V14), not
  written beside the music.
- **CLAP plugins can open their own editor.** The plugin rack exposes Open, Close and Fit controls
  for plugins that implement `clap.gui`; the editor is a floating native window, not an embedded
  surface. Plugins without a GUI report a clear error in the rack while parameters, bypass and
  presets stay available.
- **Headless mode.** `shadowplayer --headless [--play PATH]…` runs the audio engine, queues files,
  publishes media metadata and serves the same loopback control socket without opening a window.
  Transport, seek, volume and quit are driven by `shadowplayer-remote`.
- **An opt-in update check with verified downloads.** Settings → Tools can ask GitHub for the
  latest release, compare versions, select an asset for this platform, and download it. No
  checksum means no download; a mismatched SHA-256 is rejected. Nothing is installed or executed,
  there is no background polling, and the setting is off by default.

### Changed
- **BPM became part of the library and the smart-playlist vocabulary.** The BPM tag is extracted
  during the scan, stored by schema V13, and available to smart rules and the local radio seed.
- **Accessibility names reached the remaining painted controls.** Sort headers and the sort pill,
  row rating and overflow menus, playlist rename/delete/remove, album and genre rows, info-box
  dismiss, help signal cards, settings jumps and toggles, the waveform seek bar, the equalizer
  canvas and its switches, and the spatial radar now expose accessible names and roles. The
  equalizer graph also keeps its keyboard band editing.
- **The platform matrix tells the truth about capture and the network.** It no longer claims the
  app captures nothing or omits the microphone key; it documents room measurement, CD ripping,
  export, CLAP editors, headless mode, liner notes and opt-in updates, and the macOS bundle now
  carries `NSMicrophoneUsageDescription`.
- **README and Help describe the tools and the opt-in network paths** instead of the old
  "no CD/no transcoding/no auto-update/no network" list. Native library packages are named, and
  the limitations state that updates are never installed automatically and liner notes live in
  the database.

### Testing and release
- Auto-tag review and parsed lookups, skin contrast validation, cover-art batch state, podcast
  feed parsing and downloads, waveform peak buckets, crossfade envelopes, biography caching,
  column-preset round trips, lyrics editing, and the local radio seed are covered by unit tests.
- `media-tools` covers the dynamic-library loader, Markdown parsing and safe links, resampling,
  FLAC/Opus format selection, CDDB response parsing, release-asset selection and checksum
  rejection. Database tests cover the V13 and V14 migrations, podcast and biography repositories,
  and the album-notes repository. UI tests cover the measurement phases, the tools modal state and
  the liner-notes editor. Accessibility and documentation claims are checked by source and
  platform tests; CI now runs the `media-tools` suite with the player suite.

## [0.14.0] - 2026-10-22

*Released with audio-engine 1.7.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **The interface speaks German.** The copy moved into a generated catalogue: `crates/ui/build.rs`
  reads `crates/ui/locales/en.json` and `de.json` and emits the message table, so a string that is
  not in the catalogue does not compile. The language is chosen in Settings → *Library & App*, or
  left on *Auto*, where `LC_ALL`, `LC_MESSAGES` or `LANG` decide. A string with no translation
  falls back to English rather than showing a key, and the fallback is tested. The German catalogue
  covers the labels, buttons and menus; the long prose in the Help manual is still English.
- **Smart playlists say when they refresh.** A new setting chooses the policy: after every scan
  (the default), on a timer, or only when the user asks. The result is cached either way, so the
  timer and the manual policy do not re-run the query over the whole library every time the view is
  opened, and the *Refresh* button next to a smart playlist does what it says. The library-change
  policy notices the change before the list is rebuilt, so the answer is never one scan old.
- **The engine's creative effects reach the playback path.** The chorus, flanger, phaser,
  ping-pong delay and saturation that the engine already implemented are now a stage in the
  production graph — after the stereo stage, before the transport stages — with one command that
  carries the whole insert, so the settings can never disagree with each other. The DSP panel grew
  an *Effects* tab to drive them. Every effect is off until it is asked for, and the insert is
  transparent while they are off.
- **A 100,000-track library has published numbers.** `docs/PERFORMANCE.md` states what a library
  that size costs the UI, and `crates/ui/tests/library_budget.rs` asserts it in CI: a frame with
  100,000 tracks stays under 16 ms, a rebuild under 250 ms. The measurement is a release build, and
  the library is synthetic and deterministic, so the same work is measured on every machine.

### Changed
- **Changing the sort no longer copies the library.** The visible list was rebuilt from scratch
  whenever the sort changed: the library was copied out of the state — half a million string
  allocations at 100,000 tracks — and sorted again. The list that is already on screen is now
  re-ordered in place, and a counter in the library state makes the difference testable.
- **The sort is measured, not assumed.** Two alternatives were tried and rejected on the numbers:
  a case-insensitive comparison that allocates nothing was five times slower than the cached keys,
  and sorting an index permutation was slower than sorting the records directly. Both are recorded
  in `docs/PERFORMANCE.md` so they are not tried a third time.

### Fixed
- **The Settings jump survives a translation.** The Settings search jumped to a row named in
  English while the rows were rendered in the user's language, so the jump landed on nothing in
  German. The target is translated with the row.
- **The Help manual's interactive cards are marked in every language.** The card that says the
  section is interactive was recognised by an English word, so the marker disappeared once the
  interface spoke German. The comparison is against the translated word now.

### Testing and release
- The player suite covers the locale selection and the environment fallback, the three smart-playlist
  refresh policies, and the creative-effects insert (transparent while off, audible when engaged,
  and surviving a graph rebuild). The library budget is a release-mode test in CI, and the
  benchmark behind the published numbers compiles there too.

## [0.13.0] - 2026-10-22

*Released with audio-engine 1.6.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **Star ratings belong to the listener.** Rate any track from 1 to 5 in the songs table, clear it
  from the context menu, or rate a multi-selection in one action. Ratings live in the library
  database (schema V11), sort and filter beside the other columns, and survive a restart.
  `Ctrl + 1`–`Ctrl + 5` rate the selection (or the playing track when nothing is selected),
  `Ctrl + 0` clears it, and `Ctrl + Alt + Up/Down` steps it.
- **Four more playlist dialects.** PLS, XSPF, ASX and WPL are sniffed by content before the file
  extension is trusted, so a PLS file named `.m3u` still imports. The playlist editor exports
  M3U8 by default and offers every supported format in a dropdown; XSPF, ASX, WPL and PLS write as
  well as read.
- **Imports from six more players.** Winamp, foobar2000, MusicBee, iTunes, Clementine and
  Strawberry playlists are discovered alongside Rhythmbox and Audacious. Directory scans are
  deterministic, pointer files can carry `file://` values, and duplicate discoveries collapse into
  one row.
- **Smart playlists can ask about ratings and stream detail.** New fields: Rating, Has Lyrics,
  Bit Depth and Never Played. Invalid field/operator/value combinations are rejected before SQL
  runs, the editor shows the reason and disables Save, and a stored invalid rule matches nothing
  instead of silently matching everything.
- **One set of output facts.** The Settings diagnostics card and the Bit-Perfect Signal Path
  Inspector now render the same list of requested/negotiated backend, access mode, verification,
  device, format, buffer and period. ALSA reports its real period size through the engine's new
  `OutputInfo.period_frames`.
- **The platform matrix is generated.** `crates/ui/src/features.rs` is the source for both the
  in-app Help table and the README, and a test ties the claims to the `cfg` gates in the code.
- **The release artefacts describe the real bundle.** A committed `Info.plist` keeps the Dock icon,
  omits microphone permission, and is rewritten to the workspace version by
  `crates/app/macos/package-app.sh`; the Windows `.ico` and version resource are checked by
  structure tests. The MPRIS artwork helper now produces valid `file://` URIs for Windows drive and
  UNC paths, including when tested on a Unix host.
- **A platform-independent Pure Direct proof.** The null-output end-to-end suite captures what the
  sink receives after decode and the production DSP pipeline, and asserts it is bit-identical to
  the decoder's samples. The suite also proves it can detect a gain change.

### Fixed
- **Short null-output blocks were discarded.** The capture tail was written in an `else if` branch
  attached to underrun detection, so a stream's final partial block was counted as starvation and
  then thrown away. It is now always captured.
- **Descending sorts applied to only the last key.** `ORDER BY rating, title DESC` sorted the rating
  ascending; the direction now applies to every key. Favorites remain one-sided by design.
- **Windows and UNC playlist paths survive a Unix import.** `file://server/share/...` becomes a
  `//server/share/...` UNC path, drive-letter paths are not rebased under the playlist directory,
  and `localhost` is accepted case-insensitively.

### Testing and release
- Player suites now cover the V11 migration and indexes, rating persistence across reopen, rating
  clamp/bulk/sort/clear behaviour, smart-field SQL and invalid-rule matching, format sniffing and
  round trips with golden fixtures, discovery deduplication, UNC/drive imports, rating geometry and
  hit-testing, legacy preference defaults, and the bundle/icon inputs.
- CI runs the MPRIS playerctl-style integration test under `dbus-run-session` and a headless
  Wayland launch of the real release binary. The golden-screenshot harness is still absent from the
  tree, so the old job that referenced a non-existent test target was replaced by that smoke job.

## [0.12.0] - 2026-10-22

*Released with audio-engine 1.5.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **The full-screen Now Playing is a real player.** Previous, play/pause and next are SVG icons from
  the app's icon set instead of Unicode media symbols that Inter cannot draw; elapsed and remaining
  times sit beside the seek bar, the transport is pinned to the bottom of the viewport, and the
  lyrics panel moves to the right of the artwork so the group uses the whole window.
- **The song table has a column model.** Title gets the largest share (42 %, against 26 % each for
  artist and album), the header separators drag, and the widths persist across restarts. A density
  toggle switches between 36 px comfortable and 26 px compact rows, the quality chip is now a 3 px
  edge marker with a tooltip instead of consuming title width, and the context menu can reset the
  columns.
- **A shared text-truncation helper.** `components::text::truncated_galley` is now the one way the
  app lays out a single line: album, artist, genre, folder and queue titles elide with `…` instead of
  hard-clipping at the card edge.
- **A font-coverage guard.** `crates/ui/tests/font_coverage.rs` parses the cmap tables of both bundled
  Inter faces, scans user-facing string literals, and fails with the file, line and offending
  character when a glyph is missing. It also verifies every SVG referenced by `icons.rs`.
- **Theme and spacing contracts for future skins.** `crates/ui/tests/theme_purity.rs` rejects colour
  literals outside `theme.rs`; `crates/ui/tests/spacing_scale.rs` rejects literal radii and margins.
  `docs/THEMING.md` records the token names, WCAG contrast bars and asset requirements a skin must
  satisfy. Translucent fades now go through `theme::with_alpha`.

### Changed
- **The Listening History modal measures against its frame, not a nested layout.** The period buttons
  and the "First scrobble …" date are siblings with explicit widths, and both bar charts allocate
  the body's inner width, so nothing is clipped at 640, 900 or 1280 px. The headline tiles shrink to
  fit a 640 px window instead of widening the body.
- **Documentation claims are tested.** `crates/ui/tests/docs_truth.rs` checks the README shortcut
  table, limitations list, test commands and engine badge against their sources, the manual's live
  backend/codec/AutoEQ lists against the engine, and the newest CHANGELOG entry against the manifests.

## [0.11.0] - 2026-10-21

*Released with audio-engine 1.4.0. The player and the engine are versioned separately and released
together; the startup log prints both.*

### Added
- **The player has an icon, and so does the release.** `crates/app/assets/icon.svg` is the source of
  truth; the rasterised forms (16–1024 px PNG, `.ico`, `.icns`) are generated from it by
  `assets/make-icons.sh` and committed, and the window, the Explorer preview, the `.msi` and the
  `.dmg` all carry the same mark. The rasteriser is `resvg` — the same one the app uses to draw its
  SVG icons — so the launcher mark and the in-app mark are the same shape at every size.
- **A release pipeline.** `cargo-dist` builds the player for six targets and produces `.deb`,
  `.rpm`, `AppImage`, `.msi`, `.dmg` and shell installers with checksums; a tag push opens a
  **draft** GitHub Release for a human to read and publish. The tag's version must equal the
  workspace version, so a mistyped tag cannot produce a release whose name lies about what is
  inside it.
- **The engine ships the three binaries its README always described.** `audio-engine-cli` is a
  scriptable player and inspector (`--json` for one object per line, `probe` for a file's format and
  the engine's bit-perfect verdict, `--list-devices` for a backend's own devices); `replaygain-scanner`
  measures EBU R128 and ReplayGain 2.0 and can write the result back into the files; `engine-tui` is
  a Ratatui player whose meters, spectrum and EQ curve are read from the running engine and whose EQ
  panel writes back to it. The TUI's keybinding table is a specification now: a test asserts every
  binding in the README is implemented.
- **Real output-device pickers.** Each backend enumerates its *own* devices — the WASAPI render
  endpoints, the installed ASIO drivers, the macOS device UIDs — instead of borrowing cpal's list,
  which was wrong for every native backend: cpal's WASAPI backend opens shared-mode streams, and the
  ASIO drivers are not in cpal's list at all. The Settings page now offers a picker as well as the
  free-text field, and a device chosen from it is the device the engine opens.
- **A "This Build" section in the manual**, which lists the compiled backends, the decodable
  formats, the versions and the platform — read from the live engine rather than written out. Every
  capability claim in the manual that names a backend is now rendered from
  `engine::output::available_backends()`, so a build without the native backend cannot claim it.
- **A log file.** The app writes to stderr *and* to `shadowplayer.log` in its data directory, so a
  launch from the Explorer on Windows — which has no console — still leaves a record.

### Changed
- **Bit-perfect is now reachable on Windows.** The native WASAPI Exclusive and ASIO backends are
  compiled into the Windows build, so it negotiates a real exclusive stream instead of playing
  through the OS mixer. The escape hatch is one line in the manifest, and the Settings diagnostics
  card always reports which backend was *actually* negotiated.
- **The file a library row points at is identified by the filesystem, not by its name.** A file's
  identity is the device it lives on and its index within that device, so a rename — including a
  rename that only changed the case, which a case-insensitive filesystem treats as the same file
  under two spellings — keeps one row instead of becoming two tracks. Before this, a case-only
  rename on Windows or macOS produced a second track with its own play count, because the old
  spelling still resolved and was therefore never pruned.
- **The manual's claims are computed.** "Exclusive ALSA, WASAPI Exclusive, CoreAudio Hog, or ASIO"
  is now whatever the running build actually has; the AutoEQ section says how many measurements are
  bundled instead of claiming a database of four thousand; and the manual says outright when a
  build has no exclusive backend at all and is therefore not bit-perfect.

### Fixed
- **A duplicate row could survive a case-insensitive filesystem.** The V10 migration records each
  row's filesystem identity and merges the rows that turn out to be the same file, keeping the one
  with the most plays and moving the others' favourites, playlists, history and scrobbles to it. A
  hard link is *not* merged: its entry is still in the directory, so it is two entries on purpose.
- **A file that was renamed kept its old row and gained a new one.** The scanner now asks the
  filesystem what the file is before it asks what it is called.

---

## [0.10.0] - 2026-10-21

### Added
- **A permanent, local listening history.** Every counted play is now written to a `scrobbles` table (migration V9) that is never pruned: the artist, title, album, timestamp and listened time are copied in at write time, so the row survives the track being removed from the library, and nothing is ever sent anywhere. The seven-day `play_history` window stays for Recently Played.
- **Listening History**, the local answer to a Last.fm profile: week/month/year/all-time totals, the top artists, albums and tracks, what has been on repeat for the last seven days, 30-day and 24-hour charts, current and longest streaks, and CSV, JSON and Last.fm export. It is reached from the scrobble log or the command palette and never makes a network request.
- **Per-track DSP overrides are stored as a diff.** A track can override any part of the global chain — EQ, graphic EQ, tone, dynamics, bass management, LFE, channel mix — and only the parameters it actually changes are saved. Everything else is inherited at playback time, so a global compressor or crossfeed change reaches every track that did not override it, while the track's own curve and preamp keep winning. The editor lists the overridden parameters by panel block and says how many parameters differ from the global chain. Profiles saved by earlier versions (graphic EQ and preamp) are converted when applied.
- **Settings search.** The Settings page has a search field: typing shows matching labels, card titles and descriptions, and choosing a result switches to its category, scrolls the row into view and flashes it. The same index feeds the command palette, so any setting can be reached from `Ctrl+Space` without knowing which tab it lives on. A test asserts that the index and the rendered page contain exactly the same settings, so a setting cannot be added without becoming searchable.

### Changed
- **The scrobble log is now a permanent record.** It reads the full history instead of the seven-day window, reserves space for the timestamp so a long title cannot run underneath it, and links to Listening History above the rows rather than after a potentially endless list.
- **The per-track DSP dialog explains inheritance in terms of what was not overridden**, rather than naming blocks that the track may in fact own.

### Fixed
- **The scrobble log no longer loses its history after seven days.** The log read the rolling `play_history` window; it now reads the permanent table, and V9 backfills the rows the window still had.

## [0.9.0] - 2026-10-21

### Changed
- **Settings and Help now share the app's shell.** Both had drifted into looking like a different product: a 22px page heading printed directly under the topbar's own title, a second row of engine badges that belonged with the machine's state, cards with different margins in each page, and two different tab strips — Settings had an evenly-divided segmented control, Help a wrapped row of free-floating buttons, and the queue/lyrics panel a third. There is now one page shell (`components/page.rs`) and one strip; both pages are the same few parts as every other page.
- **The Help manual's 230px hero is gone.** An icon block, a two-line title, a version badge and a second row just for the search field pushed the actual manual below the fold on a laptop. The lead line, the search field and the section strip now take about a third of the space and the search sits exactly where a reader last left it.
- **Settings rows read as two columns.** The controls used to start after a fixed 240px label column and then stop in the middle of the card, leaving a gutter to the right; the label column now grows with the window and the sliders fill the row, so their value badges land on the card's edge.
- **The interactive sections of the manual say "Try it"**, so a sandbox is not mistaken for another paragraph.
- **The glossary is alphabetical and two-column**, which is how a glossary is used.
- **Dialogs shrink to their content.** A two-line dialog used to open at the full height of the window with an empty scroll area under it.

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

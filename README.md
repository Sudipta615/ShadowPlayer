<div align="center">

# ShadowPlayer

**Audiophile playback for the music you already own.**

A local-first desktop music player built in Rust: bit-perfect output, a
studio-grade DSP chain, a 100,000-track library that stays fast, and no
accounts, cloud, or telemetry.

[![Version](https://img.shields.io/badge/version-0.18.0-blue?style=flat-square)](CHANGELOG.md)
[![License](https://img.shields.io/badge/license-Apache--2.0-green?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows%20%7C%20macOS-lightgrey?style=flat-square)](#platform-support)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=flat-square)](https://www.rust-lang.org/)

[Why ShadowPlayer](#why-shadowplayer) ·
[Features](#feature-tour) ·
[Install](#installing) ·
[Build](#building-from-source) ·
[Shortcuts](#keyboard-shortcuts) ·
[Limitations](#known-limitations)

</div>

> **Bit-perfect is a verified state, not a marketing checkbox.** ShadowPlayer
> shows the backend, sample rate, bit depth, and access mode the engine actually
> negotiated. If exclusive output is refused, it says so and falls back to
> shared mode instead of keeping the claim.

---

## Why ShadowPlayer

Most players ask you to choose between convenience and fidelity. ShadowPlayer
is built on the belief that you should not have to.

| | |
| :--- | :--- |
| **Honest audio** | Every output claim comes from the live engine. Pure Direct bypasses the entire DSP chain, and the app names the stages it skipped. |
| **Local by default** | Your library, play history, ratings, playlists, and settings live in one SQLite database on your machine. Network features are explicit, opt-in actions. |
| **Built for real libraries** | The songs table is virtualized. A 100,000-track library draws in roughly **0.23 ms per frame**, and sorting re-orders the visible list instead of rebuilding the library. |
| **A serious DSP studio** | Parametric and graphic EQ, room correction, crossfeed, spatial audio, creative effects, CLAP plugins, and named chain presets — with a one-click path back to untouched audio. |

ShadowPlayer is the desktop player in this repository. The audio engine
(`crates/engine`) is a separate, independently versioned Rust workspace with
its own README, test matrix, and C ABI.

---

## Feature tour

### Library

- **Scans that follow the disk.** Recursive folder roots, per-root filesystem
  watchers, explicit re-scans, single-file import, and deletion reconciliation.
- **Identity that survives a move.** A renamed or relocated file keeps its play
  count, favorites, ratings, playlist membership, and lyrics.
- **Views for the way you listen.** Songs, Albums, Artists, Genres, and
  Folders; sortable and re-orderable columns; saved column presets; quick-filter
  chips; multi-select and bulk actions.
- **Metadata you approve.** AcoustID + MusicBrainz auto-tagging with a
  field-by-field review before anything is written, and bulk cover-art lookup
  from the Cover Art Archive with a local cache.
- **Notes and biographies.** Markdown album liner notes stored in the library,
  plus artist and album biographies from text you write or `.txt`/`.md` files
  you point at.
- **Playlists that fit the job.** Standard playlists; smart rule-based
  playlists with a stated refresh policy; duplicate finding; a batch organizer
  with a reviewable rename plan; M3U import and export.
- **Podcasts and local radio.** RSS/Atom subscriptions with per-episode
  downloads and resume, plus smart radio seeded from a track's genre, BPM, and
  rating.
- **Lyrics where you expect them.** Embedded tags, `.lrc` sidecars, ID3v2
  `SYLT` timed lyrics, non-UTF-8 files, a synced-lyrics panel, and an inline
  `.lrc` editor.

### Playback

- **Gapless, crossfade, fade, and hard-stop transitions.** Shuffle and repeat
  work with a queue that survives a restart.
- **Pure Direct.** A one-switch path that bypasses the entire DSP chain, with a
  modal that names exactly which stages are out of the signal path and why.
- **Format coverage from the engine.** FLAC, ALAC, WAV, AIFF, MP3, AAC, Ogg
  Vorbis, Opus, WavPack, DSD, CUE sheets, and the rest of the engine's decoders.
- **ReplayGain and EBU R128.** Read from tags, from the on-disk loudness cache,
  or from a library-wide scan.
- **Waveform seek bars.** Peaks are computed once in the background and shared
  by the now-playing bar and the full-screen view.
- **Audition before commit.** A crossfade can be rendered locally and heard
  against the current queue without changing it.

### DSP studio

- Parametric / AutoEQ and graphic EQ (10–64 bands) with one explicit mode
  selector, one shared preamp, complete preset curves, dynamics, filters,
  crossfeed, stereo enhancement, spatial and screen audio, room correction,
  and creative effects including chorus, flanger, phaser, delay, and
  saturation. The true-peak limiter is on by default; static auto-headroom is
  opt-in and clearly labelled as fixed attenuation rather than a compressor.
- Text-curve import and export for AutoEQ, Equalizer APO, and REW files, plus
  a durable preset library that can bind a curve to an output device or a
  track.
- 8,850 AutoEQ parametric corrections with their measurement source, a
  derived preamp, and search. AutoEQ is MIT-licensed; the notice ships in
  Help and in `crates/ui/assets/autoeq/ATTRIBUTION.md`.
- A CLAP plugin rack with host bypass, parameter access, and native editor
  windows for plugins that implement `clap.gui`.
- Named chain presets, per-track DSP profiles, and a global chain that can be
  restored in one action.
- A room-correction measurement wizard that records a sweep from the input you
  choose, shows each engine stage, auditions the result, and cancels cleanly.

### Tools

- **CD ripper.** Probe a drive, fetch CDDB metadata when available, select
  tracks, and extract to WAV, FLAC 16/24-bit, or Opus.
- **Transcode export.** Batch-copy local files to FLAC or 48 kHz Opus, carrying
  tags where the source and target formats allow.
- **Opt-in update check.** Contacts GitHub only when asked, selects the right
  asset, requires a published SHA-256 checksum, verifies the bytes, and hands
  back a file. It never installs or runs the download.
- **Loudness inspector, scrobble log, listening history, and library
  statistics** for the questions a music collection inevitably raises.

### Desktop integration

- MPRIS2 on Linux, System Media Transport Controls on Windows, and Now Playing
  on macOS, including artwork and media-key transport.
- Native file and folder pickers, including the XDG portal under Wayland.
- Six interface languages: English, German, French, Spanish, Japanese, and
  Chinese.
- Headless mode for servers and scripted listening sessions.
- Light, dark, and OLED themes, plus custom `.shadowtheme` files with a live
  editor and WCAG AA contrast validation.

---

## The signal path

```text
  your files
      │
      ▼
  scanner / library ──────► SQLite (tracks, playlists, history, settings)
      │
      ▼
  queue + playback policy
      │
      ▼
  audio-session ──► engine ──► DSP chain ──► exclusive output ──► DAC
                       │            │
                       │            └── Pure Direct bypasses this
                       └── verified backend, rate, depth, and access mode
```

The important part is the feedback loop: the UI never guesses what the engine
did. It reads the negotiated output state and renders that, including the reason
when a request could not be honoured.

### Engineering promises

- **0% CPU when idle.** Repaints are event-driven; playback and animation ask
  for frames, an idle app does not.
- **No I/O on the UI thread.** Scans, tag reads and writes, artwork, waveform
  decoding, network requests, and CD extraction run on workers.
- **Virtualized lists.** The drawing cost is proportional to visible rows, not
  library size.
- **A tested performance budget.** CI keeps a 100,000-track library inside the
  numbers published in [docs/PERFORMANCE.md](docs/PERFORMANCE.md).

---

## Installing

Releases are published as GitHub artefacts and native packages. The release
workflow builds six targets and attaches the installers to a draft GitHub
Release.

| Platform | What you get |
| :--- | :--- |
| Linux x86-64 / arm64 | `.deb`, `.rpm`, `AppImage`, and a shell installer |
| Windows x86-64 / arm64 | `.msi` and a `.zip` |
| macOS x86-64 / arm64 | `.dmg` and a `.tar.gz` |

Download the artefact for your platform from the
[releases page](https://github.com/Sudipta615/audio_engine/releases) and run it.
Nothing is installed outside your user account except by the `.deb`, `.rpm`,
and `.msi` packages, which install the binary and the icon.

**From source**

```bash
git clone <this repo> && cd shadowplayer
cargo build --release
cargo run --release -p shadowplayer
```

### First run

1. Open **Folders & library manager** and add the folders that hold your music.
2. Open **Settings → Output** and choose the device you want. The diagnostics
   card shows the backend and access mode that were actually negotiated.
3. Press **Space** and listen. If you want the shortest possible path to the
   DAC, enable **Pure Direct** with `Ctrl + B`.

Nothing is written next to your music unless you explicitly organize files,
write ReplayGain tags, rip a CD, or run a transcode export. Each of those
actions shows its destination and refuses to overwrite an existing file
silently.

---

## Platform support

What works where. The table below is generated from
`crates/ui/src/features.rs`, and the "This Build" section of the in-app manual
renders the same matrix. A test fails if either drifts from the platform-`cfg`
gates in the code.

<!-- platform-features:start -->
| Capability | Linux | Windows | macOS | Notes |
| :--- | :---: | :---: | :---: | :--- |
| Bit-perfect exclusive output | Yes | Yes | Yes | ALSA hw: on Linux; WASAPI Exclusive and ASIO on Windows; CoreAudio hog mode on macOS. |
| System media integration | Yes | Yes | Yes | MPRIS2, System Media Transport Controls, and macOS Now Playing respectively. |
| MPRIS2 and playerctl | Yes | - | - | org.mpris.MediaPlayer2.shadowplayer on the session bus, including play/pause and metadata. |
| SMTC artwork | - | Yes | - | Track artwork and metadata in the Windows media flyout and hardware media keys. |
| macOS Now Playing | - | - | Yes | souvlaki publishes metadata and artwork; the app bundle remains a normal foreground app (LSUIElement = false). |
| Native file and folder picker | Yes | Yes | Yes | rfd with the XDG desktop portal on Linux, and the native dialogs on Windows and macOS. |
| Wayland portal picker | Yes | - | - | The XDG portal backend is used under Wayland; the picker does not depend on an X11 display. |
| Windows application icon | - | Yes | - | The executable and window carry the committed multi-resolution .ico. |
| macOS app bundle | - | - | Yes | The committed package-app.sh wraps the binary in a foreground .app with the icon and the microphone usage description. |
| Room-correction microphone capture | Yes | Yes | Yes | Records the measurement sweep from a chosen input; the macOS bundle declares NSMicrophoneUsageDescription. |
| CD ripping | Yes | Yes | Yes | Audio CDs through dynamically loaded libcdio/libcdio-paranoia; CDDB metadata is optional through libcddb. |
| FLAC and Opus export | Yes | Yes | Yes | 16/24-bit FLAC and 48 kHz Ogg-Opus through dynamically loaded libsndfile. |
| CLAP plugin editor windows | Yes | Yes | Yes | A plugin that implements clap.gui opens its own floating native editor from the plugin rack. |
| Headless mode | Yes | Yes | Yes | --headless runs the engine and the loopback control socket without opening a window. |
| Opt-in update check | Yes | Yes | Yes | Contacts GitHub only when enabled; a download is SHA-256 verified and is never installed automatically. |
| Markdown album liner notes | Yes | Yes | Yes | Notes are stored in the library database and rendered in a safe Markdown subset. |
| Null output backend | Yes | Yes | Yes | A platform-independent output used by the bit-identical Pure Direct test. |
<!-- platform-features:end -->

**Bit-perfect means what it says, and the app tells you when it cannot.**
Exclusive output requires the operating system's permission: on Linux the
device must be openable directly (being in the `audio` group usually is
enough), on Windows the endpoint must not be held by another application, and
on macOS the app must be able to claim the device. When it cannot, the app
says which backend it asked for, which one it got, and whether the access was
*verified* — it never keeps the claim it could not keep. The Settings
diagnostics card and the Bit-Perfect Signal Path Inspector both show it.

---

## Keyboard shortcuts

| Key | Action |
| :--- | :--- |
| Space / K | Play / pause the active track |
| J | Skip to the previous track in the queue |
| L | Skip to the next track in the queue |
| M | Toggle mute / unmute on the master output |
| S | Toggle random shuffle |
| R | Cycle repeat mode (off → all → one) |
| Left Arrow | Seek backward by 5 seconds |
| Right Arrow | Seek forward by 5 seconds |
| Up Arrow | Raise the master volume by 5 % |
| Down Arrow | Lower the master volume by 5 % |
| / | Jump to the search box |
| Ctrl + F | Jump to the search box |
| Ctrl + Space | Open the command palette |
| F | Full-screen Now Playing |
| Ctrl + B | Toggle bit-perfect Pure Direct mode |
| Ctrl + P | Open Settings & DSP studio |
| Ctrl + N | Open the playlist editor (create or manage playlists) |
| Ctrl + O | Open Folders & library manager |
| Ctrl + E | Toggle the equalizer & DSP panel |
| Ctrl + 1 to 5 | Rate the selected tracks or the playing track 1–5 stars |
| Ctrl + 0 | Clear the rating on the selection or the playing track |
| Ctrl + Alt + Up / Down | Step the selection's rating up or down |
| Escape | Close the open dialog (one layer per press), or clear the search |

Every shortcut is registered in `crates/ui/src/shortcuts.rs`; the in-app Help
view and this table are checked against that file.

---

## Remote control

`shadowplayer-remote` drives the running window over its loopback socket. It
never opens the library or the audio device, and the socket listens only on
`127.0.0.1` with a random per-launch token in `<data_dir>/instance.json`:

```bash
shadowplayer-remote play
shadowplayer-remote seek -10        # relative seconds
shadowplayer-remote seek-to 120     # absolute seconds
shadowplayer-remote volume +5       # percentage-point delta
shadowplayer-remote state
shadowplayer-remote quit
```

The same socket makes a second launch activate the first window. There is no
option to bind a LAN address, so this does not add a network service.

---

## Headless mode

For a server, a minimal desktop, or a scripted listening session, `--headless`
runs the audio engine and the same loopback control service without creating a
window:

```bash
shadowplayer --headless --play /music/album/01.flac --play /music/album/02.flac
shadowplayer-remote state
shadowplayer-remote play
shadowplayer-remote quit
```

The process queues every file passed with `--play`, opens the output device,
and accepts transport, seek, volume, and quit commands. It publishes the same
media metadata as the windowed build, so desktop media integration keeps
working. Without `--play` it idles until a command arrives.

---

## Building from source

```bash
git clone <this repo> && cd shadowplayer

cargo build --release          # the player and its path dependencies
cargo run --release -p shadowplayer
```

The engine is a path dependency outside the player workspace, so it builds with
the player in one command — no extra step.

**Toolchain.** Current stable Rust. The engine declares `rust-version = "1.98"`
and one of its subcrates uses edition 2024; an older toolchain fails with a
clear version error from the engine, not from the app.

**Linux system packages** (Debian/Ubuntu names):

```bash
sudo apt install build-essential pkg-config libasound2-dev \
                 libxkbcommon-dev libwayland-dev libx11-dev
```

ALSA exclusive mode needs access to the device: being in the `audio` group is
usually enough, and the Settings diagnostics card reports the backend that was
actually negotiated when it is not.

The CD ripper and the FLAC/Opus exporter load their native libraries at runtime,
so no development headers are needed to build. On Debian/Ubuntu the optional
runtime packages are `libcdio-cdda2`, `libcdio-paranoia2`, `libcddb2`, and
`libsndfile1`; a machine without them still builds, plays, and updates, and the
Tools card names the missing library when you try to use that tool.

**Windows / macOS.** `cargo build --release` works out of the box. The Windows
build compiles the engine's native exclusive backends (`wasapi-native` and
`asio-native`) in, so it negotiates a real exclusive stream rather than the OS
mixer; the macOS build uses CoreAudio hog mode. If the native path is
unavailable, the app falls back to shared mode **and says so**. See
`crates/engine/README.md` for the backend details.

### Where your data lives

| Platform | Database |
| :--- | :--- |
| Linux | `~/.local/share/shadowplayer/library.db` |
| macOS | `~/Library/Application Support/shadowplayer/library.db` |
| Windows | `%LOCALAPPDATA%\shadowplayer\library.db` |

The database is an ordinary SQLite file. There is no automatic backup: copy it
if it matters to you. Scans never delete audio files, only rows whose file is
gone. Album liner notes live in the same database, not beside the music.

---

## Performance

The numbers below are measured on production code paths with a **100,000-track**
library in a **release** build. They are budgets, not anecdotes: CI asserts them.

| Operation | Cost | Budget |
| :--- | ---: | ---: |
| Frame with a 100,000-track library | 0.23 ms (worst 0.29 ms) | 16 ms |
| Rebuild of the visible list | 131 ms | 250 ms |
| Sort of 100,000 tracks by title | 73 ms | 250 ms |
| Copy of 100,000 tracks | 64 ms | 250 ms |
| A view that did not change | 0 ms | — |

A 60 fps frame is 16.7 ms. The frame itself stays well under a millisecond
regardless of library size because the songs table only builds the rows on
screen. Rebuilds happen when the view changes — a tab, filter, or search — not
every frame. See [docs/PERFORMANCE.md](docs/PERFORMANCE.md) for the methodology
and the alternatives that were measured and rejected.

---

## Tests

```bash
# Player workspace (engine excluded: it has its own workspace and suite)
cargo test -p domain -p database -p metadata -p lyrics -p library \
           -p playback -p search -p audio-session -p media-tools -p ui --lib \
           -- --test-threads=1

# The version bump is guarded by a test, not by remembering to do it.
cargo test -p shadowplayer --test version_lockstep

# The library budget: a 100,000-track library inside the published budgets.
cargo test --release -p ui --test library_budget -- --nocapture

# The library benchmark behind those numbers.
cargo bench -p ui --bench library_100k

# Engine (its own workspace)
cd crates/engine && cargo test

# The engine's benchmark suite
cd crates/engine && cargo bench
```

CI runs `cargo fmt --check`, `clippy -D warnings`, the player suite, the
engine's suite on Linux, Windows, and macOS, the benchmark compile, the library
budget, and the documentation-truth checks on every push. See
`.github/workflows/ci.yml` for the exact commands.

### Development-only automation

`cargo run -p shadowplayer --features eval-tools` compiles the
`SHADOWPLAYER_EVAL*` hooks: they drive the UI to a chosen state, take a
screenshot, and exit. The feature is off by default, so those hooks — and the
`process::exit` they can reach from an environment variable — are not part of a
normal build, and neither are the CLAP eval script or the scaffolding behind
them.

### Releasing

The player and the engine are versioned separately — the engine carries its own
version because it is a standalone library with its own workspace and tests —
but they are released together. To cut a release:

1. Bump `version` in `crates/*/Cargo.toml` (the player workspace) and, when the
   engine changed, `crates/engine/Cargo.toml` and its subcrates too.
2. Move the `[Unreleased]` section of `CHANGELOG.md` under the new version and
   date, and check the version badges in `README.md` and
   `crates/engine/README.md`.
3. Run the app once: startup prints both versions from `engine::VERSION` and
   `engine::NAME`, which is the source of truth if a build ever reports the
   wrong pair.
4. Tag the commit `v<version>` and push the tag. The release workflow checks
   that the tag matches the workspace version, builds the artefacts, and opens
   a **draft** release.

Step 1 is guarded by `crates/app/tests/version_lockstep.rs`, which fails if the
player crates disagree, if the engine's subcrates disagree, if the CHANGELOG
names a different version than the manifests, or if the engine README quotes a
version that is no longer true.

---

## Known limitations

These are boundaries we cannot honestly remove with a documentation edit.
Deliberate product choices live in
[What ShadowPlayer is not](#what-shadowplayer-is-not).

- **Pre-1.0 formats can change.** Database schemas and settings formats may
  migrate between releases. Migrations run automatically and are one-way.
- **Exclusive output can be refused by the operating system.** ALSA, WASAPI
  Exclusive, ASIO, and CoreAudio hog mode all depend on device access that
  another application, a driver, or a permission prompt can deny. ShadowPlayer
  falls back to shared mode and reports the backend it actually got.
- **Some format and tooling boundaries are inherited.** Musepack (`.mpc`) and
  TAK (`.tak`) have no validated pure-Rust decoder in this tree and fail with a
  named error. CD ripping and FLAC/Opus export load `libcdio`/`libcdio-paranoia`,
  optionally `libcddb`, and `libsndfile` at runtime; the build succeeds without
  them and the Tools card names anything missing.
- **Accessibility is broad, not certified.** AccessKit is enabled, icon-only
  controls carry accessible names, and the main editing surfaces work from the
  keyboard. The app has not had a professional screen-reader audit, so the
  keyboard remains the most thoroughly exercised path.
- **The long-form manual is English.** The interface catalogues cover English,
  German, French, Spanish, Japanese, and Chinese; the Help manual's prose is
  still English. Missing strings fall back to English instead of showing a key.
- **Engine-level boundaries are inherited in full.** There is no talkback or
  duplex monitoring. Ambisonic bus rotation is exact only for orders 1–3. The
  N-input mix bus has no automatic summation headroom. `plugin-dylib` plugins
  run in-process without a sandbox, so only load plugins you trust. CI
  validates the output path through a null backend rather than a physical DAC;
  real driver negotiation and hardware clocks remain environment-dependent.
  See the [engine limitations](crates/engine/README.md#-known-limitations) for
  the precise contracts.

---

## What ShadowPlayer is not

These are deliberate omissions, not missing features. They keep the app small,
predictable, and focused on the music you own.

- **Not a streaming client, account system, or telemetry product.** No cloud
  library, no recommendation feed, no usage reporting. Auto-tagging, cover-art
  lookup, podcast subscriptions, and the update check contact the network only
  when you invoke them.
- **Not a video player.** Audio is the product.
- **Not a background updater.** The update check is opt-in, downloads are
  SHA-256 verified, and installing is a deliberate act you perform.
- **Not a LAN remote-control server.** The control socket binds only to
  `127.0.0.1` and uses a fresh per-launch token.
- **Not a multi-window free-for-all.** A second launch activates the running
  window by default. Concurrent transports are an explicit Settings opt-in;
  the first window owns the remote-control socket.
- **Not a GUI-less library manager.** Headless mode deliberately exposes
  transport, seek, volume, metadata, and quit over the loopback socket. Library
  management, CD ripping, and the DSP studio are windowed features.
- **Not an installer for your operating system.** It produces native packages
  and verified downloads; applying them is your decision.

---

## License

Apache-2.0 — see [`LICENSE`](LICENSE). The engine (`crates/engine`) is licensed
the same way and ships its own copy at `crates/engine/LICENSE-APACHE`.

# ShadowPlayer

A local-first, audiophile desktop music player for Linux, Windows and macOS:
Rust, [egui](https://github.com/emilk/egui), SQLite, and a real bit-perfect audio
engine — no accounts and no telemetry. Network access exists only for features
you switch on: auto-tagging, cover-art lookup, podcast subscriptions, and the
opt-in update check.

The player is the `crates/` workspace. The audio engine it is built on
(`crates/engine`) is a separate, independently versioned workspace with its own
README, test matrix and C ABI; it is consumed here as a path dependency.

---

## What it does

**Library**
- Folder scans and single-file imports, with a filesystem watcher per root and
  deletion reconciliation, so the library tracks the disk without a manual
  rescan.
- Stable track identity: a file that is moved or renamed keeps its play count,
  favorites, playlist membership and lyrics.
- Songs table with sortable, re-orderable columns, saved column presets,
  quick-filter chips, multi-select and bulk actions; Albums, Artists, Genres and
  Folders views.
- Auto-tagging through AcoustID fingerprints and MusicBrainz, with a
  field-by-field review before any tag is written; bulk cover-art lookup from
  the Cover Art Archive with a local cache.
- Artist and album biographies from text you write or `.txt`/`.md` files you
  point at, cached in the library database.
- Podcast subscriptions (RSS/Atom) with per-episode downloads and resume, plus
  local smart radio seeded from a track's genre, BPM and rating.
- Standard playlists, smart (rule-based) playlists with a stated refresh
  policy — after every scan, on a timer, or only when you ask — duplicate
  finder, batch organizer with a reviewable rename plan, M3U import/export.
- Embedded tag lyrics and `.lrc` sidecars (including ID3v2 `SYLT` timed lyrics
  and non-UTF-8 files), with a synced-lyrics panel and an inline `.lrc` editor.
- Markdown album liner notes, previewed in the album view and stored in the
  library database alongside the album identity.

**Playback**
- Gapless, crossfade, fade and hard-stop transitions; shuffle and repeat with a
  queue that survives a restart. Waveform seek bars share a cached peak table,
  and a crossfade can be auditioned locally before it is used.
- Bit-perfect "Pure Direct" mode that bypasses the whole DSP chain, with a
  modal that says exactly which stages are bypassed and why.
- CUE sheets, DSD, all codecs the engine ships; ReplayGain / EBU R128 (from
  tags, from an on-disk loudness cache, or from a library-wide scan).
- Full DSP studio: parametric and graphic EQ, dynamics, filters, crossfeed,
  stereo enhancement, spatial/screen audio, room correction, creative effects
  (chorus, flanger, phaser, delay, saturation), CLAP plugin rack and named
  chain presets. Plugins that implement `clap.gui` open their own editor window;
  the rack keeps parameters and host bypass reachable when they do not.

**Tools**
- Room-correction measurement wizard: choose the input device and sweep length,
  watch each stage, audition the result, and cancel without keeping a partial
  recording.
- CD ripper: probe a drive, fetch CDDB metadata when available, pick tracks, and
  extract to WAV, FLAC 16/24-bit or Opus.
- Transcode export: batch-copy local files to FLAC or 48 kHz Opus, with tags
  copied where the source and format allow.
- Opt-in update check against GitHub releases. A download is SHA-256 verified
  and handed back as a file; ShadowPlayer never installs or runs it.

**Desktop**
- MPRIS2 on Linux (media keys, shells, `playerctl`), System Media Transport
  Controls on Windows, Now Playing on macOS.
- Headless mode (`--headless`) runs the engine and the loopback control socket
  without a window, for a server or a minimal listening session.
- Light, dark, and OLED themes, custom `.shadowtheme` themes, an interface in English,
  German, French, Spanish, Japanese or Chinese, toasts for anything that fails,
  and a Help view whose shortcut list is generated from the same descriptions
  the app uses.

---

## Installing

Releases are published as GitHub artefacts and as native packages. The
release workflow builds six targets and attaches the installers to a draft
GitHub Release.

| Platform | What you get |
| :--- | :--- |
| Linux x86-64 / arm64 | `.deb`, `.rpm`, `AppImage`, and a shell installer |
| Windows x86-64 / arm64 | `.msi` and a `.zip` |
| macOS x86-64 / arm64 | `.dmg` and a `.tar.gz` |

Download the artefact for your platform from the
[releases page](https://github.com/Sudipta615/audio_engine/releases) and run
it. Nothing is installed outside your user account except by the `.deb`,
`.rpm` and `.msi` packages, which install the binary and the icon.

**From source**

```bash
git clone <this repo> && cd shadowplayer
cargo build --release
cargo run --release -p shadowplayer
```

---

## What ShadowPlayer is not

A player with a hundred features is not the same as a player that is good at
one thing. This is what ShadowPlayer deliberately does not do, so that nobody
has to discover it by disappointment:

- **No streaming services, internet radio, accounts or telemetry.** Auto-tagging,
  cover-art lookup, podcast subscriptions and the update check contact the network
  only when you ask them to. Your library, listening history and statistics stay
  on your machine.
- **No automatic updates.** The update check is opt-in, a download is verified
  against the release's SHA-256 checksum, and the app never installs or runs
  the file for you. Installing is a deliberate act.
- **No video playback.**
- **One transport owner at a time.** A second launch brings the running
  window forward and exits. Concurrent windows are an explicit opt-in in
  Settings → Library; each has its own playback, while the first window owns
  the remote-control socket.
- **Six interface languages, and the long prose is not fully translated.**
  English, German, French, Spanish, Japanese and Chinese catalogues cover
  labels, buttons and menus; the long prose in the Help manual is still
  English, and a missing string falls back to English rather than showing a
  key.

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

## Building and running

```bash
git clone <this repo> && cd shadowplayer

cargo build --release          # workspace: player + crates
cargo run --release -p shadowplayer
```

The engine is a path dependency outside the workspace, so it builds with the
player in one command — no extra step.

**Toolchain.** Current stable Rust. The engine declares `rust-version = "1.98"`
and one of its subcrates uses edition 2024; if your toolchain is older you will
get a clear version error from the engine, not from the app.

**Linux system packages** (Debian/Ubuntu names):

```bash
sudo apt install build-essential pkg-config libasound2-dev \
                 libxkbcommon-dev libwayland-dev libx11-dev
```

ALSA exclusive mode needs access to the device: being in the `audio` group is
usually enough, and the Settings tab's diagnostics card reports the backend that
was actually negotiated when it is not.

The CD ripper and the FLAC/Opus exporter load their native libraries at runtime,
so no development headers are needed to build. On Debian/Ubuntu the optional
runtime packages are `libcdio-cdda2`, `libcdio-paranoia2`, `libcddb2` and
`libsndfile1`; a machine without them still builds, plays and updates, and the
Tools card names the missing library if you try to use that tool.

**Windows / macOS.** `cargo build --release` works out of the box. The Windows
build compiles the engine's native exclusive backends (`wasapi-native` and
`asio-native`) in, so it negotiates a real exclusive stream rather than the
OS mixer; the macOS build uses CoreAudio hog mode. If the native path is
unavailable, the app falls back to shared mode **and says so** — the Settings
diagnostics card reports the backend that was actually negotiated. See
`crates/engine/README.md` for the backends in detail.

**Where your data lives**

| Platform | Database |
| :--- | :--- |
| Linux | `~/.local/share/shadowplayer/library.db` |
| macOS | `~/Library/Application Support/shadowplayer/library.db` |
| Windows | `%LOCALAPPDATA%\shadowplayer\library.db` |

Nothing is ever written next to your music except when you explicitly organize
files, write ReplayGain tags, rip a CD or run a transcode export. Each of those
actions shows its destination and never silently overwrites an existing file.

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

### Remote control

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

The same socket is what makes a second launch activate the first. There is no
option to bind a LAN address, so this does not add a network service.

### Headless mode

For a server, a minimal desktop or a scripted listening session, `--headless`
runs the audio engine and the same loopback control service without creating a
window:

```bash
shadowplayer --headless --play /music/album/01.flac --play /music/album/02.flac
shadowplayer-remote state
shadowplayer-remote play
shadowplayer-remote quit
```

The process queues the files given with `--play` (repeat the flag for more than
one), opens the output device, and accepts transport, seek, volume and quit
commands over the loopback socket. It publishes the same media metadata as the
windowed build, so desktop media integration keeps working. Without `--play` it
idles until a command arrives.

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

CI runs `cargo fmt --check`, `clippy -D warnings`, the test suite above, the
engine's suite, the engine's suite **on Windows and macOS as well as Linux**,
the benchmark suite's compile, the library budget, and the
documentation-truth checks, on every push; see `.github/workflows/ci.yml`.

### A big library

The player is built for libraries that are larger than a demo: the songs
table is virtualised, so the frame cost does not grow with the library, and
changing the sort re-orders the list that is already on screen instead of
copying the library again. The numbers are published in
[docs/PERFORMANCE.md](docs/PERFORMANCE.md), and CI asserts them.

### Development-only automation

`cargo run -p shadowplayer --features eval-tools` compiles the
`SHADOWPLAYER_EVAL*` hooks: they drive the UI to a chosen state, take a
screenshot and exit. The feature is off by default, so those hooks — and the
`process::exit` they can reach from an environment variable — are not part of a
normal build, and neither are the CLAP eval script or the ~500 lines of
scaffolding behind them.

### Releasing

The player and the engine are versioned separately — the engine carries its own
version (`crates/engine`) because it is a standalone library with its own
workspace and tests — but they are released together. To cut a release:

1. Bump `version` in `crates/*/Cargo.toml` (the player workspace) and, when the
   engine changed, `crates/engine/Cargo.toml` and its sub-crates too.
2. Move the `[Unreleased]` section of `CHANGELOG.md` under the new version and
   date, and check the version badge in `crates/engine/README.md`.
3. `cargo run` prints both versions at startup from `engine::VERSION` and
   `engine::NAME`; the log is the source of truth if a build ever reports the
   wrong pair.
4. Tag the commit `v<version>` and push the tag. The release workflow checks
   that the tag matches the workspace version, builds the artefacts, and opens
   a **draft** release.

Step 1 is guarded by a test: `crates/app/tests/version_lockstep.rs` fails the
build if the player crates disagree about their version, if the engine's
sub-crates do, if the CHANGELOG names a different version than the manifests,
or if the engine's README quotes a version that is no longer true.

---

## Known limitations

Stated plainly, because the alternative is finding out the hard way:

- **Accessibility is broad but not certified.** egui's accessibility layer
  (accesskit) is enabled, icon-only controls carry accessible names, and the
  main editing surfaces — the equalizer, queue, playlist editor, album view and
  transport — work from the keyboard. The app has not been through a
  professional screen-reader audit, so the keyboard remains the most thoroughly
  exercised path. Every command below works without a mouse.
- **English and German, and German is not finished.** The interface speaks
  English and German: the catalogue lives in `crates/ui/locales`, the build
  script refuses to compile a string that is not in it, and a missing
  translation falls back to English rather than showing a key. The German
  catalogue currently covers the labels, buttons and menus; the long prose in
  the Help manual is still English. Pick a language in Settings → *Library &
  App*, or leave it on *Auto* and let `LC_ALL`, `LC_MESSAGES` or `LANG`
  decide.
- **Exclusive output needs the operating system's permission.** The app
  negotiates genuine exclusive output on all three platforms — ALSA on Linux,
  WASAPI Exclusive and ASIO on Windows, CoreAudio hog mode on macOS — but each
  of them can be refused: the Linux device must be openable directly (being in
  the `audio` group usually is enough), the Windows endpoint must not be held
  by another application, and the macOS device must be claimable. When it is
  refused, the app falls back to shared mode **and says so**; the Settings
  diagnostics card always shows which backend was negotiated and whether the
  access was verified, so "bit-perfect" is never claimed on a shared path.
- **No automated database backup.** `library.db` is an ordinary SQLite file; copy
  it if it matters. Scans never delete files, only rows whose file is gone.
  Album liner notes live in that database too; they are not written into the
  music files.
- **CD ripping and FLAC/Opus export need their native libraries at runtime.**
  The app loads them dynamically, so the build succeeds without them, but the
  tool reports `libcdio-paranoia`, `libcddb` or `libsndfile` as missing until
  the platform package is installed. CDDB lookups are best-effort; a lookup
  failure never loses audio, it only leaves the track titles as numbers.
- **The update check is opt-in and never installs anything.** It contacts
  GitHub, chooses an asset for this platform, requires a published SHA-256
  checksum, verifies the bytes, and returns the verified path. Applying the
  update is the user's next step, not the app's. There is no background poll.
- **Room correction needs a microphone and a quiet room.** The wizard records a
  sweep from the input you choose and never falls back silently to the system
  mixer. On macOS the microphone prompt is declared in the app bundle; if
  permission is denied, measurement fails with an error instead of capturing
  something else.
- **Headless mode has no window.** It exposes the loopback control socket and
  media integration, not the full UI; library management, CD ripping and the
  DSP studio are windowed features.
- **MPRIS2 is Linux-only**; Windows and macOS use SMTC and Now Playing
  respectively, which are not configurable in the same way.
- **Pre-1.0.** `0.x` versions may change settings formats or the database schema
  between releases; migrations run automatically and are one-way.
- **Two formats are named but not decoded.** Musepack (`.mpc`) and TAK (`.tak`)
  have no pure-Rust decoder in this tree, and the project does not take an FFI
  dependency on one. Opening such a file fails with an error that names the
  format; it is never mis-parsed as something else.
- **The engine's own limitations are the engine's.** The engine README lists
  them precisely — talkback and duplex monitoring, order-4+ ambisonic rotation,
  the N-input mix bus's automatic summation headroom, physical-DAC validation,
  in-process plugin sandboxing, and the rest. Where the engine is honest about
  a boundary, the player inherits that boundary rather than papering over it.

---

## License

Apache-2.0 — see [`LICENSE`](LICENSE). The engine (`crates/engine`) is licensed
the same way and ships its own copy at `crates/engine/LICENSE-APACHE`.

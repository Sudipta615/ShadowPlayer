# ShadowPlayer

A local-first, audiophile desktop music player for Linux, Windows and macOS:
Rust, [egui](https://github.com/emilk/egui), SQLite, and a real bit-perfect audio
engine — no accounts, no telemetry, no network.

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
- Songs table with sortable columns, quick-filter chips, multi-select and bulk
  actions; Albums, Artists, Genres and Folders views.
- Standard playlists, smart (rule-based) playlists, duplicate finder, batch
  organizer with a reviewable rename plan, M3U import/export.
- Embedded tag lyrics and `.lrc` sidecars (including ID3v2 `SYLT` timed lyrics
  and non-UTF-8 files), with a synced-lyrics panel.

**Playback**
- Gapless, crossfade, fade and hard-stop transitions; shuffle and repeat with a
  queue that survives a restart.
- Bit-perfect "Pure Direct" mode that bypasses the whole DSP chain, with a
  modal that says exactly which stages are bypassed and why.
- CUE sheets, DSD, all codecs the engine ships; ReplayGain / EBU R128 (from
  tags, from an on-disk loudness cache, or from a library-wide scan).
- Full DSP studio: parametric and graphic EQ, dynamics, filters, crossfeed,
  stereo enhancement, spatial/screen audio, room correction, CLAP plugin rack
  and named chain presets.

**Desktop**
- MPRIS2 on Linux (media keys, shells, `playerctl`), System Media Transport
  Controls on Windows, Now Playing on macOS.
- Light and dark themes, toasts for anything that fails, and a Help view whose
  shortcut list is generated from the same descriptions the app uses.

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

- **No streaming services, no internet radio, no accounts, no telemetry.** The
  app makes no network requests at all. Your library, your listening history
  and your statistics stay on your machine.
- **No skins or user themes yet.** There are two themes, light and dark. A
  theming system is on the way; it is not here yet and the app does not
  pretend otherwise.
- **No CD ripping, no video, no transcoding.**
- **No auto-update.** Updates are manual. This is a consequence of making no
  network requests, not an oversight.
- **No multi-instance in 0.x.** A second launch takes over the first.
- **English only.** There is no localisation layer yet.

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
| macOS app bundle | - | - | Yes | The committed package-app.sh wraps the binary in a foreground .app with the icon and no microphone key. |
| No microphone permission | Yes | Yes | Yes | The app captures nothing, and the macOS bundle omits NSMicrophoneUsageDescription. |
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
files or write ReplayGain tags.

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

---

## Tests

```bash
# Player workspace (engine excluded: it has its own workspace and suite)
cargo test -p domain -p database -p metadata -p lyrics -p library \
           -p playback -p search -p audio-session -p ui --lib -- --test-threads=1

# The version bump is guarded by a test, not by remembering to do it.
cargo test -p shadowplayer --test version_lockstep

# Engine (its own workspace)
cd crates/engine && cargo test

# The engine's benchmark suite
cd crates/engine && cargo bench
```

CI runs `cargo fmt --check`, `clippy -D warnings`, the test suite above, the
engine's suite, the engine's suite **on Windows and macOS as well as Linux**,
the benchmark suite's compile, and the documentation-truth checks, on every
push; see `.github/workflows/ci.yml`.

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

- **Partial screen-reader support.** egui's accessibility layer (accesskit) *is*
  enabled, so assistive technology sees the app's control tree rather than a
  blank window. The labelling is not complete yet — some icon-only controls are
  still unnamed — and full keyboard operation is the recommended way to drive
  the app. Every command below works without a mouse.
- **English only.** All copy is hard-coded; there is no localisation layer yet.
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

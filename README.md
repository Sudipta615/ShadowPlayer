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
- MPRIS2 on Linux (media keys, shells, `playerctl`).
- Light and dark themes, toasts for anything that fails, and a Help view whose
  shortcut list is generated from the same descriptions the app uses.

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

**Windows / macOS.** `cargo build --release` works out of the box, using
shared-mode WASAPI and CoreAudio respectively. The engine's native exclusive
backends (`wasapi-native`, `asio-native`, hog mode) are engine features that
this app does not enable; see `crates/engine/README.md`.

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
| Ctrl + B | Toggle bit-perfect Pure Direct mode |
| Ctrl + P | Open Settings & DSP studio |
| Ctrl + N | Open the playlist editor (create or manage playlists) |
| Ctrl + O | Open Folders & library manager |
| Ctrl + E | Toggle the equalizer & DSP panel |
| Escape | Close the open dialog (one layer per press), or clear the search |

---

## Tests

```bash
# Player workspace (engine excluded: it has its own workspace and suite)
cargo test -p domain -p database -p metadata -p lyrics -p library \
           -p playback -p search -p audio-session -p ui --lib -- --test-threads=1

# Engine (its own workspace, ~1300 tests)
cd crates/engine && cargo test
```

CI runs `cargo fmt --check`, `clippy -D warnings`, the test suite above and the
engine's suite on every push; see `.github/workflows/ci.yml`.

### Development-only automation

`cargo run -p shadowplayer --features eval-tools` compiles the
`SHADOWPLAYER_EVAL*` hooks: they drive the UI to a chosen state, take a
screenshot and exit. The feature is off by default, so those hooks — and the
`process::exit` they can reach from an environment variable — are not part of a
normal build, and neither are the CLAP eval script or the ~500 lines of
scaffolding behind them.

### Releasing

The player and the engine are versioned separately — the engine carries its own
version (`crates/engine`, currently `1.x`) because it is a standalone library
with its own workspace and tests — but they are released together. To cut a
release:

1. Bump `version` in `crates/*/Cargo.toml` (the player workspace) and, when the
   engine changed, `crates/engine/Cargo.toml` too.
2. Move the `[Unreleased]` section of `CHANGELOG.md` under the new version and
   date, and check the MSRV badge in `crates/engine/README.md`.
3. `cargo run` prints both versions at startup from `engine::VERSION` and
   `engine::NAME`; the log is the source of truth if a build ever reports the
   wrong pair.

---

## Known limitations

Stated plainly, because the alternative is finding out the hard way:

- **Partial screen-reader support.** egui's accessibility layer (accesskit) *is*
  enabled, so assistive technology sees the app's control tree rather than a
  blank window. The labelling is not complete yet — some icon-only controls are
  still unnamed — and full keyboard operation is the recommended way to drive
  the app. Every command below works without a mouse.
- **English only.** All copy is hard-coded; there is no localisation layer yet.
  This is a direction, not a promise — see `AUDIT.md` (C10).
- **Exclusive output where native backends exist.** This build negotiates
  genuine exclusive/bit-perfect output on Linux (ALSA); on Windows and macOS it
  plays through the shared-mode OS mixer unless the engine is built with its
  native backends. The Settings diagnostics card always shows which backend was
  negotiated, so "bit-perfect" is never claimed on a shared path.
- **No automated database backup.** `library.db` is an ordinary SQLite file; copy
  it if it matters. Scans never delete files, only rows whose file is gone.
- **MPRIS2 is Linux-only**, and only when a session bus is available.
- **Pre-1.0.** `0.x` versions may change settings formats or the database schema
  between releases; migrations run automatically and are one-way.

---

## License

Apache-2.0 — see [`LICENSE`](LICENSE). The engine (`crates/engine`) is licensed
the same way and ships its own copy at `crates/engine/LICENSE-APACHE`.

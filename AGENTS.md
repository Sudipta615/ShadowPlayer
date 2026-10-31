# AGENTS.md — ShadowPlayer engineering contract

This is the authoritative contract for every change in the player workspace
(`crates/*`, excluding `crates/engine`). It is not a style guide. Each rule
below exists because breaking it produces a defect that compiles, passes a
smoke test, and still reaches a user.

`crates/engine` is a **separate workspace** with its own contract. Read
[`crates/engine/AGENTS.md`](crates/engine/AGENTS.md) before touching it; the
realtime rules there override anything in this file.

---

## 1. Architecture & topology

ShadowPlayer is an audiophile music player: an `egui`/`eframe` desktop UI over
an independent realtime audio engine, with SQLite as the durable store. The
dependency direction is one-way:

```
app (binary) → ui → { state, services, playback, search } → {
    audio-session → engine (FFI)      database → SQLite
    library → metadata → domain       media-tools (subprocess / dynamic libs)
}
```

| Crate | Responsibility | Must not |
| --- | --- | --- |
| `domain` | Plain data: ids, models, smart-playlist rules, path keys, events, text normalisation. No I/O. | Depend on any other player crate. |
| `database` | SQLite schema, migrations, and one repository per domain (`repos/`). | Contain UI types, spawn threads, or block a frame. |
| `metadata` | Tag read/write and embedded lyric extraction. | Touch the database or the UI. |
| `lyrics` | LRC/plain lyric parsing. | Perform I/O. |
| `search` | Query parsing and result ranking over the in-memory library. | Query SQLite directly. |
| `library` | Scanning, file identity/fingerprinting, artwork lookup, M3U import/export, folder watching, library organisation. | Render UI or own playback state. |
| `playback` | Transport/queue coordination and playback policy above the engine. | Allocate or lock on the audio thread. |
| `audio-session` | The safe bridge to `engine`: spawns the engine, converts commands and events across channels, owns DSP/device control. | Leak engine handles into `ui`. |
| `media-tools` | CD extraction, export/transcode, liner notes, update verification. Loads native codecs dynamically. | Run on the UI thread. |
| `ui` | Widgets, views, application state, background services, theming, i18n. | Call SQLite or the engine directly outside a service/worker. |
| `app` | Process startup: instance lock, data dirs, logging, wiring `AudioSession` + `Database` into `ShadowPlayerApp`. | Accumulate feature logic. |
| `clap-fixture-plugin` | Deterministic CLAP test plugin used by engine/DSP tests. | Ship in a release. |

The engine boundary is deliberately narrow: `audio-session` is the only crate
that may speak FFI to `crates/engine`. If UI code needs a new engine
capability, the capability is added to `audio-session` first.

### Layering rules

1. **State is data, services are behaviour.** `ui::state` holds `Serialize`
   snapshots and pure reducers; `ui::services` owns threads, channels, and
   I/O. A view reads state and emits actions — it does not start work.
2. **No upward dependencies.** `database` never knows about `ui`; `playback`
   never knows about `egui`. A dependency that points upward is an architecture
   bug, not an import to add.
3. **No global mutable state.** Services are owned by `ShadowPlayerApp` and
   passed by reference. `static mut`, `lazy_static` caches of user data, and
   hidden singletons are forbidden.
4. **The database is the source of truth.** In-memory state is a projection
   that can be rebuilt from repositories; do not make correctness depend on a
   cache surviving.

---

## 2. UI & performance contract

These are product guarantees with tests behind them
([`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)). Violating them is a
regression even if the feature "works".

1. **0% CPU when idle.** With playback stopped and no animation, the app must
   not request continuous repaints. `egui::Context::request_repaint` is for an
   active reason (playback progress, animation, a newly finished task), never
   as a heartbeat. Do not replace event-driven updates with polling.
2. **60 fps cap during playback/animation.** Request repaints at the frame
   rate the work needs; never spin the event loop.
3. **Virtualize every unbounded list.** The songs table renders only visible
   rows. A library of 100,000 tracks must not produce 100,000 widgets, strings,
   or clones in a frame. Budgets are enforced by
   `cargo test --release -p ui --test library_budget`.
4. **No I/O on the UI thread.** File reads/writes, scanning, network requests,
   audio decoding, tag writing, artwork fetching, and database queries beyond
   a bounded single-row lookup belong on a worker thread or in a service that
   communicates by channel. Blocking the frame loop is a defect, not a slow
   feature.
5. **Clone budget.** `Track` is expensive. Pass `&Track`/`TrackId`; clone only
   at ownership boundaries, never once per row per frame. Cache derived sort
   keys instead of recomputing them per comparison.
6. **Rebuild only when the view changes.** `LibraryState` tracks what it last
   built. A sort change re-orders the existing list; it does not re-copy the
   library.
7. **Allocation discipline in draw code.** Avoid `format!`, `Vec` growth, and
   font/layout invalidation inside per-row widgets unless bounded by visible
   rows.
8. **Every image upload is cached by key.** Textures are created once and
   released explicitly; no per-frame `load_texture`.

---

## 3. The honesty & audio-truth contract

The UI must never claim more than the hardware and engine actually delivered.

1. **Report the negotiated truth, not the request.** Show the *actual* sample
   rate, bit depth, channel count, and backend. If the engine fell back to a
   shared backend, say so plainly and name the reason
   (`bit_perfect_cause`, `OutputAccessState`, `OutputInfo::actual_backend`).
2. **"Bit-perfect" is a verified state, never an assumption.** The badge is
   shown only when the engine confirms exclusive access and an untouched
   stream. A failed negotiation shows the fallback, not the aspiration.
3. **No invented capability.** Format support, platform features, and DSP
   features are reported from code registries and `cfg` gates. If a decoder or
   service is absent, render an explicit "not available" state. Never render a
   control that silently does nothing.
4. **No silent degradation.** Fallbacks, resampling, downmixing, and disabled
   DSP blocks are surfaced in the UI and logged once. A user who asked for
   lossless must be able to see that lossless was not delivered.
5. **Numbers carry units and provenance.** "24-bit / 96 kHz" means the stream
   the engine received, not the file's metadata claim, unless the label says
   otherwise.
6. **Docs and UI agree.** `ui::features` is the source of truth for the
   platform matrix and tests tie it to `cfg` gates. Update the in-app Help and
   README in the same change that moves a boundary.

---

## 4. The anti-pattern law: NO GOD FILES, NO GOD OBJECTS

### 4.1 Definitions (enforced)

- **God file**: a source file that mixes several unrelated domains, or that
  exceeds roughly **800–1000 lines** without a cohesive reason. Existing
  offenders are being decomposed; new ones must not appear.
- **God object**: a struct that (a) owns state for several unrelated features,
  (b) holds repository/service handles it merely forwards, (c) has multiple
  modal states distinguished by booleans/`Option` soup, or (d) has an `impl`
  block large enough to be a module.
- **God function**: a render or update function that both decides policy and
  draws several screens/categories, typically hundreds of lines with nested
  `if` ladders.

### 4.2 Required structure

- **Single Responsibility Principle.** One module = one reason to change. A
  settings category file renders that category; it does not own persistence.
- **Composition over accumulation.** Split state into focused structs
  (`DspState`, `LibraryState`, `PodcastState`, …) and compose them. Adding a
  feature must extend the smallest owning struct, not the app root.
- **Typed actions, not wide mutation.** Views emit a typed action enum or
  message; a coordinator applies it. Avoid `&mut` plumbing of the whole app
  into widgets and avoid dozens of unrelated `pub` fields.
- **Module façade pattern.** When a file is split into `foo/mod.rs` plus
  submodules, `mod.rs` declares the submodules and re-exports their public
  items (`pub use bar::*;`). Public paths and behaviour stay identical.
- **Visibility minimisation.** Submodule items start private/`pub(crate)`;
  promote only what the façade or another module genuinely needs.

### 4.3 Refactoring protocol (step by step)

1. **Baseline first.** Run `cargo check --workspace`, the full test command in
   §6, and `cargo clippy --workspace --all-targets -- -D warnings`. Record the
   result. Do not refactor on a red tree.
2. **Map the file.** List every public item, private helper, `impl`, test, and
   static-reflection dependency (`include_str!`, macro-generated code,
   `#[path]`). Identify cross-item state and shared helpers before moving code.
3. **Define boundaries.** Create `foo/` with one module per cohesive concern
   and a `mod.rs` façade. Shared helpers go to a `common.rs`, not duplicated.
4. **Move, don't rewrite.** Preserve bodies, doc comments, and test logic
   byte-for-byte where possible. Behaviour changes belong in a separate commit.
5. **Keep the public API stable.** `pub use submodule::*;` in the façade.
   External callers must not need import changes. If a signature must change,
   it is a breaking change and needs an explicit, separate decision.
6. **Preserve reflection tests.** Tests that parse source with `include_str!`
   must be updated to read every new source file (or the concatenated
   category sources), never deleted or weakened. Drift checks must keep
   checking the whole surface.
7. **Verify per phase.** After each decomposition run `cargo fmt`, `cargo
   check --workspace`, the relevant crate tests, then the full suite and
   clippy from §6. Add a regression test if the split uncovered an untested
   contract.
8. **Commit atomically.** One phase per commit, message naming the file and the
   resulting modules. Do not mix refactors with feature work or formatting of
   unrelated files.
9. **Stop at the budget.** A new file that grows past ~800 lines during the
   split means the boundary is wrong — split again rather than recreating the
   god file one level down.

### 4.4 Review checklist

- Does any new file exceed ~800 lines? Why?
- Does any struct now know about two unrelated features?
- Can a view be rendered from borrowed state plus a typed action sink?
- Are all public paths unchanged?
- Are tests (including source-reflection tests) still exercising the whole
  surface?
- Is a frame still O(visible rows), not O(library)?

---

## 5. Development workflows

### Adding a UI component / view

1. Put reusable primitives in `ui/src/widgets` or `ui/src/components`; screens
   in `ui/src/views`; overlays in `ui/src/modals`.
2. Read state immutably and return/emit a typed action or event. Do not start
   threads, query the database, or block in `show`/`update`.
3. Use theme tokens (`ui::theme::Theme`) and `tr!`/`tr_args!` for user-facing
   strings. No hard-coded colours or English literals in new UI.
4. Keep draw code allocation-light and virtualized for lists.

### Adding a settings entry

1. Add the renderer to the owning `settings_tab/categories/*` module.
2. Add the entry to `SETTINGS_INDEX` so search finds it. The reflection tests
   (`every_rendered_setting_is_indexed`, `every_indexed_setting_is_rendered_once`)
   enforce both directions; a new label that is not indexed fails CI.
3. Persist through the typed settings/config struct — never ad-hoc SQL from UI.

### Adding a database table or migration

1. Add the migration to `database/src/schema.rs` in order; never edit an
   applied migration. Migrations must be forward-only and idempotent on
   reopen.
2. Add a repository method in the matching `repos/*.rs`; use `parse_id`,
   `fts_query`, and the `LIKE` helpers from `repos/common.rs` rather than
   hand-rolling escaping.
3. Add a test that opens an existing database, migrates, and round-trips the
   new data.

### Adding a background service

1. Create `ui/src/services/<name>_service.rs` owning its thread and channels.
2. Communicate with a bounded, typed message enum; expose progress and errors
   as events. Never share `Arc<Mutex<AppState>>`.
3. Poll/consume events in the app update loop without blocking; request a
   repaint when new work arrives.
4. Log failures once, surface them in the UI, and make cancellation explicit.

### Shortcuts & translations

- Register shortcuts in `ui/src/shortcuts.rs` (or the owning view's scoped
  handler) and document them in `help_view/reference.rs` and the Help page.
- Every user-facing string uses `tr!`/`tr_args!` and exists in
  `crates/ui/locales/en.json` (and `de.json`). New `Msg` variants must be
  added to both catalogues; the catalogue tests fail otherwise.
- Changing a keybind requires updating the shortcut reference in the same
  change.

---

## 6. Pre-commit & CI validation

Run from the repository root. All must pass before a PR.

```bash
# 1. Format
cargo fmt --all -- --check

# 2. Compile every crate and target
cargo check --workspace

# 3. Full player test suite (engine has its own; run serially for DB/env safety)
cargo test -p domain -p database -p metadata -p lyrics -p library \
  -p playback -p search -p audio-session -p media-tools \
  -p shadow-recommender -p ui --lib \
  -- --test-threads=1

# 4. Integration guards
cargo test -p shadowplayer --test platform_parity -- --test-threads=1
cargo test -p shadowplayer --test version_lockstep -- --test-threads=1

# 5. Performance budgets (release; UI changes that touch list rendering)
cargo test --release -p ui --test library_budget -- --nocapture --test-threads=1

# 6. Lints — warnings are errors
cargo clippy --workspace --bins --lib --all-targets -- -D warnings

# 7. Engine changes only (separate workspace)
cd crates/engine && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test
```

### Versioning

The player workspace and the engine version independently. A release bumps the
player crates together and is guarded by
`crates/app/tests/version_lockstep.rs`; update `CHANGELOG.md`, the README
version badge, and the engine version/README when engine code changed.

### Commit hygiene

- One logical change per commit; refactors separate from behaviour.
- Conventional-style subjects (`feat(ui):`, `fix(database):`, `refactor(ui):`,
  `docs:`, `test:`), imperative and specific.
- Never commit generated output, `target/`, local databases, or cache files.

## 7. Ownership of shared subsystems

Several concepts exist on both sides of the engine boundary. These are the
canonical owners; a change that forks one of them updates this table.

| Concept | Canonical owner | Notes |
| --- | --- | --- |
| Audio DSP math (FFT, spectral/temporal descriptors, R128 loudness, true peak) | `crates/engine` (`dsp::analysis`, `dsp::loudness`) | Player crates reach it through `audio-session`; a player-side feature loop is application policy and must say so in its module docs. |
| Scan → library identity | `crates/library` (`scanner`, `identity`, `fingerprint`) | `ui::services::scanner_service` only drives it on a worker thread. |
| Playlist / M3U import | `crates/library` (`m3u`, `playlist_format`, `import_playlists`) | The engine's `playlist::io` serves the standalone engine CLI and is not imported by the player. |
| CUE model | `crates/domain` (`cue`) for the parsed model; `crates/engine` for stream splitting | |
| Tag extraction | `crates/metadata` (`extractor`) | `engine::decode::tags` is the mid-stream decoder view. |
| LRC parsing | `crates/lyrics` (`parser`) | Every other crate calls it; no second LRC parser. |
| Desktop media integration (MPRIS/SMTC) | `crates/ui` (`services::mpris_service`) | The engine's own MPRIS server is for its CLI; the app must not start both. |

## 8. State ownership hierarchy

A setting has exactly one durable owner. This is the order, from the UI
inwards; a layer may only write the layer it owns.

1. **Interaction state** — `ui::state` (pure, `Serialize` snapshots, no I/O).
2. **Player settings** — `database::repos::settings`, written only by
   `ui::app::settings_io` through the typed settings struct. Locale, theme,
   view preferences, output selection the *player* owns.
3. **Library data** — `database` repositories (tracks, playlists, presets,
   zones). `ui::app::eq_presets` writes presets through `EqPresetsRepo`; the
   EQ panel only emits typed requests.
4. **Runtime control state** — `audio-session` (`session::{device,dsp,
   transport}`) converts UI intent into engine commands. It holds no durable
   copy.
5. **Engine live state** — `EngineConfig` on the control thread.
6. **Engine durable state** — narrow and engine-owned: EQ/limiter/output
   selection in the engine's versioned state file, written only by
   `engine::dsp_persistence` / `spatial_persistence`.

Rules:

- No key is written by two layers. If the player persists a value, the engine
  must not also persist it; if the engine owns it, the player must load it
  back from the engine rather than keeping a parallel copy.
- Startup is load-then-apply: read the owning layer once, hand it downwards.
  There is no second replay path.
- A change that adds a persisted field names its owning layer in the commit
  message.

## 9. UI layer: one canonical implementation per screen

The UI has four layers with distinct jobs. The same name may appear in more
than one of them, but the *implementation* of a screen exists once.

- `ui/src/views/` — screen policy: read state, render the screen's widget,
  apply its result (convert a typed action into a service/coordinator call).
- `ui/src/components/` — widgets and panels. A `<name>_panel.rs` is the drawing
  half of `views/<name>_view.rs`; it takes borrowed state and returns a typed
  action. It must not query the database or start work.
- `ui/src/layout/` — shell geometry only (panel sizes, margins, rail layout).
- `ui/src/app/render/` — dispatch: turn a layout/widget callback into app
  state changes.

So `views/albums_view.rs` (policy) + `components/albums_panel.rs` (drawing) is
the intended split, and a second `components/albums_view.rs` is not allowed.
Sidebar and drawer follow the same three-way split: geometry in `layout/`,
widget in `components/`, dispatch in `app/render/`.

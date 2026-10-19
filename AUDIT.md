# ShadowPlayer — Implementation Audit & Remediation Plan

_Audit date: 2026-10-19 · HEAD `866b372` (post-`v0.4.0`) · workspace version `0.4.0`, engine `1.2.1`_

## 0. Baseline (measured)

| Check | Result |
| :--- | :--- |
| `cargo check --workspace --bins --lib` | ✅ clean, no warnings |
| Non-engine unit tests (`domain, database, metadata, lyrics, library, playback, search, audio-session, ui`) | ✅ 77 pass / 0 fail |
| Search crate tests | ⚠️ 0 exist |
| Playback crate tests | ⚠️ 1 (`queue insert/remove` only) |
| `app` crate tests | ⚠️ 0 |
| Engine tests | 1291 `#[test]` in-tree |
| Root `README.md` / `LICENSE` / CI | ❌ absent |
| `CHANGELOG.md` | ❌ stops at `0.4.0`; 11 feature commits are undocumented |

**Headline:** the product is functionally complete — all 22 changelog/post-changelog features are wired UI → state → `AudioSession` → engine. The real debt is **correctness in the data/library layer**, **a set of UI controls that do nothing or lie**, **settings that don't round-trip**, and **documentation/version drift**. Nothing in the audit is a compile-time break.

Severity legend: **P0** data loss / crash / silent corruption · **P1** user-visible wrong behaviour · **P2** missing feature or exposure · **P3** robustness / polish.

---

## 1. P0 — Data integrity, crashes, silent corruption

- [x] **A1 · Divide-by-zero panic in the scanner.**
  `crates/metadata/src/extractor.rs:84-90` guards `duration_secs > 0.0` but divides by `duration_secs as u64 * 1000`. For any sub-second file (jingle, clip) `duration_secs > 0.0` is true while `as u64 == 0` → integer division by zero panics the scan worker.
  **Fix:** compute in `f64` (`file_size_bytes as f64 * 8.0 / duration_secs / 1000.0`) or guard `duration_secs >= 1.0`.

- [x] **A2 · `%`-prefix delete/query crosses path boundaries.**
  `crates/database/src/repos.rs:360-365` (`delete_by_path_prefix`), `:399-403` (`get_tracks_by_folder`), `:790-795` (`purge_path_prefix`). Build `format!("{}%", prefix)` with no trailing separator and no `ESCAPE`, so `/music/rock` also matches `/music/rockabilly`, and any `%`/`_` in a folder name becomes a wildcard. Reachable from the delete-folder UI and from `crates/app/src/main.rs:23,28`.
  **Fix:** normalise to a trailing `/`, use `LIKE ?1 ESCAPE '\'` with escaped `% _ \`.

- [x] **A3 · Scanner follows symlinks and never prunes deletions.**
  `crates/library/src/scanner.rs:63` uses `WalkDir::new(root).follow_links(true)` → indexes files outside the root and double-indexes symlink aliases; `:65` `filter_map(|e| e.ok())` drops permission/loop errors silently. There is **no reconciliation of removed files anywhere** in the crate, so deleted tracks stay in the library forever (watcher only adds/updates).
  **Fix:** `follow_links(false)`; collect visited paths and delete rows under the root that no longer exist after a full scan (respect `scan_roots` and the content-fingerprint relink path).

- [x] **A4 · Content fingerprint can steal another track's identity.**
  `crates/library/src/fingerprint.rs:39-54` hashes size + first/last 64 KiB only, so two different files that share head/tail collide. `crates/library/src/identity.rs:55-57` then relinks when the old path merely `.exists()` is false — a temporarily unmounted drive causes the row (play counts, favourites, playlist membership, lyrics) to be adopted by the new file, and the original is re-indexed as a duplicate when the mount returns.
  **Fix:** hash the whole file (or add a middle sample + a cheap strong hash); only relink when the scan root is mounted/reachable and the hash is unique.

- [x] **A5 · Corrupt `id` text fabricates a random identity.**
  `crates/database/src/repos.rs:17` (and `:149,306,460,881,1104`) use `TrackId::from_str(&s).unwrap_or_default()`. `TrackId::default()` is a fresh UUIDv4, so a malformed row yields a track that can never be matched back to the DB (delete/update silently no-op).
  **Fix:** return a `rusqlite` conversion error instead of fabricating.

- [x] **A6 · `ScanRootsRepo::add` returns a phantom id on conflict.**
  `crates/database/src/repos.rs:895-911` inserts with `ON CONFLICT(path) DO NOTHING` but always returns a newly generated `ScanRootId`. Re-adding an existing root hands back an id that isn't in the table, so later `update_last_scanned`/`remove` no-op.
  **Fix:** `INSERT … ON CONFLICT(path) DO UPDATE SET path=excluded.path RETURNING id`, or re-`SELECT` after the conflict.

- [x] **A7 · Settings can be silently wiped.**
  `crates/database/src/repos.rs:828` writes `serde_json::to_string(settings).unwrap_or_default()` (an empty string on failure) and `:818-823` swallows the parse error and returns defaults. Combined with `crates/domain/src/models.rs:187-222` (`AppSettings` has **no `#[serde(default)]`**), adding or renaming any field makes deserialization fail → the user's entire configuration silently reverts to default on next launch.
  **Fix:** `#[serde(default)]` on the struct + per-field defaults; never persist a serialization failure.

- [x] **A8 · CUE albums are double-indexed.**
  `crates/library/src/scanner.rs:116-145` splits a `.cue` into virtual tracks, but the referenced `.flac`/`.wav` is itself a supported extension and is indexed as a separate full-length track in the same walk. Every CUE album appears twice; the `.cue` filename is also discarded from the virtual path (`crates/metadata/src/extractor.rs:233-234`), which breaks playback (`crates/playback/src/coordinator.rs:145-174` guesses `<stem>.cue`/`album.cue`/`CDImage.cue` and silently falls back to playing the whole file from 0:00).
  **Fix:** keep the real cue path in the virtual path; skip the underlying audio file when a covering `.cue` exists; open the recorded cue.

- [x] **A9 · Audio worker thread can leak; events can be dropped silently.**
  `crates/audio-session/src/session.rs:51-83` and `:107-132`: the engine worker is spawned, then the event-forwarder; if the forwarder spawn fails, `?` returns before `Self` exists, so `Drop` never runs and the worker thread runs forever. The forwarder handles are discarded (`:1012-1019` only joins `worker_thread`), so `Drop` can return while they still touch engine receivers. Both event channels are `bounded(256)` + `try_send` (`:62,:72`), so a `SourceFinished` can be lost when the consumer is busy → auto-advance stalls with no trace.
  **Fix:** build handles in a way that a spawn failure stops the worker; join forwarders; use an unbounded or lossy-with-counter channel and log overflows.

- [x] **A10 · `PRAGMA foreign_keys` failure is silent.**
  `crates/database/src/schema.rs:5-8` wraps all four PRAGMAs in `let _ =`. On a filesystem where WAL/foreign-keys can't be set (some network mounts) every `ON DELETE CASCADE` silently stops working, leaving favourites, playlist entries and play history orphaned.
  **Fix:** check the PRAGMA result; log a warning; verify `PRAGMA foreign_keys` reads back `1`.

- [x] **A11 · Migration V3 is not idempotent.**
  `crates/database/src/schema.rs:241` runs `ALTER TABLE tracks ADD COLUMN content_hash TEXT;` (no `IF NOT EXISTS`) gated only on the `schema_migrations` row. If that row is ever lost/reverted the whole DB fails to open.
  **Fix:** guard on `PRAGMA table_info(tracks)` before the `ALTER`.

---

## 2. P1 — User-visible wrong behaviour

### Playback (`crates/playback/src/coordinator.rs`)
- [x] **B1 · `clear_queue` keeps the current track.** `:273-279` clears the queue/index but not `current_track`, so `toggle_play_pause` (`:176-186`) re-opens and plays a track that is no longer listed. Clear `current_track`/listening state too.
- [x] **B2 · Turning shuffle off doesn't restore the real order.** `:116-122` sets `unshuffled_queue = Some(self.queue.clone())` on *every* `play_track` while shuffle is on. After the first shuffle the "original" is already a permutation, so disabling shuffle restores a shuffled order. Snapshot only when the queue is genuinely unshuffled.
- [x] **B3 · Forward seeks count as listening.** `:435-441` adds `pos - last_position` whenever `pos >= last_position`, so dragging to the end of a track registers a play (`register_play`). Only credit contiguous playback.
- [x] **B4 · Repeat-One breaks for CUE tracks.** `:398-405` reopens `track.path` with `open_file`, bypassing the CUE branch in `open_and_play_track`, so a `…#cue:N` path fails; it also duplicates the engine's own `set_repeat_mode(One)`.
- [x] **B5 · Non-`SourceFinished` engine events are dropped.** `:383-411` matches `_ => {}`, so engine errors/device-loss/underrun events never reach the UI. Route them to a log/toast.
- [x] **B6 · Swallowed write errors.** `:455-459` and `:472-475` `let _ =` on `increment_play_count`, `record_play`, `save_queue`. Surface (or at least log) failures — stats and queue persistence currently fail silently.
- [x] **B7 (perf) · `remap_paths` is O(queue × moved).** `:244-271` does a linear scan per moved track; use a `HashMap`.

### UI controls that do nothing or lie
- [x] **B8 · "Sort by Favorites" sorts by DateAdded.** `crates/ui/src/components/songs_table.rs:424-426` — the heart header tooltip says Favorites but sets `SortField::DateAdded`. There is no `SortField::Favorite`. Add one and route it.
- [x] **B9 · Active-track highlight is a no-op.** `crates/ui/src/components/songs_table.rs:556` `if is_active { text_primary } else { text_primary }` — identical branches, so the playing track is not distinguished by colour.
- [x] **B10 · Row overflow "⋮" button does nothing.** `crates/ui/src/components/songs_table.rs:622-624` paints and styles the icon but attaches no click handler/popup; only right-click works. Wire it to the same context menu, or remove the affordance.
- [x] **B11 · Now-playing spectrum visualizer is a mock.** `crates/ui/src/components/visualizer.rs` ignores `_spectrum_db` and always draws the fixed `MOCK_BARS` (`:12`); `crates/ui/src/components/now_playing.rs:168` passes real data in. A working implementation already exists at `crates/ui/src/widgets/spectrum_visualizer.rs` (used by `views/main_playback_view.rs:182`) — delete the mock and use it.
- [x] **B12 · The whole "Advanced Audio & DSP" modal is unreachable.** `crates/ui/src/components/advanced_audio.rs:19,60-62`: `is_open` is only ever read and reset, never set `true`, so `show_advanced_audio_modal` (≈250 lines of DSP controls) can never render. `NowPlayingEvents.open_advanced_audio` (`components/now_playing.rs:13,59`) is likewise never set; its handler (`views/main_playback_view.rs:133-135`) only closes the help modal. `AdvancedAudioActions.loudness_norm_changed` (`:39`) has no handler.
  **Decide:** either wire the modal to a real entry point (and handle `loudness_norm_changed`), or delete the module. Leaving it is dead weight and a trap.
- [x] **B13 · Crossfade is always forced on.** `crates/ui/src/state/dsp_state.rs:46-50` calls `set_crossfade_config` with `enabled: true` unconditionally, ignoring `transition_mode`/`crossfade_duration_secs`. Users who pick a non-crossfade transition still get crossfade.
- [x] **B14 · Spatial "distance" is cosmetic.** `crates/ui/src/components/settings_tab.rs:1233,1250` writes `source_distance_m` and shows it, but `sync_all_to_session` never passes it to `set_spatial_screen` (`state/dsp_state.rs:85,226`). `screen_half_w`/`screen_elevation`/`screen_gain` (`:141-143`) have no UI and are frozen at defaults. `air_absorption_enabled` (`:137`) round-trips through persistence but nothing renders it or syncs it. Either implement or remove these fields.
- [x] **B15 · Virtual-centre azimuth is clobbered every frame.** `crates/ui/src/components/settings_tab.rs:1242` `state.screen_center_az = state.source_azimuth_deg;` overwrites any independent "Virtual Center" adjustment, so the two controls can't be used independently.

### Persistence / version drift
- [x] **B16 · Settings fields persisted but never restored.** `crates/ui/src/app.rs:402-477` writes `sort_order` (hard-coded `Ascending`), `audio_profile` (hard-coded `Normal`), `window_width/height` (hard-coded `1280×800`), and `selected_device`; `load_persisted_settings` (`:310-335`) reads **none** of them. `crates/app/src/main.rs:64` unconditionally applies `AudioProfileChoice::Fidelity` and never applies persisted device / bit-perfect / shuffle / repeat. Window size is not actually persisted.
- [x] **B17 · Settings-tab persistent struct drops fields.** `PersistentSettingsTabState` (`components/settings_tab.rs:277-324`) omits `source_azimuth_deg`, `screen_center_az`, `source_distance_m`, `screen_half_w`, `screen_elevation`, `screen_gain`, `capture_path`, `capture_device`, and the mix-bus scratch fields (`:148-160`), so they reset every launch. The `From`/`apply_to` mirrors (`:326-407`) are hand-maintained (34 fields each) with no compile-time completeness check — that is exactly how the omissions happened. Derive the mirror from the live struct (serde) or generate it.
- [x] **B18 · Help text contradicts behaviour.** The in-app Help omits `Ctrl+E` (`app.rs:2035-2037` implements it; `components/help_modal.rs:49-64` and `views/help_view.rs:1049-1069` don't list it) and documents "Escape → dismiss modal" although Escape (`app.rs:2038-2047`) closes only search / new-playlist / help / track-properties / EQ — not the CLAP rack, smart-playlist, playlist editor, bit-perfect, organize, duplicates or AutoEq modals. `Ctrl+N` is documented as "create new playlist" but opens the playlist editor.
- [x] **B19 · Version strings drift.** `Audio Engine v1.2.1` is hard-coded in `crates/app/src/main.rs:60`, `views/settings_view.rs:92,103`, `components/settings_tab.rs:2684`, `components/help_modal.rs:34`; only `views/help_view.rs:159` uses `env!("CARGO_PKG_VERSION")`. Centralise on the engine's `CARGO_PKG_VERSION` (and app's own).
- [x] **B20 · No sort-direction control.** The UI has `SortField` but no ascending/descending toggle; `sort_order` is written as `Ascending` and never read (`app.rs:418`, `state/library_state.rs:109`). Everything sorts ascending. Add a direction toggle + persist it.

### Database / search / lyrics correctness
- [x] **B21 · Album year is non-aggregated in a `GROUP BY`.** `crates/database/src/repos.rs:942-962` — `SELECT album, artist, year, COUNT(*) … GROUP BY album, artist` lets SQLite pick an arbitrary `year`, so album year can change between runs. Use `MIN(year)`/`MAX(year)`.
- [x] **B22 · FTS query mangles input.** `crates/database/src/repos.rs:374-390` strips everything but alphanumerics+spaces and appends `*`: `AC/DC` → `ACDC*`, `don't` → `dont*` (never matching the tokenizer's `ac`/`dc`), and there is no `ESCAPE`/phrase/boolean support. Build a proper FTS query from the tokenizer's own output or match tokens individually.
- [x] **B23 · Playlist-position compaction is a correlated self-`UPDATE`.** `crates/database/src/repos.rs:560-570` counts rows of the table it is updating; correctness depends on unspecified row order. Rewrite as a Rust-side re-index in one transaction.
- [x] **B24 · Smart-playlist date rules can never match.** `crates/domain/src/smart.rs:96-133` restricts date fields to `InLastDays/…`, but `rule_to_clause` (`repos.rs:1145-1157`) routes a stored `Equals` on `Added`/`LastPlayed` through the *text* branch (`added_at = '2024' COLLATE NOCASE`), which can't match a unix timestamp. Validate rules on load and reject/repair impossible field/operator pairs.
- [x] **B25 · Corruption dropped without a trace.** Smart-playlist JSON (`repos.rs:1107`, `:1273`) and settings JSON use `unwrap_or_default()`; one malformed field silently discards all rules. Log and quarantine instead.
- [x] **B26 · LRC parser: offsets ignored, metadata leaks into the lyric body.** `crates/lyrics/src/parser.rs:21-43` has no `[offset:…]` support, and after consuming timestamps it `break`s on the next `[`, so `[00:12.00][ar:Artist]text` yields the literal body `"[ar:Artist]text"`.
- [x] **B27 · Non-UTF-8 lyrics/m3u are unreadable.** `crates/lyrics/src/parser.rs:119,150` and `crates/library/src/m3u.rs:139-143` use `fs::read_to_string`; Latin-1/Shift-JIS/Windows-1252 sidecars and playlists silently vanish. Decode with a fallback (detect/`lossy` with a warning).
- [x] **B28 · Synced (SYLT) lyrics ignored.** `crates/metadata/src/lyrics.rs:34-45` reads only unsynced `ItemKey::Lyrics`; ID3 `SYLT` timed lyrics are never surfaced, and `read_from_path` failures return `None` silently.
- [x] **B29 · Organize can overwrite and can't cross devices.** `crates/library/src/organize.rs:330-356` re-checks nothing; `fs::rename` silently overwrites an existing destination on Unix (the module doc promises "never overwrites") and fails entirely across filesystems (no copy+fsync+delete). `sanitize_component` (`:308-324`) doesn't handle Windows reserved names (`CON`, `NUL`, …). Use `create_new`/`link` semantics and a copy fallback.
- [x] **B30 · M3U edge cases.** `crates/library/src/m3u.rs:51-68` doesn't percent-decode `file:///…/My%20Song.flac`; `:155-158` `strip_prefix` without canonicalisation writes spuriously absolute paths; filenames containing `\n` corrupt the output.
- [x] **B31 · Watcher robustness.** `crates/library/src/watcher.rs:19-48` drops `notify` errors, has no debounce (the callback fires per raw event), never unwatches (handle discarded → no lifecycle), and supports one root per watcher. `with_poll_interval(2s)` is inert on inotify.
- [x] **B32 · Output-backend list isn't filtered by platform or compiled feature.** `crates/ui/src/components/settings_tab.rs:1044-1048` always offers Auto, ALSA Exclusive, WASAPI Exclusive, CoreAudio Hog and ASIO Direct. On Linux, picking WASAPI/ASIO/CoreAudio silently falls through to cpal shared output in `crates/engine/src/output/output.rs:449-472` — the UI keeps reporting the chosen backend and only a `log::warn!` records the fallback. Filter the list by target OS + enabled features, or surface the actual negotiated backend in the (otherwise good) runtime-diagnostics card.

---

## 3. P2 — Missing features & unexposed capability

- [x] **C1 · Sort direction toggle** (see B20) — the most user-visible missing control.
- [x] **C2 · Repo README + LICENSE + CI.** There is no root `README.md`, no `LICENSE`, and no `.github/` workflow. Add a README that states what ShadowPlayer is, how to build/run, platform requirements (ALSA permissions, XDG portal) and the screen-reader/known-limitations honesty the engine README models. Add `cargo fmt --check`, `clippy -D warnings`, `cargo test` CI.
- [x] **C3 · Engine README links 8 files that don't exist.** `crates/engine/README.md` (and its badge table) links `docs/GETTING_STARTED.md`, `docs/ENGINE_SPEC.md`, `docs/OWNERS_GUIDE.md`, `docs/ARCHITECTURE.md`, `docs/SIGNAL_FLOW.md`, `docs/EMBEDDING.md`, `docs/HISTORY.md`, `docs/LICENSES_AND_ATTRIBUTION.md`, plus `AGENTS.md` — none are present in the repo. Either commit the docs or remove the links.
- [x] **C4 · CHANGELOG is stale.** 11 feature commits landed after `0.4.0` (`CLAP rack`, `batch organize`, `chain presets`, `tag lyrics`, `recently-added + chips + toasts`, `bass/LFE/up-down-mix`, `duplicates`, `DSP precision + diagnostics`, `M3U`, `watch roots + reconcile`, `ReplayGain tag write`). Add an `[Unreleased]` section (and bump versions when released).
- [x] **C5 · Unexposed engine subsystems.** Exported by `engine::lib` but used by **zero** front-end code: `network_audio` (AES67/RTP receiver/session), `engine::mpris` (duplicated by the UI's own zbus service), `profile` (AutoEq analysis), `eval`, `governance`, `standards`, `fx`, `buffer`, `track_cache`, `state`. Decide per subsystem: expose it in the UI, or document it as engine-only. `engine::mpris` vs `ui/services/mpris_service.rs` is a genuine duplicate to reconcile.
- [x] **C6 · Dead UI code to delete or wire.** `modals/new_playlist_modal.rs` (exported, never called — `app.rs:2464-2467` opens `playlist_editor` instead), `AppState::new_playlist_name` (`state/app_state.rs:34,68`), the `RightTab::Settings` arm + `show_settings_tab` (`components/right_panel.rs:374-377`, `components/settings_tab.rs:613-618`), `widgets/eq_curve.rs::EqCurveWidget` (unused; EQ curve drawn inline in `eq_panel.rs`), `components/topbar.rs::{TopBar,TopBarResponse}` (unused; nav is `layout/sidebar.rs`), `SettingsTabState::write_rg_path` (`components/settings_tab.rs:167`).
- [x] **C7 · Dead DB schema/models.** The `lyrics` and `artwork` tables and `LyricsRecord`/`ArtworkRecord` models (`schema.rs:143-158`, `models.rs:108-131`) have no repository and are never constructed; lyrics are re-read on demand and artwork dimensions/mime are never persisted. Either implement the repos (and delete cached thumbnails with the track) or drop the tables/models.
- [x] **C8 · `PlayerEvent` is dead.** `crates/domain/src/events.rs:12-20` is never constructed anywhere.
- [x] **C9 · `saved_queue` has no FK and a write-only `path`.** `crates/database/src/schema.rs:163-167` — orphan rows persist; deleted tracks silently vanish from the queue while the row (and the never-read `path`) remains.
- [x] **C10 · i18n.** All UI copy is hard-coded English (no abstraction). Long-term; note as a direction, not a sprint.

---

## 4. P3 — Robustness, performance, hygiene

- [x] **D1 · Swallowed errors everywhere.** Dozens of `let _ = session.send_command(...)` in `crates/audio-session/src/session.rs` (e.g. `:324,327,367,412,436,476,544,610,623,930`) and repo writes in `crates/ui/src/app.rs` (`:1935,1944,2484,2488,2496`). `session.set_output_device` returns `()` (`:334`). Add a return path or a log on failure.
- [x] **D2 · Silent "empty" results.** `crates/ui/src/app.rs:2497` (`get_tracks(...).unwrap_or_default()`), `crates/search/src/engine.rs:41,56`, `crates/ui/src/state/library_state.rs:~215,222` — a failed read is indistinguishable from an empty list.
- [x] **D3 · `search` results are shallow.** `crates/search/src/engine.rs:41-61` derives artists/albums only from the already-truncated track page, so an artist whose tracks rank past `limit` never appears; it reloads the full playlist table per keystroke. Query artists/albums directly.
- [x] **D4 · Missing indexes.** `crates/database/src/schema.rs:109-132`: no index on `play_history(track_id)` (used by `list_most_played` and FK cascade), `playlist_tracks(track_id)`, or `tracks(genre)`; `idx_tracks_album` doesn't cover `WHERE album=? AND artist=?`.
- [x] **D5 · FTS rebuilt on unrelated updates.** `crates/database/src/schema.rs:184-188` `tracks_au` fires for every `UPDATE tracks`, including play-count increments and favourite toggles on the hot playback path. Restrict the trigger to the indexed columns (`UPDATE OF title, artist, album, genre, path`).
- [x] **D6 · N+1 duplicate detection.** `crates/database/src/repos.rs:189-214` issues one full-track `SELECT` per duplicate hash. Fetch the hashes once and join.
- [x] **D7 · Double file probe per scan.** `crates/metadata/src/extractor.rs:35-36,63-74` runs the engine extractor and then opens the same file again with `symphonia`; `extract_loudness_metadata` re-runs on every changed file.
- [x] **D8 · Sorting allocates per comparison.** `crates/ui/src/state/library_state.rs:220` area uses `a.title.to_lowercase().cmp(&b.title.to_lowercase())` (two allocations per comparison) and `SortField::Album` compares case-sensitively — inconsistent. Precompute sort keys.
- [x] **D9 · Silent scan skips.** `crates/library/src/scanner.rs:92` skips a file entirely if `path.metadata()` fails, and `:104` treats same-second, same-size rewrites as unchanged (second-granularity mtime). Record and log skips; consider sub-second mtime.
- [x] **D10 · Inconsistent logging.** `crates/database/src/schema.rs:194,224,248` use `eprintln!`, bypassing the configured logger; migration failures are also non-fatal in some paths.
- [x] **D11 · Eval/screenshot harness in the shipped binary.** `crates/ui/src/app.rs:1501-1922` (~500 lines of `SHADOWPLAYER_EVAL_*` env-var hooks, including `process::exit(0)` at `:1921`) is compiled into every build. It is runtime-gated, but should be behind a `eval-tools` feature so it doesn't ship.
- [x] **D12 · Frozen engine version metadata.** `crates/engine/Cargo.toml` is `publish = false` and versioned separately (`1.2.1`) while the app is `0.4.0`; ensure a release process keeps app and engine in step (see B19).
- [x] **D13 · Non-UTF-8 path handling.** `crates/domain/src/ids.rs:71-75` uses `to_string_lossy()` for track ids, and `repos.rs:86,132` persists the lossy string, so two distinct non-UTF-8 paths can collapse to the same id.
- [x] **D14 · Tests for risky logic.** No tests for `search`, `library::identity`, `library::artwork`, `library::watcher`, `metadata::extractor`; `playback` covers only queue insert/remove (nothing for shuffle/repeat/`clear_queue`/listening accounting/`remap_paths`); `database` doesn't test `delete_by_path_prefix`, `purge_path_prefix`, `ScanRootsRepo::add` idempotency, or malformed ids; `app` has 0. The P0/P1 fixes above should each land with a regression test.
- [x] **D15 · Magic numbers.** `playback/coordinator.rs:12` (`PLAY_THRESHOLD_SECS`), `:220` (`3.0` restart), `:152-156` (cue guesses); `database/repos.rs:641-643` (7-day window, min 5 plays), `:1222` (`LIMIT 100_000`); `app/src/main.rs:60`; `artwork.rs:33` (`160×160`). Hoist into named constants/config.

---

## 5. Engine-internal findings (`crates/engine`)

The engine is a huge, unusually well-documented crate (1291 tests; zero `todo!()`/`unimplemented!()` in production paths; realtime zero-alloc contract enforced by a test). Most findings below are **honestly documented as limitations in the code itself** — they are capability gaps, not deception, but several are silent failures that violate the engine's own "never a guessed claim" principle. None of E1–E4 are reachable from the desktop UI today (PipeWire/JACK aren't in the backend selector, see B32), but the engine is published as a standalone library, so they matter.

### P0/P1 — silent wrong behaviour
- [x] **E1 · Simulated PipeWire/JACK backends discard audio and always succeed.** `output/pipewire.rs:437-492` and `output/jack.rs:423-479` spawn a plain thread that pops frames and throws them away on a `sleep`-paced loop — no `pipewire`/`jack` dependency, no graph, no callback. Because `open()` never fails, the `Err(e) => fall back to cpal` arms in `output/output.rs:449-472` are unreachable: an integrator who selects these backends gets **silence** while the engine logs "using native … pro-audio backend".
- [x] **E2 · PipeWire `enumerate_nodes()` invents devices.** `output/pipewire.rs:227-258` shells to `pw-dump` and on any failure returns three hardcoded fake nodes (2/8/16 ch). A UI enumerating devices will list endpoints that don't exist.
- [x] **E3 · JACK `probe_daemon()` always succeeds.** `output/jack.rs:179-195` computes `has_jack_socket` then runs an `if … {}` no-op, returning `Ok(())` unconditionally. JACK `output_info` also claims `is_exclusive = true` while `access_mode = Shared` (`:341-345`).
- [x] **E4 · ASIO advertises fiction.** `output/asio_output/mod.rs:643-666` reports fixed rates/formats/channel counts (44100–768000, `[2,6,8,12,16]`) rather than the driver's `ASIOCaps`; `reconfigure_sample_format` (`:707-714`) stores the request but never re-initialises `IASIO`, so a live format switch silently doesn't take effect. `:806-889` `unwrap()`s the driver handle on control paths.
- [x] **E5 · Concert-hall convolution ignores the IR's sample rate.** `dsp/convolution.rs:343-344,438-439` sets `ir_loaded_sample_rate = Some(self.sample_rate)` (the *stream* rate), not the file's, and never resamples — a 44.1 kHz IR on a 48 kHz stream is ~8.8% wrong, and because the recorded rate equals the stream rate `ir_needs_reload()` can never fire. A truncated IR also fails silently (`:498,503` `while let Ok(Some(..))`), FFT errors are dropped (`:642-836`), and `dropped_frames()` is dead (always 0, `:1139`). IR-load errors are additionally swallowed at `dsp/pipeline/mod.rs:611` and `graph2/prod/arena/construction.rs:554` (`let _ = …load_ir_from_file`).
- [x] **E6 · FFI doesn't validate handle liveness.** `src/ffi.rs` entry points (~40 sites, e.g. `:344-352,364-369,2020-2023`) do `handle.as_ref()` and dereference; only `engine_destroy` consults the `live_handles()` registry. A stale pointer after destroy is UB instead of `InvalidHandle`.
- [x] **E7 · `LufsEnergy` dynamics detection returns magnitude, not energy.** `dsp/dynamics/detector.rs:245-251` returns `(filtered_l.abs(), filtered_r.abs())` whereas the other modes return squared/mean-square quantities — a detector configured for LUFS/energy behaves differently from `MeanSquare`.
- [x] **E8 · Loudness gating diagnostics are placeholders.** `dsp/loudness/analysis.rs:299-307` hardcodes `gated_below_absolute/relative` and `ungated_blocks` to `0`; any R128 gate report built from it is not a real census.
- [x] **E9 · CLAP lifecycle gaps.** `plugin_clap_node.rs`: `clap_host_get_extension` always returns null (`:234-239`) so plugins needing `params`/`log`/`thread-check` degrade; `request_restart/process/callback` are empty no-ops (`:243-251`); `entry.deinit()` is never called on unload (`:97-118,463-466`); the `clap_process_status` return is discarded, buffers are hardcoded stereo, and `process_block_f64` is empty (`:1031-1121`). `plugin_sandbox.rs` is documented as non-production and its `run_plugin_worker_stdio` is a trivial gain multiplier — isolation is unimplemented.
- [x] **E10 · `AudioAnalyzer` can dead-lock after a panic.** `dsp/analyzer.rs:238-244,318` — the `processing.compare_exchange(false,true)` guard is only released at the end of `update`; a panic in between leaves it `true` forever and every later `update` returns early, freezing all meters. Also `unsafe impl Send/Sync` over `UnsafeCell` state.
- [x] **E11 · Resampler drops whole chunks on under-allocation.** `dsp/resampler/core.rs:501,522-526,555` skips an entire chunk (audible dropout) with only a `log::error!` when scratch/output is too small, and `process` errors are only warned.
- [x] **E12 · Native playlist save is non-atomic.** `playlist/io.rs:226-234` `std::fs::write` straight to the target — a crash/full disk mid-write destroys the existing playlist. Temp-file + rename.
- [x] **E13 · Decoder bridge buffers whole files to RAM and uses a predictable temp name.** `decode/decoder.rs:47-76` `read_to_end`s APE/Opus/WavPack/TTA sources and writes `playtune_decoder_bridge_{pid}_{len}.{ext}` in the temp dir (clobber/symlink risk; cleanup only on the normal path).

### P2/P3 — capability gaps, dead code, hygiene
- [x] **E14 · ASIO hardware volume unimplemented.** `output/cpal_output/mod.rs:55-65` — documented; callers using `HardwarePreferred` must special-case.
- [x] **E15 · ALSA period/buffer negotiation not read back.** `output/alsa_output/mod.rs:204-223` discards `set_period_size_near`/`set_buffer_size_near` results and hardwires `period_frames = 1024` in the render path (`:413`).
- [x] **E16 · WASAPI exclusive path is unvalidated and `pause()` busy-waits.** `output/wasapi_output/mod.rs:565-574` polls a flag with 100 µs sleeps for up to 50 ms on the caller thread; the module self-describes as not yet hardware-validated.
- [x] **E17 · True-peak meter is not BS.1770-5 Annex 2 conformant.** `dsp/true_peak.rs:20-36,66` uses a 400-tap Kaiser sinc instead of the 48-tap prototype; documented, but any "R128 true-peak" claim should be softened.
- [x] **E18 · Shibata dither only has 44.1/48 kHz tables.** `dsp/dither.rs:64-88,157-165` switch at 46 kHz, so 96/192 kHz reuse the 48 kHz coefficients.
- [x] **E19 · HRTF out-of-grid directions fall back to an analytic model.** `spatial/health.rs:270`, `spatial/hrtf/dataset.rs:548` — documented and reported via health, but binaural quality degrades silently without a dense dataset.
- [x] **E20 · ADM scene construction swallows errors.** `spatial/adm.rs:97-99` `let _ = scene.create_bed(layout); let _ = scene.create_field();`.
- [x] **E21 · Codec capability registry is unreachable.** `decode/codecs.rs:1-20` self-admits "no production caller"; not exported through FFI, so a host can't query it and it can only drift.
- [x] **E22 · `.oga` never dispatches to the Opus backend.** `decode/decoder.rs:177-182` routes all `.oga` to Symphonia because the Opus probe needs a path; Opus-in-Ogg `.oga` fails if Symphonia lacks Opus.
- [x] **E23 · `Playlist::from_parsed` throws away parsed metadata.** `playlist/mod.rs:111-123` ignores `ParsedPlaylist.metadata` even though every parser populates it and the doc promises it is "kept rather than discarded".
- [x] **E24 · Dead/duplicate attributes.** `decode/decoder.rs:575-577` stacks two `#[test]` attributes on one function (in the no-opus/no-wavpack cfg build); `dsp/resampler_handle.rs:101-102` `impl Default` is gated by both `feature = "resample"` **and** `not(feature = "resample")`, so it never compiles (the module is a documented non-wired design artifact).
- [x] **E25 · Missing backends in the desktop build.** `PipeWire`/`Jack` variants exist in `config::AudioBackend` and `create_output` always compiles their arms, but the engine isn't built with those features (see `cargo tree`) and the UI doesn't offer them (B32). Decide: implement them for real (E1–E3) or gate the variants/arms behind the feature and remove the fake paths.

---

## 6. Verified non-issues (do not "fix")

- `views/*` are thin delegating wrappers over `components/*` (e.g. `views/albums_view.rs:6` → `components::albums_view::AlbumsView`) and **are** used by `app.rs:2151-2263`. `layout/sidebar.rs` (renderer) vs `components/sidebar.rs` (`NavItem`/`SideBar`) is a deliberate split, not duplication.
- `.cargo/config.toml` `[profile.*]` **is** honoured by Cargo 1.98 — verified `debuginfo=line`/no-incremental on a rebuild. The low-memory build tuning works.
- `graphic_gains: [f32;64]` indexing is bounded by `num_bands()`; `catmull_rom_spline` guards `len<2`; the few `unwrap()`s in UI are behind early returns or `#[cfg(test)]`.
- Every workspace crate compiles without warnings because the unused items are `pub` in library crates (no `dead_code` warning) — hence the manual dead-code list in C6.

---

## 7. Suggested sequencing

**Milestone 1 — Stop the bleeding (P0).** A1, A2, A5, A7, A10, A3, A4. Each with a regression test (D14). Small and mechanical; they prevent corruption and a scanner crash. _Effort: ~1 week._

**Milestone 2 — Playback & UI honesty (P1 subset).** B1–B7, B8–B15, B16–B20, B32. Visible defects and controls that lie. _Effort: ~1 week._

**Milestone 3 — Data-layer correctness (P1 rest).** B21–B31, plus A6/A8/A11 if not already done. _Effort: ~1 week._

**Milestone 4 — Engine correctness.** E1–E6, E9–E12 (backends honesty, FFI liveness, convolution IR, CLAP lifecycle, resampler dropouts, atomic playlist save). Decide E25 (implement PipeWire/JACK properly or feature-gate them away). _Effort: ~2 weeks._

**Milestone 5 — Exposure & docs (P2).** C1–C7 (decide C5/C7 by feature intent). Add README/LICENSE/CI (C2) and refresh CHANGELOG (C4) as part of the release. _Effort: ~3–5 days._

**Milestone 6 — Hygiene (P3).** D1–D15 and E13–E24 as a steady background burn-down; prioritise D4/D5/D6 (perf on large libraries), D1/D2/D3 (error visibility), D14 (tests).

**Cross-cutting rule:** every change in Milestones 1–4 ships with a test that fails before and passes after; run `cargo check --workspace --bins --lib` + the crate's tests before committing. The engine has its own workspace (`crates/engine`) and its own 1291-test suite — run it from `crates/engine` for E-items.

---

## 8. Feature-completeness verdict

All 22 product features (MPRIS2, smart playlists, library-wide ReplayGain + tag write, multi-select/bulk, stable identity, rfd import, playlist manager, edit-song-info, sleep timer, play stats, virtualization/artwork LRU, M3U import/export, watch roots + startup reconcile, duplicate finder, batch organize/rename, CLAP rack, chain presets, tag/sidecar lyrics, recently-added + chips + toasts, bass/LFE/up-and-down-mix, DSP precision + runtime diagnostics) are **implemented end-to-end**. There are no stub features. The gaps are correctness, robustness, persistence and documentation drift, not missing functionality — with three exceptions: **sort direction** (B20/C1), the **unreachable Advanced Audio modal** (B12), and the **fake PipeWire/JACK backends** (E1–E3, currently unreachable from the UI). The engine advertises far more than the desktop app exposes (E25 / C5), so the standalone-library surface and the app surface should be reconciled deliberately rather than left to drift.

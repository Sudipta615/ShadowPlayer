# Contributing to ShadowPlayer

Thanks for taking the time to contribute. ShadowPlayer is two Rust workspaces
with different rules, so the first thing to know is which one you are changing.

| Workspace | Path | What lives there |
| --- | --- | --- |
| Player | repository root | `shadowplayer` app, UI, library, database, metadata, playback, search, recommendation |
| Engine | `crates/engine` | realtime audio engine, DSP, decoders, output backends, C ABI |

The engine is independently versioned and has its own contract in
[`crates/engine/AGENTS.md`](crates/engine/AGENTS.md). Read it before touching
anything under `crates/engine`; it documents the realtime, honesty and DSP
rules that unit tests cannot enforce on their own.

## Prerequisites

- Rust **1.98** or newer (`rustup show` should select it).
- Linux: `build-essential`, `pkg-config`, `libasound2-dev`, plus
  `libxkbcommon-dev`, `libwayland-dev`, `libx11-dev` and `dbus` for the UI.
- macOS: Xcode command-line tools.
- Windows: the MSVC build tools and the Rust `x86_64-pc-windows-msvc` target.

## Build and run

```bash
# Player
cargo run -p shadowplayer

# Engine only (its own workspace)
cd crates/engine && cargo test

# Headless engine CLI
cd crates/engine && cargo run --bin engine-cli -- --help
```

## Before you commit

Player workspace:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --bins --lib --all-targets -- -D warnings
cargo test -p domain -p database -p metadata -p lyrics -p library \
           -p playback -p search -p audio-session -p media-tools \
           -p shadow-recommender -p ui --lib -- --test-threads=1
```

Engine workspace:

```bash
cd crates/engine
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo bench --no-run
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

CI runs all of the above, plus the 100,000-track library budget, the
documentation-truth tests, the cross-platform builds, `cargo-deny` and
`cargo-audit`. It also runs the engine's DSP and buffer tests under
AddressSanitizer, compiles the fuzz targets on every push (and runs them on a
schedule), and enforces coverage floors of 30% lines for the player workspace
and 70% lines for the engine. A change that only passes locally is not done
yet.

## Tests are part of the change

- A bug fix needs a test that fails before the fix.
- A new capability needs the test that proves it; for the engine that is
  usually `tests/codec_matrix.rs`, `tests/suite_manifest.rs` or a fidelity
  test under `tests/fidelity/`.
- A README number, capability list or limitation is a claim. The tests under
  `crates/app/tests/docs_truth.rs` and `crates/engine/tests/suite_manifest.rs`
  check those claims, so update the test in the same commit as the claim.

## Code style

- Run `cargo fmt`; formatting is enforced, not reviewed.
- Warnings are errors in CI. Do not add `#[allow(...)]` to silence a lint
  without a comment explaining why the lint is wrong here.
- Prefer explicit error types over `unwrap()`/`expect()` in production code.
  When an invariant genuinely makes a panic impossible, say so in the
  `expect("…")` message and, where the invariant is non-obvious, add a comment.
- Keep the audio thread allocation-free and lock-free. The engine's
  `tests/realtime_alloc_budget.rs` is the guard; extend it when a new stage
  allocates during setup.
- Public items get rustdoc. The engine treats documentation as a published
  surface and builds it with `-D warnings`.

## Localisation

UI strings go through `tr!` / `tr_args!`, backed by
`crates/ui/locales/en.json`. The build script rejects a call site with no
catalogue entry and a catalogue entry with no call site, so add both in the
same change. Run the scanner regression test after touching the build script:

```bash
cargo test -p ui --test i18n_source_scan
```

## Commits and pull requests

- Keep commits focused; the subject uses the existing
  `type(scope): summary` style (`fix(ui):`, `feat(engine):`, `docs:`).
- Explain *why* in the body, not just what changed.
- Do not mix a behavioural change with a large mechanical refactor.
- A pull request should state what you ran, and call out anything you could
  not run (for example, a Windows-only backend on a Linux machine).

## Reporting security issues

Do not open a public issue for a vulnerability. See [`SECURITY.md`](SECURITY.md).

## License

By contributing you agree that your contribution is licensed under the
Apache License 2.0, the same licence as the project.

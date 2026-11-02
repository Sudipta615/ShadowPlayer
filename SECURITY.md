# Security Policy

ShadowPlayer plays untrusted files and loads third-party plugin binaries, so a
memory-safety or parser bug is a security bug, not only a crash.

## Supported versions

| Line | Version | Support |
| --- | --- | --- |
| Player | `0.27.x` | Security fixes |
| Engine | `1.12.x` | Security fixes |
| Older | `< 0.27` / `< 1.12` | Best effort; upgrade first |

## Reporting a vulnerability

**Do not open a public issue, discussion or pull request for a vulnerability.**

Use GitHub's private vulnerability reporting:

1. Open the repository's **Security** tab.
2. Choose **Report a vulnerability**.
3. Include the details below.

If private reporting is unavailable, contact the maintainer through the contact
method on their GitHub profile and ask for a private channel before sending
details.

Please include:

- the affected component (player, engine, a specific decoder, the C ABI, the
  CLAP host, a platform backend);
- the version or commit;
- the platform and build features;
- a minimal reproducer (file, playlist, LRC, packet capture or plugin) if one
  can be shared safely;
- the observed behaviour and the impact you believe it has;
- whether you intend to publish, and any deadline you are working to.

## Scope

In scope:

- memory-safety bugs in the audio decoders, resamplers, DSP and spatial
  renderers (including the `unsafe` SIMD and FFI boundaries);
- panics or unbounded resource use triggered by a malformed audio file,
  playlist, LRC file, CUE sheet, SOFA/ADM document or network stream;
- path traversal, command execution, or unexpected file writes caused by
  untrusted input;
- bugs in the C ABI (`crates/engine/src/ffi.rs`) reachable from a host;
- unsafe loading or callback handling in the CLAP plugin host;
- supply-chain issues in the dependency graph.

Out of scope:

- bugs that require the attacker to already control the user's account, the
  audio device configuration, or a plugin the user deliberately installed;
- denial of service caused only by playing an extremely large but otherwise
  valid file on a machine below the documented requirements;
- missing hardening that has no demonstrated impact;
- issues in third-party dependencies that are already public upstream — report
  those upstream, and tell us so we can pin or patch.

## What we do

- We acknowledge a report within **7 days**.
- We aim to ship a fix or a documented mitigation within **90 days**, and we
  will keep you informed if a fix takes longer.
- We will credit you in the release notes unless you ask to stay anonymous.
- We do not run a paid bug-bounty programme.

## Defences already in the tree

- `cargo-deny` (licences, advisories, sources) and `cargo-audit` run on every
  push.
- The engine's test suite runs on Linux, Windows and macOS; the unsafe DSP
  paths run under AddressSanitizer in CI.
- Fuzz targets for the decoders, playlist parsers and LRC parser run on a
  schedule.
- Playlist parsing performs no I/O and never passes input to a shell.
- Every `extern "C"` entry point is wrapped in `catch_unwind`.

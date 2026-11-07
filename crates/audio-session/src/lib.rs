pub mod session;

pub use session::*;

/// The engine, re-exported for the crates above this one.
///
/// `AGENTS.md` §1 makes this crate the single bridge to `crates/engine`. The
/// player crates still need the engine's plain data types (snapshots, output
/// info, codec registries, tag types), and taking them from here keeps that
/// dependency one-way: `ui` and `app` depend on this crate, not on the engine,
/// so a new engine capability has to arrive through the bridge.
pub use engine;

#[cfg(test)]
mod tests;

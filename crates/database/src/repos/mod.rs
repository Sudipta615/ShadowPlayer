//! Database repositories, split by domain.
//!
//! Each submodule owns one cohesive area of persistence and exposes its
//! repository structs. This module is the façade: `crate::repos::*` (and
//! therefore `crate::*`) keeps re-exporting every repository exactly as the
//! former single-file module did, so downstream imports need no changes.
//!
//! Repositories are stateless handles over [`crate::db::Database`]; they may
//! call the crate-private helpers in [`common`] but must not reach into another
//! repository's SQL.

mod common;
mod history;
mod library_nodes;
mod media;
mod playlist;
mod settings;
mod track;

pub use history::*;
pub use library_nodes::*;
pub use media::*;
pub use playlist::*;
pub use settings::*;
pub use track::*;

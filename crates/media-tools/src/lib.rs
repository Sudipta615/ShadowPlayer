//! Media tooling that sits beside playback rather than in its realtime path:
//! CD extraction, lossy/lossless export, album liner notes, and opt-in update
//! verification.
//!
//! The native codecs and CD libraries are loaded dynamically. A build therefore
//! does not need development headers, and a machine that lacks a library gets a
//! clear [`MediaError::LibraryMissing`] at the point of use instead of a link
//! error at startup.
//!
//! Nothing in this crate installs software, accepts untrusted plugin code, or
//! performs a network request unless the caller explicitly asks for it.

pub mod cd;
pub mod dynlib;
pub mod markdown;
pub mod transcode;
pub mod update;

use std::path::PathBuf;

/// An error from one of the media tools.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("{what} needs the system library {library}, which could not be loaded: {detail}")]
    LibraryMissing {
        what: &'static str,
        library: &'static str,
        detail: String,
    },
    #[error("unsupported media operation: {0}")]
    Unsupported(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("could not decode {path}: {message}")]
    Decode { path: PathBuf, message: String },
    #[error("could not write {path}: {message}")]
    Encode { path: PathBuf, message: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CD error: {0}")]
    Cd(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("update check is disabled; it is opt-in")]
    UpdateDisabled,
    #[error("no compatible release asset was found for this platform")]
    NoCompatibleAsset,
    #[error("release {version} has no SHA-256 checksum; refusing to download it")]
    MissingChecksum { version: String },
    #[error("SHA-256 mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, MediaError>;

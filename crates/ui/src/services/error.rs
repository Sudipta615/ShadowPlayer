//! A small, structured error type for UI services.
//!
//! Services historically reported failures as `String`s, which erased the
//! distinction between I/O, network, parse and logic errors and made
//! programmatic handling impossible. New code should return
//! [`ServiceError`] instead; the string form is still available through
//! [`std::fmt::Display`] for toasts and logs.
//!
//! Migration is incremental: existing `Result<T, String>` call sites can be
//! converted one service at a time by mapping their error construction to a
//! [`ServiceError`] variant and calling `.to_string()` at the UI boundary.

use std::fmt;

/// Failure reported by a UI service.
#[derive(Debug, Clone)]
pub enum ServiceError {
    /// A worker thread could not be spawned.
    Spawn(String),
    /// A worker stopped unexpectedly or was cancelled.
    Worker(String),
    /// Decoding or parsing a payload failed.
    Decode(String),
    /// A requested resource was not found.
    NotFound(String),
    /// The request was invalid or unsupported.
    Invalid(String),
    /// Anything else, including wrapped I/O and network errors.
    Other(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(m) => write!(f, "could not start the worker: {m}"),
            Self::Worker(m) => write!(f, "the worker stopped unexpectedly: {m}"),
            Self::Decode(m) => write!(f, "decode failed: {m}"),
            Self::NotFound(m) => write!(f, "not found: {m}"),
            Self::Invalid(m) => write!(f, "invalid request: {m}"),
            Self::Other(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<std::io::Error> for ServiceError {
    fn from(error: std::io::Error) -> Self {
        Self::Other(error.to_string())
    }
}

impl From<String> for ServiceError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<&str> for ServiceError {
    fn from(message: &str) -> Self {
        Self::Other(message.to_string())
    }
}

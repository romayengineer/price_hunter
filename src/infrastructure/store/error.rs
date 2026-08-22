//! Typed errors from the PocketBase data layer. Services and the binary keep
//! `anyhow::Result` and `?` converts automatically (`Error` implements
//! `std::error::Error`, and anyhow wraps any error).

use thiserror::Error;

/// Errors surfaced by the store's public entry points.
#[derive(Debug, Error)]
pub enum Error {
    /// Bad or missing configuration (e.g. no password).
    #[error("configuration error: {0}")]
    Config(String),
    /// Could not authenticate as superuser.
    #[error("PocketBase authentication failed: {0}")]
    Auth(String),
    /// A PocketBase request failed (HTTP or transport) or the response could
    /// not be parsed.
    #[error("PocketBase request failed: {0}")]
    Http(String),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Http(format!("{e:#}"))
    }
}



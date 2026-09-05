//! Typed error for the persistence port. The domain layer uses this instead of
//! `anyhow`, which is reserved for application/CLI code; infrastructure adapters
//! map their concrete failures into this type at the boundary.

use std::fmt::{Display, Formatter};

/// Errors surfaced by the [`PriceStore`](super::ports::PriceStore) port.
#[derive(Debug, PartialEq, Eq)]
pub enum PriceStoreError {
    /// A store request failed (HTTP, transport, parse or auth).
    Request(String),
}

impl Display for PriceStoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PriceStoreError::Request(msg) => write!(f, "store request failed: {msg}"),
        }
    }
}

impl std::error::Error for PriceStoreError {}

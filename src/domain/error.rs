//! Typed error for the persistence port. The domain layer uses this instead of
//! `anyhow`, which is reserved for application/CLI code; infrastructure adapters
//! map their concrete failures into this type at the boundary.

/// Errors surfaced by the [`PriceStore`](super::ports::PriceStore) port.
#[derive(Debug, thiserror::Error)]
pub enum PriceStoreError {
    /// A store request failed (HTTP, transport, parse or auth).
    #[error("store request failed: {0}")]
    Request(String),
}

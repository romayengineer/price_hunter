//! Domain layer: pure types and business logic with no I/O dependencies.
//! Nothing here touches the network, the database, or the filesystem — the
//! only outward dependency is the [`PriceStore`](ports::PriceStore) port,
//! which infrastructure adapters implement.
//!
//! The pure entities live in the `price_hunter_domain` crate; this module
//! re-exports them so existing `crate::domain::model` (etc.) paths keep
//! working, and keeps the HTML-dependent `detect` pipeline locally.

pub mod detect;

pub use price_hunter_domain::error;
pub use price_hunter_domain::matching;
pub use price_hunter_domain::model;
pub use price_hunter_domain::ports;
pub use price_hunter_domain::text;
pub use price_hunter_domain::time;

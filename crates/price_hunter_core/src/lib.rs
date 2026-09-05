//! Price Hunter core: pure domain types, detection pipeline, fuzzy matching,
//! and application use cases. No browser, filesystem, or PocketBase I/O — all
//! persistence goes through the [`domain::ports::PriceStore`] port.

#![deny(missing_docs)]

pub mod application;
pub mod domain;

pub use domain::detect;
pub use domain::matching;
pub use application::export;

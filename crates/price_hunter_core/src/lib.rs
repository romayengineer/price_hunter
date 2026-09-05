//! Price Hunter core: HTML grid detection over `price_hunter_domain` types.
//! Use cases and pure logic live in `price_hunter_domain`; this crate keeps
//! only the scraper-based traversal.

#![deny(missing_docs)]

pub mod domain;

pub use domain::detect;

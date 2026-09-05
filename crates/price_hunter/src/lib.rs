//! Price Hunter: detects product price grids in arbitrary e-commerce HTML and
//! captures them.
//!
//! Layered layout: `domain` (pure types + logic), `application` (use cases),
//! `infrastructure` (adapters: PocketBase, browser, HTTP server, files).
//! The `pub use` aliases below keep the historic public module paths (`detect`,
//! `store`, ...) stable for the binary and integration tests.

#![deny(missing_docs)]

pub mod infrastructure;

// Re-export core so `crate::domain`/`crate::application` keep working in
// infrastructure and `price_hunter::detect` stays stable for tests.
pub use price_hunter_core::application;
pub use price_hunter_core::domain;

pub use price_hunter_core::domain::detect;
pub use price_hunter_core::domain::matching;
pub use price_hunter_core::application::export;
pub use crate::infrastructure::autoscrape;
pub use crate::infrastructure::browser;
pub use crate::infrastructure::capture;
pub use crate::infrastructure::config;
pub use crate::infrastructure::instance;
pub use crate::infrastructure::matrix_server;
pub use crate::infrastructure::store;
pub use crate::infrastructure::terminal;

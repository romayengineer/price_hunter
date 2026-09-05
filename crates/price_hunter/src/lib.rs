//! Price Hunter: detects product price grids in arbitrary e-commerce HTML and
//! captures them.
//!
//! Layered layout: `price_hunter_domain` (pure types + use cases),
//! `price_hunter_core` (HTML detection traversal), `infrastructure` (adapters:
//! PocketBase, browser, HTTP server, files).

#![deny(missing_docs)]

pub mod infrastructure;

// Stable paths for the binary and tests.
pub use price_hunter_core::domain::detect;
pub use price_hunter_domain::usecases::export;
pub use crate::infrastructure::autoscrape;
pub use crate::infrastructure::browser;
pub use crate::infrastructure::capture;
pub use crate::infrastructure::config;
pub use crate::infrastructure::instance;
pub use crate::infrastructure::matrix_server;
pub use crate::infrastructure::store;
pub use crate::infrastructure::terminal;

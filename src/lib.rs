//! Price Hunter: detects product price grids in arbitrary e-commerce HTML and
//! captures them.
//!
//! Layered layout: `domain` (pure types + logic), `application` (use cases),
//! `infrastructure` (adapters: PocketBase, browser, HTTP server, files).
//! The `pub use` aliases below keep the historic public module paths (`detect`,
//! `store`, `services`, ...) stable for the binary and integration tests.

pub mod application;
pub mod domain;
pub mod infrastructure;

pub use crate::application as services;
pub use crate::domain::detect;
pub use crate::domain::matching;
pub use crate::infrastructure::browser;
pub use crate::infrastructure::capture;
pub use crate::infrastructure::config;
pub use crate::infrastructure::export;
pub use crate::infrastructure::instance;
pub use crate::infrastructure::matrix_server;
pub use crate::infrastructure::store;

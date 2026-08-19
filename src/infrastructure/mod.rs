//! Infrastructure layer: adapters that implement the domain's [`PriceStore`]
//! port and the external I/O boundaries (browser, files, HTTP server, config).
//! Everything here depends on the domain; nothing in `domain` depends on it.

pub mod autoscrape;
pub mod browser;
pub mod capture;
pub mod config;
pub mod export;
pub mod instance;
pub mod matrix_server;
pub mod store;
mod util;

//! Offline fixture-based detection tests: one module per e-commerce site, each
//! asserting `detect_grid` extracts the expected products (name + price) and
//! picks the right container from a saved `tests/fixtures/<site>.html`.
//! These need no network or browser and always run.

mod common;
#[path = "sites/mod.rs"]
mod sites;

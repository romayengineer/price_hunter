//! Application-layer integration tests that exercise the use cases
//! (`matching`, `brands`, `matrix`) against an in-memory [`PriceStore`] fake
//! instead of PocketBase. They verify the port decouples the application from
//! infrastructure and run fully offline in `cargo test`.

#[path = "application/mod.rs"]
mod application;

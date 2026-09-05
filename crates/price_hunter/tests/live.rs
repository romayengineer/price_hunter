//! Integration tests that need external services (a live browser session,
//! plain network access, or a running PocketBase). All are `#[ignore]`d so a
//! normal `cargo test` skips them; run with `cargo test --test live -- --ignored`
//! (optionally filtered, e.g. `cargo test --test live compreahora -- --ignored`).

#[path = "live/mod.rs"]
mod live;

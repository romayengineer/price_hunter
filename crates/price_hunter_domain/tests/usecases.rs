//! Use-case integration tests exercising `usecases` against an in-memory
//! [`PriceStore`](price_hunter_domain::ports::PriceStore) fake instead of
//! PocketBase. Fully offline.

#[path = "usecases/mod.rs"]
mod usecases;

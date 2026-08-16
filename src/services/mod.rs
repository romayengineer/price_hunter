//! Orchestration layer: each service composes `store` data access with the
//! pure `matching`/detect logic. Services never touch the PocketBase client
//! directly — all persistence goes through `Store` methods.

pub mod brands;
pub mod matching;
pub mod matrix;
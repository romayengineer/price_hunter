//! Application use cases composing domain logic with the [`PriceStore`](crate::ports::PriceStore)
//! port. Nothing here touches I/O directly — all persistence goes through the
//! injected port.

pub mod brands;
pub mod export;
pub mod imports;
pub mod matching;
pub mod matrix;
pub mod persist;
pub mod prune;

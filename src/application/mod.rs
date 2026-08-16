//! Application layer: use cases that compose the domain logic with the
//! [`PriceStore`] port. Nothing here touches the PocketBase client or any other
//! I/O directly — all persistence goes through the injected port.

pub mod brands;
pub mod matching;
pub mod matrix;

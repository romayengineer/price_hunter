//! Domain layer: pure types and business logic with no I/O dependencies.
//! Nothing here touches the network, the database, or the filesystem — the
//! only outward dependency is the [`PriceStore`] port (`domain::ports`),
//! which infrastructure adapters implement.

pub mod detect;
pub mod matching;
pub mod model;
pub mod ports;
pub mod time;

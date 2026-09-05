//! Price Hunter domain: pure types and business logic with no I/O.
//! Nothing here touches the network, the database, the filesystem, or HTML
//! parsing — the only outward dependencies are pure libraries (`serde` for
//! record shapes, `strsim` for fuzzy matching, `chrono` for timestamps) plus
//! the [`ports::PriceStore`] port, which infrastructure adapters implement.

#![deny(missing_docs)]

pub mod error;
pub mod matching;
pub mod model;
pub mod ports;
pub mod text;
pub mod time;

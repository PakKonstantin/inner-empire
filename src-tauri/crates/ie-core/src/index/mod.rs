//! The vault index.
//!
//! SQLite, holding only derived data. Every table can be reconstructed from
//! the Markdown files, and the code is written on that assumption: a corrupt
//! or outdated index is discarded and rebuilt rather than repaired, because a
//! rebuild cannot be wrong and a repair can.

pub mod db;
pub mod indexer;
pub mod queries;
pub mod resolve;
pub mod schema;
pub mod writer;

pub use db::{IndexDb, OpenOutcome};
pub use indexer::{EventOutcome, IgnoreRules, IndexProgress, Indexer, ScanReport};
pub use queries::{GraphOptions, UnresolvedTarget};
pub use resolve::Resolution;

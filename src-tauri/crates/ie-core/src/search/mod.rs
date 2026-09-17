//! Search: a small query language, executed against the index.

pub mod engine;
pub mod fuzzy;
pub mod query;

pub use engine::{search, quick_switch, FileMatch, SearchHit, SearchOptions, SearchResults};
pub use query::{parse, Comparison, Filter, Query, Structural, Term};

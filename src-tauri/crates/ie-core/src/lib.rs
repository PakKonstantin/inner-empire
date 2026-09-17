//! Inner Empire core.

pub mod error;
pub mod events;
pub mod export;
pub mod index;
pub mod links;
pub mod logging;
pub mod markdown;
pub mod model;
pub mod search;
pub mod session;
pub mod templates;
pub mod vault;
pub mod workspace;

pub use error::{CoreError, Diagnostic, Result, Severity};

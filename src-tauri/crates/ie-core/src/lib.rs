//! Inner Empire core.

pub mod error;
pub mod index;
pub mod links;
pub mod markdown;
pub mod model;
pub mod vault;

pub use error::{CoreError, Diagnostic, Result, Severity};

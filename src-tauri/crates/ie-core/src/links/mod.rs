//! Link resolution: turning what the user wrote into a file, and keeping every
//! reference correct when files move.

pub mod reference;
pub mod rename;
pub mod resolver;

pub use reference::{heading_matches, slugify, LinkTarget};
pub use rename::{plan_rename, RenameEdit, RenamePlan};
pub use resolver::{LinkResolution, LinkResolver, ResolutionOutcome};

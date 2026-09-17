//! The vault: a plain folder the user owns, plus the small amount of app state
//! that travels with it.

pub mod path;

pub use path::{sanitize_segment, validate_segment, VaultPath, APP_DIR};

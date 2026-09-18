//! The bridge between the shared Rust core and a native host.
//!
//! See `docs/ios/IMPLEMENTATION_PLAN.md` §5 for why this exists and why it is
//! shaped the way it is. In one line: `ie-core` already expects to be hosted,
//! and this is the third host.
uniffi::setup_scaffolding!();

pub mod diff;
pub mod editing;
pub mod error;
pub mod handle;
pub mod host;
pub mod types;

pub use error::{FfiError, Result};
pub use handle::VaultHandle;
pub use host::{HostConfig, StorageKind};

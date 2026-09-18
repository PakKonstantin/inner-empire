//! The IPC surface.
//!
//! Every command is small and typed, and every one of them reaches the vault
//! through `AppState`, so there is exactly one place where "is a vault open?"
//! is answered and exactly one type of error that can come back.

pub mod files;
pub mod index_queries;
pub mod search;
pub mod vault;
pub mod workspace;

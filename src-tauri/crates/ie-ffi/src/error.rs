//! Errors, as something Swift can branch on.
//!
//! `CoreError` already has a shape the caller can act on — the UI turns
//! `CaseCollision` into a rename prompt and `NotFound` into a create-note
//! offer — and losing that at the bridge would mean Swift parsing prose. So
//! every variant survives the crossing with its payload, and UniFFI renders it
//! as a Swift `enum` conforming to `Error`.
//!
//! Two variants exist here that have no counterpart in the core, because they
//! describe things only a host can know about.

use ie_core::error::CoreError;

/// Everything the bridge can fail with.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("no vault is open")]
    NoVaultOpen,

    #[error("{path} is not inside the vault")]
    OutsideVault { path: String },

    #[error("{path} does not exist in this vault")]
    NotFound { path: String },

    #[error("{path} already exists")]
    AlreadyExists { path: String },

    #[error("{path} is a folder, not a note")]
    NotANote { path: String },

    /// A name differing only by capitalisation from something already there.
    /// Legal on a case-sensitive volume, silently destructive on the
    /// case-insensitive one iOS ships with, so it is refused on both.
    #[error("{requested} collides with the existing {existing} on case-insensitive filesystems")]
    CaseCollision { requested: String, existing: String },

    #[error("{name} is not a usable file name: {reason}")]
    InvalidName { name: String, reason: String },

    #[error("the link target {target} matches several notes: {candidates}")]
    AmbiguousLink { target: String, candidates: String },

    #[error("frontmatter in {path} is not valid YAML: {message}")]
    InvalidFrontmatter { path: String, message: String },

    #[error("the search query could not be parsed: {message}")]
    InvalidQuery { message: String },

    #[error("the index is unusable and must be rebuilt: {message}")]
    IndexCorrupt { message: String },

    #[error("index operation failed: {message}")]
    Index { message: String },

    #[error("{operation} is not available on this platform")]
    Unsupported { operation: String },

    #[error("{message}")]
    Platform { code: String, message: String },

    #[error("{operation} is not allowed: {reason}")]
    Refused { operation: String, reason: String },

    /// The file changed underneath an open buffer.
    ///
    /// Not a core error, because the desktop resolves it differently: the
    /// file may have been edited in the Files app, by iCloud, or by a desktop
    /// client syncing the same folder. The only correct response is to ask,
    /// which is why this exists as its own variant rather than as a `Refused`
    /// the UI would have to read the prose of.
    #[error("{path} changed on disk since it was opened")]
    ExternalModification {
        path: String,
        /// When the buffer was loaded.
        opened_modified_ms: i64,
        /// What the file says now.
        current_modified_ms: i64,
    },

    /// The host handed the bridge something it could not use — a path that
    /// escapes the vault, a malformed property value.
    #[error("{message}")]
    InvalidArgument { message: String },
}

impl FfiError {
    /// The same stable identifier the desktop uses, so a log line or a bug
    /// report means the same thing on both platforms.
    pub fn code(&self) -> &'static str {
        match self {
            FfiError::NoVaultOpen => "no_vault_open",
            FfiError::OutsideVault { .. } => "outside_vault",
            FfiError::NotFound { .. } => "not_found",
            FfiError::AlreadyExists { .. } => "already_exists",
            FfiError::NotANote { .. } => "not_a_note",
            FfiError::CaseCollision { .. } => "case_collision",
            FfiError::InvalidName { .. } => "invalid_name",
            FfiError::AmbiguousLink { .. } => "ambiguous_link",
            FfiError::InvalidFrontmatter { .. } => "invalid_frontmatter",
            FfiError::InvalidQuery { .. } => "invalid_query",
            FfiError::IndexCorrupt { .. } => "index_corrupt",
            FfiError::Index { .. } => "index_error",
            FfiError::Unsupported { .. } => "unsupported",
            FfiError::Platform { .. } => "platform_error",
            FfiError::Refused { .. } => "refused",
            FfiError::ExternalModification { .. } => "external_modification",
            FfiError::InvalidArgument { .. } => "invalid_argument",
        }
    }

    /// Whether the right response is to discard the index and rescan. A cache
    /// being unusable is never a reason to show the user an error.
    pub fn warrants_index_rebuild(&self) -> bool {
        matches!(self, FfiError::IndexCorrupt { .. })
    }
}

impl From<CoreError> for FfiError {
    fn from(error: CoreError) -> Self {
        // `warrants_index_rebuild` also covers SQLite's corruption codes, which
        // arrive as `CoreError::Index` and would otherwise lose that meaning.
        let rebuild = error.warrants_index_rebuild();
        match error {
            CoreError::NoVaultOpen => FfiError::NoVaultOpen,
            CoreError::OutsideVault { path } => FfiError::OutsideVault {
                path: path.to_string_lossy().to_string(),
            },
            CoreError::NotFound(path) => FfiError::NotFound {
                path: path.as_str().to_string(),
            },
            CoreError::AlreadyExists(path) => FfiError::AlreadyExists {
                path: path.as_str().to_string(),
            },
            CoreError::NotANote { path } => FfiError::NotANote {
                path: path.as_str().to_string(),
            },
            CoreError::CaseCollision {
                requested,
                existing,
            } => FfiError::CaseCollision {
                requested,
                existing: existing.as_str().to_string(),
            },
            CoreError::InvalidName { name, reason } => FfiError::InvalidName { name, reason },
            CoreError::AmbiguousLink { target, candidates } => {
                FfiError::AmbiguousLink { target, candidates }
            }
            CoreError::InvalidFrontmatter { path, message } => FfiError::InvalidFrontmatter {
                path: path.as_str().to_string(),
                message,
            },
            CoreError::InvalidQuery { message } => FfiError::InvalidQuery { message },
            CoreError::IndexCorrupt { message } => FfiError::IndexCorrupt { message },
            CoreError::Index(e) if rebuild => FfiError::IndexCorrupt {
                message: e.to_string(),
            },
            CoreError::Index(e) => FfiError::Index {
                message: e.to_string(),
            },
            CoreError::Platform(e) => match e {
                ie_platform::PlatformError::Unsupported { operation } => FfiError::Unsupported {
                    operation: operation.to_string(),
                },
                other => FfiError::Platform {
                    code: other.code().to_string(),
                    message: other.to_string(),
                },
            },
            CoreError::Json(e) => FfiError::InvalidArgument {
                message: e.to_string(),
            },
            CoreError::Refused { operation, reason } => FfiError::Refused {
                operation: operation.to_string(),
                reason,
            },
        }
    }
}

impl From<ie_platform::PlatformError> for FfiError {
    fn from(error: ie_platform::PlatformError) -> Self {
        FfiError::from(CoreError::Platform(error))
    }
}

pub type Result<T> = std::result::Result<T, FfiError>;

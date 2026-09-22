use std::path::PathBuf;

use crate::vault::path::VaultPath;

/// Every failure the core can produce.
///
/// Variants are shaped by what the caller can *do* about them, not by where
/// they came from: the UI turns `CaseCollision` into a rename prompt,
/// `IndexCorrupt` into a silent rebuild, and `NotFound` into a create-note
/// offer. There is no catch-all string variant, because a string cannot be
/// branched on.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("no vault is open")]
    NoVaultOpen,

    #[error("{path} is not inside the vault")]
    OutsideVault { path: PathBuf },

    #[error("{0} does not exist in this vault")]
    NotFound(VaultPath),

    #[error("{0} already exists")]
    AlreadyExists(VaultPath),

    #[error("{path} is a folder, not a note")]
    NotANote { path: VaultPath },

    /// A name that differs only by capitalisation from an existing entry.
    /// Allowed on ext4, silently destructive on NTFS, so it is refused
    /// everywhere to keep vaults portable.
    #[error("{requested} collides with the existing {existing} on case-insensitive filesystems")]
    CaseCollision {
        requested: String,
        existing: VaultPath,
    },

    #[error("{name} is not a usable file name: {reason}")]
    InvalidName { name: String, reason: String },

    #[error("the link target {target} matches several notes: {candidates}")]
    AmbiguousLink { target: String, candidates: String },

    #[error("frontmatter in {path} is not valid YAML: {message}")]
    InvalidFrontmatter { path: VaultPath, message: String },

    #[error("the search query could not be parsed: {message}")]
    InvalidQuery { message: String },

    #[error("the index is unusable and must be rebuilt: {message}")]
    IndexCorrupt { message: String },

    #[error("index operation failed: {0}")]
    Index(#[from] rusqlite::Error),

    #[error("{0}")]
    Platform(#[from] ie_platform::PlatformError),

    #[error("could not read or write JSON state: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{operation} is not allowed: {reason}")]
    Refused {
        operation: &'static str,
        reason: String,
    },
}

pub type Result<T> = std::result::Result<T, CoreError>;

impl CoreError {
    /// A stable identifier the UI can switch on. Never localise or reformat
    /// these; the prose in `Display` is what the user reads.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::NoVaultOpen => "no_vault_open",
            CoreError::OutsideVault { .. } => "outside_vault",
            CoreError::NotFound(_) => "not_found",
            CoreError::AlreadyExists(_) => "already_exists",
            CoreError::NotANote { .. } => "not_a_note",
            CoreError::CaseCollision { .. } => "case_collision",
            CoreError::InvalidName { .. } => "invalid_name",
            CoreError::AmbiguousLink { .. } => "ambiguous_link",
            CoreError::InvalidFrontmatter { .. } => "invalid_frontmatter",
            CoreError::InvalidQuery { .. } => "invalid_query",
            CoreError::IndexCorrupt { .. } => "index_corrupt",
            CoreError::Index(_) => "index_error",
            CoreError::Platform(e) => e.code(),
            CoreError::Json(_) => "json_error",
            CoreError::Refused { .. } => "refused",
        }
    }

    /// Whether the right response is to discard the index and rescan.
    pub fn warrants_index_rebuild(&self) -> bool {
        match self {
            CoreError::IndexCorrupt { .. } => true,
            CoreError::Index(rusqlite::Error::SqliteFailure(e, _)) => matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            ),
            _ => false,
        }
    }
}

/// A non-fatal problem found while scanning a vault. Collected and shown
/// together rather than raised one at a time, because a vault with a hundred
/// case collisions should produce one report, not a hundred modal dialogs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Diagnostic {
    /// Two files whose names differ only by case. Fine on ext4; on NTFS one
    /// of them would shadow the other.
    #[serde(rename_all = "camelCase")]
    CaseConflict { paths: Vec<VaultPath> },

    /// A name that Windows would reject, found in a vault authored elsewhere.
    #[serde(rename_all = "camelCase")]
    UnportableName { path: VaultPath, reason: String },

    /// A leftover `.ie-tmp-*` file: a previous write was interrupted.
    #[serde(rename_all = "camelCase")]
    InterruptedWrite { path: VaultPath },

    /// The file could not be read; it is left untouched and excluded from the
    /// index rather than being treated as empty.
    #[serde(rename_all = "camelCase")]
    UnreadableFile { path: VaultPath, message: String },

    /// Frontmatter that is not valid YAML. The file is still indexed for
    /// full-text and links; only its properties are skipped.
    #[serde(rename_all = "camelCase")]
    MalformedFrontmatter { path: VaultPath, message: String },

    /// A symbolic link pointing outside the vault. Not followed.
    #[serde(rename_all = "camelCase")]
    EscapingSymlink { path: VaultPath },
}

impl Diagnostic {
    pub fn severity(&self) -> Severity {
        match self {
            Diagnostic::CaseConflict { .. } | Diagnostic::InterruptedWrite { .. } => {
                Severity::Warning
            }
            Diagnostic::UnreadableFile { .. } => Severity::Error,
            _ => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

use std::fmt;
use std::path::{Path, PathBuf};

/// Every fallible platform operation returns this. The variants exist so the
/// layers above can react differently (retry, prompt, rebuild) instead of
/// pattern-matching on error strings.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{path} was not found")]
    NotFound { path: PathBuf },

    #[error("permission denied for {path}")]
    PermissionDenied { path: PathBuf },

    #[error("{path} already exists")]
    AlreadyExists { path: PathBuf },

    #[error("{path} is a directory, not a file")]
    IsADirectory { path: PathBuf },

    #[error("{path} is a file, not a directory")]
    NotADirectory { path: PathBuf },

    #[error("{path} is not valid UTF-8")]
    InvalidUtf8 { path: PathBuf },

    #[error("could not determine a standard {purpose} directory for this platform")]
    NoStandardDirectory { purpose: &'static str },

    #[error("filesystem watcher failed: {message}")]
    Watcher { message: String },

    /// The running platform has no meaning for this operation — "reveal in the
    /// file manager" on a phone, for instance. A typed refusal rather than a
    /// silent no-op, so the UI can hide the affordance instead of offering
    /// something that does nothing.
    #[error("{operation} is not available on this platform")]
    Unsupported { operation: &'static str },

    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, PlatformError>;

impl PlatformError {
    /// Classify a `std::io::Error` into the richer variants where possible so
    /// callers do not have to inspect `ErrorKind` themselves.
    pub fn from_io(
        operation: &'static str,
        path: impl AsRef<Path>,
        source: std::io::Error,
    ) -> Self {
        let path = path.as_ref().to_path_buf();
        match source.kind() {
            std::io::ErrorKind::NotFound => PlatformError::NotFound { path },
            std::io::ErrorKind::PermissionDenied => PlatformError::PermissionDenied { path },
            std::io::ErrorKind::AlreadyExists => PlatformError::AlreadyExists { path },
            _ => PlatformError::Io {
                operation,
                path,
                source,
            },
        }
    }

    /// A stable machine-readable code, used by the IPC layer so the UI can
    /// branch without parsing prose.
    pub fn code(&self) -> &'static str {
        match self {
            PlatformError::NotFound { .. } => "not_found",
            PlatformError::PermissionDenied { .. } => "permission_denied",
            PlatformError::AlreadyExists { .. } => "already_exists",
            PlatformError::IsADirectory { .. } => "is_a_directory",
            PlatformError::NotADirectory { .. } => "not_a_directory",
            PlatformError::InvalidUtf8 { .. } => "invalid_utf8",
            PlatformError::NoStandardDirectory { .. } => "no_standard_directory",
            PlatformError::Watcher { .. } => "watcher_failed",
            PlatformError::Unsupported { .. } => "unsupported",
            PlatformError::Io { .. } => "io_error",
        }
    }
}

/// Which OS family the running adapter targets. Used for diagnostics and for
/// the UI's hotkey labels; never for branching inside `ie-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformKind {
    Linux,
    Windows,
    MacOs,
    Ios,
}

impl fmt::Display for PlatformKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            PlatformKind::Linux => "linux",
            PlatformKind::Windows => "windows",
            PlatformKind::MacOs => "macos",
            PlatformKind::Ios => "ios",
        };
        f.write_str(name)
    }
}

//! Turning core errors into something the UI can both show and branch on.
//!
//! A command that fails must tell the frontend three things: a stable code to
//! switch on, prose to show the user, and whether there is a recovery the app
//! can offer. A bare string would give it only the second.

use ie_core::CoreError;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// Stable identifier, e.g. `case_collision`. Never localised.
    pub code: String,
    /// One sentence, for the user.
    pub message: String,
    /// True when discarding and rebuilding the index would fix this.
    pub recoverable_by_reindex: bool,
}

impl From<CoreError> for CommandError {
    fn from(error: CoreError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
            recoverable_by_reindex: error.warrants_index_rebuild(),
        }
    }
}

impl From<ie_platform::PlatformError> for CommandError {
    fn from(error: ie_platform::PlatformError) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
            recoverable_by_reindex: false,
        }
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            code: "json_error".into(),
            message: error.to_string(),
            recoverable_by_reindex: false,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl CommandError {
    pub fn refused(reason: impl Into<String>) -> Self {
        Self {
            code: "refused".into(),
            message: reason.into(),
            recoverable_by_reindex: false,
        }
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

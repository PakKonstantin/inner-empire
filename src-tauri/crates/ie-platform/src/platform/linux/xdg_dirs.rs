//! XDG Base Directory Specification locations.
//!
//! `~/.config/inner-empire`, `~/.local/share/inner-empire`,
//! `~/.local/share/inner-empire/logs`, `~/.cache/inner-empire`, with the
//! `XDG_*_HOME` environment variables honoured by the `directories` crate.

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::dirs::AppDirs;
use crate::error::{PlatformError, Result};

pub const QUALIFIER: &str = "";
pub const ORGANIZATION: &str = "";
pub const APPLICATION: &str = "inner-empire";

#[derive(Debug, Clone)]
pub struct XdgDirs {
    project: ProjectDirs,
}

impl XdgDirs {
    pub fn new() -> Result<Self> {
        let project = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).ok_or(
            PlatformError::NoStandardDirectory {
                purpose: "application",
            },
        )?;
        Ok(Self { project })
    }
}

impl AppDirs for XdgDirs {
    fn config_dir(&self) -> Result<PathBuf> {
        Ok(self.project.config_dir().to_path_buf())
    }

    fn data_dir(&self) -> Result<PathBuf> {
        Ok(self.project.data_dir().to_path_buf())
    }

    fn log_dir(&self) -> Result<PathBuf> {
        // XDG has no log directory; the convention is a subdirectory of the
        // data directory rather than the state directory, which is for things
        // the user would want restored.
        Ok(self.project.data_dir().join("logs"))
    }

    fn cache_dir(&self) -> Result<PathBuf> {
        Ok(self.project.cache_dir().to_path_buf())
    }
}

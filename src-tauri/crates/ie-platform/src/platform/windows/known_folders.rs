//! Windows standard directories.
//!
//! Resolved through the `directories` crate, which calls `SHGetKnownFolderPath`
//! for `FOLDERID_RoamingAppData` and `FOLDERID_LocalAppData`. Settings go to
//! the roaming profile so they follow a domain user between machines; logs and
//! caches stay local because they are machine-specific and can be large.

use std::path::PathBuf;

use directories::ProjectDirs;

use crate::dirs::AppDirs;
use crate::error::{PlatformError, Result};

pub const QUALIFIER: &str = "";
pub const ORGANIZATION: &str = "InnerEmpire";
pub const APPLICATION: &str = "InnerEmpire";

#[derive(Debug, Clone)]
pub struct WindowsDirs {
    project: ProjectDirs,
}

impl WindowsDirs {
    pub fn new() -> Result<Self> {
        let project = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION).ok_or(
            PlatformError::NoStandardDirectory {
                purpose: "application",
            },
        )?;
        Ok(Self { project })
    }
}

impl AppDirs for WindowsDirs {
    fn config_dir(&self) -> Result<PathBuf> {
        Ok(self.project.config_dir().to_path_buf())
    }

    fn data_dir(&self) -> Result<PathBuf> {
        Ok(self.project.data_dir().to_path_buf())
    }

    fn log_dir(&self) -> Result<PathBuf> {
        Ok(self.project.cache_dir().join("logs"))
    }

    fn cache_dir(&self) -> Result<PathBuf> {
        Ok(self.project.cache_dir().to_path_buf())
    }
}

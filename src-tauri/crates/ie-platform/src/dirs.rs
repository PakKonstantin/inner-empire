use std::path::{Path, PathBuf};

use crate::error::{PlatformError, Result};

/// Where the application may keep state that is *not* part of a vault.
///
/// Vault-scoped state (workspace layout, index, canvas files) deliberately
/// does not come from here: it lives inside the vault so the vault stays
/// self-contained and portable between machines.
pub trait AppDirs: Send + Sync {
    /// User settings, recent-vault list, hotkey overrides.
    fn config_dir(&self) -> Result<PathBuf>;
    /// Recovery journals, installed plugin registry.
    fn data_dir(&self) -> Result<PathBuf>;
    /// Rotating log files.
    fn log_dir(&self) -> Result<PathBuf>;
    /// Regenerable scratch space; safe to delete at any time.
    fn cache_dir(&self) -> Result<PathBuf>;

    /// Create all four so later writes cannot fail on a missing parent.
    fn ensure_all(&self) -> Result<()> {
        for dir in [
            self.config_dir()?,
            self.data_dir()?,
            self.log_dir()?,
            self.cache_dir()?,
        ] {
            std::fs::create_dir_all(&dir)
                .map_err(|e| PlatformError::from_io("create_dir_all", &dir, e))?;
        }
        Ok(())
    }
}

pub type SharedAppDirs = std::sync::Arc<dyn AppDirs>;

/// Portable mode: every directory lives under a single root next to the
/// executable, so the app can run from a USB stick and leave nothing behind.
///
/// Selected when a file named `portable.txt` sits beside the binary. It is
/// platform-neutral; the brief asks for it on Windows, but nothing here is
/// Windows-specific.
#[derive(Debug, Clone)]
pub struct PortableDirs {
    root: PathBuf,
}

impl PortableDirs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Detect portable mode from the running executable's location.
    pub fn detect() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        if dir.join("portable.txt").exists() {
            Some(Self::new(dir.join("data")))
        } else {
            None
        }
    }
}

impl AppDirs for PortableDirs {
    fn config_dir(&self) -> Result<PathBuf> {
        Ok(self.root.join("config"))
    }
    fn data_dir(&self) -> Result<PathBuf> {
        Ok(self.root.join("data"))
    }
    fn log_dir(&self) -> Result<PathBuf> {
        Ok(self.root.join("logs"))
    }
    fn cache_dir(&self) -> Result<PathBuf> {
        Ok(self.root.join("cache"))
    }
}

/// Directories rooted at an arbitrary base, for tests.
#[derive(Debug, Clone)]
pub struct TestDirs(pub PathBuf);

impl TestDirs {
    pub fn new(base: impl AsRef<Path>) -> Self {
        Self(base.as_ref().to_path_buf())
    }
}

impl AppDirs for TestDirs {
    fn config_dir(&self) -> Result<PathBuf> {
        Ok(self.0.join("config"))
    }
    fn data_dir(&self) -> Result<PathBuf> {
        Ok(self.0.join("data"))
    }
    fn log_dir(&self) -> Result<PathBuf> {
        Ok(self.0.join("logs"))
    }
    fn cache_dir(&self) -> Result<PathBuf> {
        Ok(self.0.join("cache"))
    }
}

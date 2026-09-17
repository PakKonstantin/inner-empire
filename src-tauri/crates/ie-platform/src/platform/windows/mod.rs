//! Windows adapter.
//!
//! Compiled only on Windows targets. It uses no Win32 bindings and touches no
//! registry key: everything it needs is either in `std` or reachable through
//! the shell verbs Windows already exposes, which keeps the adapter small and
//! its behaviour easy to reason about.

use std::path::Path;
use std::sync::Arc;

use crate::dirs::AppDirs;
use crate::error::{PlatformKind, Result};
use crate::platform::PlatformOps;

pub mod known_folders;

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsPlatform;

impl WindowsPlatform {
    pub fn new() -> Self {
        Self
    }
}

impl PlatformOps for WindowsPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn sync_dir(&self, _dir: &Path) -> Result<()> {
        // NTFS journals the directory entry as part of the rename transaction,
        // and Windows offers no handle to a directory that `FlushFileBuffers`
        // accepts. The preceding `sync_all` on the temporary file plus the
        // atomic `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` that `fs::rename`
        // performs already give the guarantee the caller asked for.
        Ok(())
    }

    fn assumed_case_sensitive(&self) -> bool {
        // NTFS is case-preserving but case-insensitive by default. Per-directory
        // case sensitivity can be enabled, which is exactly why the filesystem
        // layer probes instead of trusting this value.
        false
    }

    fn app_dirs(&self) -> Result<Arc<dyn AppDirs>> {
        Ok(Arc::new(known_folders::WindowsDirs::new()?))
    }

    #[cfg(windows)]
    fn open_path(&self, path: &Path) -> Result<()> {
        use crate::error::PlatformError;
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // `cmd /c start` resolves the file association without needing
        // ShellExecute bindings. The empty title argument is required because
        // `start` treats a first quoted argument as the window title.
        Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| PlatformError::from_io("spawn", path, e))
    }

    #[cfg(not(windows))]
    fn open_path(&self, path: &Path) -> Result<()> {
        // Cross-compilation and unit tests reach this arm; there is nothing to
        // open on a non-Windows host.
        let _ = path;
        Ok(())
    }

    #[cfg(windows)]
    fn reveal_in_file_manager(&self, path: &Path) -> Result<()> {
        use crate::error::PlatformError;
        use std::process::Command;
        Command::new("explorer")
            .arg(format!("/select,{}", path.to_string_lossy()))
            .spawn()
            .map(|_| ())
            .map_err(|e| PlatformError::from_io("spawn", path, e))
    }

    #[cfg(not(windows))]
    fn reveal_in_file_manager(&self, path: &Path) -> Result<()> {
        let _ = path;
        Ok(())
    }

    fn open_external_url(&self, url: &str) -> Result<()> {
        if !crate::shell::is_safe_external_url(url) {
            return Err(crate::error::PlatformError::PermissionDenied {
                path: std::path::PathBuf::from(url),
            });
        }
        self.open_path(Path::new(url))
    }
}

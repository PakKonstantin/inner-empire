//! iOS / iPadOS adapter.
//!
//! The third host for the same core. What makes that possible is that `ie-core`
//! asks the host for a `FileSystem`, a `FileWatcher`, a `Clock` and an
//! `AppDirs` and for nothing else; this module supplies all four and links
//! against no Apple framework, because the two operations that genuinely need
//! Objective-C are declared as a trait the host implements.
//!
//! The differences from the desktop adapters, all of them:
//!
//! - **Directories are given, not derived.** iOS reassigns the container path
//!   between launches, so nothing may be computed or remembered.
//! - **The shell is unreachable.** There is no file manager to reveal a path
//!   in, so those methods refuse with [`PlatformError::Unsupported`] rather
//!   than pretending to succeed.
//! - **Case sensitivity defaults to insensitive.** APFS ships case-insensitive
//!   on iOS, the same assumption the Windows adapter makes, and for the same
//!   reason: a vault that works here must work there.

use std::path::Path;
use std::sync::Arc;

use crate::dirs::AppDirs;
use crate::error::{PlatformError, PlatformKind, Result};
use crate::platform::PlatformOps;

pub mod container_dirs;
pub mod coordinated_fs;
pub mod file_provider;
pub mod presenter_watcher;

pub use container_dirs::ContainerDirs;
pub use coordinated_fs::CoordinatedFileSystem;
pub use file_provider::{icloud, FileProvider, LocalFolder};
pub use presenter_watcher::{PresenterWatch, PresenterWatcher};

/// The per-OS behaviours, for iOS.
///
/// Carries the host-supplied directories when it has them. `app_dirs()` is only
/// reached through `HostServices::for_current_platform`, which iOS does not
/// use — it builds `HostServices` explicitly, because only the host knows where
/// its container is — so the directory-less constructor is the common one.
#[derive(Debug, Default, Clone)]
pub struct IosPlatform {
    dirs: Option<ContainerDirs>,
}

impl IosPlatform {
    pub fn new() -> Self {
        Self { dirs: None }
    }

    pub fn with_dirs(dirs: ContainerDirs) -> Self {
        Self { dirs: Some(dirs) }
    }
}

impl PlatformOps for IosPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Ios
    }

    fn sync_dir(&self, dir: &Path) -> Result<()> {
        // APFS, like ext4, needs the directory fsynced for a rename to be
        // durable across a power loss. Identical to the Linux adapter.
        let handle =
            std::fs::File::open(dir).map_err(|e| PlatformError::from_io("open_dir", dir, e))?;
        handle
            .sync_all()
            .map_err(|e| PlatformError::from_io("fsync_dir", dir, e))
    }

    fn assumed_case_sensitive(&self) -> bool {
        // The probe in `StdFileSystem` is always preferred and usually works.
        // This is the answer when it cannot run — a read-only directory, say —
        // and on iOS the safe answer is `false`: treating a case-insensitive
        // volume as sensitive is what lets `Note.md` silently overwrite
        // `note.md`.
        false
    }

    fn app_dirs(&self) -> Result<Arc<dyn AppDirs>> {
        match &self.dirs {
            Some(dirs) => Ok(Arc::new(dirs.clone())),
            // Not a failure mode a user can hit: the host always passes its
            // container in. Worth a typed error rather than a guess, because a
            // guessed container path on iOS is a path that does not exist.
            None => Err(PlatformError::NoStandardDirectory {
                purpose: "application container (the host must supply it)",
            }),
        }
    }

    fn open_path(&self, _path: &Path) -> Result<()> {
        Err(PlatformError::Unsupported {
            operation: "open a path with the system handler",
        })
    }

    fn reveal_in_file_manager(&self, _path: &Path) -> Result<()> {
        Err(PlatformError::Unsupported {
            operation: "reveal a path in the file manager",
        })
    }

    fn open_external_url(&self, _url: &str) -> Result<()> {
        // `UIApplication.open` is a main-thread UIKit call and belongs on the
        // host's side of the bridge, where it can also be gated on a user tap.
        Err(PlatformError::Unsupported {
            operation: "open an external URL",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_operations_refuse_rather_than_pretending() {
        let platform = IosPlatform::new();

        for result in [
            platform.open_path(Path::new("/vault/Note.md")),
            platform.reveal_in_file_manager(Path::new("/vault/Note.md")),
            platform.open_external_url("https://example.com"),
        ] {
            let err = result.unwrap_err();
            assert!(matches!(err, PlatformError::Unsupported { .. }));
            assert_eq!(err.code(), "unsupported");
        }
    }

    #[test]
    fn case_sensitivity_falls_back_the_cautious_way() {
        // Assuming sensitivity on an insensitive volume is how `Note.md`
        // quietly replaces `note.md`.
        assert!(!IosPlatform::new().assumed_case_sensitive());
    }

    #[test]
    fn directories_come_from_the_host_or_not_at_all() {
        assert!(IosPlatform::new().app_dirs().is_err());

        let platform = IosPlatform::with_dirs(ContainerDirs::under_library(Path::new(
            "/container/Library",
        )));
        let dirs = platform.app_dirs().unwrap();
        assert_eq!(
            dirs.cache_dir().unwrap(),
            std::path::PathBuf::from("/container/Library/Caches")
        );
    }

    #[test]
    fn it_reports_itself_as_ios() {
        assert_eq!(IosPlatform::new().kind(), PlatformKind::Ios);
        assert_eq!(PlatformKind::Ios.to_string(), "ios");
    }
}

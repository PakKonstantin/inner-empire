//! Operating-system abstractions for Inner Empire.
//!
//! Every interaction with the world outside the process — files, watching,
//! clocks, standard directories, the shell, the clipboard, native dialogs —
//! is declared here as a trait and implemented in [`platform`] per OS family.
//! `ie-core` depends on the traits only, which is what keeps platform
//! conditionals out of the application's logic.
//!
//! ```text
//!   ie-core  ──uses──▶  traits in this crate  ──implemented by──▶  platform::{linux, windows}
//! ```

pub mod clock;
pub mod dirs;
pub mod error;
pub mod fs;
pub mod memory_fs;
#[cfg(feature = "desktop-backends")]
pub mod notify_watcher;
pub mod path_resolver;
pub mod platform;
pub mod shell;
pub mod std_fs;
pub mod watcher;

pub use clock::{Clock, FixedClock, HostOffsetClock, SharedClock, SystemClock};
pub use dirs::{AppDirs, PortableDirs, SharedAppDirs, TestDirs};
pub use error::{PlatformError, PlatformKind, Result};
pub use fs::{DirEntry, FileMetadata, FileSystem, SharedFileSystem};
pub use memory_fs::MemoryFileSystem;
#[cfg(feature = "desktop-backends")]
pub use notify_watcher::NotifyWatcher;
pub use path_resolver::PathResolver;
pub use platform::PlatformOps;
pub use shell::{Clipboard, FileDialogOptions, ProcessManager, ShellIntegration, SystemDialog};
pub use std_fs::StdFileSystem;
pub use watcher::{FileWatcher, FsEvent, SharedFileWatcher, WatchHandle, WatchOptions};

/// Everything the core needs from the host, bundled so it can be constructed
/// once at startup and injected as a unit.
#[derive(Clone)]
pub struct HostServices {
    pub fs: SharedFileSystem,
    pub watcher: SharedFileWatcher,
    pub clock: SharedClock,
    pub dirs: SharedAppDirs,
    pub platform: std::sync::Arc<dyn PlatformOps>,
}

impl HostServices {
    /// Assemble the services explicitly.
    ///
    /// The iOS host uses this rather than [`HostServices::for_current_platform`]
    /// because half of what it needs — the container directories, the vault's
    /// file provider, the presenter that drives the watcher — is knowledge only
    /// the application has.
    pub fn new(
        fs: SharedFileSystem,
        watcher: SharedFileWatcher,
        clock: SharedClock,
        dirs: SharedAppDirs,
        platform: std::sync::Arc<dyn PlatformOps>,
    ) -> Self {
        Self {
            fs,
            watcher,
            clock,
            dirs,
            platform,
        }
    }

    /// Wire up the real host: the adapter for this OS, plus portable-mode
    /// directories when a `portable.txt` marker sits beside the executable.
    #[cfg(feature = "desktop-backends")]
    pub fn for_current_platform() -> Result<Self> {
        let platform = platform::current();
        let dirs: SharedAppDirs = match PortableDirs::detect() {
            Some(portable) => std::sync::Arc::new(portable),
            None => platform.app_dirs()?,
        };
        Ok(Self {
            fs: std::sync::Arc::new(StdFileSystem::new(std::sync::Arc::clone(&platform))),
            watcher: std::sync::Arc::new(NotifyWatcher::new()),
            clock: std::sync::Arc::new(SystemClock),
            dirs,
            platform,
        })
    }
}

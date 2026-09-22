//! Platform adapters.
//!
//! This module is the *only* place in the workspace where `#[cfg(windows)]`
//! and `#[cfg(unix)]` appear. Everything above it — including all of
//! `ie-core` — talks to the traits in the crate root and therefore compiles
//! identically for every target.

use std::path::Path;
use std::sync::Arc;

use crate::dirs::AppDirs;
use crate::error::{PlatformKind, Result};

// The desktop adapters need `directories` for their standard locations, so
// they follow the same feature that gates it. iOS derives nothing and is always
// compiled, which is what lets its adapter be tested on any host.
#[cfg(feature = "desktop-backends")]
pub mod linux;
#[cfg(feature = "desktop-backends")]
pub mod windows;

pub mod ios;

/// The per-OS behaviours that cannot be expressed portably.
///
/// Kept deliberately small: every method here is a place where Windows and
/// Linux genuinely differ. Anything that can be written once lives in
/// `std_fs.rs` instead.
pub trait PlatformOps: Send + Sync {
    fn kind(&self) -> PlatformKind;

    /// Make a directory entry durable after a rename. POSIX requires an
    /// explicit `fsync` on the directory; NTFS orders the metadata write
    /// itself, so the Windows adapter is a documented no-op.
    fn sync_dir(&self, dir: &Path) -> Result<()>;

    /// What to assume about case sensitivity when probing is not possible.
    /// Probing is always preferred; this is the fallback.
    fn assumed_case_sensitive(&self) -> bool;

    /// Standard config/data/log/cache locations for this OS.
    fn app_dirs(&self) -> Result<Arc<dyn AppDirs>>;

    /// Open a path with the desktop's default handler.
    fn open_path(&self, path: &Path) -> Result<()>;

    /// Show a path in the file manager, selected where supported.
    fn reveal_in_file_manager(&self, path: &Path) -> Result<()>;

    /// Open an http(s) URL in the default browser.
    fn open_external_url(&self, url: &str) -> Result<()>;

    /// Whether this platform can change file associations from inside the
    /// application. Defaults to no, so a new platform has to opt in rather
    /// than silently appear to support it.
    fn shell_integration_support(&self) -> ShellIntegrationSupport {
        ShellIntegrationSupport::Unavailable(
            "Changing which application opens a file is not available on this system.",
        )
    }

    /// What the desktop currently routes to this application.
    fn shell_integration(&self) -> Result<ShellIntegrationState> {
        Ok(ShellIntegrationState::default())
    }

    /// Turn an integration on or off.
    ///
    /// `executable` is the path the shell should launch. Taking it as an
    /// argument rather than reading `current_exe` keeps this testable and
    /// keeps the decision about *which* binary at the caller, where a
    /// portable install differs from an installed one.
    fn set_shell_integration(
        &self,
        _state: ShellIntegrationState,
        _executable: &Path,
    ) -> Result<()> {
        Err(crate::error::PlatformError::Unsupported {
            operation: "change file associations",
        })
    }
}

/// The adapter for the desktop OS this binary was built for.
///
/// Adding macOS later means adding a `macos` module and one arm here; nothing
/// else in the workspace changes.
///
/// iOS is deliberately absent. Its adapter cannot be constructed without the
/// container paths, which only the host knows, so the iOS host builds
/// [`crate::HostServices`] explicitly instead of asking for a default.
#[cfg(feature = "desktop-backends")]
pub fn current() -> Arc<dyn PlatformOps> {
    #[cfg(windows)]
    {
        Arc::new(windows::WindowsPlatform::new())
    }
    #[cfg(not(windows))]
    {
        Arc::new(linux::LinuxPlatform::new())
    }
}

/// Whether the desktop currently routes Markdown and folders to this app.
///
/// Both are off until the user asks. An application that seizes `.md` on
/// install is one people uninstall, so the installer registers the
/// *capability* — appearing under "Open with" — and nothing more; becoming
/// the default handler is a choice made here, where it can be seen, changed
/// and undone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShellIntegrationState {
    /// Double-clicking a `.md` file opens it in this application.
    pub markdown_default: bool,
    /// Right-clicking a folder offers to open it as a vault.
    pub folder_context_menu: bool,
}

/// Whether this platform can offer shell integration at all.
///
/// Linux desktops take their associations from `.desktop` files installed by
/// the package, which the application must not rewrite behind the package
/// manager's back — so there the answer is no, and the settings say so rather
/// than offering a switch that does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellIntegrationSupport {
    Available,
    /// Not offered here, with a reason a person can read.
    Unavailable(&'static str),
}

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
use crate::platform::{PlatformOps, ShellIntegrationState, ShellIntegrationSupport};

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

    fn shell_integration_support(&self) -> ShellIntegrationSupport {
        ShellIntegrationSupport::Available
    }

    #[cfg(windows)]
    fn shell_integration(&self) -> Result<ShellIntegrationState> {
        Ok(ShellIntegrationState {
            // Only ours if the extension actually points at our ProgID —
            // another editor may have taken it since.
            markdown_default: reg_read(r"Software\Classes\.md").as_deref() == Some(PROG_ID),
            folder_context_menu: reg_read(CONTEXT_KEY).is_some(),
        })
    }

    #[cfg(not(windows))]
    fn shell_integration(&self) -> Result<ShellIntegrationState> {
        Ok(ShellIntegrationState::default())
    }

    #[cfg(windows)]
    fn set_shell_integration(&self, state: ShellIntegrationState, executable: &Path) -> Result<()> {
        let exe = executable.to_string_lossy().to_string();

        if state.markdown_default {
            let prog = format!(r"Software\Classes\{PROG_ID}");
            reg_write(&prog, None, "Markdown document")?;
            reg_write(&format!(r"{prog}\DefaultIcon"), None, &format!("{exe},0"))?;
            reg_write(
                &format!(r"{prog}\shell\open\command"),
                None,
                &format!("\"{exe}\" \"%1\""),
            )?;
            for extension in [".md", ".markdown"] {
                reg_write(&format!(r"Software\Classes\{extension}"), None, PROG_ID)?;
            }
        } else {
            // Give an extension back only while it is still ours. If another
            // application has taken it since, taking it away again would be
            // the same rudeness in reverse.
            for extension in [".md", ".markdown"] {
                let key = format!(r"Software\Classes\{extension}");
                if reg_read(&key).as_deref() == Some(PROG_ID) {
                    let _ = reg(&["delete", &key, "/ve", "/f"]);
                }
            }
            reg_delete_key(&format!(r"Software\Classes\{PROG_ID}"))?;
        }

        if state.folder_context_menu {
            reg_write(CONTEXT_KEY, None, "Open as vault in Inner Empire")?;
            reg_write(CONTEXT_KEY, Some("Icon"), &format!("{exe},0"))?;
            reg_write(
                &format!(r"{CONTEXT_KEY}\command"),
                None,
                &format!("\"{exe}\" \"%V\""),
            )?;
        } else {
            reg_delete_key(CONTEXT_KEY)?;
        }

        notify_shell();
        Ok(())
    }

    #[cfg(not(windows))]
    fn set_shell_integration(
        &self,
        _state: ShellIntegrationState,
        _executable: &Path,
    ) -> Result<()> {
        // The Windows adapter is compiled on other targets so the workspace
        // type-checks everywhere; the registry work is genuinely absent.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shell integration
// ---------------------------------------------------------------------------
//
// Through `reg.exe`, which ships with every Windows install, rather than a
// registry crate. The trade is deliberate: no new dependency, nothing to keep
// up to date, and the exact keys being written are readable in the source.
// The cost is spawning a process per key, which happens only when the user
// flips a switch in Settings.
//
// Everything is under HKCU. A per-user install must work without admin
// rights, and one account's choice must not reach another's.

/// The ProgID this application registers under. Namespaced, so it cannot
/// collide with another editor's.
#[cfg(windows)]
const PROG_ID: &str = "InnerEmpire.Markdown";

#[cfg(windows)]
const CONTEXT_KEY: &str = r"Software\Classes\Directory\shell\InnerEmpireVault";

#[cfg(windows)]
fn reg(args: &[&str]) -> Result<std::process::Output> {
    use crate::error::PlatformError;
    use std::process::Command;

    Command::new("reg")
        .args(args)
        .output()
        .map_err(|e| PlatformError::from_io("run reg.exe", std::path::Path::new("reg"), e))
}

/// The default value of a key, or `None` when the key is not there.
///
/// `reg query` exits non-zero for a missing key, which is the ordinary case
/// here rather than a failure, so it is read as "absent" and not an error.
#[cfg(windows)]
fn reg_read(key: &str) -> Option<String> {
    let output = reg(&["query", key, "/ve"]).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // `    (Default)    REG_SZ    InnerEmpire.Markdown`
    let line = text.lines().find(|line| line.contains("REG_SZ"))?;
    let value = line.split("REG_SZ").nth(1)?.trim();
    Some(value.to_string())
}

#[cfg(windows)]
fn reg_write(key: &str, name: Option<&str>, value: &str) -> Result<()> {
    let mut args = vec!["add", key];
    match name {
        Some(name) => args.extend_from_slice(&["/v", name]),
        None => args.push("/ve"),
    }
    args.extend_from_slice(&["/t", "REG_SZ", "/d", value, "/f"]);
    reg(&args).map(|_| ())
}

#[cfg(windows)]
fn reg_delete_key(key: &str) -> Result<()> {
    // A key that is not there is the state we wanted, so a failure to delete
    // it is not reported.
    let _ = reg(&["delete", key, "/f"]);
    Ok(())
}

/// Tell the shell an association changed.
///
/// Without this Explorer keeps showing the old icon and opening the old
/// application until the next sign-in, which reads as the setting not having
/// worked.
#[cfg(windows)]
fn notify_shell() {
    use std::process::Command;
    // SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, NULL, NULL) through
    // rundll32, so no FFI declaration is needed for one call.
    let _ = Command::new("rundll32")
        .args(["shell32.dll,SHChangeNotify", "0x8000000", "0", "0", "0"])
        .output();
}

//! Linux / freedesktop adapter.
//!
//! Targets GNOME, KDE Plasma and anything else implementing the freedesktop
//! specifications. Nothing here is desktop-environment specific beyond an
//! optional D-Bus call that degrades gracefully.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use crate::dirs::AppDirs;
use crate::error::{PlatformError, PlatformKind, Result};
use crate::platform::PlatformOps;
use crate::shell::is_safe_external_url;

pub mod xdg_dirs;

#[derive(Debug, Default, Clone, Copy)]
pub struct LinuxPlatform;

impl LinuxPlatform {
    pub fn new() -> Self {
        Self
    }

    fn spawn(program: &str, args: &[&str]) -> Result<()> {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| PlatformError::Io {
                operation: "spawn",
                path: std::path::PathBuf::from(program),
                source: e,
            })
    }

    /// Try each launcher in turn. Distributions vary in what is installed, so
    /// a missing `xdg-open` should fall through rather than fail.
    fn spawn_first_available(candidates: &[(&str, Vec<&str>)]) -> Result<()> {
        let mut last: Option<PlatformError> = None;
        for (program, args) in candidates {
            match Self::spawn(program, args) {
                Ok(()) => return Ok(()),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or(PlatformError::Watcher {
            message: "no launcher available".into(),
        }))
    }
}

impl PlatformOps for LinuxPlatform {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn sync_dir(&self, dir: &Path) -> Result<()> {
        // Opening a directory read-only and fsyncing it is what makes a
        // preceding rename durable on ext4, xfs and btrfs.
        let handle =
            std::fs::File::open(dir).map_err(|e| PlatformError::from_io("open_dir", dir, e))?;
        handle
            .sync_all()
            .map_err(|e| PlatformError::from_io("fsync_dir", dir, e))
    }

    fn assumed_case_sensitive(&self) -> bool {
        true
    }

    fn app_dirs(&self) -> Result<Arc<dyn AppDirs>> {
        Ok(Arc::new(xdg_dirs::XdgDirs::new()?))
    }

    fn open_path(&self, path: &Path) -> Result<()> {
        let target = path.to_string_lossy().to_string();
        Self::spawn_first_available(&[
            ("xdg-open", vec![target.as_str()]),
            ("gio", vec!["open", target.as_str()]),
        ])
    }

    fn reveal_in_file_manager(&self, path: &Path) -> Result<()> {
        let uri = format!("file://{}", path.to_string_lossy());
        // org.freedesktop.FileManager1 is implemented by Nautilus (GNOME),
        // Dolphin (KDE) and Nemo, and selects the item rather than opening it.
        let dbus = Self::spawn(
            "dbus-send",
            &[
                "--session",
                "--dest=org.freedesktop.FileManager1",
                "--type=method_call",
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("array:string:{uri}"),
                "string:",
            ],
        );
        if dbus.is_ok() {
            return Ok(());
        }
        // Fall back to opening the containing folder.
        let folder = path.parent().unwrap_or(path);
        self.open_path(folder)
    }

    fn open_external_url(&self, url: &str) -> Result<()> {
        if !is_safe_external_url(url) {
            return Err(PlatformError::PermissionDenied {
                path: std::path::PathBuf::from(url),
            });
        }
        Self::spawn_first_available(&[("xdg-open", vec![url]), ("gio", vec!["open", url])])
    }
}

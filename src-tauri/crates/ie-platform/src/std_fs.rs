//! The real filesystem, implemented once for every platform.
//!
//! Only two operations genuinely differ between Windows and Linux — making a
//! directory entry durable, and deciding case sensitivity — and both are
//! delegated to `PlatformOps`. Everything else is `std::fs`, which already
//! handles separator and encoding differences.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{PlatformError, Result};
use crate::fs::{metadata_from_std, DirEntry, FileMetadata, FileSystem};
use crate::platform::PlatformOps;

/// Prefix for the temporary sibling used by `write_atomic`. It is recognisable
/// so a crash leaves evidence the vault can report rather than mystery files.
pub const TEMP_PREFIX: &str = ".ie-tmp-";

pub struct StdFileSystem {
    ops: Arc<dyn PlatformOps>,
    counter: AtomicU64,
    /// Case sensitivity is a property of the mount, so it is probed once per
    /// directory and cached. Probing writes one short-lived hidden file.
    case_cache: Mutex<HashMap<PathBuf, bool>>,
}

impl StdFileSystem {
    pub fn new(ops: Arc<dyn PlatformOps>) -> Self {
        Self {
            ops,
            counter: AtomicU64::new(0),
            case_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Convenience constructor using the adapter for the current OS.
    pub fn for_current_platform() -> Self {
        Self::new(crate::platform::current())
    }

    fn temp_sibling(&self, target: &Path) -> Result<PathBuf> {
        let dir = target
            .parent()
            .ok_or_else(|| PlatformError::NotADirectory {
                path: target.to_path_buf(),
            })?;
        let stem = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(dir.join(format!("{TEMP_PREFIX}{}-{n}-{}", std::process::id(), stem)))
    }

    /// Is `name` a leftover from an interrupted `write_atomic`?
    pub fn is_temp_artifact(name: &str) -> bool {
        name.starts_with(TEMP_PREFIX)
    }
}

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|e| PlatformError::from_io("read", path, e))
    }

    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()> {
        let temp = self.temp_sibling(path)?;

        // Scope the handle so it is closed before the rename; Windows refuses
        // to rename a file that still has an open handle.
        {
            let mut file = std::fs::File::create(&temp)
                .map_err(|e| PlatformError::from_io("create_temp", &temp, e))?;
            file.write_all(contents)
                .map_err(|e| PlatformError::from_io("write_temp", &temp, e))?;
            file.flush()
                .map_err(|e| PlatformError::from_io("flush_temp", &temp, e))?;
            // Durability of the bytes themselves, before anything points at them.
            file.sync_all()
                .map_err(|e| PlatformError::from_io("fsync_temp", &temp, e))?;
        }

        // `fs::rename` replaces an existing destination atomically on both
        // POSIX (`rename(2)`) and Windows (`MoveFileEx` with
        // `MOVEFILE_REPLACE_EXISTING`). A reader therefore sees either the old
        // file or the new one, never a half-written one.
        if let Err(e) = std::fs::rename(&temp, path) {
            let _ = std::fs::remove_file(&temp);
            return Err(PlatformError::from_io("rename_temp", path, e));
        }

        if let Some(dir) = path.parent() {
            // A failure here means the rename may not survive a power cut, but
            // the file on disk is already correct — worth a warning, not an error.
            if let Err(e) = self.ops.sync_dir(dir) {
                tracing::warn!(directory = %dir.display(), error = %e, "could not fsync directory after atomic write");
            }
        }
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|e| PlatformError::from_io("create_dir_all", path, e))
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path).map_err(|e| PlatformError::from_io("remove_file", path, e))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path).map_err(|e| PlatformError::from_io("remove_dir_all", path, e))
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        std::fs::rename(from, to).map_err(|e| PlatformError::from_io("rename", from, e))?;
        if let Some(dir) = to.parent() {
            let _ = self.ops.sync_dir(dir);
        }
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<u64> {
        std::fs::copy(from, to).map_err(|e| PlatformError::from_io("copy", from, e))
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        let symlink_meta = std::fs::symlink_metadata(path)
            .map_err(|e| PlatformError::from_io("symlink_metadata", path, e))?;
        let is_symlink = symlink_meta.file_type().is_symlink();
        // Report the target's facts for a symlink, but remember that it is one:
        // the indexer uses this to avoid following links out of the vault.
        let meta = if is_symlink {
            std::fs::metadata(path).unwrap_or(symlink_meta)
        } else {
            symlink_meta
        };
        Ok(metadata_from_std(&meta, is_symlink))
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let iter =
            std::fs::read_dir(path).map_err(|e| PlatformError::from_io("read_dir", path, e))?;
        let mut out = Vec::new();
        for entry in iter {
            let entry = entry.map_err(|e| PlatformError::from_io("read_dir_entry", path, e))?;
            let entry_path = entry.path();
            // A single unreadable entry must not abort the whole listing.
            let metadata = match self.metadata(&entry_path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(path = %entry_path.display(), error = %e, "skipping unreadable entry");
                    continue;
                }
            };
            out.push(DirEntry {
                file_name: entry.file_name().to_string_lossy().to_string(),
                path: entry_path,
                metadata,
            });
        }
        Ok(out)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        let canonical = std::fs::canonicalize(path)
            .map_err(|e| PlatformError::from_io("canonicalize", path, e))?;
        // Windows canonicalization yields a `\\?\` extended-length prefix that
        // is correct but leaks into every message and comparison. Strip it; the
        // remaining path is still absolute and still valid.
        let text = canonical.to_string_lossy();
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(stripped));
        }
        Ok(canonical)
    }

    fn is_case_sensitive(&self, dir: &Path) -> Result<bool> {
        if let Some(cached) = self
            .case_cache
            .lock()
            .ok()
            .and_then(|c| c.get(dir).copied())
        {
            return Ok(cached);
        }

        let probe_name = format!("{TEMP_PREFIX}CaseProbe-{}", std::process::id());
        let upper = dir.join(&probe_name);
        let lower = dir.join(probe_name.to_lowercase());

        let sensitive = match std::fs::File::create(&upper) {
            Ok(_) => {
                // If the lowercase spelling resolves to the file we just made,
                // the mount folded the case.
                let folded = std::fs::metadata(&lower).is_ok();
                let _ = std::fs::remove_file(&upper);
                !folded
            }
            Err(_) => {
                // Read-only or otherwise unprobeable: fall back to the OS default.
                self.ops.assumed_case_sensitive()
            }
        };

        if let Ok(mut cache) = self.case_cache.lock() {
            cache.insert(dir.to_path_buf(), sensitive);
        }
        Ok(sensitive)
    }

    fn sync_dir(&self, dir: &Path) -> Result<()> {
        self.ops.sync_dir(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs_under_test() -> StdFileSystem {
        StdFileSystem::for_current_platform()
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let target = dir.path().join("note.md");

        fs.write_atomic(&target, b"first").unwrap();
        assert_eq!(fs.read_to_string(&target).unwrap(), "first");

        fs.write_atomic(&target, b"second, which is longer")
            .unwrap();
        assert_eq!(
            fs.read_to_string(&target).unwrap(),
            "second, which is longer"
        );

        let leftovers: Vec<_> = fs
            .read_dir(dir.path())
            .unwrap()
            .into_iter()
            .filter(|e| StdFileSystem::is_temp_artifact(&e.file_name))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn atomic_write_creates_parent_relative_temp_in_same_directory() {
        // The temp file must be a sibling: a rename across filesystems is not
        // atomic, so using a global temp directory would break the guarantee.
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let target = dir.path().join("sub").join("note.md");
        fs.create_dir_all(target.parent().unwrap()).unwrap();
        let temp = fs.temp_sibling(&target).unwrap();
        assert_eq!(temp.parent(), target.parent());
    }

    #[test]
    fn metadata_reports_directories_and_sizes() {
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let file = dir.path().join("a.md");
        fs.write_atomic(&file, b"12345").unwrap();

        assert!(fs.metadata(dir.path()).unwrap().is_dir);
        let meta = fs.metadata(&file).unwrap();
        assert!(!meta.is_dir);
        assert_eq!(meta.len, 5);
        assert!(meta.modified_ms.is_some());
    }

    #[test]
    fn case_sensitivity_probe_agrees_with_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let sensitive = fs.is_case_sensitive(dir.path()).unwrap();

        fs.write_atomic(&dir.path().join("Alpha.md"), b"x").unwrap();
        let lower_exists = fs.exists(&dir.path().join("alpha.md"));
        assert_eq!(sensitive, !lower_exists);

        // The second call must come from the cache and agree.
        assert_eq!(fs.is_case_sensitive(dir.path()).unwrap(), sensitive);
    }

    #[test]
    fn read_of_missing_file_is_classified_as_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let err = fs.read(&dir.path().join("nope.md")).unwrap_err();
        assert_eq!(err.code(), "not_found");
    }

    #[test]
    fn invalid_utf8_is_reported_rather_than_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let fs = fs_under_test();
        let file = dir.path().join("binary.md");
        fs.write_atomic(&file, &[0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(fs.read_to_string(&file).unwrap_err().code(), "invalid_utf8");
    }
}

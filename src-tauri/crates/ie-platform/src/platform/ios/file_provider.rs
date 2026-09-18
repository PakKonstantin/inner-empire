//! The two things the host must do that POSIX cannot.
//!
//! A vault on iOS may live behind a File Provider — iCloud Drive, Dropbox,
//! Working Copy. Security-scoped access lets this process make ordinary POSIX
//! calls inside such a directory, and almost everything works. Two things do
//! not, and they are the entire callback surface between the core and Swift:
//!
//! 1. **A file may not be on the device.** iCloud evicts file contents under
//!    storage pressure, leaving a placeholder. Only
//!    `startDownloadingUbiquitousItem` brings it back.
//! 2. **`rename(2)` is the wrong atomic replace.** It works, but it bypasses
//!    the provider's bookkeeping, which is how spurious "conflicted copy"
//!    siblings appear. `FileManager.replaceItemAt` under `NSFileCoordinator`
//!    is the documented replace and the provider understands it.
//!
//! Everything else — reads, directory walks, metadata, the case-sensitivity
//! probe — stays in Rust on the POSIX path, which is what keeps a full scan of
//! a thousand notes at one coordination bracket instead of several thousand.

use std::path::Path;

use crate::error::Result;

/// Implemented by the host for a vault that lives behind a File Provider.
pub trait FileProvider: Send + Sync {
    /// Make sure `path`'s contents are on the device, blocking until they are.
    ///
    /// Called only when a read or a listing has already seen a placeholder, so
    /// on an "On My iPhone" vault it never runs at all.
    fn ensure_materialized(&self, path: &Path) -> Result<()>;

    /// Atomically replace `target` with `source`, which is a sibling temporary
    /// file this crate has already written and fsynced.
    ///
    /// `source` must not survive the call: on success it has become `target`,
    /// and on failure the implementation removes it.
    fn replace_item(&self, target: &Path, source: &Path) -> Result<()>;

    /// The name a not-yet-downloaded `file_name` would appear under, if this
    /// provider uses placeholders.
    ///
    /// iCloud writes `Note.md` as `.Note.md.icloud` while the contents are
    /// evicted, so a directory listing shows the placeholder and *not* the
    /// note. Returning `None` means this provider has no such representation,
    /// which is the right answer for a local folder.
    fn placeholder_name(&self, file_name: &str) -> Option<String> {
        let _ = file_name;
        None
    }

    /// The reverse of [`FileProvider::placeholder_name`]: given a directory
    /// entry, the real file it stands for.
    fn name_behind_placeholder(&self, entry_name: &str) -> Option<String> {
        let _ = entry_name;
        None
    }
}

/// A vault that is just a folder — "On My iPhone", or a local directory.
///
/// Nothing is ever evicted, so `ensure_materialized` has nothing to do, and
/// `rename(2)` is the correct atomic replace because there is no provider whose
/// bookkeeping could be bypassed.
#[derive(Debug, Default, Clone, Copy)]
pub struct LocalFolder;

impl FileProvider for LocalFolder {
    fn ensure_materialized(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    fn replace_item(&self, target: &Path, source: &Path) -> Result<()> {
        if let Err(e) = std::fs::rename(source, target) {
            let _ = std::fs::remove_file(source);
            return Err(crate::error::PlatformError::from_io(
                "replace_item",
                target,
                e,
            ));
        }
        Ok(())
    }
}

/// The `.Name.icloud` convention, shared by every iCloud-backed provider.
///
/// Free functions rather than methods so a host implementing [`FileProvider`]
/// over the real iCloud APIs can reuse the naming without reimplementing it.
pub mod icloud {
    /// `Note.md` → `.Note.md.icloud`
    pub fn placeholder_name(file_name: &str) -> String {
        format!(".{file_name}.icloud")
    }

    /// `.Note.md.icloud` → `Some("Note.md")`, anything else → `None`.
    pub fn name_behind_placeholder(entry_name: &str) -> Option<String> {
        let inner = entry_name.strip_prefix('.')?.strip_suffix(".icloud")?;
        if inner.is_empty() {
            return None;
        }
        Some(inner.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icloud_placeholder_convention_round_trips() {
        assert_eq!(icloud::placeholder_name("Note.md"), ".Note.md.icloud");
        assert_eq!(
            icloud::name_behind_placeholder(".Note.md.icloud").as_deref(),
            Some("Note.md")
        );
    }

    #[test]
    fn ordinary_hidden_files_are_not_mistaken_for_placeholders() {
        // `.inner-empire` and the atomic-write temporaries both start with a
        // dot; neither is a placeholder.
        assert_eq!(icloud::name_behind_placeholder(".inner-empire"), None);
        assert_eq!(icloud::name_behind_placeholder(".ie-tmp-1-0-Note.md"), None);
        assert_eq!(icloud::name_behind_placeholder("Note.md"), None);
        // A file genuinely named `.icloud` is not a placeholder for "".
        assert_eq!(icloud::name_behind_placeholder(".icloud"), None);
    }
}

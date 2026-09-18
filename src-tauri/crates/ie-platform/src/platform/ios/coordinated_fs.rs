//! The iOS filesystem: POSIX, with two detours for File Provider vaults.
//!
//! Delegates to [`StdFileSystem`] for everything that works unchanged, and
//! overrides the four operations that a placeholder or a provider's bookkeeping
//! would otherwise break:
//!
//! | operation  | why it differs |
//! |------------|----------------|
//! | `read`     | the file may be evicted; materialise and retry |
//! | `metadata` | the same, so `exists` does not lie about an evicted note |
//! | `read_dir` | a placeholder entry must be reported under the real name |
//! | `write_atomic` | `rename(2)` bypasses the provider; use its replace |
//!
//! Every other method is the desktop's, verbatim. That matters: the atomic
//! write's durability protocol, the symlink handling, the case-sensitivity
//! probe and the `\\?\` stripping are all behaviours the vault depends on, and
//! reimplementing them here would be a second place for them to drift.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{PlatformError, Result};
use crate::fs::{DirEntry, FileMetadata, FileSystem};
use crate::platform::ios::file_provider::FileProvider;
use crate::platform::PlatformOps;
use crate::std_fs::StdFileSystem;

pub struct CoordinatedFileSystem {
    inner: StdFileSystem,
    provider: Arc<dyn FileProvider>,
}

impl CoordinatedFileSystem {
    pub fn new(ops: Arc<dyn PlatformOps>, provider: Arc<dyn FileProvider>) -> Self {
        Self {
            inner: StdFileSystem::new(ops),
            provider,
        }
    }

    /// The placeholder standing in for `path`, if this provider uses them and
    /// one is actually present.
    fn placeholder_for(&self, path: &Path) -> Option<PathBuf> {
        let name = path.file_name()?.to_str()?;
        let placeholder = self.provider.placeholder_name(name)?;
        let candidate = path.with_file_name(placeholder);
        candidate.exists().then_some(candidate)
    }

    /// Bring `path` onto the device if it is currently a placeholder.
    ///
    /// Returns whether anything was done, so callers only retry when a retry
    /// could succeed.
    fn materialize_if_placeholder(&self, path: &Path) -> Result<bool> {
        if self.placeholder_for(path).is_none() {
            return Ok(false);
        }
        tracing::debug!(path = %path.display(), "materialising an evicted file");
        self.provider.ensure_materialized(path)?;
        Ok(true)
    }
}

impl FileSystem for CoordinatedFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        match self.inner.read(path) {
            Ok(bytes) => Ok(bytes),
            Err(e) => {
                // Only a missing file can be an eviction. A permission error or
                // a real I/O failure must surface as itself, not be retried.
                if !matches!(e, PlatformError::NotFound { .. })
                    || !self.materialize_if_placeholder(path)?
                {
                    return Err(e);
                }
                self.inner.read(path)
            }
        }
    }

    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()> {
        use std::io::Write;

        let temp = self.inner.temp_sibling(path)?;
        {
            let mut file = std::fs::File::create(&temp)
                .map_err(|e| PlatformError::from_io("create_temp", &temp, e))?;
            file.write_all(contents)
                .map_err(|e| PlatformError::from_io("write_temp", &temp, e))?;
            file.flush()
                .map_err(|e| PlatformError::from_io("flush_temp", &temp, e))?;
            // The bytes are durable before anything points at them, exactly as
            // on desktop. A crash between here and the replace leaves a
            // `.ie-tmp-*` file, which `diagnostics()` reports.
            file.sync_all()
                .map_err(|e| PlatformError::from_io("fsync_temp", &temp, e))?;
        }

        // The provider performs the swap. On iCloud this is
        // `NSFileCoordinator` + `replaceItemAt`; on a plain folder it is
        // `rename(2)`. Either way the temporary is gone when this returns.
        self.provider.replace_item(path, &temp)?;

        if let Some(dir) = path.parent() {
            if let Err(e) = self.sync_dir(dir) {
                tracing::warn!(directory = %dir.display(), error = %e, "could not fsync directory after atomic write");
            }
        }
        Ok(())
    }

    fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        match self.inner.metadata(path) {
            Ok(m) => Ok(m),
            Err(e @ PlatformError::NotFound { .. }) => {
                // An evicted note still exists as far as the vault is
                // concerned; reporting it missing would make the indexer delete
                // its row and the explorer hide it.
                match self.placeholder_for(path) {
                    Some(placeholder) => self.inner.metadata(&placeholder),
                    None => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let entries = self.inner.read_dir(path)?;
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            match self.provider.name_behind_placeholder(&entry.file_name) {
                // Report the note, not the placeholder: `Note.md`, at the path
                // the vault would expect, carrying the placeholder's size and
                // mtime. Reading it materialises it, and the size change on the
                // next scan is what makes the indexer pick up the real content.
                Some(real_name) => out.push(DirEntry {
                    path: entry.path.with_file_name(&real_name),
                    file_name: real_name,
                    metadata: entry.metadata,
                }),
                None => out.push(entry),
            }
        }
        Ok(out)
    }

    fn create_dir_all(&self, path: &Path) -> Result<()> {
        self.inner.create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        // Removing an evicted file means removing its placeholder; there is no
        // point downloading content in order to delete it.
        match self.inner.remove_file(path) {
            Err(e @ PlatformError::NotFound { .. }) => match self.placeholder_for(path) {
                Some(placeholder) => self.inner.remove_file(&placeholder),
                None => Err(e),
            },
            other => other,
        }
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.inner.remove_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        // A move within the vault, not a replace-in-place: the provider tracks
        // it correctly, and forcing a download first would turn renaming a
        // folder of evicted notes into a multi-megabyte operation.
        match self.inner.rename(from, to) {
            Err(e @ PlatformError::NotFound { .. }) => match self.placeholder_for(from) {
                Some(placeholder) => {
                    let name = to.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
                        PlatformError::InvalidUtf8 {
                            path: to.to_path_buf(),
                        }
                    })?;
                    let target = match self.provider.placeholder_name(name) {
                        Some(p) => to.with_file_name(p),
                        None => to.to_path_buf(),
                    };
                    self.inner.rename(&placeholder, &target)
                }
                None => Err(e),
            },
            other => other,
        }
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<u64> {
        // Copying does need the bytes.
        self.materialize_if_placeholder(from)?;
        self.inner.copy(from, to)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        self.inner.canonicalize(path)
    }

    fn is_case_sensitive(&self, dir: &Path) -> Result<bool> {
        self.inner.is_case_sensitive(dir)
    }

    fn sync_dir(&self, dir: &Path) -> Result<()> {
        self.inner.sync_dir(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::ios::file_provider::{icloud, LocalFolder};
    use crate::platform::ios::IosPlatform;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A provider that behaves like iCloud: files start as `.Name.icloud`
    /// placeholders and the real file appears only when asked for.
    ///
    /// This is the whole point of testing the adapter on a Linux host — the
    /// eviction behaviour is a naming convention plus a download call, and both
    /// can be reproduced exactly without an Apple device.
    struct FakeICloud {
        downloads: AtomicUsize,
        contents: std::sync::Mutex<std::collections::HashMap<PathBuf, Vec<u8>>>,
    }

    impl FakeICloud {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                downloads: AtomicUsize::new(0),
                contents: std::sync::Mutex::new(std::collections::HashMap::new()),
            })
        }

        /// Put `path` in the vault as an evicted file: a placeholder on disk,
        /// the real bytes held "in the cloud".
        fn evict(&self, path: &Path, bytes: &[u8]) {
            let name = path.file_name().unwrap().to_str().unwrap();
            std::fs::write(path.with_file_name(icloud::placeholder_name(name)), b"").unwrap();
            self.contents
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), bytes.to_vec());
        }
    }

    impl FileProvider for FakeICloud {
        fn ensure_materialized(&self, path: &Path) -> Result<()> {
            self.downloads.fetch_add(1, Ordering::Relaxed);
            let bytes = self
                .contents
                .lock()
                .unwrap()
                .remove(path)
                .unwrap_or_default();
            std::fs::write(path, bytes).unwrap();
            let name = path.file_name().unwrap().to_str().unwrap();
            let _ = std::fs::remove_file(path.with_file_name(icloud::placeholder_name(name)));
            Ok(())
        }

        fn replace_item(&self, target: &Path, source: &Path) -> Result<()> {
            LocalFolder.replace_item(target, source)
        }

        fn placeholder_name(&self, file_name: &str) -> Option<String> {
            Some(icloud::placeholder_name(file_name))
        }

        fn name_behind_placeholder(&self, entry_name: &str) -> Option<String> {
            icloud::name_behind_placeholder(entry_name)
        }
    }

    fn ios_ops() -> Arc<dyn PlatformOps> {
        Arc::new(IosPlatform::new())
    }

    #[test]
    fn a_local_vault_behaves_exactly_like_the_desktop() {
        let dir = tempfile::tempdir().unwrap();
        let fs = CoordinatedFileSystem::new(ios_ops(), Arc::new(LocalFolder));
        let path = dir.path().join("Note.md");

        fs.write_atomic(&path, b"# Hello").unwrap();
        assert_eq!(fs.read_to_string(&path).unwrap(), "# Hello");
        fs.write_atomic(&path, b"# Replaced").unwrap();
        assert_eq!(fs.read_to_string(&path).unwrap(), "# Replaced");

        // No temporary survived the replace.
        let leftovers: Vec<_> = fs
            .read_dir(dir.path())
            .unwrap()
            .into_iter()
            .filter(|e| StdFileSystem::is_temp_artifact(&e.file_name))
            .collect();
        assert!(leftovers.is_empty(), "left {leftovers:?} behind");
    }

    #[test]
    fn reading_an_evicted_note_downloads_it_once() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        let path = dir.path().join("Note.md");
        provider.evict(&path, b"# From the cloud");

        let fs = CoordinatedFileSystem::new(ios_ops(), provider.clone());
        assert_eq!(fs.read_to_string(&path).unwrap(), "# From the cloud");
        assert_eq!(provider.downloads.load(Ordering::Relaxed), 1);

        // Now that it is local, a second read must not touch the network.
        assert_eq!(fs.read_to_string(&path).unwrap(), "# From the cloud");
        assert_eq!(provider.downloads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn an_evicted_note_is_listed_under_its_real_name() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        provider.evict(&dir.path().join("Evicted.md"), b"body");
        std::fs::write(dir.path().join("Local.md"), b"body").unwrap();

        let fs = CoordinatedFileSystem::new(ios_ops(), provider);
        let mut names: Vec<_> = fs
            .read_dir(dir.path())
            .unwrap()
            .into_iter()
            .map(|e| e.file_name)
            .collect();
        names.sort();

        // The indexer must see a note here, not a hidden `.Evicted.md.icloud`
        // that it would skip — otherwise an evicted note silently leaves the
        // vault.
        assert_eq!(
            names,
            vec!["Evicted.md".to_string(), "Local.md".to_string()]
        );
    }

    #[test]
    fn an_evicted_note_still_exists() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        let path = dir.path().join("Evicted.md");
        provider.evict(&path, b"body");

        let fs = CoordinatedFileSystem::new(ios_ops(), provider.clone());
        assert!(fs.exists(&path), "an evicted note must not read as deleted");
        assert!(fs.is_file(&path));
        // Asking whether it exists must not cost a download.
        assert_eq!(provider.downloads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn deleting_an_evicted_note_does_not_download_it_first() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        let path = dir.path().join("Evicted.md");
        provider.evict(&path, b"body");

        let fs = CoordinatedFileSystem::new(ios_ops(), provider.clone());
        fs.remove_file(&path).unwrap();

        assert_eq!(provider.downloads.load(Ordering::Relaxed), 0);
        assert!(fs.read_dir(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn renaming_an_evicted_note_moves_the_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        let from = dir.path().join("Old.md");
        let to = dir.path().join("New.md");
        provider.evict(&from, b"body");

        let fs = CoordinatedFileSystem::new(ios_ops(), provider.clone());
        fs.rename(&from, &to).unwrap();

        assert_eq!(provider.downloads.load(Ordering::Relaxed), 0);
        let names: Vec<_> = fs
            .read_dir(dir.path())
            .unwrap()
            .into_iter()
            .map(|e| e.file_name)
            .collect();
        assert_eq!(names, vec!["New.md".to_string()]);
    }

    #[test]
    fn a_genuinely_missing_file_is_still_missing() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        let fs = CoordinatedFileSystem::new(ios_ops(), provider.clone());

        let err = fs.read(&dir.path().join("Nowhere.md")).unwrap_err();
        assert!(matches!(err, PlatformError::NotFound { .. }));
        // No placeholder, so nothing to download — the error must not be
        // turned into a pointless round trip.
        assert_eq!(provider.downloads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_app_directory_is_not_mistaken_for_a_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let provider = FakeICloud::new();
        std::fs::create_dir(dir.path().join(".inner-empire")).unwrap();

        let fs = CoordinatedFileSystem::new(ios_ops(), provider);
        let names: Vec<_> = fs
            .read_dir(dir.path())
            .unwrap()
            .into_iter()
            .map(|e| e.file_name)
            .collect();
        assert_eq!(names, vec![".inner-empire".to_string()]);
    }
}

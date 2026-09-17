//! Shared scaffolding for the integration tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ie_core::index::{IndexDb, Indexer};
use ie_core::vault::path::VaultPath;
#[allow(unused_imports)]
use ie_platform::FileSystem as _;
use ie_platform::{FixedClock, MemoryFileSystem, SharedClock, SharedFileSystem};

/// A vault held entirely in memory, so tests are fast and platform-independent
/// — including the ability to emulate NTFS case folding while running on ext4.
///
/// Shared by several test binaries, each of which uses a different subset of
/// the helpers, so the unused-code warning is silenced for the module rather
/// than for whichever method one file happens not to call.
#[allow(dead_code)]
pub struct TestVault {
    pub fs: SharedFileSystem,
    pub clock: SharedClock,
    pub root: PathBuf,
    pub db: IndexDb,
    now_ms: std::cell::Cell<i64>,
}

#[allow(dead_code)]
impl TestVault {
    pub fn new() -> Self {
        Self::with_case_sensitivity(true)
    }

    /// `case_sensitive = false` reproduces how the same vault behaves on NTFS.
    pub fn with_case_sensitivity(case_sensitive: bool) -> Self {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::new(case_sensitive));
        let root = PathBuf::from("/vault");
        fs.create_dir_all(&root).unwrap();
        Self {
            fs,
            clock: Arc::new(FixedClock::new(1_700_000_000_000)),
            root,
            db: IndexDb::open_in_memory().unwrap(),
            now_ms: std::cell::Cell::new(1_700_000_000_000),
        }
    }

    pub fn indexer(&self) -> Indexer {
        Indexer::new(Arc::clone(&self.fs), Arc::clone(&self.clock))
    }

    /// Write a file, creating parent folders as needed.
    pub fn write(&self, relative: &str, contents: &str) -> VaultPath {
        let path = VaultPath::parse(relative).unwrap();
        let fs_path = path.to_fs_path(&self.root);
        if let Some(parent) = fs_path.parent() {
            self.fs.create_dir_all(parent).unwrap();
        }
        self.fs.write_atomic(&fs_path, contents.as_bytes()).unwrap();
        self.now_ms.set(self.now_ms.get() + 1000);
        path
    }

    pub fn delete(&self, relative: &str) {
        let path = VaultPath::parse(relative).unwrap();
        self.fs.remove_file(&path.to_fs_path(&self.root)).unwrap();
    }

    pub fn fs_path(&self, relative: &str) -> PathBuf {
        VaultPath::parse(relative).unwrap().to_fs_path(&self.root)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Index everything, discarding progress reports.
    pub fn scan(&mut self) -> ie_core::index::ScanReport {
        let indexer = self.indexer();
        let root = self.root.clone();
        indexer.full_scan(&mut self.db, &root, |_| {}).unwrap()
    }

    pub fn path(relative: &str) -> VaultPath {
        VaultPath::parse(relative).unwrap()
    }
}

use std::path::{Path, PathBuf};

use crate::error::{PlatformError, Result};

/// What the app needs to know about an entry on disk. Deliberately smaller
/// than `std::fs::Metadata`: it carries only portable facts, so the same
/// struct is meaningful on NTFS and ext4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub is_dir: bool,
    pub is_symlink: bool,
    pub len: u64,
    /// Milliseconds since the Unix epoch. `None` when the filesystem does not
    /// report a modification time, which some network mounts do.
    pub modified_ms: Option<i64>,
    pub readonly: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub file_name: String,
    pub metadata: FileMetadata,
}

/// The single doorway between `ie-core` and the real filesystem.
///
/// `ie-core` depends on this trait and never on `std::fs`, which is what lets
/// the core stay free of platform conditionals and lets tests substitute an
/// in-memory implementation.
pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> Result<Vec<u8>>;

    fn read_to_string(&self, path: &Path) -> Result<String> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|_| PlatformError::InvalidUtf8 {
            path: path.to_path_buf(),
        })
    }

    /// Replace `path`'s contents such that a concurrent reader observes either
    /// the complete previous content or the complete new content, never a
    /// partial write. Implementations must write to a temporary sibling,
    /// fsync it, rename over the target, and then durably record the rename.
    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()>;

    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn remove_file(&self, path: &Path) -> Result<()>;
    fn remove_dir_all(&self, path: &Path) -> Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> Result<u64>;
    fn metadata(&self, path: &Path) -> Result<FileMetadata>;
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf>;

    fn exists(&self, path: &Path) -> bool {
        self.metadata(path).is_ok()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| m.is_dir).unwrap_or(false)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| !m.is_dir).unwrap_or(false)
    }

    /// Does the filesystem holding `dir` distinguish `Note.md` from `note.md`?
    ///
    /// This is a property of the *mount*, not of the OS: a case-sensitive
    /// directory on Windows and a case-insensitive one on Linux both exist.
    /// The answer decides whether the vault must warn about name collisions,
    /// so it is probed rather than assumed.
    fn is_case_sensitive(&self, dir: &Path) -> Result<bool>;

    /// Make a directory's own entries durable. Needed after a rename so the
    /// new name survives a power loss, not just the file's bytes.
    fn sync_dir(&self, dir: &Path) -> Result<()>;
}

/// A shared, boxed filesystem handle. `ie-core` stores this, never a concrete
/// type, so swapping in a test double is a constructor argument.
pub type SharedFileSystem = std::sync::Arc<dyn FileSystem>;

pub(crate) fn metadata_from_std(meta: &std::fs::Metadata, is_symlink: bool) -> FileMetadata {
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok());
    FileMetadata {
        is_dir: meta.is_dir(),
        is_symlink,
        len: meta.len(),
        modified_ms,
        readonly: meta.permissions().readonly(),
    }
}

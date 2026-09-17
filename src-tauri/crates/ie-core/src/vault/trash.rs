//! An in-vault trash.
//!
//! Deletion must be recoverable, but it must not depend on the OS. The
//! Recycle Bin and the freedesktop trash specification behave differently,
//! are unavailable on network mounts and removable drives, and would put a
//! user's notes somewhere the vault cannot see. So the trash is a folder
//! inside the vault with a manifest, which works identically on GNOME, KDE and
//! Windows, travels with the vault, and can be inspected with a file manager.

use std::path::Path;

use ie_platform::SharedFileSystem;

use crate::error::{CoreError, Result};
use crate::vault::path::{VaultPath, APP_DIR};

pub const TRASH_DIR: &str = "trash";
pub const MANIFEST_FILE: &str = "trash.json";

/// One deleted item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    /// Where it was when it was deleted, so restore can put it back.
    pub original_path: VaultPath,
    pub trashed_ms: i64,
    /// Name inside the trash folder. Prefixed with the id so two files called
    /// `Untitled.md` can both be in the trash at once.
    pub stored_as: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashManifest {
    #[serde(default)]
    pub entries: Vec<TrashEntry>,
}

pub struct Trash {
    fs: SharedFileSystem,
    root: std::path::PathBuf,
}

impl Trash {
    pub fn new(fs: SharedFileSystem, vault_root: &Path) -> Self {
        Self {
            fs,
            root: vault_root.to_path_buf(),
        }
    }

    fn trash_dir(&self) -> std::path::PathBuf {
        self.root.join(APP_DIR).join(TRASH_DIR)
    }

    fn manifest_path(&self) -> std::path::PathBuf {
        self.trash_dir().join(MANIFEST_FILE)
    }

    pub fn load_manifest(&self) -> Result<TrashManifest> {
        let path = self.manifest_path();
        if !self.fs.exists(&path) {
            return Ok(TrashManifest::default());
        }
        let text = self.fs.read_to_string(&path)?;
        // A damaged manifest must not make the app refuse to start: the files
        // are still in the trash folder and can be recovered by hand.
        Ok(serde_json::from_str(&text).unwrap_or_default())
    }

    fn save_manifest(&self, manifest: &TrashManifest) -> Result<()> {
        let dir = self.trash_dir();
        self.fs.create_dir_all(&dir)?;
        let json = serde_json::to_string_pretty(manifest)?;
        self.fs.write_atomic(&self.manifest_path(), json.as_bytes())?;
        Ok(())
    }

    /// Move a file or folder into the trash.
    pub fn trash(&self, path: &VaultPath, now_ms: i64, id: String) -> Result<TrashEntry> {
        if path.is_root() {
            return Err(CoreError::Refused {
                operation: "trash",
                reason: "the vault root cannot be deleted".into(),
            });
        }
        if path.is_app_internal() {
            return Err(CoreError::Refused {
                operation: "trash",
                reason: "files the application owns cannot be deleted from the explorer".into(),
            });
        }

        let source = path.to_fs_path(&self.root);
        let metadata = self.fs.metadata(&source)?;

        let stored_as = format!("{id}-{}", path.file_name());
        let destination = self.trash_dir().join(&stored_as);
        self.fs.create_dir_all(&self.trash_dir())?;
        self.fs.rename(&source, &destination)?;

        let entry = TrashEntry {
            id,
            original_path: path.clone(),
            trashed_ms: now_ms,
            stored_as,
            is_dir: metadata.is_dir,
            size: metadata.len,
        };

        let mut manifest = self.load_manifest()?;
        manifest.entries.push(entry.clone());
        self.save_manifest(&manifest)?;
        Ok(entry)
    }

    /// Put an entry back where it came from.
    ///
    /// If something now occupies the original path, the restored file gets a
    /// numbered sibling name rather than overwriting it. Returns where it
    /// actually landed.
    pub fn restore(&self, id: &str) -> Result<VaultPath> {
        let mut manifest = self.load_manifest()?;
        let index = manifest
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| CoreError::Refused {
                operation: "restore",
                reason: format!("nothing in the trash has the id {id}"),
            })?;
        let entry = manifest.entries[index].clone();

        let source = self.trash_dir().join(&entry.stored_as);
        if !self.fs.exists(&source) {
            manifest.entries.remove(index);
            self.save_manifest(&manifest)?;
            return Err(CoreError::NotFound(entry.original_path));
        }

        let target = self.unique_destination(&entry.original_path)?;
        if let Some(parent) = target.to_fs_path(&self.root).parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.rename(&source, &target.to_fs_path(&self.root))?;

        manifest.entries.remove(index);
        self.save_manifest(&manifest)?;
        Ok(target)
    }

    /// Delete one entry for good.
    pub fn purge(&self, id: &str) -> Result<()> {
        let mut manifest = self.load_manifest()?;
        let Some(index) = manifest.entries.iter().position(|e| e.id == id) else {
            return Ok(());
        };
        let entry = manifest.entries.remove(index);
        let stored = self.trash_dir().join(&entry.stored_as);
        if self.fs.exists(&stored) {
            if entry.is_dir {
                self.fs.remove_dir_all(&stored)?;
            } else {
                self.fs.remove_file(&stored)?;
            }
        }
        self.save_manifest(&manifest)?;
        Ok(())
    }

    /// Empty the trash.
    pub fn purge_all(&self) -> Result<usize> {
        let manifest = self.load_manifest()?;
        let count = manifest.entries.len();
        for entry in &manifest.entries {
            let stored = self.trash_dir().join(&entry.stored_as);
            if self.fs.exists(&stored) {
                let _ = if entry.is_dir {
                    self.fs.remove_dir_all(&stored)
                } else {
                    self.fs.remove_file(&stored)
                };
            }
        }
        self.save_manifest(&TrashManifest::default())?;
        Ok(count)
    }

    /// Drop entries older than `max_age_ms`. Returns how many were removed.
    pub fn purge_older_than(&self, now_ms: i64, max_age_ms: i64) -> Result<usize> {
        let manifest = self.load_manifest()?;
        let expired: Vec<String> = manifest
            .entries
            .iter()
            .filter(|e| now_ms - e.trashed_ms > max_age_ms)
            .map(|e| e.id.clone())
            .collect();
        for id in &expired {
            self.purge(id)?;
        }
        Ok(expired.len())
    }

    pub fn list(&self) -> Result<Vec<TrashEntry>> {
        let mut entries = self.load_manifest()?.entries;
        entries.sort_by(|a, b| b.trashed_ms.cmp(&a.trashed_ms));
        Ok(entries)
    }

    /// Find a free path near `desired`, appending ` (1)`, ` (2)` and so on.
    fn unique_destination(&self, desired: &VaultPath) -> Result<VaultPath> {
        if !self.fs.exists(&desired.to_fs_path(&self.root)) {
            return Ok(desired.clone());
        }
        let stem = desired.stem();
        let extension = desired.extension();
        for attempt in 1..1000 {
            let name = match &extension {
                Some(ext) => format!("{stem} ({attempt}).{ext}"),
                None => format!("{stem} ({attempt})"),
            };
            let candidate = desired.with_file_name(&name)?;
            if !self.fs.exists(&candidate.to_fs_path(&self.root)) {
                return Ok(candidate);
            }
        }
        Err(CoreError::Refused {
            operation: "restore",
            reason: "could not find a free name to restore into".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_platform::{FileSystem, MemoryFileSystem};
    use std::sync::Arc;

    struct Harness {
        fs: SharedFileSystem,
        root: std::path::PathBuf,
        trash: Trash,
    }

    fn harness() -> Harness {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::default());
        let root = std::path::PathBuf::from("/vault");
        fs.create_dir_all(&root).unwrap();
        let trash = Trash::new(Arc::clone(&fs), &root);
        Harness { fs, root, trash }
    }

    impl Harness {
        fn write(&self, relative: &str, contents: &str) -> VaultPath {
            let path = VaultPath::parse(relative).unwrap();
            let fs_path = path.to_fs_path(&self.root);
            self.fs.create_dir_all(fs_path.parent().unwrap()).unwrap();
            self.fs.write_atomic(&fs_path, contents.as_bytes()).unwrap();
            path
        }

        fn exists(&self, relative: &str) -> bool {
            self.fs
                .exists(&VaultPath::parse(relative).unwrap().to_fs_path(&self.root))
        }
    }

    #[test]
    fn deleting_moves_the_file_out_of_the_vault_but_keeps_it() {
        let h = harness();
        let path = h.write("Notes/Idea.md", "content");

        let entry = h.trash.trash(&path, 1000, "id1".into()).unwrap();

        assert!(!h.exists("Notes/Idea.md"), "gone from where it was");
        assert_eq!(entry.original_path, path);
        assert_eq!(h.trash.list().unwrap().len(), 1);
    }

    #[test]
    fn restoring_puts_the_file_back_with_its_content() {
        let h = harness();
        let path = h.write("Notes/Idea.md", "content");
        h.trash.trash(&path, 1000, "id1".into()).unwrap();

        let restored = h.trash.restore("id1").unwrap();

        assert_eq!(restored, path);
        assert_eq!(
            h.fs.read_to_string(&path.to_fs_path(&h.root)).unwrap(),
            "content"
        );
        assert!(h.trash.list().unwrap().is_empty());
    }

    #[test]
    fn restoring_onto_an_occupied_path_does_not_overwrite_what_is_there() {
        let h = harness();
        let path = h.write("Note.md", "original");
        h.trash.trash(&path, 1000, "id1".into()).unwrap();
        h.write("Note.md", "a different note with the same name");

        let restored = h.trash.restore("id1").unwrap();

        assert_eq!(restored.as_str(), "Note (1).md");
        assert_eq!(
            h.fs.read_to_string(&path.to_fs_path(&h.root)).unwrap(),
            "a different note with the same name",
            "the occupying file must be untouched"
        );
        assert_eq!(
            h.fs.read_to_string(&restored.to_fs_path(&h.root)).unwrap(),
            "original"
        );
    }

    #[test]
    fn two_files_with_the_same_name_can_be_in_the_trash_at_once() {
        let h = harness();
        let a = h.write("A/Untitled.md", "first");
        let b = h.write("B/Untitled.md", "second");
        h.trash.trash(&a, 1000, "id1".into()).unwrap();
        h.trash.trash(&b, 2000, "id2".into()).unwrap();

        assert_eq!(h.trash.list().unwrap().len(), 2);
        h.trash.restore("id1").unwrap();
        h.trash.restore("id2").unwrap();
        assert_eq!(h.fs.read_to_string(&a.to_fs_path(&h.root)).unwrap(), "first");
        assert_eq!(h.fs.read_to_string(&b.to_fs_path(&h.root)).unwrap(), "second");
    }

    #[test]
    fn purging_removes_the_file_for_good() {
        let h = harness();
        let path = h.write("Note.md", "content");
        h.trash.trash(&path, 1000, "id1".into()).unwrap();

        h.trash.purge("id1").unwrap();

        assert!(h.trash.list().unwrap().is_empty());
        assert!(h.trash.restore("id1").is_err());
    }

    #[test]
    fn emptying_the_trash_removes_everything() {
        let h = harness();
        for name in ["A.md", "B.md", "C.md"] {
            let path = h.write(name, "x");
            h.trash.trash(&path, 1000, name.to_string()).unwrap();
        }
        assert_eq!(h.trash.purge_all().unwrap(), 3);
        assert!(h.trash.list().unwrap().is_empty());
    }

    #[test]
    fn retention_only_removes_entries_past_their_age() {
        let h = harness();
        let old = h.write("Old.md", "x");
        let fresh = h.write("Fresh.md", "x");
        h.trash.trash(&old, 1_000, "old".into()).unwrap();
        h.trash.trash(&fresh, 9_000, "fresh".into()).unwrap();

        let removed = h.trash.purge_older_than(10_000, 5_000).unwrap();

        assert_eq!(removed, 1);
        let remaining = h.trash.list().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "fresh");
    }

    #[test]
    fn the_listing_is_newest_first() {
        let h = harness();
        for (name, time) in [("A.md", 1000), ("B.md", 3000), ("C.md", 2000)] {
            let path = h.write(name, "x");
            h.trash.trash(&path, time, name.to_string()).unwrap();
        }
        let ids: Vec<_> = h.trash.list().unwrap().into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["B.md", "C.md", "A.md"]);
    }

    #[test]
    fn app_internal_files_cannot_be_deleted_through_the_trash() {
        let h = harness();
        let path = h.write(".inner-empire/index.db", "x");
        let error = h.trash.trash(&path, 1000, "id".into()).unwrap_err();
        assert_eq!(error.code(), "refused");
    }

    #[test]
    fn the_vault_root_cannot_be_deleted() {
        let h = harness();
        let error = h.trash.trash(&VaultPath::root(), 1000, "id".into()).unwrap_err();
        assert_eq!(error.code(), "refused");
    }

    #[test]
    fn a_damaged_manifest_does_not_prevent_the_trash_from_working() {
        let h = harness();
        h.fs.create_dir_all(&h.root.join(APP_DIR).join(TRASH_DIR))
            .unwrap();
        h.fs.write_atomic(
            &h.root.join(APP_DIR).join(TRASH_DIR).join(MANIFEST_FILE),
            b"{ not json",
        )
        .unwrap();

        assert!(h.trash.list().unwrap().is_empty());
        let path = h.write("Note.md", "x");
        assert!(h.trash.trash(&path, 1000, "id".into()).is_ok());
    }

    #[test]
    fn a_whole_folder_can_be_trashed_and_restored() {
        let h = harness();
        h.write("Folder/a.md", "one");
        h.write("Folder/nested/b.md", "two");

        let folder = VaultPath::parse("Folder").unwrap();
        let entry = h.trash.trash(&folder, 1000, "id".into()).unwrap();
        assert!(entry.is_dir);
        assert!(!h.exists("Folder/a.md"));

        h.trash.restore("id").unwrap();
        assert!(h.exists("Folder/a.md"));
        assert!(h.exists("Folder/nested/b.md"));
    }
}

//! The recovery journal.
//!
//! Every write to a note is atomic, so a crash can never leave a *damaged*
//! file. What it can leave behind is work the user did in the seconds since
//! the last autosave. This journal closes that window: unsaved buffers are
//! written periodically to the application's data directory, and anything
//! newer than the file it belongs to is offered back on the next launch.
//!
//! It lives outside the vault on purpose. A recovery entry is a private,
//! machine-local scrap of half-finished text, not something that should sync
//! to another machine or appear in the user's file manager.

use std::path::{Path, PathBuf};

use ie_platform::{SharedAppDirs, SharedFileSystem};

use crate::error::Result;
use crate::vault::path::VaultPath;

pub const RECOVERY_DIR: &str = "recovery";

/// One note's unsaved text.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub path: VaultPath,
    pub content: String,
    /// When the journal entry was written.
    pub saved_ms: i64,
    /// The file's modification time when the buffer was opened, so a stale
    /// entry can be told from a genuinely newer one.
    pub base_modified_ms: i64,
}

/// What a recovery entry means, once the file it belongs to has been examined.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCandidate {
    pub path: VaultPath,
    /// The unsaved text.
    pub content: String,
    pub saved_ms: i64,
    /// True when the file on disk already holds this text, so there is nothing
    /// to recover and the entry is only waiting to be cleaned up.
    pub already_saved: bool,
    /// True when the file changed after the journal entry was written, so
    /// restoring would discard newer work.
    pub file_is_newer: bool,
}

pub struct RecoveryJournal {
    fs: SharedFileSystem,
    root: PathBuf,
}

impl RecoveryJournal {
    /// Open the journal for one vault. Entries are kept per vault id, so two
    /// vaults holding a note at the same path do not overwrite each other.
    pub fn new(fs: SharedFileSystem, dirs: &SharedAppDirs, vault_id: &str) -> Result<Self> {
        let root = dirs
            .data_dir()?
            .join(RECOVERY_DIR)
            .join(sanitize_id(vault_id));
        Ok(Self { fs, root })
    }

    fn entry_path(&self, path: &VaultPath) -> PathBuf {
        // A vault path contains slashes, so it is hashed rather than mirrored
        // as a directory tree: the journal stays flat and no name can escape it.
        self.root.join(format!("{}.json", key_for(path)))
    }

    /// Record a buffer's unsaved text.
    pub fn record(&self, entry: &RecoveryEntry) -> Result<()> {
        self.fs.create_dir_all(&self.root)?;
        let json = serde_json::to_string(entry)?;
        self.fs
            .write_atomic(&self.entry_path(&entry.path), json.as_bytes())?;
        Ok(())
    }

    /// Drop a note's entry, once its content is safely on disk.
    pub fn forget(&self, path: &VaultPath) -> Result<()> {
        let target = self.entry_path(path);
        if self.fs.exists(&target) {
            self.fs.remove_file(&target)?;
        }
        Ok(())
    }

    pub fn forget_all(&self) -> Result<()> {
        if self.fs.exists(&self.root) {
            self.fs.remove_dir_all(&self.root)?;
        }
        Ok(())
    }

    /// Everything the journal holds, classified against the vault as it is now.
    ///
    /// `current_content` and `current_modified` describe the file on disk;
    /// passing them in keeps this module free of any notion of where the vault
    /// lives.
    pub fn candidates(
        &self,
        mut inspect: impl FnMut(&VaultPath) -> Option<(String, i64)>,
    ) -> Result<Vec<RecoveryCandidate>> {
        if !self.fs.exists(&self.root) {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for file in self.fs.read_dir(&self.root)? {
            if file.metadata.is_dir || !file.file_name.ends_with(".json") {
                continue;
            }
            let Ok(text) = self.fs.read_to_string(&file.path) else {
                continue;
            };
            // A damaged entry is skipped rather than failing the whole scan:
            // one unreadable scrap must not hide the others.
            let Ok(entry) = serde_json::from_str::<RecoveryEntry>(&text) else {
                continue;
            };

            let (already_saved, file_is_newer) = match inspect(&entry.path) {
                Some((content, modified_ms)) => {
                    (content == entry.content, modified_ms > entry.saved_ms)
                }
                // The file is gone. The entry is still worth offering, because
                // it is the only copy of that text.
                None => (false, false),
            };

            out.push(RecoveryCandidate {
                path: entry.path,
                content: entry.content,
                saved_ms: entry.saved_ms,
                already_saved,
                file_is_newer,
            });
        }

        out.sort_by(|a, b| b.saved_ms.cmp(&a.saved_ms));
        Ok(out)
    }

    /// Remove entries whose text is already on disk, and anything older than
    /// `max_age_ms`. Returns how many were removed.
    pub fn prune(
        &self,
        now_ms: i64,
        max_age_ms: i64,
        inspect: impl FnMut(&VaultPath) -> Option<(String, i64)>,
    ) -> Result<usize> {
        let candidates = self.candidates(inspect)?;
        let mut removed = 0;
        for candidate in candidates {
            if candidate.already_saved || now_ms - candidate.saved_ms > max_age_ms {
                self.forget(&candidate.path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn location(&self) -> &Path {
        &self.root
    }
}

/// A short, filesystem-safe key for a vault path.
fn key_for(path: &VaultPath) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.as_str().as_bytes());
    digest[..12].iter().map(|b| format!("{b:02x}")).collect()
}

/// A vault id comes from the vault's own settings file, which the user could
/// have edited, so it is sanitised before becoming a directory name.
fn sanitize_id(id: &str) -> String {
    let cleaned: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "unnamed".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_platform::{MemoryFileSystem, SharedAppDirs, TestDirs};
    use std::sync::Arc;

    fn journal() -> RecoveryJournal {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::default());
        let dirs: SharedAppDirs = Arc::new(TestDirs::new("/appdata"));
        RecoveryJournal::new(fs, &dirs, "vault-1").unwrap()
    }

    fn entry(path: &str, content: &str, saved_ms: i64) -> RecoveryEntry {
        RecoveryEntry {
            path: VaultPath::parse(path).unwrap(),
            content: content.to_string(),
            saved_ms,
            base_modified_ms: 0,
        }
    }

    #[test]
    fn an_entry_is_offered_back_when_the_file_lacks_its_text() {
        let journal = journal();
        journal
            .record(&entry("A.md", "unsaved text", 1000))
            .unwrap();

        let candidates = journal
            .candidates(|_| Some(("old text".to_string(), 500)))
            .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].content, "unsaved text");
        assert!(!candidates[0].already_saved);
        assert!(!candidates[0].file_is_newer);
    }

    #[test]
    fn an_entry_whose_text_is_already_on_disk_is_marked_as_such() {
        let journal = journal();
        journal.record(&entry("A.md", "same text", 1000)).unwrap();

        let candidates = journal
            .candidates(|_| Some(("same text".to_string(), 500)))
            .unwrap();

        assert!(candidates[0].already_saved, "nothing to recover here");
    }

    #[test]
    fn a_file_changed_after_the_entry_is_flagged_rather_than_overwritten() {
        let journal = journal();
        journal
            .record(&entry("A.md", "older unsaved text", 1000))
            .unwrap();

        let candidates = journal
            .candidates(|_| Some(("newer text from elsewhere".to_string(), 5000)))
            .unwrap();

        assert!(
            candidates[0].file_is_newer,
            "restoring would discard work done after the crash"
        );
    }

    #[test]
    fn an_entry_for_a_deleted_file_is_still_offered() {
        let journal = journal();
        journal
            .record(&entry("Gone.md", "the only copy", 1000))
            .unwrap();

        let candidates = journal.candidates(|_| None).unwrap();

        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].already_saved);
    }

    #[test]
    fn forgetting_removes_one_entry_and_leaves_the_others() {
        let journal = journal();
        journal.record(&entry("A.md", "a", 1000)).unwrap();
        journal.record(&entry("B.md", "b", 1000)).unwrap();

        journal.forget(&VaultPath::parse("A.md").unwrap()).unwrap();

        let remaining = journal.candidates(|_| None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path.as_str(), "B.md");
    }

    #[test]
    fn recording_twice_replaces_rather_than_accumulates() {
        let journal = journal();
        journal.record(&entry("A.md", "first", 1000)).unwrap();
        journal.record(&entry("A.md", "second", 2000)).unwrap();

        let candidates = journal.candidates(|_| None).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].content, "second");
    }

    #[test]
    fn candidates_are_newest_first() {
        let journal = journal();
        journal.record(&entry("A.md", "a", 1000)).unwrap();
        journal.record(&entry("B.md", "b", 3000)).unwrap();
        journal.record(&entry("C.md", "c", 2000)).unwrap();

        let paths: Vec<String> = journal
            .candidates(|_| None)
            .unwrap()
            .into_iter()
            .map(|c| c.path.as_str().to_string())
            .collect();
        assert_eq!(paths, vec!["B.md", "C.md", "A.md"]);
    }

    #[test]
    fn pruning_clears_saved_and_expired_entries_only() {
        let journal = journal();
        journal.record(&entry("Saved.md", "on disk", 9000)).unwrap();
        journal.record(&entry("Old.md", "ancient", 1000)).unwrap();
        journal.record(&entry("Fresh.md", "recent", 9500)).unwrap();

        let removed = journal
            .prune(10_000, 5_000, |path| {
                if path.as_str() == "Saved.md" {
                    Some(("on disk".to_string(), 9000))
                } else {
                    None
                }
            })
            .unwrap();

        assert_eq!(removed, 2, "the saved one and the expired one");
        let remaining = journal.candidates(|_| None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path.as_str(), "Fresh.md");
    }

    #[test]
    fn a_damaged_entry_does_not_hide_the_others() {
        let journal = journal();
        journal.record(&entry("Good.md", "kept", 1000)).unwrap();
        journal
            .fs
            .write_atomic(&journal.root.join("corrupt.json"), b"{ not json")
            .unwrap();

        let candidates = journal.candidates(|_| None).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path.as_str(), "Good.md");
    }

    #[test]
    fn a_vault_id_cannot_escape_the_journal_directory() {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::default());
        let dirs: SharedAppDirs = Arc::new(TestDirs::new("/appdata"));
        let journal = RecoveryJournal::new(fs, &dirs, "../../etc").unwrap();

        assert!(
            journal.location().to_string_lossy().contains("______etc"),
            "a crafted id must not climb out: {}",
            journal.location().display()
        );
    }

    #[test]
    fn an_empty_journal_reports_nothing_rather_than_failing() {
        assert!(journal().candidates(|_| None).unwrap().is_empty());
    }
}

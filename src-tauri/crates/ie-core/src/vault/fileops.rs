//! Creating, renaming, moving and deleting the user's files.
//!
//! Every mutation goes through here so three rules are applied consistently:
//! names are legal on every target platform, a name that would collide under
//! case folding is refused rather than silently overwriting, and nothing is
//! written except through the atomic path.

use std::path::Path;

use ie_platform::SharedFileSystem;

use crate::error::{CoreError, Result};
use crate::vault::path::{sanitize_segment, validate_segment, VaultPath};

/// What to do when a target name already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collision {
    /// Refuse, so the caller can prompt.
    Fail,
    /// Append ` (1)`, ` (2)`, … until a free name is found.
    Rename,
    /// Replace, for the rare case where the caller has already confirmed.
    Overwrite,
}

pub struct FileOps {
    fs: SharedFileSystem,
    root: std::path::PathBuf,
}

impl FileOps {
    pub fn new(fs: SharedFileSystem, root: &Path) -> Self {
        Self {
            fs,
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve(&self, path: &VaultPath) -> std::path::PathBuf {
        path.to_fs_path(&self.root)
    }

    pub fn exists(&self, path: &VaultPath) -> bool {
        self.fs.exists(&self.resolve(path))
    }

    pub fn read(&self, path: &VaultPath) -> Result<String> {
        Ok(self.fs.read_to_string(&self.resolve(path))?)
    }

    pub fn read_bytes(&self, path: &VaultPath) -> Result<Vec<u8>> {
        Ok(self.fs.read(&self.resolve(path))?)
    }

    /// Write a note. Always atomic, so a crash mid-write cannot truncate it.
    pub fn write(&self, path: &VaultPath, contents: &str) -> Result<()> {
        let target = self.resolve(path);
        if let Some(parent) = target.parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.write_atomic(&target, contents.as_bytes())?;
        Ok(())
    }

    pub fn write_bytes(&self, path: &VaultPath, contents: &[u8]) -> Result<()> {
        let target = self.resolve(path);
        if let Some(parent) = target.parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.write_atomic(&target, contents)?;
        Ok(())
    }

    /// Create a note, refusing to clobber an existing one.
    pub fn create_note(
        &self,
        path: &VaultPath,
        contents: &str,
        collision: Collision,
    ) -> Result<VaultPath> {
        let target = self.apply_collision_policy(path, collision)?;
        self.write(&target, contents)?;
        Ok(target)
    }

    pub fn create_folder(&self, path: &VaultPath) -> Result<()> {
        if path.is_root() {
            return Ok(());
        }
        self.guard_case_collision(path)?;
        self.fs.create_dir_all(&self.resolve(path))?;
        Ok(())
    }

    /// Move or rename, refusing anything that would lose data.
    pub fn move_entry(
        &self,
        from: &VaultPath,
        to: &VaultPath,
        collision: Collision,
    ) -> Result<VaultPath> {
        if from == to {
            return Ok(to.clone());
        }
        if from.is_root() {
            return Err(CoreError::Refused {
                operation: "move",
                reason: "the vault root cannot be moved".into(),
            });
        }
        if to.is_within(from) {
            return Err(CoreError::Refused {
                operation: "move",
                reason: "a folder cannot be moved inside itself".into(),
            });
        }
        if !self.exists(from) {
            return Err(CoreError::NotFound(from.clone()));
        }

        for segment in to.segments() {
            validate_segment(segment)?;
        }

        // A pure change of capitalisation is a legitimate rename, not a
        // collision — and on a case-insensitive filesystem it is the one case
        // where source and destination are the same file.
        let case_only_rename = from.fold() == to.fold();
        let target = if case_only_rename {
            to.clone()
        } else {
            self.apply_collision_policy(to, collision)?
        };

        let destination = self.resolve(&target);
        if let Some(parent) = destination.parent() {
            self.fs.create_dir_all(parent)?;
        }

        if case_only_rename && self.fs.exists(&destination) {
            // NTFS refuses to rename a file onto itself, so go via a temporary
            // name. Two renames instead of one, and no window where the file
            // does not exist under some name.
            let staging = target.with_file_name(&format!(
                ".ie-case-{}-{}",
                std::process::id(),
                target.file_name()
            ))?;
            self.fs.rename(&self.resolve(from), &self.resolve(&staging))?;
            self.fs.rename(&self.resolve(&staging), &destination)?;
        } else {
            self.fs.rename(&self.resolve(from), &destination)?;
        }

        Ok(target)
    }

    /// Duplicate a note next to itself.
    pub fn duplicate(&self, path: &VaultPath) -> Result<VaultPath> {
        let contents = self.read_bytes(path)?;
        let target = self.free_name(path)?;
        self.write_bytes(&target, &contents)?;
        Ok(target)
    }

    /// Remove without going through the trash. Only for callers that have
    /// already made the deletion recoverable some other way.
    pub fn remove_permanently(&self, path: &VaultPath) -> Result<()> {
        if path.is_root() {
            return Err(CoreError::Refused {
                operation: "delete",
                reason: "the vault root cannot be deleted".into(),
            });
        }
        let target = self.resolve(path);
        let metadata = self.fs.metadata(&target)?;
        if metadata.is_dir {
            self.fs.remove_dir_all(&target)?;
        } else {
            self.fs.remove_file(&target)?;
        }
        Ok(())
    }

    /// List one folder, folders first then files, each alphabetically.
    pub fn list_folder(&self, folder: &VaultPath) -> Result<Vec<(VaultPath, bool)>> {
        let dir = self.resolve(folder);
        let mut entries: Vec<(VaultPath, bool)> = self
            .fs
            .read_dir(&dir)?
            .into_iter()
            .filter_map(|entry| {
                let path = VaultPath::from_fs_path(&self.root, &entry.path).ok()?;
                if path.is_app_internal() {
                    return None;
                }
                Some((path, entry.metadata.is_dir))
            })
            .collect();

        entries.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.0.file_name().to_lowercase().cmp(&b.0.file_name().to_lowercase()))
        });
        Ok(entries)
    }

    /// Turn arbitrary text into a note path under `folder`.
    ///
    /// Used when the user creates a note from an unresolved link, where the
    /// link text may contain characters no filesystem accepts.
    pub fn note_path_for_title(&self, folder: &VaultPath, title: &str) -> Result<VaultPath> {
        let name = sanitize_segment(title.trim());
        let with_extension = if name.to_lowercase().ends_with(".md") {
            name
        } else {
            format!("{name}.md")
        };
        folder.join(&with_extension)
    }

    fn apply_collision_policy(&self, path: &VaultPath, collision: Collision) -> Result<VaultPath> {
        match collision {
            Collision::Overwrite => Ok(path.clone()),
            Collision::Rename => self.free_name(path),
            Collision::Fail => {
                if self.exists(path) {
                    return Err(CoreError::AlreadyExists(path.clone()));
                }
                self.guard_case_collision(path)?;
                Ok(path.clone())
            }
        }
    }

    /// Refuse a name that differs from an existing sibling only by case.
    ///
    /// On ext4 both files can exist; on NTFS the second silently becomes the
    /// first. Allowing it would produce a vault that loses a note when it is
    /// copied to Windows, so it is refused on every platform.
    fn guard_case_collision(&self, path: &VaultPath) -> Result<()> {
        let parent = self.resolve(&path.parent());
        if !self.fs.exists(&parent) {
            return Ok(());
        }
        let wanted = path.file_name().to_lowercase();
        for entry in self.fs.read_dir(&parent)? {
            if entry.file_name.to_lowercase() == wanted && entry.file_name != path.file_name() {
                return Err(CoreError::CaseCollision {
                    requested: path.file_name().to_string(),
                    existing: VaultPath::from_fs_path(&self.root, &entry.path)?,
                });
            }
        }
        Ok(())
    }

    /// The first free name at or after `desired`.
    fn free_name(&self, desired: &VaultPath) -> Result<VaultPath> {
        if !self.exists(desired) && self.guard_case_collision(desired).is_ok() {
            return Ok(desired.clone());
        }
        let stem = desired.stem();
        let extension = desired.extension();
        for attempt in 1..10_000 {
            let name = match &extension {
                Some(ext) => format!("{stem} {attempt}.{ext}"),
                None => format!("{stem} {attempt}"),
            };
            let candidate = desired.with_file_name(&name)?;
            if !self.exists(&candidate) && self.guard_case_collision(&candidate).is_ok() {
                return Ok(candidate);
            }
        }
        Err(CoreError::Refused {
            operation: "create",
            reason: "could not find a free name".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_platform::{FileSystem, MemoryFileSystem};
    use std::sync::Arc;

    fn ops(case_sensitive: bool) -> (FileOps, SharedFileSystem, std::path::PathBuf) {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::new(case_sensitive));
        let root = std::path::PathBuf::from("/vault");
        fs.create_dir_all(&root).unwrap();
        let ops = FileOps::new(Arc::clone(&fs), &root);
        (ops, fs, root)
    }

    fn p(text: &str) -> VaultPath {
        VaultPath::parse(text).unwrap()
    }

    #[test]
    fn creating_a_note_writes_it_and_makes_its_folders() {
        let (ops, _, _) = ops(true);
        let path = ops
            .create_note(&p("Deep/Nested/Note.md"), "# Hi\n", Collision::Fail)
            .unwrap();
        assert_eq!(path.as_str(), "Deep/Nested/Note.md");
        assert_eq!(ops.read(&path).unwrap(), "# Hi\n");
    }

    #[test]
    fn creating_over_an_existing_note_fails_by_default() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("A.md"), "one", Collision::Fail).unwrap();
        let error = ops
            .create_note(&p("A.md"), "two", Collision::Fail)
            .unwrap_err();
        assert_eq!(error.code(), "already_exists");
        assert_eq!(ops.read(&p("A.md")).unwrap(), "one");
    }

    #[test]
    fn the_rename_policy_finds_a_free_name_instead_of_failing() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("A.md"), "one", Collision::Fail).unwrap();
        let second = ops.create_note(&p("A.md"), "two", Collision::Rename).unwrap();
        assert_eq!(second.as_str(), "A 1.md");
        let third = ops.create_note(&p("A.md"), "three", Collision::Rename).unwrap();
        assert_eq!(third.as_str(), "A 2.md");
    }

    #[test]
    fn a_name_colliding_only_by_case_is_refused_even_where_it_would_work() {
        // On ext4 both files can exist; allowing it produces a vault that loses
        // a note the moment it is opened on Windows.
        let (ops, _, _) = ops(true);
        ops.create_note(&p("MyNote.md"), "one", Collision::Fail).unwrap();
        let error = ops
            .create_note(&p("mynote.md"), "two", Collision::Fail)
            .unwrap_err();
        assert_eq!(error.code(), "case_collision");
    }

    #[test]
    fn renaming_a_note_moves_it_and_keeps_its_content() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("Old.md"), "content", Collision::Fail).unwrap();
        let new = ops
            .move_entry(&p("Old.md"), &p("New.md"), Collision::Fail)
            .unwrap();
        assert_eq!(new.as_str(), "New.md");
        assert!(!ops.exists(&p("Old.md")));
        assert_eq!(ops.read(&new).unwrap(), "content");
    }

    #[test]
    fn changing_only_capitalisation_is_a_rename_not_a_collision() {
        for case_sensitive in [true, false] {
            let (ops, _, _) = ops(case_sensitive);
            ops.create_note(&p("mynote.md"), "content", Collision::Fail)
                .unwrap();
            let new = ops
                .move_entry(&p("mynote.md"), &p("MyNote.md"), Collision::Fail)
                .unwrap();
            assert_eq!(new.as_str(), "MyNote.md");
            assert_eq!(
                ops.read(&new).unwrap(),
                "content",
                "case-only rename lost content on case_sensitive={case_sensitive}"
            );
        }
    }

    #[test]
    fn moving_a_note_into_another_folder_creates_the_folder() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("Note.md"), "x", Collision::Fail).unwrap();
        let moved = ops
            .move_entry(&p("Note.md"), &p("Archive/2026/Note.md"), Collision::Fail)
            .unwrap();
        assert_eq!(moved.as_str(), "Archive/2026/Note.md");
        assert!(ops.exists(&moved));
    }

    #[test]
    fn a_folder_cannot_be_moved_inside_itself() {
        let (ops, _, _) = ops(true);
        ops.create_folder(&p("Projects")).unwrap();
        let error = ops
            .move_entry(&p("Projects"), &p("Projects/Sub"), Collision::Fail)
            .unwrap_err();
        assert_eq!(error.code(), "refused");
    }

    #[test]
    fn moving_something_that_is_not_there_says_so() {
        let (ops, _, _) = ops(true);
        let error = ops
            .move_entry(&p("Ghost.md"), &p("New.md"), Collision::Fail)
            .unwrap_err();
        assert_eq!(error.code(), "not_found");
    }

    #[test]
    fn moving_onto_an_existing_file_fails_rather_than_overwriting_it() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("A.md"), "a", Collision::Fail).unwrap();
        ops.create_note(&p("B.md"), "b", Collision::Fail).unwrap();
        assert!(ops.move_entry(&p("A.md"), &p("B.md"), Collision::Fail).is_err());
        assert_eq!(ops.read(&p("B.md")).unwrap(), "b");
    }

    #[test]
    fn duplicating_a_note_makes_a_sibling_with_the_same_content() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("Note.md"), "content", Collision::Fail).unwrap();
        let copy = ops.duplicate(&p("Note.md")).unwrap();
        assert_eq!(copy.as_str(), "Note 1.md");
        assert_eq!(ops.read(&copy).unwrap(), "content");
    }

    #[test]
    fn listing_puts_folders_first_then_files_alphabetically() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("zebra.md"), "", Collision::Fail).unwrap();
        ops.create_note(&p("Apple.md"), "", Collision::Fail).unwrap();
        ops.create_folder(&p("Zulu")).unwrap();
        ops.create_folder(&p("alpha")).unwrap();

        let names: Vec<String> = ops
            .list_folder(&VaultPath::root())
            .unwrap()
            .into_iter()
            .map(|(path, _)| path.file_name().to_string())
            .collect();
        assert_eq!(names, vec!["alpha", "Zulu", "Apple.md", "zebra.md"]);
    }

    #[test]
    fn listing_hides_the_applications_own_folder() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("Note.md"), "", Collision::Fail).unwrap();
        ops.create_folder(&p(".inner-empire")).unwrap();

        let names: Vec<String> = ops
            .list_folder(&VaultPath::root())
            .unwrap()
            .into_iter()
            .map(|(path, _)| path.file_name().to_string())
            .collect();
        assert_eq!(names, vec!["Note.md"]);
    }

    #[test]
    fn a_title_with_illegal_characters_becomes_a_usable_filename() {
        let (ops, _, _) = ops(true);
        let path = ops
            .note_path_for_title(&p("Notes"), "Q1: Plan? <draft>")
            .unwrap();
        assert_eq!(path.as_str(), "Notes/Q1- Plan- -draft-.md");
        assert!(ops.create_note(&path, "", Collision::Fail).is_ok());
    }

    #[test]
    fn a_title_that_already_ends_in_md_is_not_given_a_second_extension() {
        let (ops, _, _) = ops(true);
        let path = ops.note_path_for_title(&p(""), "Readme.md").unwrap();
        assert_eq!(path.as_str(), "Readme.md");
    }

    #[test]
    fn permanent_removal_works_for_files_and_folders() {
        let (ops, _, _) = ops(true);
        ops.create_note(&p("Folder/a.md"), "", Collision::Fail).unwrap();
        ops.remove_permanently(&p("Folder/a.md")).unwrap();
        assert!(!ops.exists(&p("Folder/a.md")));

        ops.create_note(&p("Folder/b.md"), "", Collision::Fail).unwrap();
        ops.remove_permanently(&p("Folder")).unwrap();
        assert!(!ops.exists(&p("Folder")));
    }

    #[test]
    fn the_vault_root_is_protected_from_deletion() {
        let (ops, _, _) = ops(true);
        assert_eq!(
            ops.remove_permanently(&VaultPath::root()).unwrap_err().code(),
            "refused"
        );
    }
}

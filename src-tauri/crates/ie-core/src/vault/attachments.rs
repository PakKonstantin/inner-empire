//! Where a new attachment goes.
//!
//! The setting lives in `VaultSettings`; turning it into a path is here rather
//! than in a host, because it is vault semantics and every host must answer it
//! the same way. Two hosts disagreeing would scatter one vault's attachments
//! across two folders, and the user would only notice much later.

use crate::error::Result;
use crate::vault::path::{sanitize_segment, VaultPath};
use crate::vault::settings::AttachmentLocation;

/// The folder an attachment for `note` belongs in.
///
/// `note` is `None` when there is no note in context — a share extension
/// handing over a file, an import with nothing open. The note-relative modes
/// then fall back to the vault root, which is the only honest answer: putting
/// it "beside the note" when there is no note would be inventing one.
pub fn target_folder(location: &AttachmentLocation, note: Option<&VaultPath>) -> Result<VaultPath> {
    Ok(match location {
        AttachmentLocation::VaultFolder { folder } => folder.clone(),
        AttachmentLocation::NextToNote => note.map(VaultPath::parent).unwrap_or_default(),
        AttachmentLocation::SubfolderOfNote { name } => {
            let parent = note.map(VaultPath::parent).unwrap_or_default();
            parent.join(name)?
        }
    })
}

/// The full path a file called `file_name` should be written to.
///
/// The name is sanitised, so a file arriving from a share sheet with a colon
/// or a trailing dot in its name cannot produce a path that is legal here and
/// illegal on Windows. It does not resolve collisions — that is
/// `FileOps::create_note` with `Collision::Rename`, which knows what is
/// already there.
pub fn target_path(
    location: &AttachmentLocation,
    note: Option<&VaultPath>,
    file_name: &str,
) -> Result<VaultPath> {
    target_folder(location, note)?.join(&sanitize_segment(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> VaultPath {
        VaultPath::parse(text).unwrap()
    }

    fn vault_folder() -> AttachmentLocation {
        AttachmentLocation::VaultFolder {
            folder: path("Attachments"),
        }
    }

    #[test]
    fn a_vault_folder_ignores_which_note_is_open() {
        let note = path("Projects/Deep/Note.md");
        assert_eq!(
            target_path(&vault_folder(), Some(&note), "diagram.png").unwrap(),
            path("Attachments/diagram.png")
        );
        assert_eq!(
            target_path(&vault_folder(), None, "diagram.png").unwrap(),
            path("Attachments/diagram.png")
        );
    }

    #[test]
    fn next_to_the_note_means_the_notes_own_folder() {
        let note = path("Projects/Deep/Note.md");
        assert_eq!(
            target_path(&AttachmentLocation::NextToNote, Some(&note), "diagram.png").unwrap(),
            path("Projects/Deep/diagram.png")
        );
    }

    #[test]
    fn a_subfolder_hangs_off_the_notes_folder() {
        let note = path("Projects/Deep/Note.md");
        let location = AttachmentLocation::SubfolderOfNote {
            name: "assets".into(),
        };
        assert_eq!(
            target_path(&location, Some(&note), "diagram.png").unwrap(),
            path("Projects/Deep/assets/diagram.png")
        );
    }

    #[test]
    fn with_no_note_the_note_relative_modes_fall_back_to_the_root() {
        // A share extension has no note open. "Beside the note" cannot mean
        // anything, and inventing a note would be worse than the root.
        assert_eq!(
            target_path(&AttachmentLocation::NextToNote, None, "shared.png").unwrap(),
            path("shared.png")
        );
        let location = AttachmentLocation::SubfolderOfNote {
            name: "assets".into(),
        };
        assert_eq!(
            target_path(&location, None, "shared.png").unwrap(),
            path("assets/shared.png")
        );
    }

    #[test]
    fn a_name_that_windows_would_reject_is_made_safe() {
        // A screenshot from iOS can be called "Screenshot 2026-09-17 at 10:30.png".
        // The colon is legal on APFS and illegal on NTFS, so a vault created on
        // a phone would not open on a desktop.
        let target = target_path(&vault_folder(), None, "Shot 2026-09-17 at 10:30.png").unwrap();
        assert!(!target.as_str().contains(':'), "{target:?}");
        assert!(target.as_str().starts_with("Attachments/"));
    }

    #[test]
    fn a_name_that_would_escape_the_vault_cannot() {
        // The separators are what makes a traversal, not the dots. Sanitising
        // turns the whole thing into one filename, so it lands in the
        // attachments folder with an odd name rather than anywhere else.
        let target = target_path(&vault_folder(), None, "../../etc/passwd").unwrap();
        assert_eq!(target.parent(), path("Attachments"));
        assert_eq!(
            target.segments().count(),
            2,
            "{target:?} is not one file in one folder"
        );
        assert!(!target.file_name().contains('/'), "{target:?}");
    }

    #[test]
    fn a_windows_style_traversal_cannot_escape_either() {
        let target = target_path(&vault_folder(), None, r"..\..\Windows\System32\x.dll").unwrap();
        assert_eq!(target.parent(), path("Attachments"));
        assert_eq!(target.segments().count(), 2, "{target:?}");
    }

    #[test]
    fn a_note_at_the_root_puts_its_attachments_at_the_root() {
        let note = path("Note.md");
        assert_eq!(
            target_path(&AttachmentLocation::NextToNote, Some(&note), "a.png").unwrap(),
            path("a.png")
        );
    }
}

//! Per-vault settings.
//!
//! These live inside the vault at `.inner-empire/vault.json` rather than in
//! the OS config directory, because they describe *this vault* — where its
//! attachments go, what its daily-note format is — and must travel with it
//! when the folder is copied to another machine. Application-wide preferences
//! (theme, hotkeys, recent vaults) live in the platform config directory
//! instead.

use crate::vault::path::VaultPath;

pub const VAULT_SETTINGS_FILE: &str = "vault.json";
pub const VAULT_SETTINGS_VERSION: u32 = 1;

/// Where a new attachment goes when it is pasted or dropped into a note.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum AttachmentLocation {
    /// One folder for the whole vault.
    #[serde(rename_all = "camelCase")]
    VaultFolder { folder: VaultPath },
    /// Beside the note that references it.
    NextToNote,
    /// In a subfolder of the note's own folder.
    #[serde(rename_all = "camelCase")]
    SubfolderOfNote { name: String },
}

impl Default for AttachmentLocation {
    fn default() -> Self {
        AttachmentLocation::VaultFolder {
            folder: VaultPath::parse("Attachments").expect("a literal, valid path"),
        }
    }
}

/// How a link is written when the app inserts one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkStyle {
    /// `[[Note]]` — shortest form that resolves unambiguously.
    #[default]
    ShortestWikiLink,
    /// `[[Folder/Note]]` — always the full path, which never becomes
    /// ambiguous as the vault grows.
    AbsoluteWikiLink,
    /// `[Note](Folder/Note.md)` — portable to editors that do not know about
    /// wiki links.
    MarkdownLink,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyNoteSettings {
    pub folder: VaultPath,
    /// `strftime`-like pattern using the subset in `templates::datefmt`.
    pub format: String,
    pub template: Option<VaultPath>,
    /// Open today's note when the vault opens.
    pub open_on_startup: bool,
}

impl Default for DailyNoteSettings {
    fn default() -> Self {
        Self {
            folder: VaultPath::parse("Daily").expect("a literal, valid path"),
            format: "YYYY-MM-DD".into(),
            template: None,
            open_on_startup: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSettings {
    pub version: u32,
    /// Stable identifier, used to key recovery journals and recent-vault
    /// entries so they survive the folder being moved or renamed.
    pub id: String,
    pub name: String,
    pub created_ms: i64,
    pub attachments: AttachmentLocation,
    pub templates_folder: VaultPath,
    pub daily_notes: DailyNoteSettings,
    pub link_style: LinkStyle,
    /// Update links in other notes when a file is renamed or moved.
    pub update_links_on_rename: bool,
    /// Folder names the indexer skips, beyond the built-in list.
    #[serde(default)]
    pub extra_ignored_folders: Vec<String>,
    /// New notes are created here when no folder is selected.
    #[serde(default)]
    pub new_note_folder: Option<VaultPath>,
}

impl VaultSettings {
    pub fn new(name: impl Into<String>, id: impl Into<String>, created_ms: i64) -> Self {
        Self {
            version: VAULT_SETTINGS_VERSION,
            id: id.into(),
            name: name.into(),
            created_ms,
            attachments: AttachmentLocation::default(),
            templates_folder: VaultPath::parse("Templates").expect("a literal, valid path"),
            daily_notes: DailyNoteSettings::default(),
            link_style: LinkStyle::default(),
            update_links_on_rename: true,
            extra_ignored_folders: Vec::new(),
            new_note_folder: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_json_with_portable_paths() {
        let settings = VaultSettings::new("My Vault", "01J", 1_700_000_000_000);
        let json = serde_json::to_string_pretty(&settings).unwrap();
        assert!(!json.contains('\\'), "a backslash would not survive a move to Linux");

        let restored: VaultSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn a_settings_file_from_an_older_build_still_loads() {
        // Fields added later must have defaults, or opening an existing vault
        // after an upgrade would fail.
        let minimal = r#"{
            "version": 1, "id": "abc", "name": "V", "createdMs": 0,
            "attachments": {"mode": "nextToNote"},
            "templatesFolder": "Templates",
            "dailyNotes": {"folder": "Daily", "format": "YYYY-MM-DD", "template": null, "openOnStartup": false},
            "linkStyle": "shortestWikiLink",
            "updateLinksOnRename": true
        }"#;
        let settings: VaultSettings = serde_json::from_str(minimal).unwrap();
        assert!(settings.extra_ignored_folders.is_empty());
        assert_eq!(settings.new_note_folder, None);
    }

    #[test]
    fn attachment_locations_serialise_as_a_tagged_union() {
        let json = serde_json::to_string(&AttachmentLocation::SubfolderOfNote {
            name: "media".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"mode":"subfolderOfNote","name":"media"}"#);
    }
}

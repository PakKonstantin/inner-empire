//! Loading and saving the workspace.
//!
//! The file lives inside the vault, so reopening a vault on another machine
//! restores the same tabs and panes. It contains only `VaultPath` values, and a
//! test asserts that no backslash ever reaches it.

use ie_platform::SharedFileSystem;

use crate::error::Result;
use crate::model::workspace::{Workspace, WORKSPACE_VERSION};
use crate::vault::path::{VaultPath, APP_DIR};

pub const WORKSPACE_FILE: &str = "workspace.json";
/// Named workspaces the user saved, so a "writing" layout and a "review"
/// layout can coexist.
pub const SAVED_DIR: &str = "workspaces";

pub struct WorkspaceStore {
    fs: SharedFileSystem,
    root: std::path::PathBuf,
}

impl WorkspaceStore {
    pub fn new(fs: SharedFileSystem, root: &std::path::Path) -> Self {
        Self {
            fs,
            root: root.to_path_buf(),
        }
    }

    fn current_path(&self) -> std::path::PathBuf {
        self.root.join(APP_DIR).join(WORKSPACE_FILE)
    }

    fn saved_path(&self, name: &str) -> Result<std::path::PathBuf> {
        let safe = crate::vault::path::sanitize_segment(name);
        Ok(self
            .root
            .join(APP_DIR)
            .join(SAVED_DIR)
            .join(format!("{safe}.json")))
    }

    /// Load the active workspace.
    ///
    /// A missing or unreadable file yields the default layout rather than an
    /// error: losing a saved layout is a nuisance, refusing to open the vault
    /// because of it would be a fault.
    pub fn load(&self) -> Workspace {
        let path = self.current_path();
        if !self.fs.exists(&path) {
            return Workspace::default();
        }
        match self
            .fs
            .read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Workspace>(&text).ok())
        {
            Some(workspace) if workspace.version == WORKSPACE_VERSION => workspace,
            Some(_) => {
                tracing::info!("workspace file is from another version; starting fresh");
                Workspace::default()
            }
            None => {
                tracing::warn!(path = %path.display(), "workspace file unreadable; starting fresh");
                Workspace::default()
            }
        }
    }

    pub fn save(&self, workspace: &Workspace) -> Result<()> {
        let path = self.current_path();
        if let Some(parent) = path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(workspace)?;
        self.fs.write_atomic(&path, json.as_bytes())?;
        Ok(())
    }

    pub fn save_as(&self, name: &str, workspace: &Workspace) -> Result<()> {
        let path = self.saved_path(name)?;
        if let Some(parent) = path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(workspace)?;
        self.fs.write_atomic(&path, json.as_bytes())?;
        Ok(())
    }

    pub fn load_saved(&self, name: &str) -> Result<Workspace> {
        let path = self.saved_path(name)?;
        let text = self.fs.read_to_string(&path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn list_saved(&self) -> Result<Vec<String>> {
        let dir = self.root.join(APP_DIR).join(SAVED_DIR);
        if !self.fs.exists(&dir) {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = self
            .fs
            .read_dir(&dir)?
            .into_iter()
            .filter(|entry| !entry.metadata.is_dir && entry.file_name.ends_with(".json"))
            .map(|entry| entry.file_name.trim_end_matches(".json").to_string())
            .collect();
        names.sort();
        Ok(names)
    }

    pub fn delete_saved(&self, name: &str) -> Result<()> {
        let path = self.saved_path(name)?;
        if self.fs.exists(&path) {
            self.fs.remove_file(&path)?;
        }
        Ok(())
    }

    /// Reset to the default layout and persist it.
    pub fn reset(&self) -> Result<Workspace> {
        let workspace = Workspace::default();
        self.save(&workspace)?;
        Ok(workspace)
    }

    /// Load and drop tabs whose files are gone.
    pub fn load_reconciled<F: Fn(&VaultPath) -> bool + Copy>(
        &self,
        exists: F,
    ) -> (Workspace, Vec<VaultPath>) {
        let mut workspace = self.load();
        let removed = workspace.reconcile(exists);
        (workspace, removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{PaneLayout, PaneNode, TabMode, TabState};
    use ie_platform::MemoryFileSystem;
    use std::sync::Arc;

    fn store() -> (WorkspaceStore, SharedFileSystem, std::path::PathBuf) {
        let fs: SharedFileSystem = Arc::new(MemoryFileSystem::default());
        let root = std::path::PathBuf::from("/vault");
        fs.create_dir_all(&root).unwrap();
        (WorkspaceStore::new(Arc::clone(&fs), &root), fs, root)
    }

    fn workspace_with(paths: &[&str]) -> Workspace {
        Workspace {
            layout: PaneLayout {
                root: PaneNode::Leaf {
                    id: "main".into(),
                    tabs: paths
                        .iter()
                        .enumerate()
                        .map(|(i, p)| TabState {
                            id: format!("tab-{i}"),
                            path: VaultPath::parse(p).unwrap(),
                            pinned: i == 0,
                            mode: TabMode::Edit,
                            scroll_line: 12,
                            cursor_offset: 42,
                        })
                        .collect(),
                    active_tab_id: Some("tab-0".into()),
                },
                active_pane_id: "main".into(),
            },
            active_file: paths.first().map(|p| VaultPath::parse(p).unwrap()),
            ..Workspace::default()
        }
    }

    #[test]
    fn a_missing_workspace_file_yields_the_default_layout() {
        let (store, _, _) = store();
        assert_eq!(store.load(), Workspace::default());
    }

    #[test]
    fn saving_and_loading_round_trips_every_field() {
        let (store, _, _) = store();
        let original = workspace_with(&["Projects/Plan.md", "Notes/Idea.md"]);
        store.save(&original).unwrap();
        assert_eq!(store.load(), original);
    }

    #[test]
    fn the_saved_file_contains_only_portable_paths() {
        let (store, fs, root) = store();
        store
            .save(&workspace_with(&["Projects/2026/Plan.md"]))
            .unwrap();

        let text = fs
            .read_to_string(&root.join(APP_DIR).join(WORKSPACE_FILE))
            .unwrap();
        assert!(text.contains("Projects/2026/Plan.md"));
        assert!(
            !text.contains('\\'),
            "a backslash would not survive a move to Linux"
        );
    }

    #[test]
    fn an_unreadable_workspace_file_does_not_stop_the_vault_opening() {
        let (store, fs, root) = store();
        fs.create_dir_all(&root.join(APP_DIR)).unwrap();
        fs.write_atomic(&root.join(APP_DIR).join(WORKSPACE_FILE), b"{ truncated")
            .unwrap();
        assert_eq!(store.load(), Workspace::default());
    }

    #[test]
    fn a_workspace_from_a_future_version_is_ignored_rather_than_misread() {
        let (store, fs, root) = store();
        let mut future = workspace_with(&["A.md"]);
        future.version = 999;
        fs.create_dir_all(&root.join(APP_DIR)).unwrap();
        fs.write_atomic(
            &root.join(APP_DIR).join(WORKSPACE_FILE),
            serde_json::to_string(&future).unwrap().as_bytes(),
        )
        .unwrap();

        assert_eq!(store.load(), Workspace::default());
    }

    #[test]
    fn loading_drops_tabs_whose_files_have_gone() {
        let (store, _, _) = store();
        store
            .save(&workspace_with(&["Kept.md", "Deleted.md"]))
            .unwrap();

        let (workspace, removed) = store.load_reconciled(|p| p.as_str() == "Kept.md");

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].as_str(), "Deleted.md");
        assert_eq!(workspace.layout.root.open_paths().len(), 1);
    }

    #[test]
    fn named_workspaces_can_be_saved_listed_loaded_and_deleted() {
        let (store, _, _) = store();
        store
            .save_as("Writing", &workspace_with(&["A.md"]))
            .unwrap();
        store.save_as("Review", &workspace_with(&["B.md"])).unwrap();

        assert_eq!(store.list_saved().unwrap(), vec!["Review", "Writing"]);
        assert_eq!(
            store
                .load_saved("Writing")
                .unwrap()
                .layout
                .root
                .open_paths()[0]
                .as_str(),
            "A.md"
        );

        store.delete_saved("Writing").unwrap();
        assert_eq!(store.list_saved().unwrap(), vec!["Review"]);
    }

    #[test]
    fn a_workspace_name_with_illegal_characters_is_still_usable() {
        let (store, _, _) = store();
        store
            .save_as("Q1: Review?", &workspace_with(&["A.md"]))
            .unwrap();
        assert_eq!(store.list_saved().unwrap(), vec!["Q1- Review-"]);
    }

    #[test]
    fn resetting_replaces_the_stored_layout_with_the_default() {
        let (store, _, _) = store();
        store.save(&workspace_with(&["A.md", "B.md"])).unwrap();
        let reset = store.reset().unwrap();
        assert_eq!(reset, Workspace::default());
        assert_eq!(store.load(), Workspace::default());
    }

    #[test]
    fn deleting_a_workspace_that_is_not_there_is_not_an_error() {
        let (store, _, _) = store();
        assert!(store.delete_saved("never existed").is_ok());
    }
}

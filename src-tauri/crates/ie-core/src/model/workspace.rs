//! Workspace persistence.
//!
//! Everything here is stored as JSON inside the vault, at
//! `.inner-empire/workspace.json`, and contains only `VaultPath` values. A
//! workspace file therefore survives being copied between a Windows and a
//! Linux machine, which a layout referencing absolute paths would not.

use crate::vault::path::VaultPath;

/// One open document in a pane.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabState {
    pub id: String,
    pub path: VaultPath,
    /// Pinned tabs survive "close others" and sort ahead of unpinned ones.
    #[serde(default)]
    pub pinned: bool,
    /// Which view the tab shows: the editor, the reading view, the graph, …
    #[serde(default)]
    pub mode: TabMode,
    /// Scroll offset in lines, restored on reopen.
    #[serde(default)]
    pub scroll_line: usize,
    /// Cursor offset in the document, restored on reopen.
    #[serde(default)]
    pub cursor_offset: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TabMode {
    #[default]
    Edit,
    Read,
    Canvas,
    Graph,
    Pdf,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// The pane tree.
///
/// A binary split tree rather than a flat list, because that is what makes
/// arbitrary nesting (`split a pane that is itself a split`) representable
/// without special cases, and what lets a resize affect exactly two children.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PaneNode {
    /// A leaf holding tabs.
    #[serde(rename_all = "camelCase")]
    Leaf {
        id: String,
        tabs: Vec<TabState>,
        active_tab_id: Option<String>,
    },
    /// Two children side by side or stacked.
    #[serde(rename_all = "camelCase")]
    Split {
        id: String,
        direction: SplitDirection,
        /// Fraction of the available space given to the first child, 0.1–0.9.
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    pub fn id(&self) -> &str {
        match self {
            PaneNode::Leaf { id, .. } | PaneNode::Split { id, .. } => id,
        }
    }

    /// Every leaf, left to right, depth first.
    pub fn leaves(&self) -> Vec<&PaneNode> {
        match self {
            PaneNode::Leaf { .. } => vec![self],
            PaneNode::Split { first, second, .. } => {
                let mut out = first.leaves();
                out.extend(second.leaves());
                out
            }
        }
    }

    /// Every path open anywhere in the tree, in tab order.
    pub fn open_paths(&self) -> Vec<VaultPath> {
        self.leaves()
            .into_iter()
            .filter_map(|leaf| match leaf {
                PaneNode::Leaf { tabs, .. } => Some(tabs.iter().map(|t| t.path.clone())),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Drop tabs whose file no longer exists.
    ///
    /// Called when a workspace is restored: a note deleted outside the app
    /// should not resurrect as a broken tab. Returns the paths removed so the
    /// UI can mention them.
    pub fn prune<F: Fn(&VaultPath) -> bool + Copy>(&mut self, exists: F) -> Vec<VaultPath> {
        match self {
            PaneNode::Leaf {
                tabs, active_tab_id, ..
            } => {
                let mut removed = Vec::new();
                tabs.retain(|tab| {
                    if exists(&tab.path) {
                        true
                    } else {
                        removed.push(tab.path.clone());
                        false
                    }
                });
                let active_still_open = active_tab_id
                    .as_ref()
                    .is_some_and(|id| tabs.iter().any(|t| &t.id == id));
                if !active_still_open {
                    *active_tab_id = tabs.first().map(|t| t.id.clone());
                }
                removed
            }
            PaneNode::Split { first, second, .. } => {
                let mut removed = first.prune(exists);
                removed.extend(second.prune(exists));
                removed
            }
        }
    }

    /// Collapse splits whose leaves are all empty, so closing the last tab in
    /// a split does not leave a dead region on screen.
    pub fn collapse_empty(self) -> Option<PaneNode> {
        match self {
            PaneNode::Leaf { ref tabs, .. } if tabs.is_empty() => None,
            leaf @ PaneNode::Leaf { .. } => Some(leaf),
            PaneNode::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => match (first.collapse_empty(), second.collapse_empty()) {
                (Some(a), Some(b)) => Some(PaneNode::Split {
                    id,
                    direction,
                    ratio,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSidebar {
    pub visible: bool,
    /// Pixels. Stored rather than derived so the layout is stable across
    /// window sizes.
    pub width: f32,
    /// Which panel is showing, e.g. `files`, `search`, `tags`, `backlinks`.
    pub active_panel: String,
}

impl Default for WorkspaceSidebar {
    fn default() -> Self {
        Self {
            visible: true,
            width: 260.0,
            active_panel: "files".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneLayout {
    pub root: PaneNode,
    pub active_pane_id: String,
}

/// The persisted workspace.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    /// Bumped when the shape changes so an old file can be migrated rather
    /// than misread.
    pub version: u32,
    pub layout: PaneLayout,
    pub left_sidebar: WorkspaceSidebar,
    pub right_sidebar: WorkspaceSidebar,
    pub active_file: Option<VaultPath>,
    /// Opaque graph view state (zoom, pan, filters), owned by the frontend.
    #[serde(default)]
    pub graph_state: Option<serde_json::Value>,
}

pub const WORKSPACE_VERSION: u32 = 1;

impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: WORKSPACE_VERSION,
            layout: PaneLayout {
                root: PaneNode::Leaf {
                    id: "pane-root".into(),
                    tabs: Vec::new(),
                    active_tab_id: None,
                },
                active_pane_id: "pane-root".into(),
            },
            left_sidebar: WorkspaceSidebar::default(),
            right_sidebar: WorkspaceSidebar {
                visible: true,
                width: 300.0,
                active_panel: "backlinks".into(),
            },
            active_file: None,
            graph_state: None,
        }
    }
}

impl Workspace {
    /// Remove tabs for files that no longer exist and collapse the resulting
    /// empty panes. Returns the paths that were dropped.
    pub fn reconcile<F: Fn(&VaultPath) -> bool + Copy>(&mut self, exists: F) -> Vec<VaultPath> {
        let removed = self.layout.root.prune(exists);

        let root = std::mem::replace(
            &mut self.layout.root,
            PaneNode::Leaf {
                id: "pane-root".into(),
                tabs: Vec::new(),
                active_tab_id: None,
            },
        );
        if let Some(collapsed) = root.collapse_empty() {
            self.layout.root = collapsed;
        }
        // The active pane may have just been collapsed away.
        let live_ids: Vec<String> = self
            .layout
            .root
            .leaves()
            .iter()
            .map(|p| p.id().to_string())
            .collect();
        if !live_ids.contains(&self.layout.active_pane_id) {
            if let Some(first) = live_ids.first() {
                self.layout.active_pane_id = first.clone();
            }
        }
        if self
            .active_file
            .as_ref()
            .is_some_and(|path| !exists(path))
        {
            self.active_file = None;
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: &str, paths: &[&str]) -> PaneNode {
        PaneNode::Leaf {
            id: id.into(),
            tabs: paths
                .iter()
                .enumerate()
                .map(|(i, p)| TabState {
                    id: format!("{id}-tab-{i}"),
                    path: VaultPath::parse(p).unwrap(),
                    pinned: false,
                    mode: TabMode::Edit,
                    scroll_line: 0,
                    cursor_offset: 0,
                })
                .collect(),
            active_tab_id: Some(format!("{id}-tab-0")),
        }
    }

    #[test]
    fn a_split_tree_reports_its_leaves_left_to_right() {
        let tree = PaneNode::Split {
            id: "s".into(),
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(leaf("a", &["a.md"])),
            second: Box::new(PaneNode::Split {
                id: "s2".into(),
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(leaf("b", &["b.md"])),
                second: Box::new(leaf("c", &["c.md"])),
            }),
        };
        let ids: Vec<_> = tree.leaves().iter().map(|l| l.id().to_string()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
        assert_eq!(tree.open_paths().len(), 3);
    }

    #[test]
    fn restoring_a_workspace_drops_tabs_whose_files_are_gone() {
        let mut workspace = Workspace {
            layout: PaneLayout {
                root: leaf("a", &["kept.md", "deleted.md"]),
                active_pane_id: "a".into(),
            },
            ..Workspace::default()
        };

        let removed = workspace.reconcile(|p| p.as_str() == "kept.md");
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].as_str(), "deleted.md");
        assert_eq!(workspace.layout.root.open_paths().len(), 1);
    }

    #[test]
    fn an_emptied_split_collapses_into_its_surviving_side() {
        let mut workspace = Workspace {
            layout: PaneLayout {
                root: PaneNode::Split {
                    id: "s".into(),
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                    first: Box::new(leaf("a", &["gone.md"])),
                    second: Box::new(leaf("b", &["kept.md"])),
                },
                active_pane_id: "a".into(),
            },
            ..Workspace::default()
        };

        workspace.reconcile(|p| p.as_str() == "kept.md");
        assert!(matches!(workspace.layout.root, PaneNode::Leaf { .. }));
        assert_eq!(workspace.layout.root.id(), "b");
        assert_eq!(
            workspace.layout.active_pane_id, "b",
            "the active pane must follow when its own pane is collapsed"
        );
    }

    #[test]
    fn the_active_tab_moves_on_when_its_file_disappears() {
        let mut pane = leaf("a", &["gone.md", "kept.md"]);
        pane.prune(|p| p.as_str() == "kept.md");
        match pane {
            PaneNode::Leaf { active_tab_id, tabs, .. } => {
                assert_eq!(tabs.len(), 1);
                assert_eq!(active_tab_id, Some(tabs[0].id.clone()));
            }
            _ => panic!("expected a leaf"),
        }
    }

    #[test]
    fn a_workspace_serialises_with_only_relative_forward_slash_paths() {
        let workspace = Workspace {
            layout: PaneLayout {
                root: leaf("a", &["Projects/2026/plan.md"]),
                active_pane_id: "a".into(),
            },
            ..Workspace::default()
        };
        let json = serde_json::to_string(&workspace).unwrap();
        assert!(json.contains("Projects/2026/plan.md"));
        assert!(!json.contains('\\'), "a backslash would not survive the trip to Linux");

        let restored: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, workspace);
    }
}

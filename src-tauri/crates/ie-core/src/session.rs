//! A open vault, and everything that hangs off it.
//!
//! `VaultSession` is the application's unit of work: it owns the vault folder,
//! its index, its file operations, its trash and its watcher, and it is where
//! the operations that need several of those at once live — saving a note and
//! re-indexing it, renaming a note and rewriting every link to it.
//!
//! It knows nothing about Tauri, React or IPC. That is what lets the same type
//! serve a future command-line tool or headless indexer.

use std::path::{Path, PathBuf};

use ie_platform::{FsEvent, HostServices, WatchHandle, WatchOptions};

use crate::error::{CoreError, Diagnostic, Result};
use crate::index::{
    indexer::EventOutcome, IndexDb, IndexProgress, Indexer, OpenOutcome, ScanReport,
};
use crate::links::rename::{self, RenamePlan};
use crate::markdown::{MarkdownParser, MarkdownTransformer};
use crate::model::{Note, Property};
use crate::vault::settings::{VaultSettings, VAULT_SETTINGS_FILE};
use crate::vault::{Collision, FileOps, Trash, VaultPath, APP_DIR};
use crate::workspace::WorkspaceStore;

/// How a vault was opened, so the UI can explain a long first scan.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenReport {
    pub root: String,
    pub name: String,
    pub index: OpenOutcome,
    /// True when the vault folder had no `.inner-empire` directory.
    pub created: bool,
    pub case_sensitive: bool,
}

pub struct VaultSession {
    root: PathBuf,
    settings: VaultSettings,
    host: HostServices,
    ops: FileOps,
    trash: Trash,
    workspaces: WorkspaceStore,
    db: IndexDb,
    indexer: Indexer,
    parser: MarkdownParser,
    transformer: MarkdownTransformer,
    watch: Option<Box<dyn WatchHandle>>,
    case_sensitive: bool,
}

impl VaultSession {
    /// Open a folder as a vault, creating the app's own directory if absent.
    ///
    /// Any folder is a valid vault. There is no import step and no proprietary
    /// container: the user points at a directory of Markdown files and it
    /// works, which is the whole premise.
    pub fn open(host: HostServices, root: &Path) -> Result<(Self, OpenReport)> {
        let root = host
            .fs
            .canonicalize(root)
            .unwrap_or_else(|_| root.to_path_buf());

        let app_dir = root.join(APP_DIR);
        let created = !host.fs.exists(&app_dir);
        host.fs.create_dir_all(&app_dir)?;

        let ops = FileOps::new(std::sync::Arc::clone(&host.fs), &root);
        let settings = Self::load_or_create_settings(&host, &root)?;

        let (db, index_outcome) = IndexDb::open(&app_dir.join("index.db"))?;
        db.set_meta("vault_id", &settings.id)?;

        let case_sensitive = host.fs.is_case_sensitive(&app_dir).unwrap_or(true);

        let mut ignore = crate::index::IgnoreRules::default();
        ignore
            .folder_names
            .extend(settings.extra_ignored_folders.iter().cloned());

        let session = Self {
            trash: Trash::new(std::sync::Arc::clone(&host.fs), &root),
            workspaces: WorkspaceStore::new(std::sync::Arc::clone(&host.fs), &root),
            indexer: Indexer::new(
                std::sync::Arc::clone(&host.fs),
                std::sync::Arc::clone(&host.clock),
            )
            .with_ignore_rules(ignore),
            parser: MarkdownParser::new(),
            transformer: MarkdownTransformer::new(),
            watch: None,
            db,
            ops,
            case_sensitive,
            host,
            root: root.clone(),
            settings: settings.clone(),
        };

        let report = OpenReport {
            root: root.to_string_lossy().to_string(),
            name: settings.name,
            index: index_outcome,
            created,
            case_sensitive,
        };
        Ok((session, report))
    }

    /// Create a new vault folder with the conventional subfolders and a
    /// welcome note, then open it.
    pub fn create(host: HostServices, root: &Path, name: &str) -> Result<(Self, OpenReport)> {
        host.fs.create_dir_all(root)?;
        let (session, report) = Self::open(host, root)?;

        for folder in ["Notes", "Projects", "Attachments", "Templates", "Daily"] {
            session.ops.create_folder(&VaultPath::parse(folder)?)?;
        }

        let welcome = VaultPath::parse("Notes/Welcome.md")?;
        if !session.ops.exists(&welcome) {
            session.ops.create_note(
                &welcome,
                &format!(
                    "---\ntitle: Welcome\ntags:\n  - getting-started\n---\n\n\
                     # Welcome to {name}\n\n\
                     This vault is a plain folder of Markdown files. Everything you write \
                     stays on your machine, in a format any editor can open.\n\n\
                     ## Try these\n\n\
                     - Link to another note with double brackets, like [[Notes/Welcome]].\n\
                     - Tag a note by writing #getting-started anywhere in it.\n\
                     - Press Ctrl+P for the command palette, Ctrl+O to jump to a note.\n\
                     - Drop an image into a note to file it under Attachments.\n\n\
                     ## Where things live\n\n\
                     Your notes are yours. The `.inner-empire` folder holds a search index \
                     that can be deleted at any time; it rebuilds itself from these files. ^index-is-a-cache\n"
                ),
                Collision::Fail,
            )?;
        }

        Ok((session, report))
    }

    fn load_or_create_settings(host: &HostServices, root: &Path) -> Result<VaultSettings> {
        let path = root.join(APP_DIR).join(VAULT_SETTINGS_FILE);
        if host.fs.exists(&path) {
            if let Ok(text) = host.fs.read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str::<VaultSettings>(&text) {
                    return Ok(settings);
                }
                tracing::warn!("vault settings unreadable; recreating with defaults");
            }
        }

        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Vault".to_string());
        let settings =
            VaultSettings::new(name, uuid::Uuid::now_v7().to_string(), host.clock.now_ms());
        let json = serde_json::to_string_pretty(&settings)?;
        host.fs.write_atomic(&path, json.as_bytes())?;
        Ok(settings)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> &VaultSettings {
        &self.settings
    }

    pub fn ops(&self) -> &FileOps {
        &self.ops
    }

    pub fn trash(&self) -> &Trash {
        &self.trash
    }

    pub fn workspaces(&self) -> &WorkspaceStore {
        &self.workspaces
    }

    pub fn db(&self) -> &IndexDb {
        &self.db
    }

    pub fn connection(&self) -> &rusqlite::Connection {
        self.db.connection()
    }

    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub fn save_settings(&mut self, settings: VaultSettings) -> Result<()> {
        let path = self.root.join(APP_DIR).join(VAULT_SETTINGS_FILE);
        let json = serde_json::to_string_pretty(&settings)?;
        self.host.fs.write_atomic(&path, json.as_bytes())?;
        self.settings = settings;
        Ok(())
    }

    /// Index the whole vault.
    pub fn scan<F: FnMut(IndexProgress)>(&mut self, progress: F) -> Result<ScanReport> {
        let root = self.root.clone();
        let indexer = std::mem::replace(
            &mut self.indexer,
            Indexer::new(
                std::sync::Arc::clone(&self.host.fs),
                std::sync::Arc::clone(&self.host.clock),
            ),
        );
        let report = indexer.full_scan(&mut self.db, &root, progress);
        self.indexer = indexer;
        report
    }

    /// Start watching the vault. Batches are handed to `sink` on the watcher's
    /// own thread; the caller is expected to forward them to whatever owns this
    /// session and call [`apply_events`](Self::apply_events).
    pub fn start_watch(&mut self, sink: Box<dyn Fn(Vec<FsEvent>) + Send + 'static>) -> Result<()> {
        self.stop_watch();
        let handle = self
            .host
            .watcher
            .watch(self.root.clone(), WatchOptions::default(), sink)?;
        self.watch = Some(handle);
        Ok(())
    }

    pub fn stop_watch(&mut self) {
        if let Some(handle) = self.watch.take() {
            handle.stop();
        }
    }

    pub fn apply_events(&mut self, events: &[FsEvent]) -> Result<EventOutcome> {
        let root = self.root.clone();
        let indexer = std::mem::replace(
            &mut self.indexer,
            Indexer::new(
                std::sync::Arc::clone(&self.host.fs),
                std::sync::Arc::clone(&self.host.clock),
            ),
        );
        let outcome = indexer.apply_events(&mut self.db, &root, events);
        self.indexer = indexer;
        outcome
    }

    /// Read a note along with its parsed metadata.
    pub fn read_note(&self, path: &VaultPath) -> Result<Note> {
        let content = self.ops.read(path)?;
        let parsed = self.parser.parse(&content);
        let modified_ms = self
            .host
            .fs
            .metadata(&self.ops.resolve(path))
            .ok()
            .and_then(|m| m.modified_ms)
            .unwrap_or(0);

        Ok(Note {
            title: parsed
                .metadata
                .title
                .clone()
                .unwrap_or_else(|| path.stem().to_string()),
            path: path.clone(),
            content,
            metadata: parsed.metadata,
            modified_ms,
        })
    }

    /// Write a note and bring the index up to date immediately.
    ///
    /// Not waiting for the watcher matters: the user expects backlinks and the
    /// outline to reflect what they just typed, and the watcher's debounce
    /// would make that feel laggy.
    pub fn save_note(&mut self, path: &VaultPath, content: &str) -> Result<()> {
        self.ops.write(path, content)?;
        self.reindex(path)
    }

    pub fn reindex(&mut self, path: &VaultPath) -> Result<()> {
        let root = self.root.clone();
        let indexer = std::mem::replace(
            &mut self.indexer,
            Indexer::new(
                std::sync::Arc::clone(&self.host.fs),
                std::sync::Arc::clone(&self.host.clock),
            ),
        );
        let result = indexer.index_file(&mut self.db, &root, path);
        self.indexer = indexer;
        result
    }

    /// Create a note, optionally from a template.
    pub fn create_note(
        &mut self,
        path: &VaultPath,
        content: &str,
        collision: Collision,
    ) -> Result<VaultPath> {
        let created = self.ops.create_note(path, content, collision)?;
        self.reindex(&created)?;
        Ok(created)
    }

    /// What a rename would change, without changing it.
    pub fn plan_rename(&self, from: &VaultPath, to: &VaultPath) -> Result<RenamePlan> {
        rename::plan_rename(self.connection(), from, to)
    }

    /// Rename or move a file, rewriting every link that pointed at it.
    ///
    /// The file is moved first and the links second. If a link rewrite fails
    /// the move still stands and the failure is reported, because the
    /// alternative — rolling the move back after some links have been
    /// rewritten — would leave the vault in a state neither the user nor the
    /// index expects.
    pub fn rename(&mut self, from: &VaultPath, to: &VaultPath) -> Result<RenameOutcome> {
        if !self.ops.exists(from) {
            return Err(CoreError::NotFound(from.clone()));
        }

        // Which notes reference it, and by what text, captured before the move
        // while the index still knows.
        let plan = self.plan_rename(from, to)?;
        let referrers: Vec<(VaultPath, Vec<String>)> = plan
            .edits
            .iter()
            .map(|edit| {
                let targets = rename::targets_pointing_at(self.connection(), &edit.path, from)
                    .unwrap_or_default();
                (edit.path.clone(), targets)
            })
            .collect();

        let final_path = self.ops.move_entry(from, to, Collision::Fail)?;

        let mut outcome = RenameOutcome {
            from: from.clone(),
            to: final_path.clone(),
            files_updated: 0,
            links_updated: 0,
            failures: Vec::new(),
        };

        // Move the index row first so the new path is resolvable while links
        // are being rewritten.
        self.indexer.remove_file(&mut self.db, from)?;
        self.reindex(&final_path)?;

        if self.settings.update_links_on_rename {
            let wiki_target = rename::preferred_target_text(
                self.connection(),
                &final_path,
                self.settings.link_style,
                crate::model::LinkKind::WikiLink,
            )?;
            let markdown_target = rename::preferred_target_text(
                self.connection(),
                &final_path,
                self.settings.link_style,
                crate::model::LinkKind::Markdown,
            )?;

            for (referrer, targets) in referrers {
                if targets.is_empty() || referrer == *from {
                    continue;
                }
                match self.rewrite_links_in(&referrer, &targets, &wiki_target, &markdown_target) {
                    Ok(count) if count > 0 => {
                        outcome.files_updated += 1;
                        outcome.links_updated += count;
                    }
                    Ok(_) => {}
                    Err(e) => outcome.failures.push(format!("{referrer}: {e}")),
                }
            }
        }

        Ok(outcome)
    }

    fn rewrite_links_in(
        &mut self,
        path: &VaultPath,
        targets: &[String],
        wiki_target: &str,
        markdown_target: &str,
    ) -> Result<usize> {
        let source = self.ops.read(path)?;
        let updated = rename::rewrite_source(&source, targets, wiki_target, markdown_target)?;
        if updated == source {
            return Ok(0);
        }

        let before = self.parser.parse(&source).metadata.links.len();
        self.ops.write(path, &updated)?;
        self.reindex(path)?;

        // Count how many links actually changed, for the notification.
        let changed = self
            .parser
            .parse(&updated)
            .metadata
            .links
            .iter()
            .filter(|l| l.target == wiki_target || l.target == markdown_target)
            .count();
        Ok(changed.min(before.max(changed)))
    }

    /// Move a file to the vault's trash and drop it from the index.
    pub fn delete(&mut self, path: &VaultPath) -> Result<crate::vault::TrashEntry> {
        let entry = self.trash.trash(
            path,
            self.host.clock.now_ms(),
            uuid::Uuid::now_v7().to_string(),
        )?;
        self.indexer.remove_file(&mut self.db, path)?;
        Ok(entry)
    }

    /// Put a trashed file back and re-index it.
    pub fn restore(&mut self, id: &str) -> Result<VaultPath> {
        let path = self.trash.restore(id)?;
        self.reindex(&path)?;
        Ok(path)
    }

    /// Replace a note's frontmatter without touching its body.
    pub fn set_properties(&mut self, path: &VaultPath, properties: &[Property]) -> Result<()> {
        let source = self.ops.read(path)?;
        let updated = self.transformer.set_properties(&source, properties);
        self.save_note(path, &updated)
    }

    /// Diagnostics from the last scan.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        crate::index::queries::diagnostics(self.connection(), 500)
    }
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        self.stop_watch();
    }
}

/// What a rename actually did.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameOutcome {
    pub from: VaultPath,
    pub to: VaultPath,
    pub files_updated: usize,
    pub links_updated: usize,
    /// Files whose links could not be rewritten. The rename still happened;
    /// these are reported rather than swallowed.
    pub failures: Vec<String>,
}

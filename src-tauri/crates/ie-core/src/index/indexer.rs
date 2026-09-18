//! Scanning a vault and keeping the index in step with it.
//!
//! Two paths in, one shared body of work:
//!
//! * `full_scan` walks the vault once, when a vault opens for the first time
//!   or its index had to be thrown away.
//! * `apply_events` handles the debounced stream from the filesystem watcher,
//!   touching only the files that changed.
//!
//! Re-indexing everything on every save would be simpler and is what makes an
//! app unusable at ten thousand notes, so the incremental path is the one that
//! matters. It skips a file entirely when size and modification time are
//! unchanged, and skips *parsing* when the content hash is unchanged, because
//! editors rewrite files that did not really change.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use ie_platform::{FsEvent, SharedClock, SharedFileSystem};

use crate::error::{Diagnostic, Result};
use crate::index::db::IndexDb;
use crate::index::{writer, writer::FileRecord};
use crate::markdown::MarkdownParser;
use crate::model::FileKind;
use crate::vault::path::{VaultPath, APP_DIR};

/// What the indexer refuses to look at.
#[derive(Debug, Clone)]
pub struct IgnoreRules {
    /// Folder names skipped at any depth.
    pub folder_names: Vec<String>,
    /// Skip entries whose name begins with a dot.
    pub skip_hidden: bool,
    /// Files larger than this are recorded but never read, so a 4 GB video in
    /// the attachments folder cannot stall a scan.
    pub max_parse_bytes: u64,
}

impl Default for IgnoreRules {
    fn default() -> Self {
        Self {
            folder_names: vec![
                APP_DIR.to_string(),
                ".git".into(),
                ".obsidian".into(),
                "node_modules".into(),
                ".trash".into(),
            ],
            skip_hidden: true,
            max_parse_bytes: 8 * 1024 * 1024,
        }
    }
}

impl IgnoreRules {
    pub fn ignores(&self, name: &str) -> bool {
        if self.folder_names.iter().any(|f| f == name) {
            return true;
        }
        if self.skip_hidden && name.starts_with('.') {
            return true;
        }
        // Leftovers from an interrupted atomic write are reported as a
        // diagnostic, not indexed as notes.
        name.starts_with(".ie-tmp-")
    }
}

/// Progress during a long scan, so the UI can show something honest.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub scanned: usize,
    pub indexed: usize,
    /// `None` until the walk has finished counting.
    pub total: Option<usize>,
    pub current: Option<VaultPath>,
}

/// The outcome of a scan.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub files_seen: usize,
    pub files_indexed: usize,
    /// Unchanged since the last scan, so not re-parsed.
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub duration_ms: u64,
    pub diagnostics: Vec<Diagnostic>,
}

/// How many files go into one transaction during a full scan.
///
/// One transaction for the whole vault would be fastest and would also mean a
/// failure at file 40,000 discards everything; one per file is safe and slow.
/// Batching is the compromise, and it is also what makes progress observable.
const BATCH_SIZE: usize = 256;

pub struct Indexer {
    fs: SharedFileSystem,
    clock: SharedClock,
    parser: MarkdownParser,
    ignore: IgnoreRules,
}

impl Indexer {
    pub fn new(fs: SharedFileSystem, clock: SharedClock) -> Self {
        Self {
            fs,
            clock,
            parser: MarkdownParser::new(),
            ignore: IgnoreRules::default(),
        }
    }

    pub fn with_ignore_rules(mut self, ignore: IgnoreRules) -> Self {
        self.ignore = ignore;
        self
    }

    pub fn ignore_rules(&self) -> &IgnoreRules {
        &self.ignore
    }

    /// Walk the vault, returning every file the indexer cares about.
    ///
    /// Uses the injected `FileSystem` rather than `std::fs` so the same walk
    /// runs against an in-memory vault in tests.
    pub fn walk(&self, root: &Path) -> Result<(Vec<VaultPath>, Vec<Diagnostic>)> {
        let mut found = Vec::new();
        let mut diagnostics = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        let mut visited: HashSet<String> = HashSet::new();

        while let Some(dir) = stack.pop() {
            let entries = match self.fs.read_dir(&dir) {
                Ok(entries) => entries,
                Err(e) => {
                    if let Ok(path) = VaultPath::from_fs_path(root, &dir) {
                        diagnostics.push(Diagnostic::UnreadableFile {
                            path,
                            message: e.to_string(),
                        });
                    }
                    continue;
                }
            };

            for entry in entries {
                if self.ignore.ignores(&entry.file_name) {
                    if entry.file_name.starts_with(".ie-tmp-") {
                        if let Ok(path) = VaultPath::from_fs_path(root, &entry.path) {
                            diagnostics.push(Diagnostic::InterruptedWrite { path });
                        }
                    }
                    continue;
                }

                // A symlink out of the vault would let the index address files
                // the user never put there, so it is reported and skipped.
                if entry.metadata.is_symlink {
                    let escapes = match self.fs.canonicalize(&entry.path) {
                        Ok(real) => !real.starts_with(root),
                        Err(_) => true,
                    };
                    if escapes {
                        if let Ok(path) = VaultPath::from_fs_path(root, &entry.path) {
                            diagnostics.push(Diagnostic::EscapingSymlink { path });
                        }
                        continue;
                    }
                }

                if entry.metadata.is_dir {
                    // Guard against a symlink cycle inside the vault.
                    let key = entry.path.to_string_lossy().to_string();
                    if visited.insert(key) {
                        stack.push(entry.path);
                    }
                    continue;
                }

                match VaultPath::from_fs_path(root, &entry.path) {
                    Ok(path) => found.push(path),
                    Err(_) => continue,
                }
            }
        }

        found.sort();
        Ok((found, diagnostics))
    }

    /// Index every file in the vault from scratch.
    pub fn full_scan<F: FnMut(IndexProgress)>(
        &self,
        db: &mut IndexDb,
        root: &Path,
        mut progress: F,
    ) -> Result<ScanReport> {
        let started = Instant::now();
        let (paths, mut diagnostics) = self.walk(root)?;
        let total = paths.len();

        {
            let tx = db.connection_mut().transaction()?;
            writer::clear_diagnostics(&tx)?;
            tx.commit()?;
        }

        let mut report = ScanReport {
            files_seen: total,
            ..ScanReport::default()
        };

        for (batch_number, batch) in paths.chunks(BATCH_SIZE).enumerate() {
            let tx = db.connection_mut().transaction()?;
            for path in batch {
                match self.index_one(&tx, root, path)? {
                    Indexed::Written => report.files_indexed += 1,
                    Indexed::Unchanged => report.files_unchanged += 1,
                    Indexed::Failed(diagnostic) => diagnostics.push(diagnostic),
                }
            }
            tx.commit()?;

            progress(IndexProgress {
                scanned: ((batch_number + 1) * BATCH_SIZE).min(total),
                indexed: report.files_indexed,
                total: Some(total),
                current: batch.last().cloned(),
            });
        }

        // Rows present in the index but no longer on disk. This is what makes a
        // scan converge even when files were deleted while the app was closed.
        let on_disk: HashSet<String> = paths.iter().map(|p| p.as_str().to_string()).collect();
        let stale: Vec<VaultPath> = {
            let mut stmt = db.connection().prepare("SELECT path FROM files")?;
            let rows: Vec<VaultPath> = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .filter_map(std::result::Result::ok)
                .filter(|path| !on_disk.contains(path))
                .map(VaultPath::from_indexed)
                .collect();
            rows
        };
        if !stale.is_empty() {
            let tx = db.connection_mut().transaction()?;
            for path in &stale {
                writer::remove_file(&tx, path)?;
            }
            tx.commit()?;
            report.files_removed = stale.len();
        }

        // Links written before their target had been indexed get one more pass
        // now that every file is known.
        {
            let tx = db.connection_mut().transaction()?;
            writer::resolve_all_pending(&tx)?;
            tx.commit()?;
        }

        diagnostics.extend(self.case_conflict_diagnostics(db)?);
        {
            let tx = db.connection_mut().transaction()?;
            for diagnostic in &diagnostics {
                writer::write_diagnostic(&tx, diagnostic)?;
            }
            tx.commit()?;
        }

        db.set_meta(
            crate::index::schema::META_LAST_FULL_SCAN,
            &self.clock.now_ms().to_string(),
        )?;

        report.duration_ms = started.elapsed().as_millis() as u64;
        report.diagnostics = diagnostics;
        progress(IndexProgress {
            scanned: total,
            indexed: report.files_indexed,
            total: Some(total),
            current: None,
        });
        Ok(report)
    }

    /// Re-index a single file, for an in-app save or a single watcher event.
    pub fn index_file(&self, db: &mut IndexDb, root: &Path, path: &VaultPath) -> Result<()> {
        let tx = db.connection_mut().transaction()?;
        self.index_one(&tx, root, path)?;
        // A new file may answer links written long ago, so give them a chance.
        writer::reresolve_pending(&tx, &path.stem().to_lowercase())?;
        tx.commit()?;
        Ok(())
    }

    /// Drop a file from the index.
    pub fn remove_file(&self, db: &mut IndexDb, path: &VaultPath) -> Result<()> {
        let tx = db.connection_mut().transaction()?;
        writer::remove_file(&tx, path)?;
        tx.commit()?;
        Ok(())
    }

    /// Apply a debounced batch from the filesystem watcher.
    ///
    /// Deletions are processed before creations so that a move, which arrives
    /// as an unpaired delete and create, does not briefly resolve links to two
    /// files at once.
    pub fn apply_events(
        &self,
        db: &mut IndexDb,
        root: &Path,
        events: &[FsEvent],
    ) -> Result<EventOutcome> {
        let mut outcome = EventOutcome::default();
        let mut created: Vec<VaultPath> = Vec::new();
        let mut modified: Vec<VaultPath> = Vec::new();
        let mut deleted: Vec<VaultPath> = Vec::new();

        for event in events {
            match event {
                FsEvent::Rescan { .. } => {
                    outcome.needs_full_scan = true;
                    return Ok(outcome);
                }
                FsEvent::Created(p) => {
                    if let Some(path) = self.vault_path_for(root, p) {
                        created.push(path);
                    }
                }
                FsEvent::Modified(p) => {
                    if let Some(path) = self.vault_path_for(root, p) {
                        modified.push(path);
                    }
                }
                FsEvent::Deleted(p) => {
                    if let Some(path) = self.vault_path_for(root, p) {
                        deleted.push(path);
                    }
                }
                FsEvent::Renamed { from, to } => {
                    if let Some(path) = self.vault_path_for(root, from) {
                        deleted.push(path);
                    }
                    if let Some(path) = self.vault_path_for(root, to) {
                        created.push(path);
                    }
                }
            }
        }

        // A directory event carries no file of its own; the entries beneath it
        // arrive as their own events, except when the directory itself was
        // moved, which the backend reports as a rescan.
        deleted.retain(|p| !self.fs.is_dir(&p.to_fs_path(root)));
        created.retain(|p| !self.fs.is_dir(&p.to_fs_path(root)));
        modified.retain(|p| !self.fs.is_dir(&p.to_fs_path(root)));

        let tx = db.connection_mut().transaction()?;

        for path in &deleted {
            // A delete followed by a create of the same path in one batch is a
            // save, not a deletion.
            if created.contains(path) || modified.contains(path) {
                continue;
            }
            if self.fs.exists(&path.to_fs_path(root)) {
                continue;
            }
            if writer::remove_file(&tx, path)?.is_some() {
                outcome.removed.push(path.clone());
            }
        }

        let mut touched: Vec<VaultPath> = Vec::new();
        touched.extend(created.iter().cloned());
        for path in &modified {
            if !touched.contains(path) {
                touched.push(path.clone());
            }
        }

        for path in &touched {
            if !self.fs.exists(&path.to_fs_path(root)) {
                continue;
            }
            match self.index_one(&tx, root, path)? {
                Indexed::Written => outcome.indexed.push(path.clone()),
                Indexed::Unchanged => {}
                Indexed::Failed(diagnostic) => outcome.diagnostics.push(diagnostic),
            }
        }

        // Every name that appeared or disappeared can change how existing links
        // resolve, in both directions.
        let mut tails: HashSet<String> = HashSet::new();
        for path in touched.iter().chain(outcome.removed.iter()) {
            tails.insert(path.stem().to_lowercase());
        }
        for tail in tails {
            writer::reresolve_pending(&tx, &tail)?;
        }

        tx.commit()?;
        Ok(outcome)
    }

    /// Translate a filesystem path into a vault path, or `None` if the indexer
    /// should ignore it.
    fn vault_path_for(&self, root: &Path, path: &Path) -> Option<VaultPath> {
        let vault_path = VaultPath::from_fs_path(root, path).ok()?;
        if vault_path.is_root() || vault_path.is_app_internal() {
            return None;
        }
        if vault_path
            .segments()
            .any(|segment| self.ignore.ignores(segment))
        {
            return None;
        }
        Some(vault_path)
    }

    fn index_one(
        &self,
        tx: &rusqlite::Transaction<'_>,
        root: &Path,
        path: &VaultPath,
    ) -> Result<Indexed> {
        let fs_path = path.to_fs_path(root);
        let metadata = match self.fs.metadata(&fs_path) {
            Ok(m) => m,
            Err(e) => {
                return Ok(Indexed::Failed(Diagnostic::UnreadableFile {
                    path: path.clone(),
                    message: e.to_string(),
                }))
            }
        };

        let mtime_ms = metadata.modified_ms.unwrap_or(0);
        let existing: Option<(i64, i64, Option<String>)> = tx
            .query_row(
                "SELECT size, mtime_ms, content_hash FROM files WHERE path = ?1",
                [path.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .ok();

        // The cheap check first: an untouched file costs one indexed lookup.
        if let Some((size, stored_mtime, _)) = &existing {
            if *size as u64 == metadata.len && *stored_mtime == mtime_ms {
                return Ok(Indexed::Unchanged);
            }
        }

        let kind = FileKind::from_extension(path.extension().as_deref());
        let record_base = FileRecord {
            path: path.clone(),
            kind,
            size: metadata.len,
            mtime_ms,
            content_hash: None,
            indexed_at: self.clock.now_ms(),
        };

        if kind != FileKind::Note || metadata.len > self.ignore.max_parse_bytes {
            writer::index_attachment(tx, &record_base)?;
            return Ok(Indexed::Written);
        }

        let bytes = match self.fs.read(&fs_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Ok(Indexed::Failed(Diagnostic::UnreadableFile {
                    path: path.clone(),
                    message: e.to_string(),
                }))
            }
        };
        let hash = writer::hash_content(&bytes);

        // A touched-but-unchanged file: refresh the timestamps, skip the parse.
        // Editors and sync tools rewrite files constantly, and parsing them
        // again would be the bulk of a scan's cost for no benefit.
        if let Some((_, _, Some(stored_hash))) = &existing {
            if stored_hash == &hash {
                tx.execute(
                    "UPDATE files SET size = ?1, mtime_ms = ?2, indexed_at = ?3 WHERE path = ?4",
                    rusqlite::params![
                        metadata.len as i64,
                        mtime_ms,
                        record_base.indexed_at,
                        path.as_str()
                    ],
                )?;
                return Ok(Indexed::Unchanged);
            }
        }

        let source = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                // A `.md` file that is not UTF-8 is left exactly as it is and
                // recorded as an attachment, so nothing rewrites or loses it.
                writer::index_attachment(tx, &record_base)?;
                return Ok(Indexed::Failed(Diagnostic::UnreadableFile {
                    path: path.clone(),
                    message: "file is not valid UTF-8 and was indexed as an attachment".into(),
                }));
            }
        };

        let parsed = self.parser.parse(&source);
        let record = FileRecord {
            content_hash: Some(hash),
            ..record_base
        };
        writer::index_note(tx, &record, &parsed.metadata, &source)?;

        if let Some(message) = parsed.frontmatter_error {
            return Ok(Indexed::Failed(Diagnostic::MalformedFrontmatter {
                path: path.clone(),
                message,
            }));
        }
        Ok(Indexed::Written)
    }

    fn case_conflict_diagnostics(&self, db: &IndexDb) -> Result<Vec<Diagnostic>> {
        Ok(crate::index::queries::case_conflicts(db.connection())?
            .into_iter()
            .map(|paths| Diagnostic::CaseConflict { paths })
            .collect())
    }
}

enum Indexed {
    Written,
    Unchanged,
    Failed(Diagnostic),
}

/// What changed as a result of a watcher batch, so the UI can refresh exactly
/// the panels that need it.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventOutcome {
    pub indexed: Vec<VaultPath>,
    pub removed: Vec<VaultPath>,
    /// The backend lost events; only a full rescan can be trusted now.
    pub needs_full_scan: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl EventOutcome {
    pub fn is_empty(&self) -> bool {
        self.indexed.is_empty() && self.removed.is_empty() && !self.needs_full_scan
    }
}

/// Group a set of paths by their case-folded form, to find collisions before
/// they are written rather than after.
pub fn group_by_fold(paths: &[VaultPath]) -> HashMap<String, Vec<VaultPath>> {
    let mut map: HashMap<String, Vec<VaultPath>> = HashMap::new();
    for path in paths {
        map.entry(path.fold()).or_default().push(path.clone());
    }
    map.retain(|_, group| group.len() > 1);
    map
}

//! `VaultHandle` — one open vault, as the host sees it.
//!
//! The granularity here is deliberate. The desktop's IPC surface is 73
//! fine-grained commands, which suits a browser talking over a cheap local
//! channel. On iOS every call is wrapped by the host in an `NSFileCoordinator`
//! block, so the right unit is the operation a user performs: open, scan, read
//! a note, save it, search. A full scan is one bridge call and therefore one
//! coordination bracket, not four thousand.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use ie_core::index::queries;
use ie_core::search;
use ie_core::session::VaultSession;
use ie_core::vault::VaultPath;

use crate::error::{FfiError, Result};
use crate::host::{Host, HostConfig, StorageHost, Watch};
use crate::types::*;

/// Progress during a scan, reported to the host so it can drive a progress
/// view without polling.
#[uniffi::export(with_foreign)]
pub trait ScanProgress: Send + Sync {
    fn report(&self, progress: IndexProgress);
}

/// Filesystem changes, delivered to the host as one coalesced batch.
#[uniffi::export(with_foreign)]
pub trait ChangeObserver: Send + Sync {
    fn changed(&self, events: Vec<FsEvent>);
}

/// One open vault.
///
/// Every method takes the session lock for the duration of one operation, the
/// same discipline the desktop's `AppState` uses. On the Swift side a single
/// `actor` owns this handle, so two saves to one note cannot interleave — the
/// requirement that writes never race is met by the type system rather than by
/// remembering to hold a lock.
#[derive(uniffi::Object)]
pub struct VaultHandle {
    session: Mutex<VaultSession>,
    root: PathBuf,
    vault_id: String,
    host: Host,
    /// Kept alive for as long as the vault is open; dropping it stops the
    /// watch and flushes whatever was queued.
    watch: Mutex<Option<Watch>>,
}

impl std::fmt::Debug for VaultHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately not the root path: a handle ends up in log lines and
        // panic messages, and the vault's location is the one thing this
        // process is not supposed to leak.
        f.debug_struct("VaultHandle")
            .field("vault_id", &self.vault_id)
            .finish_non_exhaustive()
    }
}

impl VaultHandle {
    fn session(&self) -> Result<MutexGuard<'_, VaultSession>> {
        self.session.lock().map_err(|_| FfiError::Refused {
            operation: "use the vault".into(),
            reason: "an earlier failure left the vault state inconsistent".into(),
        })
    }

    fn path(text: &str) -> Result<VaultPath> {
        VaultPath::parse(text).map_err(FfiError::from)
    }

    fn open_inner(
        config: &HostConfig,
        root: &Path,
        storage: Option<Arc<dyn StorageHost>>,
        create_as: Option<&str>,
    ) -> Result<(Arc<Self>, OpenReport)> {
        let host = Host::build(config, root, storage)?;

        // The cache goes in the container, named by the vault's own id — which
        // the core knows by the time it opens the database, so the location is
        // a directory rather than a file and nothing has to open the vault
        // twice to learn it.
        let options = ie_core::session::OpenOptions {
            index: ie_core::session::IndexLocation::Directory(host.dirs.index_dir()),
        };
        let (session, report) = match create_as {
            Some(name) => VaultSession::create_with(host.services.clone(), root, name, &options)?,
            None => VaultSession::open_with(host.services.clone(), root, &options)?,
        };

        let vault_id = session.settings().id.clone();
        let root = session.root().to_path_buf();
        let report = OpenReport {
            root: report.root,
            name: report.name,
            vault_id: vault_id.clone(),
            index: report.index.into(),
            created: report.created,
            case_sensitive: report.case_sensitive,
        };

        Ok((
            Arc::new(Self {
                session: Mutex::new(session),
                root,
                vault_id,
                host,
                watch: Mutex::new(None),
            }),
            report,
        ))
    }
}

#[uniffi::export]
impl VaultHandle {
    /// Open a folder as a vault.
    ///
    /// Any folder is one: there is no import step and no container format. The
    /// path comes from a security-scoped URL the host has already started
    /// accessing; it is used for this call and never stored anywhere the app
    /// would read it back.
    #[uniffi::constructor]
    pub fn open(
        config: HostConfig,
        path: String,
        storage: Option<Arc<dyn StorageHost>>,
    ) -> Result<Arc<Self>> {
        Self::open_inner(&config, Path::new(&path), storage, None).map(|(handle, _)| handle)
    }

    /// Open, and say what happened — whether the index was reused, whether the
    /// volume folds case, whether this is a first open.
    #[uniffi::constructor]
    pub fn open_reporting(
        config: HostConfig,
        path: String,
        storage: Option<Arc<dyn StorageHost>>,
        report: Arc<OpenReportSink>,
    ) -> Result<Arc<Self>> {
        let (handle, open_report) = Self::open_inner(&config, Path::new(&path), storage, None)?;
        report.set(open_report);
        Ok(handle)
    }

    /// Create a vault: the conventional folders and a welcome note, then open.
    #[uniffi::constructor]
    pub fn create(
        config: HostConfig,
        path: String,
        name: String,
        storage: Option<Arc<dyn StorageHost>>,
    ) -> Result<Arc<Self>> {
        Self::open_inner(&config, Path::new(&path), storage, Some(&name)).map(|(handle, _)| handle)
    }

    /// The vault's stable id, from `.inner-empire/vault.json`.
    ///
    /// It travels with the vault, so the host keys its bookmark and its index
    /// cache on this rather than on a path.
    pub fn vault_id(&self) -> String {
        self.vault_id.clone()
    }

    /// Whether this vault's volume distinguishes `Note.md` from `note.md`.
    pub fn is_case_sensitive(&self) -> Result<bool> {
        Ok(self.session()?.is_case_sensitive())
    }

    /// Where the index cache for this vault lives — outside the vault, in the
    /// container, for the reasons in `docs/ios/STORAGE.md`.
    pub fn index_cache_path(&self) -> String {
        self.host
            .dirs
            .index_path(&self.vault_id)
            .to_string_lossy()
            .to_string()
    }

    /// The time zone changed; the daily note must follow the device.
    pub fn set_utc_offset_seconds(&self, seconds: i32) {
        self.host.clock.set_offset_seconds(seconds);
    }

    // ------------------------------------------------------------ index ----

    /// Walk the vault and bring the index up to date.
    ///
    /// Incremental: a file whose size and mtime are unchanged is not re-parsed,
    /// so a warm launch reads a directory tree and little else. The host wraps
    /// this one call in a coordinated read of the vault.
    pub fn scan(&self, progress: Option<Arc<dyn ScanProgress>>) -> Result<ScanReport> {
        let mut session = self.session()?;
        let report = match progress {
            Some(sink) => session.scan(|p| sink.report(p.into()))?,
            None => session.scan(|_| {})?,
        };
        Ok(report.into())
    }

    /// Apply changes the host observed, without a full walk.
    pub fn apply_events(&self, events: Vec<FsEvent>) -> Result<EventOutcome> {
        let translated = events
            .iter()
            .map(|e| e.to_core(&self.root))
            .collect::<Result<Vec<_>>>()?;
        Ok(self.session()?.apply_events(&translated)?.into())
    }

    /// Start watching, delivering coalesced batches to `observer`.
    ///
    /// The host still has to drive it: register an `NSFilePresenter` and call
    /// [`VaultHandle::deliver_events`] from its callbacks. Nothing here polls.
    pub fn start_watch(&self, observer: Arc<dyn ChangeObserver>) -> Result<()> {
        let mut slot = self.watch.lock().map_err(|_| FfiError::Refused {
            operation: "start watching".into(),
            reason: "the watch state was left inconsistent".into(),
        })?;
        if slot.is_some() {
            return Ok(());
        }

        let root = self.root.clone();
        let watch = self.host.watcher.open(
            self.root.clone(),
            ie_platform::WatchOptions {
                recursive: true,
                debounce: std::time::Duration::from_millis(self.host.debounce_ms),
            },
            Box::new(move |batch| {
                let events = batch
                    .into_iter()
                    .filter_map(|event| from_core_event(&event, &root))
                    .collect::<Vec<_>>();
                if !events.is_empty() {
                    observer.changed(events);
                }
            }),
        )?;
        *slot = Some(watch);
        Ok(())
    }

    /// Push a change the host's presenter saw. Coalesced before it reaches the
    /// observer, so one save does not become four notifications.
    pub fn deliver_events(&self, events: Vec<FsEvent>) -> Result<()> {
        let slot = self.watch.lock().map_err(|_| FfiError::Refused {
            operation: "deliver events".into(),
            reason: "the watch state was left inconsistent".into(),
        })?;
        if let Some(watch) = slot.as_ref() {
            let translated = events
                .iter()
                .map(|e| e.to_core(&self.root))
                .collect::<Result<Vec<_>>>()?;
            watch.deliver(translated);
        }
        Ok(())
    }

    /// Stop watching and flush anything queued — called as the app backgrounds.
    pub fn stop_watch(&self) {
        if let Ok(mut slot) = self.watch.lock() {
            *slot = None;
        }
    }

    // ------------------------------------------------------------ notes ----

    pub fn read_note(&self, path: String) -> Result<Note> {
        let path = Self::path(&path)?;
        match self.session()?.read_note(&path) {
            Ok(note) => Ok(note.into()),
            // The core reports this as a platform error, which loses the vault
            // path and would make the host check two variants for one
            // condition. It knows the path here, so it says so.
            Err(ie_core::CoreError::Platform(ie_platform::PlatformError::NotFound { .. })) => {
                Err(FfiError::NotFound {
                    path: path.as_str().to_string(),
                })
            }
            Err(other) => Err(other.into()),
        }
    }

    /// Write a note, refusing if the file changed since it was opened.
    ///
    /// `base_modified_ms` is the `Note::modified_ms` the host got when it
    /// loaded the buffer. Passing it is not optional politeness: it is the only
    /// thing standing between a background sync and a silently overwritten
    /// edit, so a mismatch fails with `ExternalModification` and the host must
    /// resolve it.
    pub fn save_note(&self, path: String, content: String, base_modified_ms: i64) -> Result<i64> {
        let path = Self::path(&path)?;
        let mut session = self.session()?;

        let current = current_modified_ms(&self.host.services.fs, &session, &path);
        if let Some(current) = current {
            if current != base_modified_ms {
                return Err(FfiError::ExternalModification {
                    path: path.as_str().to_string(),
                    opened_modified_ms: base_modified_ms,
                    current_modified_ms: current,
                });
            }
        }

        session.save_note(&path, &content)?;
        session.clear_journal(&path)?;
        Ok(
            current_modified_ms(&self.host.services.fs, &session, &path)
                .unwrap_or(base_modified_ms),
        )
    }

    /// Write a note that the user has confirmed should replace whatever is on
    /// disk. The only path that skips the external-change check, and it exists
    /// so that the check is never skipped by accident.
    pub fn force_save_note(&self, path: String, content: String) -> Result<i64> {
        let path = Self::path(&path)?;
        let mut session = self.session()?;
        session.save_note(&path, &content)?;
        session.clear_journal(&path)?;
        Ok(current_modified_ms(&self.host.services.fs, &session, &path).unwrap_or(0))
    }

    /// Create a note, returning where it landed and when it was written.
    ///
    /// The timestamp is returned rather than left for the caller to fetch
    /// because the alternative is a host that has a buffer open with no base to
    /// compare against, and the only safe thing to do with no base is refuse to
    /// save. `Collision::Rename` means the path may differ from the one asked
    /// for, so both are reported.
    pub fn create_note(
        &self,
        path: String,
        content: String,
        collision: Collision,
    ) -> Result<CreatedNote> {
        let requested = Self::path(&path)?;
        let mut session = self.session()?;
        let created = session.create_note(&requested, &content, collision.into())?;
        let modified_ms =
            current_modified_ms(&self.host.services.fs, &session, &created).unwrap_or(0);
        Ok(CreatedNote {
            path: created,
            modified_ms,
        })
    }

    /// What a rename would do, before doing it.
    pub fn plan_rename(&self, from: String, to: String) -> Result<RenamePlan> {
        Ok(self
            .session()?
            .plan_rename(&Self::path(&from)?, &Self::path(&to)?)?
            .into())
    }

    /// Rename or move, rewriting every link that pointed at it.
    pub fn rename(&self, from: String, to: String) -> Result<RenameOutcome> {
        Ok(self
            .session()?
            .rename(&Self::path(&from)?, &Self::path(&to)?)?
            .into())
    }

    /// Move to the vault's trash. Recoverable, and never a `remove_file`.
    pub fn delete(&self, path: String) -> Result<TrashEntry> {
        Ok(self.session()?.delete(&Self::path(&path)?)?.into())
    }

    pub fn restore(&self, id: String) -> Result<VaultPath> {
        Ok(self.session()?.restore(&id)?)
    }

    pub fn set_properties(&self, path: String, properties: Vec<Property>) -> Result<()> {
        let converted: Vec<ie_core::model::Property> =
            properties.into_iter().map(Into::into).collect();
        self.session()?
            .set_properties(&Self::path(&path)?, &converted)?;
        Ok(())
    }

    // ------------------------------------------------------ attachments ----

    /// Write a file into the vault and return where it landed.
    ///
    /// Where that is comes from the vault's own settings, decided by the core,
    /// so a screenshot filed on a phone goes where a screenshot filed on a
    /// desktop goes. The name is sanitised there too: an iOS screenshot is
    /// called "Shot 2026-09-17 at 10:30.png", and that colon is legal here and
    /// illegal on Windows.
    pub fn import_attachment(
        &self,
        file_name: String,
        bytes: Vec<u8>,
        note: Option<String>,
    ) -> Result<VaultPath> {
        let note = note.map(|p| Self::path(&p)).transpose()?;
        let mut session = self.session()?;

        let location = session.settings().attachments.clone();
        let target = ie_core::vault::attachment_path(&location, note.as_ref(), &file_name)?;

        session.ops().create_folder(&target.parent())?;
        let path = session
            .ops()
            .create_note(&target, "", ie_core::vault::Collision::Rename)?;
        session.ops().write_bytes(&path, &bytes)?;
        session.reindex(&path)?;
        Ok(path)
    }

    /// The Markdown that embeds `attachment` in a note.
    ///
    /// An image embeds inline; anything else becomes a link, because a phone
    /// rendering a 40MB video inline is a phone that has stopped responding.
    pub fn embed_for(&self, attachment: String) -> Result<String> {
        let path = Self::path(&attachment)?;
        let kind = ie_core::model::FileKind::from_extension(path.extension().as_deref());
        let stem = path.stem().to_string();
        Ok(match kind {
            ie_core::model::FileKind::Image => format!("![[{}]]", path.as_str()),
            _ => format!("[[{}|{stem}]]", path.as_str()),
        })
    }

    // ----------------------------------------------------------- dailies ----

    /// A daily note, creating it from the template if it is not there yet.
    ///
    /// `day_offset` is days from today: 0 for today, -1 for yesterday, 1 for
    /// tomorrow. The format and folder come from the vault's settings, so the
    /// note a phone opens is the note a desktop opens rather than a second
    /// file for the same day.
    ///
    /// The clock's offset is the one the host supplied, not one read from the
    /// process — which matters here more than anywhere, because getting it
    /// wrong puts the note on the wrong day.
    pub fn open_daily_note(&self, day_offset: i64) -> Result<DailyNote> {
        let session = self.session()?;
        let settings = session.settings().daily_notes.clone();
        let (path, created) = ie_core::templates::ensure_daily_note(
            session.ops(),
            &settings,
            &self.host.services.clock,
            day_offset,
        )?;
        drop(session);

        if created {
            // Index it immediately: the user is about to look at its
            // backlinks, and waiting for the watcher would make them empty.
            self.session()?.reindex(&path)?;
        }
        Ok(DailyNote { path, created })
    }

    /// Where a daily note would be, without creating it.
    pub fn daily_note_path(&self, day_offset: i64) -> Result<VaultPath> {
        let session = self.session()?;
        Ok(ie_core::templates::daily_note_path(
            &session.settings().daily_notes,
            self.host.services.clock.now_ms(),
            self.host.services.clock.local_offset_seconds(),
            day_offset,
        )?)
    }

    // --------------------------------------------------------- recovery ----

    /// Record an unsaved buffer, so a crash or a termination does not lose it.
    ///
    /// The journal lives in the container, not the vault: a half-typed
    /// paragraph is machine-local and has no business syncing to a desktop.
    pub fn journal_unsaved(&self, path: String, content: String) -> Result<()> {
        self.session()?
            .journal_unsaved(&Self::path(&path)?, &content)?;
        Ok(())
    }

    pub fn clear_journal(&self, path: String) -> Result<()> {
        self.session()?.clear_journal(&Self::path(&path)?)?;
        Ok(())
    }

    /// Buffers that were never saved, with enough context for the host to say
    /// what restoring each one would cost.
    pub fn recoverable(&self) -> Result<Vec<RecoveryCandidate>> {
        Ok(self
            .session()?
            .recoverable()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub fn prune_journal(&self, max_age_ms: i64) -> Result<u32> {
        Ok(u32::try_from(self.session()?.prune_journal(max_age_ms)?).unwrap_or(u32::MAX))
    }

    /// Problems worth telling the user about: case conflicts, names that would
    /// not survive a trip to Windows, interrupted writes.
    pub fn diagnostics(&self) -> Result<Vec<Diagnostic>> {
        Ok(self
            .session()?
            .diagnostics()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    // ----------------------------------------------------------- search ----

    pub fn search(&self, query: String, options: SearchOptions) -> Result<SearchResults> {
        let parsed = search::parse(&query)?;
        let session = self.session()?;
        Ok(search::search(session.connection(), &parsed, &options.into())?.into())
    }

    /// Fuzzy filename matching, for "open note". An empty needle returns the
    /// most recently modified, which is the right answer for an empty field.
    pub fn quick_switch(&self, needle: String, limit: u32) -> Result<Vec<FileMatch>> {
        let session = self.session()?;
        Ok(
            search::quick_switch(session.connection(), &needle, limit as usize)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    // ------------------------------------------------------------ links ----

    pub fn backlinks(&self, path: String) -> Result<Vec<Backlink>> {
        let session = self.session()?;
        Ok(
            queries::backlinks(session.connection(), &Self::path(&path)?)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    pub fn outgoing_links(&self, path: String) -> Result<Vec<ResolvedLink>> {
        let session = self.session()?;
        Ok(
            queries::outgoing_links(session.connection(), &Self::path(&path)?)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    pub fn tags(&self) -> Result<Vec<TagSummary>> {
        let session = self.session()?;
        Ok(queries::tag_summaries(session.connection())?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    // ------------------------------------------------------------ graph ----

    /// The whole vault as a graph.
    ///
    /// Capped by `max_nodes`, so a huge vault degrades into a truncated graph
    /// rather than a frozen screen — and `GraphData::truncated` says so, so the
    /// view can tell the user instead of quietly showing part of their notes.
    pub fn graph(&self, options: GraphOptions) -> Result<GraphData> {
        let session = self.session()?;
        Ok(queries::graph(session.connection(), &options.try_into()?)?.into())
    }

    /// The neighbourhood around one note, `depth` links out.
    ///
    /// Undirected: a note you link to and a note that links to you are both
    /// neighbours, because both are things you would want to see from here.
    pub fn local_graph(
        &self,
        path: String,
        depth: u32,
        options: GraphOptions,
    ) -> Result<GraphData> {
        let session = self.session()?;
        Ok(queries::local_graph(
            session.connection(),
            &Self::path(&path)?,
            depth as usize,
            &options.try_into()?,
        )?
        .into())
    }

    // ------------------------------------------------------------- tree ----

    /// One level of the tree, read on demand.
    ///
    /// Fetched per folder rather than as a whole tree, so opening a vault with
    /// fifty thousand files costs one directory read rather than fifty
    /// thousand.
    pub fn list_directory(&self, path: String) -> Result<DirectoryListing> {
        let path = Self::path(&path)?;
        let session = self.session()?;
        let entries = session.ops().list_folder(&path)?;
        let conn = session.connection();

        let mut folders = Vec::new();
        let mut files = Vec::new();
        for (entry_path, is_dir) in entries {
            if is_dir {
                let children = session.ops().list_folder(&entry_path).unwrap_or_default();
                let child_folder_count = children.iter().filter(|(_, is_dir)| *is_dir).count();
                folders.push(FolderEntry {
                    name: entry_path.file_name().to_string(),
                    path: entry_path,
                    child_file_count: u32::try_from(children.len() - child_folder_count)
                        .unwrap_or(u32::MAX),
                    child_folder_count: u32::try_from(child_folder_count).unwrap_or(u32::MAX),
                });
                continue;
            }

            match queries::file(conn, &entry_path)? {
                // The indexed record, which carries the frontmatter title.
                Some(file) => files.push(FileEntry::from(file)),
                // Not indexed yet — a file that appeared between the last scan
                // and now. Listed rather than hidden, with size and mtime left
                // at zero: stat-ing it here would cost a round trip per entry
                // on a provider-backed vault, and the scan already under way
                // fills both in.
                None => files.push(FileEntry {
                    name: entry_path.file_name().to_string(),
                    title: entry_path.stem().to_string(),
                    kind: ie_core::model::FileKind::from_extension(
                        entry_path.extension().as_deref(),
                    )
                    .into(),
                    size: 0,
                    modified_ms: 0,
                    path: entry_path,
                }),
            }
        }

        Ok(DirectoryListing {
            path,
            folders,
            files,
        })
    }
}

/// Somewhere for a constructor to put its report.
///
/// UniFFI constructors return the object or an error and nothing else, and the
/// open report says things the host needs on the very first screen — whether
/// the index survived, whether the volume folds case. A one-shot cell is less
/// machinery than a second "open" entry point that returns a pair.
#[derive(Default, uniffi::Object)]
pub struct OpenReportSink {
    report: Mutex<Option<OpenReport>>,
}

#[uniffi::export]
impl OpenReportSink {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn take(&self) -> Option<OpenReport> {
        self.report.lock().ok().and_then(|mut slot| slot.take())
    }
}

impl OpenReportSink {
    fn set(&self, report: OpenReport) {
        if let Ok(mut slot) = self.report.lock() {
            *slot = Some(report);
        }
    }
}

/// The file's modification time, read from the filesystem.
///
/// Deliberately not from the index. The index is a cache, and a cache is stale
/// exactly when it matters here — the whole point of this check is to catch a
/// write that happened without the app noticing, which is also a write the
/// index has not seen. `None` means there is no file at all, so there is
/// nothing a save could be overwriting.
fn current_modified_ms(
    fs: &ie_platform::SharedFileSystem,
    session: &VaultSession,
    path: &VaultPath,
) -> Option<i64> {
    fs.metadata(&session.ops().resolve(path))
        .ok()
        .and_then(|m| m.modified_ms)
}

/// Back to the host's vocabulary: vault-relative paths, so the host is never
/// handed an absolute one it might persist.
fn from_core_event(event: &ie_platform::FsEvent, root: &Path) -> Option<FsEvent> {
    let relative = |path: &Path| -> Option<String> {
        VaultPath::from_fs_path(root, path)
            .ok()
            .map(|p| p.as_str().to_string())
    };
    Some(match event {
        ie_platform::FsEvent::Created(p) => FsEvent::Created { path: relative(p)? },
        ie_platform::FsEvent::Modified(p) => FsEvent::Modified { path: relative(p)? },
        ie_platform::FsEvent::Deleted(p) => FsEvent::Deleted { path: relative(p)? },
        ie_platform::FsEvent::Renamed { from, to } => FsEvent::Renamed {
            from: relative(from)?,
            to: relative(to)?,
        },
        ie_platform::FsEvent::Rescan { .. } => FsEvent::Rescan,
    })
}

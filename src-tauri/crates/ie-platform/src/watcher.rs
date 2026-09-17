use std::path::PathBuf;
use std::time::Duration;

use crate::error::Result;

/// A filesystem change, already normalised across backends.
///
/// `notify` reports very different raw events on inotify (Linux) and
/// ReadDirectoryChangesW (Windows) — a single save can surface as
/// create+modify+modify+close, or as a rename pair, depending on the editor
/// and the OS. Adapters collapse that noise into these five cases so
/// `ie-core` sees one vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    /// Both endpoints known: a true rename or move.
    Renamed { from: PathBuf, to: PathBuf },
    /// The backend lost events (queue overflow, or a watched directory was
    /// replaced). The only safe response is a rescan of `root`.
    Rescan { root: PathBuf },
}

impl FsEvent {
    /// The path this event is primarily about, for logging and de-duplication.
    pub fn primary_path(&self) -> &PathBuf {
        match self {
            FsEvent::Created(p)
            | FsEvent::Modified(p)
            | FsEvent::Deleted(p)
            | FsEvent::Rescan { root: p } => p,
            FsEvent::Renamed { to, .. } => to,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub recursive: bool,
    /// How long to wait for a path to go quiet before reporting it. One save
    /// from a typical editor produces several raw events inside this window.
    pub debounce: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            debounce: Duration::from_millis(150),
        }
    }
}

/// A live watch. Dropping it stops the watch and joins the backend thread.
pub trait WatchHandle: Send {
    fn stop(self: Box<Self>);
}

/// Creates watches. Implemented once per platform family so backend quirks
/// stay out of `ie-core`.
pub trait FileWatcher: Send + Sync {
    /// Watch `root`, delivering normalised, debounced batches to `sink`.
    /// The sink runs on the watcher's own thread and must not block for long.
    fn watch(
        &self,
        root: PathBuf,
        options: WatchOptions,
        sink: Box<dyn Fn(Vec<FsEvent>) + Send + 'static>,
    ) -> Result<Box<dyn WatchHandle>>;
}

pub type SharedFileWatcher = std::sync::Arc<dyn FileWatcher>;

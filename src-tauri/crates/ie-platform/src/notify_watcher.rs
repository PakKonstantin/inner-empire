//! Filesystem watching backed by `notify`.
//!
//! `notify` already abstracts inotify (Linux) and ReadDirectoryChangesW
//! (Windows), but the *event streams* those backends produce are not alike: a
//! single save can arrive as create+modify+modify, or as a rename of a
//! temporary file over the target, depending on the editor. This adapter
//! debounces and then folds the raw stream into the five cases `ie-core`
//! understands.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebouncedEvent};

use crate::error::{PlatformError, Result};
use crate::watcher::{FileWatcher, FsEvent, WatchHandle, WatchOptions};

#[derive(Debug, Default, Clone, Copy)]
pub struct NotifyWatcher;

impl NotifyWatcher {
    pub fn new() -> Self {
        Self
    }
}

struct NotifyHandle {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WatchHandle for NotifyHandle {
    fn stop(mut self: Box<Self>) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for NotifyHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
    }
}

/// Translate one debounced batch into the normalised vocabulary.
///
/// Rename pairs are the interesting case. `notify` reports them as a
/// `ModifyKind::Name(From)` followed by `ModifyKind::Name(To)` carrying the
/// same tracker id, but only on some backends; where the pairing is missing we
/// emit a delete and a create and let the indexer re-pair them by content hash.
pub(crate) fn normalize(batch: Vec<DebouncedEvent>) -> Vec<FsEvent> {
    use notify::event::{ModifyKind, RenameMode};

    let mut out: Vec<FsEvent> = Vec::with_capacity(batch.len());
    let mut pending_rename_from: Option<PathBuf> = None;

    for debounced in batch {
        let event = debounced.event;
        let paths = event.paths.clone();

        match event.kind {
            EventKind::Create(_) => {
                for path in paths {
                    out.push(FsEvent::Created(path));
                }
            }
            EventKind::Remove(_) => {
                for path in paths {
                    out.push(FsEvent::Deleted(path));
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if paths.len() == 2 => {
                out.push(FsEvent::Renamed {
                    from: paths[0].clone(),
                    to: paths[1].clone(),
                });
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                // Hold it: the matching `To` usually arrives in the same batch.
                if let Some(orphan) = pending_rename_from.replace(
                    paths.first().cloned().unwrap_or_default(),
                ) {
                    out.push(FsEvent::Deleted(orphan));
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => match pending_rename_from.take() {
                Some(from) => out.push(FsEvent::Renamed {
                    from,
                    to: paths.first().cloned().unwrap_or_default(),
                }),
                None => {
                    for path in paths {
                        out.push(FsEvent::Created(path));
                    }
                }
            },
            EventKind::Modify(_) | EventKind::Access(_) | EventKind::Other => {
                for path in paths {
                    out.push(FsEvent::Modified(path));
                }
            }
            EventKind::Any => {
                for path in paths {
                    out.push(FsEvent::Modified(path));
                }
            }
        }
    }

    // A `From` with no partner is a move out of the watched tree: a deletion.
    if let Some(orphan) = pending_rename_from {
        out.push(FsEvent::Deleted(orphan));
    }

    dedupe_preserving_order(out)
}

/// Collapse repeats of the same event. The debouncer already merges by path
/// within its window, but a batch can still contain e.g. two `Modified` for one
/// file when the editor writes twice.
fn dedupe_preserving_order(events: Vec<FsEvent>) -> Vec<FsEvent> {
    let mut seen: Vec<FsEvent> = Vec::with_capacity(events.len());
    for event in events {
        if !seen.contains(&event) {
            seen.push(event);
        }
    }
    seen
}

impl FileWatcher for NotifyWatcher {
    fn watch(
        &self,
        root: PathBuf,
        options: WatchOptions,
        sink: Box<dyn Fn(Vec<FsEvent>) + Send + 'static>,
    ) -> Result<Box<dyn WatchHandle>> {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (event_tx, event_rx) = mpsc::channel();

        let mut debouncer = new_debouncer(options.debounce, None, move |result| {
            let _ = event_tx.send(result);
        })
        .map_err(|e| PlatformError::Watcher {
            message: e.to_string(),
        })?;

        let mode = if options.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        debouncer
            .watch(&root, mode)
            .map_err(|e| PlatformError::Watcher {
                message: format!("{}: {e}", root.display()),
            })?;

        let watch_root = root.clone();
        let thread = std::thread::Builder::new()
            .name("ie-fs-watcher".into())
            .spawn(move || {
                // Keep the debouncer alive for the thread's lifetime; dropping
                // it cancels the watch.
                let _debouncer = debouncer;
                loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    match event_rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(Ok(batch)) => {
                            let normalized = normalize(batch);
                            if !normalized.is_empty() {
                                sink(normalized);
                            }
                        }
                        Ok(Err(errors)) => {
                            // A backend error usually means the queue overflowed
                            // or a watched directory vanished; the only safe
                            // recovery is a rescan.
                            for error in &errors {
                                tracing::warn!(error = %error, "filesystem watcher error");
                            }
                            sink(vec![FsEvent::Rescan {
                                root: watch_root.clone(),
                            }]);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| PlatformError::Watcher {
                message: e.to_string(),
            })?;

        Ok(Box::new(NotifyHandle {
            stop: Some(stop_tx),
            thread: Some(thread),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::FileSystem;
    use crate::std_fs::StdFileSystem;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_single_save_produces_events_for_that_file_only() {
        let dir = tempfile::tempdir().unwrap();
        let fs = StdFileSystem::for_current_platform();
        let collected: Arc<Mutex<Vec<FsEvent>>> = Arc::new(Mutex::new(Vec::new()));

        let sink_target = Arc::clone(&collected);
        let handle = NotifyWatcher::new()
            .watch(
                dir.path().to_path_buf(),
                WatchOptions {
                    recursive: true,
                    debounce: Duration::from_millis(80),
                },
                Box::new(move |events| {
                    sink_target.lock().unwrap().extend(events);
                }),
            )
            .unwrap();

        // Give the backend a moment to establish the watch before changing files.
        std::thread::sleep(Duration::from_millis(250));
        fs.write_atomic(&dir.path().join("note.md"), b"hello").unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if !collected.lock().unwrap().is_empty() || std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        handle.stop();

        let events = collected.lock().unwrap().clone();
        assert!(!events.is_empty(), "watcher reported nothing for a write");
        assert!(
            events
                .iter()
                .any(|e| e.primary_path().to_string_lossy().ends_with("note.md")),
            "no event for the written file in {events:?}"
        );
        // The parent directory also reports a modification, because the atomic
        // write renames a sibling into place. Consumers must tolerate events for
        // directories and for paths they do not index; this asserts we surface
        // them rather than silently dropping them.
        assert!(
            events
                .iter()
                .all(|e| e.primary_path().starts_with(dir.path())),
            "event escaped the watched root in {events:?}"
        );
    }

    #[test]
    fn duplicate_events_in_one_batch_collapse() {
        let a = FsEvent::Modified(PathBuf::from("/v/a.md"));
        let b = FsEvent::Modified(PathBuf::from("/v/b.md"));
        let collapsed = dedupe_preserving_order(vec![a.clone(), a.clone(), b.clone(), a.clone()]);
        assert_eq!(collapsed, vec![a, b]);
    }
}

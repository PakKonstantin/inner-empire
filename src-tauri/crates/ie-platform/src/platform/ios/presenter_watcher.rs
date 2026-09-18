//! Watching a vault on iOS.
//!
//! `notify` has no iOS backend; it would fall back to polling, which on a
//! thousand-note vault is a battery problem and still misses changes a file
//! provider makes without touching the local filesystem. iOS instead offers
//! `NSFilePresenter`, which the provider itself drives.
//!
//! The Objective-C half lives in Swift. This is the Rust half: the host
//! translates presenter callbacks into [`FsEvent`]s and pushes them in, and
//! this type does the coalescing that `ie-core::apply_events` expects — one
//! save from an editor is several raw events, and the core should see one
//! batch.
//!
//! Coalescing is here rather than in Swift deliberately. It is the part with
//! actual behaviour (a quiet window, a rescan that swallows everything queued
//! behind it, a flush at shutdown), so it belongs where it can be tested on any
//! machine.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::error::{PlatformError, Result};
use crate::watcher::{FileWatcher, FsEvent, WatchHandle, WatchOptions};

type Sink = Box<dyn Fn(Vec<FsEvent>) + Send + 'static>;

struct Shared {
    queue: Mutex<State>,
    wake: Condvar,
}

struct State {
    pending: VecDeque<FsEvent>,
    /// When the last event arrived. The batch flushes once this is older than
    /// the debounce window.
    last_arrival: Option<Instant>,
    running: bool,
}

/// A watch created by [`PresenterWatcher`].
///
/// Holding it is what keeps the coalescing thread alive; dropping or stopping
/// it flushes whatever is queued and joins the thread, so no event is lost at
/// shutdown.
pub struct PresenterWatch {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
    root: PathBuf,
}

impl PresenterWatch {
    /// The vault this watch covers, so the host can route a presenter callback
    /// to the right one when several vaults are open.
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Push events in from the host's `NSFilePresenter` callbacks.
    pub fn deliver(&self, events: impl IntoIterator<Item = FsEvent>) {
        let mut state = match self.shared.queue.lock() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !state.running {
            return;
        }
        for event in events {
            // A rescan supersedes everything already queued: the core's
            // response is to walk the whole vault, so replaying the individual
            // events before it would be wasted work.
            if matches!(event, FsEvent::Rescan { .. }) {
                state.pending.clear();
            }
            state.pending.push_back(event);
        }
        state.last_arrival = Some(Instant::now());
        drop(state);
        self.shared.wake.notify_all();
    }

    fn shutdown(&mut self) {
        {
            let mut state = match self.shared.queue.lock() {
                Ok(s) => s,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.running = false;
        }
        self.shared.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl std::fmt::Debug for PresenterWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresenterWatch")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Drop for PresenterWatch {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl WatchHandle for PresenterWatch {
    fn stop(mut self: Box<Self>) {
        self.shutdown();
    }
}

/// Creates watches fed by the host's file presenters.
#[derive(Debug, Default)]
pub struct PresenterWatcher {
    /// Set when the host has no presenter to offer — an unusable watch is a
    /// typed failure, not a watch that silently never fires.
    unavailable: AtomicBool,
}

impl PresenterWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark watching as unavailable, so `watch` fails instead of handing back a
    /// handle that will never report anything.
    pub fn set_unavailable(&self, unavailable: bool) {
        self.unavailable.store(unavailable, Ordering::Relaxed);
    }

    /// Create a watch directly, keeping the concrete type so the host can call
    /// [`PresenterWatch::deliver`] on it.
    pub fn open(&self, root: PathBuf, options: WatchOptions, sink: Sink) -> Result<PresenterWatch> {
        if self.unavailable.load(Ordering::Relaxed) {
            return Err(PlatformError::Watcher {
                message: "no file presenter is registered for this vault".into(),
            });
        }

        let shared = Arc::new(Shared {
            queue: Mutex::new(State {
                pending: VecDeque::new(),
                last_arrival: None,
                running: true,
            }),
            wake: Condvar::new(),
        });

        let debounce = options.debounce.max(Duration::from_millis(1));
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("ie-presenter-watch".into())
            .spawn(move || coalesce(worker, debounce, sink))
            .map_err(|e| PlatformError::Watcher {
                message: format!("could not start the coalescing thread: {e}"),
            })?;

        Ok(PresenterWatch {
            shared,
            thread: Some(thread),
            root,
        })
    }
}

/// Hold events until the vault has been quiet for `debounce`, then deliver them
/// as one batch. Flushes whatever is left when the watch stops.
fn coalesce(shared: Arc<Shared>, debounce: Duration, sink: Sink) {
    loop {
        let batch = {
            let mut state = match shared.queue.lock() {
                Ok(s) => s,
                Err(poisoned) => poisoned.into_inner(),
            };

            loop {
                if !state.running {
                    break state.pending.drain(..).collect::<Vec<_>>();
                }
                match state.last_arrival {
                    None => {
                        let (guard, _) = shared
                            .wake
                            .wait_timeout(state, debounce)
                            .unwrap_or_else(|p| p.into_inner());
                        state = guard;
                    }
                    Some(at) => {
                        let quiet_for = at.elapsed();
                        if quiet_for >= debounce {
                            state.last_arrival = None;
                            break state.pending.drain(..).collect::<Vec<_>>();
                        }
                        let (guard, _) = shared
                            .wake
                            .wait_timeout(state, debounce - quiet_for)
                            .unwrap_or_else(|p| p.into_inner());
                        state = guard;
                    }
                }
            }
        };

        let running = shared
            .queue
            .lock()
            .map(|s| s.running)
            .unwrap_or_else(|p| p.into_inner().running);

        if !batch.is_empty() {
            sink(batch);
        }
        if !running {
            return;
        }
    }
}

impl FileWatcher for PresenterWatcher {
    fn watch(
        &self,
        root: PathBuf,
        options: WatchOptions,
        sink: Sink,
    ) -> Result<Box<dyn WatchHandle>> {
        Ok(Box::new(self.open(root, options, sink)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn collector() -> (Sink, mpsc::Receiver<Vec<FsEvent>>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(move |batch| tx.send(batch).unwrap()), rx)
    }

    fn opts(ms: u64) -> WatchOptions {
        WatchOptions {
            recursive: true,
            debounce: Duration::from_millis(ms),
        }
    }

    #[test]
    fn a_burst_of_events_arrives_as_one_batch() {
        let (sink, rx) = collector();
        let watch = PresenterWatcher::new()
            .open(PathBuf::from("/vault"), opts(40), sink)
            .unwrap();

        // What one save through an editor looks like from a presenter.
        watch.deliver([
            FsEvent::Created(PathBuf::from("/vault/Note.md")),
            FsEvent::Modified(PathBuf::from("/vault/Note.md")),
        ]);
        watch.deliver([FsEvent::Modified(PathBuf::from("/vault/Note.md"))]);

        let batch = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(batch.len(), 3);
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
    }

    #[test]
    fn a_rescan_discards_what_was_queued_behind_it() {
        let (sink, rx) = collector();
        let watch = PresenterWatcher::new()
            .open(PathBuf::from("/vault"), opts(40), sink)
            .unwrap();

        watch.deliver([
            FsEvent::Modified(PathBuf::from("/vault/A.md")),
            FsEvent::Deleted(PathBuf::from("/vault/B.md")),
        ]);
        // Coming back from the background: anything could have changed, so the
        // core is going to walk the vault anyway.
        watch.deliver([FsEvent::Rescan {
            root: PathBuf::from("/vault"),
        }]);
        watch.deliver([FsEvent::Created(PathBuf::from("/vault/C.md"))]);

        let batch = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            batch,
            vec![
                FsEvent::Rescan {
                    root: PathBuf::from("/vault")
                },
                FsEvent::Created(PathBuf::from("/vault/C.md")),
            ]
        );
    }

    #[test]
    fn separate_bursts_arrive_separately() {
        let (sink, rx) = collector();
        let watch = PresenterWatcher::new()
            .open(PathBuf::from("/vault"), opts(30), sink)
            .unwrap();

        watch.deliver([FsEvent::Modified(PathBuf::from("/vault/A.md"))]);
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first.len(), 1);

        watch.deliver([FsEvent::Modified(PathBuf::from("/vault/B.md"))]);
        let second = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            second,
            vec![FsEvent::Modified(PathBuf::from("/vault/B.md"))]
        );
    }

    #[test]
    fn stopping_flushes_what_is_still_queued() {
        let (sink, rx) = collector();
        // A long window, so nothing would flush on its own in the time this
        // test takes: a change saved as the app is backgrounded must not be
        // dropped because the debounce had not elapsed.
        let watch = PresenterWatcher::new()
            .open(PathBuf::from("/vault"), opts(30_000), sink)
            .unwrap();

        watch.deliver([FsEvent::Modified(PathBuf::from("/vault/Late.md"))]);
        Box::new(watch).stop();

        let batch = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            batch,
            vec![FsEvent::Modified(PathBuf::from("/vault/Late.md"))]
        );
    }

    #[test]
    fn a_watch_that_cannot_be_registered_fails_rather_than_going_quiet() {
        let watcher = PresenterWatcher::new();
        watcher.set_unavailable(true);
        let (sink, _rx) = collector();

        let err = watcher
            .open(PathBuf::from("/vault"), opts(10), sink)
            .unwrap_err();
        assert!(matches!(err, PlatformError::Watcher { .. }));
    }

    #[test]
    fn events_delivered_after_stopping_are_ignored() {
        let (sink, rx) = collector();
        let watcher = PresenterWatcher::new();
        let mut watch = watcher
            .open(PathBuf::from("/vault"), opts(10), sink)
            .unwrap();
        watch.shutdown();

        watch.deliver([FsEvent::Modified(PathBuf::from("/vault/A.md"))]);
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}

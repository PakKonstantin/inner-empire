//! Bridging core events into the webview, and running indexing off the UI
//! thread.
//!
//! Two flows meet here. The watcher produces filesystem events on its own
//! thread and they must reach the indexer, which needs the session lock. And a
//! full scan takes seconds on a large vault, so it runs on a worker while the
//! window stays responsive and reports progress.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use ie_platform::FsEvent;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::SharedState;

/// How long to let watcher events accumulate before indexing them.
///
/// The watcher already debounces per path; this second stage batches *across*
/// paths, so saving twenty files at once — a sync, a git checkout — costs one
/// indexing pass rather than twenty.
const BATCH_WINDOW: Duration = Duration::from_millis(120);

/// Build the callback the watcher calls, which forwards batches to the worker.
pub fn watch_sink(app: AppHandle) -> Box<dyn Fn(Vec<FsEvent>) + Send + 'static> {
    let sender = ensure_worker(&app);
    Box::new(move |events| {
        let _ = sender.send(events);
    })
}

/// The channel the watcher writes to, held in Tauri's state so the worker
/// outlives any single vault.
struct WatcherChannel(mpsc::Sender<Vec<FsEvent>>);

fn ensure_worker(app: &AppHandle) -> mpsc::Sender<Vec<FsEvent>> {
    if let Some(channel) = app.try_state::<WatcherChannel>() {
        return channel.0.clone();
    }

    let (sender, receiver) = mpsc::channel::<Vec<FsEvent>>();
    app.manage(WatcherChannel(sender.clone()));

    let worker_app = app.clone();
    std::thread::Builder::new()
        .name("ie-indexer".into())
        .spawn(move || indexer_loop(worker_app, receiver))
        .expect("the indexer thread could not be started");

    sender
}

/// Drain watcher batches and apply them to the index.
fn indexer_loop(app: AppHandle, receiver: mpsc::Receiver<Vec<FsEvent>>) {
    let mut pending: Vec<FsEvent> = Vec::new();
    let mut deadline: Option<Instant> = None;

    loop {
        let timeout = deadline
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(250));

        match receiver.recv_timeout(timeout) {
            Ok(events) => {
                pending.extend(events);
                deadline.get_or_insert_with(|| Instant::now() + BATCH_WINDOW);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let ready = deadline.is_some_and(|d| Instant::now() >= d);
        if !ready || pending.is_empty() {
            if pending.is_empty() {
                deadline = None;
            }
            continue;
        }

        let batch = std::mem::take(&mut pending);
        deadline = None;

        let Some(state) = app.try_state::<SharedState>() else {
            continue;
        };
        let outcome = state.with_session(|session| session.apply_events(&batch));

        match outcome {
            Ok(outcome) => {
                if outcome.needs_full_scan {
                    tracing::info!("the watcher lost events; rescanning");
                    spawn_scan(app.clone(), (*state).clone(), true);
                    continue;
                }
                for path in &outcome.indexed {
                    let _ = app.emit("fileModified", serde_json::json!({ "path": path }));
                }
                for path in &outcome.removed {
                    let _ = app.emit("fileDeleted", serde_json::json!({ "path": path }));
                }
                if !outcome.is_empty() {
                    let _ = app.emit(
                        "indexUpdated",
                        serde_json::json!({
                            "indexed": outcome.indexed.len(),
                            "removed": outcome.removed.len(),
                        }),
                    );
                }
            }
            // No vault open: the batch belonged to one that has since closed.
            Err(error) if error.code == "no_vault_open" => {}
            Err(error) => {
                tracing::warn!(code = %error.code, message = %error.message, "indexing a change failed");
                let _ = app.emit(
                    "indexError",
                    serde_json::json!({ "message": error.message }),
                );
            }
        }
    }
}

/// Run a full scan on a worker thread, reporting progress as it goes.
pub fn spawn_scan(app: AppHandle, state: SharedState, needed: bool) {
    if !needed {
        // The index was reusable. Still tell the UI how many files it holds so
        // the status bar is populated.
        if let Ok(count) = state.with_index(|session| Ok(session.db().file_count().unwrap_or(0))) {
            let _ = app.emit(
                "indexCompleted",
                serde_json::json!({ "files": count, "durationMs": 0, "diagnostics": [] }),
            );
        }
        return;
    }

    std::thread::Builder::new()
        .name("ie-scan".into())
        .spawn(move || {
            let progress_app = app.clone();
            let mut last_emit = Instant::now();

            let report = state.with_session(|session| {
                session.scan(|progress| {
                    // Throttle: a scan of fifty thousand files would otherwise
                    // flood the webview with messages it cannot render.
                    if last_emit.elapsed() > Duration::from_millis(100)
                        || progress.total == Some(progress.scanned)
                    {
                        last_emit = Instant::now();
                        let _ = progress_app.emit("indexProgress", &progress);
                    }
                })
            });

            match report {
                Ok(report) => {
                    let _ = app.emit(
                        "indexCompleted",
                        serde_json::json!({
                            "files": report.files_indexed + report.files_unchanged,
                            "durationMs": report.duration_ms,
                            "diagnostics": report.diagnostics,
                        }),
                    );
                    if !report.diagnostics.is_empty() {
                        tracing::info!(
                            count = report.diagnostics.len(),
                            "the scan found problems worth reporting"
                        );
                    }
                }
                Err(error) if error.code == "no_vault_open" => {}
                Err(error) => {
                    tracing::error!(code = %error.code, message = %error.message, "the scan failed");
                    let _ = app.emit("indexError", serde_json::json!({ "message": error.message }));
                }
            }
        })
        .expect("the scan thread could not be started");
}

/// Send a transient message to the notification area.
pub fn notify(app: &AppHandle, level: &str, message: &str) {
    let _ = app.emit(
        "notice",
        serde_json::json!({ "level": level, "message": message }),
    );
}

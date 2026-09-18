//! Opening, creating and closing vaults.

use std::path::PathBuf;

use ie_core::index::IndexProgress;
use ie_core::session::{OpenReport, VaultSession};
use ie_core::vault::VaultSettings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{CommandError, CommandResult};
use crate::state::{RecentVault, SharedState};

/// Open a folder as a vault and start indexing it in the background.
///
/// The scan runs on a worker thread so a fifty-thousand-note vault does not
/// block the window from appearing; progress arrives as events.
#[tauri::command]
pub async fn open_vault(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: String,
) -> CommandResult<OpenReport> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(CommandError::refused(format!(
            "{} is not a folder",
            root.display()
        )));
    }

    close_current(&state);

    let (mut session, report) =
        VaultSession::open(state.host.clone(), &root).map_err(CommandError::from)?;

    // Let the webview load images, PDFs and media out of this vault, and only
    // this vault. The scope is granted per vault rather than once at startup,
    // so closing a vault does not leave its files reachable.
    if let Err(e) = app
        .asset_protocol_scope()
        .allow_directory(session.root(), true)
    {
        tracing::warn!(error = %e, "could not grant asset access to the vault folder");
    }

    let needs_scan = report.index.needs_full_scan();
    if let Err(e) = session.start_watch(crate::events::watch_sink(app.clone())) {
        // Without a watcher, changes made outside the app are picked up on the
        // next scan instead of immediately. Worth saying, not worth refusing to
        // open the vault over.
        tracing::warn!(error = %e, "could not watch the vault for external changes");
        crate::events::notify(
            &app,
            "warning",
            "Changes made outside the app will not appear until the index is rebuilt.",
        );
    }
    state.set_session(Some(session));
    state.remember_vault(&report.root, &report.name);

    let _ = app.emit(
        "vaultOpened",
        serde_json::json!({ "root": report.root, "name": report.name }),
    );

    crate::events::spawn_scan(app, (*state).clone(), needs_scan);
    Ok(report)
}

/// Create a new vault folder, then open it.
#[tauri::command]
pub async fn create_vault(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: String,
    name: String,
) -> CommandResult<OpenReport> {
    let root = PathBuf::from(&path);
    if root.exists()
        && root
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        return Err(CommandError::refused(format!(
            "{} already contains files. Open it as an existing vault instead.",
            root.display()
        )));
    }

    close_current(&state);
    let (mut session, report) =
        VaultSession::create(state.host.clone(), &root, &name).map_err(CommandError::from)?;

    if let Err(e) = app
        .asset_protocol_scope()
        .allow_directory(session.root(), true)
    {
        tracing::warn!(error = %e, "could not grant asset access to the vault folder");
    }

    if let Err(e) = session.start_watch(crate::events::watch_sink(app.clone())) {
        tracing::warn!(error = %e, "could not watch the vault for external changes");
    }
    state.set_session(Some(session));
    state.remember_vault(&report.root, &report.name);

    let _ = app.emit(
        "vaultOpened",
        serde_json::json!({ "root": report.root, "name": report.name }),
    );
    crate::events::spawn_scan(app, (*state).clone(), true);
    Ok(report)
}

#[tauri::command]
pub fn close_vault(app: AppHandle, state: State<'_, SharedState>) -> CommandResult<()> {
    close_current(&state);
    let _ = app.emit("vaultClosed", ());
    Ok(())
}

/// Dropping the session stops its watcher, closes its index and releases the
/// vault folder, so this is all that closing a vault requires.
fn close_current(state: &SharedState) {
    state.set_session(None);
}

#[tauri::command]
pub fn is_vault_open(state: State<'_, SharedState>) -> bool {
    state.is_open()
}

#[tauri::command]
pub fn vault_info(state: State<'_, SharedState>) -> CommandResult<VaultInfo> {
    state.with_index(|session| {
        Ok(VaultInfo {
            root: session.root().to_string_lossy().to_string(),
            settings: session.settings().clone(),
            file_count: session.db().file_count().unwrap_or(0),
            case_sensitive: session.is_case_sensitive(),
        })
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub root: String,
    pub settings: VaultSettings,
    pub file_count: usize,
    pub case_sensitive: bool,
}

#[tauri::command]
pub fn update_vault_settings(
    state: State<'_, SharedState>,
    settings: VaultSettings,
) -> CommandResult<()> {
    state.with_session(|session| session.save_settings(settings))
}

#[tauri::command]
pub fn recent_vaults(state: State<'_, SharedState>) -> Vec<RecentVault> {
    state.recent_vaults()
}

#[tauri::command]
pub fn forget_vault(state: State<'_, SharedState>, path: String) {
    state.forget_vault(&path);
}

/// Discard the index and rebuild it from the files.
///
/// Offered in settings and after an index error. It can never lose data, which
/// is why it is safe to expose as a button.
#[tauri::command]
pub async fn rebuild_index(app: AppHandle, state: State<'_, SharedState>) -> CommandResult<()> {
    state.with_session(|session| session.db().clear())?;
    crate::events::spawn_scan(app, (*state).clone(), true);
    Ok(())
}

#[tauri::command]
pub fn index_status(state: State<'_, SharedState>) -> CommandResult<IndexProgress> {
    state.with_index(|session| {
        let files = session.db().file_count().unwrap_or(0);
        Ok(IndexProgress {
            scanned: files,
            indexed: files,
            total: Some(files),
            current: None,
        })
    })
}

#[tauri::command]
pub fn vault_diagnostics(state: State<'_, SharedState>) -> CommandResult<Vec<ie_core::Diagnostic>> {
    state.with_index(|session| session.diagnostics())
}

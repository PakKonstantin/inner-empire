//! File and folder operations, and reading notes.

use ie_core::index::queries;
use ie_core::model::{DirectoryListing, FileEntry, FolderEntry, Note, Property};
use ie_core::session::RenameOutcome;
use ie_core::vault::{Collision, TrashEntry, VaultPath};
use tauri::{AppHandle, Emitter, State};

use crate::error::{CommandError, CommandResult};
use crate::state::SharedState;

/// One level of the explorer tree.
///
/// Fetched per folder rather than as a whole tree, so opening a vault with
/// fifty thousand files costs one directory read.
#[tauri::command]
pub fn list_folder(
    state: State<'_, SharedState>,
    path: VaultPath,
) -> CommandResult<DirectoryListing> {
    state.with_index(|session| {
        let entries = session.ops().list_folder(&path)?;
        let conn = session.connection();

        let mut listing = DirectoryListing {
            path: path.clone(),
            folders: Vec::new(),
            files: Vec::new(),
        };

        for (entry_path, is_dir) in entries {
            if is_dir {
                let (files, folders) = count_children(session, &entry_path);
                listing.folders.push(FolderEntry {
                    name: entry_path.file_name().to_string(),
                    path: entry_path,
                    child_file_count: files,
                    child_folder_count: folders,
                });
            } else {
                // Prefer the indexed record, which carries the title from
                // frontmatter; fall back to the filesystem for a file the
                // indexer has not reached yet.
                match queries::file(conn, &entry_path)? {
                    Some(file) => listing.files.push(file),
                    None => {
                        let metadata = session.ops().resolve(&entry_path);
                        let size = std::fs::metadata(&metadata).map(|m| m.len()).unwrap_or(0);
                        listing.files.push(FileEntry {
                            name: entry_path.file_name().to_string(),
                            title: entry_path.stem().to_string(),
                            kind: ie_core::model::FileKind::from_extension(
                                entry_path.extension().as_deref(),
                            ),
                            size,
                            modified_ms: 0,
                            path: entry_path,
                        });
                    }
                }
            }
        }
        Ok(listing)
    })
}

fn count_children(session: &ie_core::session::VaultSession, folder: &VaultPath) -> (usize, usize) {
    session
        .ops()
        .list_folder(folder)
        .map(|entries| {
            let folders = entries.iter().filter(|(_, is_dir)| *is_dir).count();
            (entries.len() - folders, folders)
        })
        .unwrap_or((0, 0))
}

#[tauri::command]
pub fn read_note(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<Note> {
    state.with_index(|session| session.read_note(&path))
}

/// Read a file as bytes, for the PDF viewer and anything else that needs the
/// raw content through IPC rather than the asset protocol.
#[tauri::command]
pub fn read_file_bytes(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<Vec<u8>> {
    state.with_index(|session| session.ops().read_bytes(&path))
}

/// The absolute path of a vault file, for the asset protocol.
///
/// The only place an absolute path crosses into the frontend, and it is
/// derived from a `VaultPath`, so it cannot address anything outside the vault.
#[tauri::command]
pub fn resolve_asset_path(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<String> {
    state.with_index(|session| Ok(session.ops().resolve(&path).to_string_lossy().to_string()))
}

#[tauri::command]
pub fn save_note(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: VaultPath,
    content: String,
) -> CommandResult<()> {
    state.with_session(|session| session.save_note(&path, &content))?;
    let _ = app.emit("fileModified", serde_json::json!({ "path": path }));
    Ok(())
}

#[tauri::command]
pub fn create_note(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: VaultPath,
    content: String,
    overwrite: Option<bool>,
) -> CommandResult<VaultPath> {
    let collision = if overwrite.unwrap_or(false) {
        Collision::Overwrite
    } else {
        Collision::Fail
    };
    let created = state.with_session(|session| session.create_note(&path, &content, collision))?;
    let _ = app.emit("fileCreated", serde_json::json!({ "path": created }));
    Ok(created)
}

/// Create a note from a link target that does not resolve yet.
#[tauri::command]
pub fn create_note_from_link(
    app: AppHandle,
    state: State<'_, SharedState>,
    target: String,
    folder: Option<VaultPath>,
) -> CommandResult<VaultPath> {
    let created = state.with_session(|session| {
        let folder = folder
            .or_else(|| session.settings().new_note_folder.clone())
            .unwrap_or_default();
        let path = session.ops().note_path_for_title(&folder, &target)?;
        let title = path.stem().to_string();
        session.create_note(&path, &format!("# {title}\n\n"), Collision::Fail)
    })?;
    let _ = app.emit("fileCreated", serde_json::json!({ "path": created }));
    Ok(created)
}

#[tauri::command]
pub fn create_folder(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<()> {
    state.with_session(|session| session.ops().create_folder(&path))
}

/// Rename or move, rewriting every link that pointed at the file.
#[tauri::command]
pub fn rename_entry(
    app: AppHandle,
    state: State<'_, SharedState>,
    from: VaultPath,
    to: VaultPath,
) -> CommandResult<RenameOutcome> {
    let outcome = state.with_session(|session| session.rename(&from, &to))?;
    let _ = app.emit(
        "fileRenamed",
        serde_json::json!({ "from": outcome.from, "to": outcome.to }),
    );
    if outcome.links_updated > 0 {
        crate::events::notify(
            &app,
            "success",
            &format!(
                "Updated {} link{} in {} note{}.",
                outcome.links_updated,
                if outcome.links_updated == 1 { "" } else { "s" },
                outcome.files_updated,
                if outcome.files_updated == 1 { "" } else { "s" }
            ),
        );
    }
    for failure in &outcome.failures {
        crate::events::notify(&app, "error", failure);
    }
    Ok(outcome)
}

/// What a rename would change, so the UI can say so before doing it.
#[tauri::command]
pub fn preview_rename(
    state: State<'_, SharedState>,
    from: VaultPath,
    to: VaultPath,
) -> CommandResult<ie_core::links::RenamePlan> {
    state.with_index(|session| session.plan_rename(&from, &to))
}

#[tauri::command]
pub fn delete_entry(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: VaultPath,
) -> CommandResult<TrashEntry> {
    let entry = state.with_session(|session| session.delete(&path))?;
    let _ = app.emit("fileDeleted", serde_json::json!({ "path": path }));
    Ok(entry)
}

#[tauri::command]
pub fn duplicate_entry(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: VaultPath,
) -> CommandResult<VaultPath> {
    let copy = state.with_session(|session| {
        let copy = session.ops().duplicate(&path)?;
        session.reindex(&copy)?;
        Ok(copy)
    })?;
    let _ = app.emit("fileCreated", serde_json::json!({ "path": copy }));
    Ok(copy)
}

#[tauri::command]
pub fn list_trash(state: State<'_, SharedState>) -> CommandResult<Vec<TrashEntry>> {
    state.with_index(|session| session.trash().list())
}

#[tauri::command]
pub fn restore_from_trash(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> CommandResult<VaultPath> {
    let path = state.with_session(|session| session.restore(&id))?;
    let _ = app.emit("fileCreated", serde_json::json!({ "path": path }));
    Ok(path)
}

#[tauri::command]
pub fn purge_from_trash(state: State<'_, SharedState>, id: String) -> CommandResult<()> {
    state.with_index(|session| session.trash().purge(&id))
}

#[tauri::command]
pub fn empty_trash(state: State<'_, SharedState>) -> CommandResult<usize> {
    state.with_index(|session| session.trash().purge_all())
}

#[tauri::command]
pub fn set_properties(
    app: AppHandle,
    state: State<'_, SharedState>,
    path: VaultPath,
    properties: Vec<Property>,
) -> CommandResult<()> {
    state.with_session(|session| session.set_properties(&path, &properties))?;
    let _ = app.emit("fileModified", serde_json::json!({ "path": path }));
    Ok(())
}

/// Copy a dropped or pasted file into the vault's attachment folder and return
/// where it landed, so the editor can insert a link to it.
#[tauri::command]
pub fn import_attachment(
    app: AppHandle,
    state: State<'_, SharedState>,
    file_name: String,
    bytes: Vec<u8>,
    note: Option<VaultPath>,
) -> CommandResult<VaultPath> {
    let created = state.with_session(|session| {
        // Where this goes is vault semantics, not shell behaviour, so the core
        // decides: the iOS client reads the same setting through the same code
        // and cannot disagree about where an attachment lands.
        let location = session.settings().attachments.clone();
        let target = ie_core::vault::attachment_path(&location, note.as_ref(), &file_name)?;
        session.ops().create_folder(&target.parent())?;

        let path = session.ops().create_note(&target, "", Collision::Rename)?;
        session.ops().write_bytes(&path, &bytes)?;
        session.reindex(&path)?;
        Ok(path)
    })?;

    let _ = app.emit("fileCreated", serde_json::json!({ "path": created }));
    Ok(created)
}

/// Note down a buffer's unsaved text.
///
/// Called on a timer while a note is dirty, so a crash loses seconds rather
/// than everything since the last autosave. The journal lives in the
/// application's data directory, not in the vault: a half-finished scrap is
/// machine-local and should not sync anywhere.
#[tauri::command]
pub fn journal_unsaved(
    state: State<'_, SharedState>,
    path: VaultPath,
    content: String,
) -> CommandResult<()> {
    state.with_index(|session| session.journal_unsaved(&path, &content))
}

#[tauri::command]
pub fn clear_journal(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<()> {
    state.with_index(|session| session.clear_journal(&path))
}

/// Unsaved work left by a previous run, if any.
#[tauri::command]
pub fn recoverable_notes(
    state: State<'_, SharedState>,
) -> CommandResult<Vec<ie_core::recovery::RecoveryCandidate>> {
    state.with_index(|session| session.recoverable())
}

/// Drop journal entries that are already saved or older than `max_age_days`.
#[tauri::command]
pub fn prune_journal(
    state: State<'_, SharedState>,
    max_age_days: Option<i64>,
) -> CommandResult<usize> {
    let max_age_ms = max_age_days.unwrap_or(7) * 86_400_000;
    state.with_index(|session| session.prune_journal(max_age_ms))
}

/// Reveal a file in the desktop's file manager.
#[tauri::command]
pub fn reveal_in_file_manager(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<()> {
    let absolute = state.with_index(|session| Ok(session.ops().resolve(&path)))?;
    state
        .host
        .platform
        .reveal_in_file_manager(&absolute)
        .map_err(CommandError::from)
}

/// Open an external link in the user's browser.
///
/// Refuses anything that is not http or https, so a crafted note cannot
/// launch an arbitrary protocol handler.
#[tauri::command]
pub fn open_external(state: State<'_, SharedState>, url: String) -> CommandResult<()> {
    state
        .host
        .platform
        .open_external_url(&url)
        .map_err(CommandError::from)
}

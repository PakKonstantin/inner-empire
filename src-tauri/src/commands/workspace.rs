//! Workspace, templates, daily notes, settings and export.

use ie_core::export::{ExportOptions, ExportResult};
use ie_core::model::Workspace;
use ie_core::templates::{self, TemplateContext, TemplateInfo};
use ie_core::vault::VaultPath;
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::state::SharedState;

/// Load the workspace, dropping tabs whose files no longer exist.
#[tauri::command]
pub fn load_workspace(state: State<'_, SharedState>) -> CommandResult<WorkspaceLoad> {
    state.with_index(|session| {
        let ops = session.ops();
        let (workspace, removed) = session.workspaces().load_reconciled(|path| ops.exists(path));
        Ok(WorkspaceLoad { workspace, removed })
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceLoad {
    pub workspace: Workspace,
    /// Tabs dropped because their file is gone, so the UI can mention it.
    pub removed: Vec<VaultPath>,
}

#[tauri::command]
pub fn save_workspace(
    state: State<'_, SharedState>,
    workspace: Workspace,
) -> CommandResult<()> {
    state.with_index(|session| session.workspaces().save(&workspace))
}

#[tauri::command]
pub fn reset_workspace(state: State<'_, SharedState>) -> CommandResult<Workspace> {
    state.with_index(|session| session.workspaces().reset())
}

#[tauri::command]
pub fn save_workspace_as(
    state: State<'_, SharedState>,
    name: String,
    workspace: Workspace,
) -> CommandResult<()> {
    state.with_index(|session| session.workspaces().save_as(&name, &workspace))
}

#[tauri::command]
pub fn load_saved_workspace(
    state: State<'_, SharedState>,
    name: String,
) -> CommandResult<Workspace> {
    state.with_index(|session| session.workspaces().load_saved(&name))
}

#[tauri::command]
pub fn list_saved_workspaces(state: State<'_, SharedState>) -> CommandResult<Vec<String>> {
    state.with_index(|session| session.workspaces().list_saved())
}

#[tauri::command]
pub fn delete_saved_workspace(state: State<'_, SharedState>, name: String) -> CommandResult<()> {
    state.with_index(|session| session.workspaces().delete_saved(&name))
}

#[tauri::command]
pub fn list_templates(state: State<'_, SharedState>) -> CommandResult<Vec<TemplateInfo>> {
    state.with_index(|session| {
        let folder = session.settings().templates_folder.clone();
        templates::list(session.ops(), &folder)
    })
}

/// Expand a template for insertion at the cursor.
#[tauri::command]
pub fn render_template(
    state: State<'_, SharedState>,
    template: VaultPath,
    target: VaultPath,
) -> CommandResult<String> {
    let clock = std::sync::Arc::clone(&state.host.clock);
    state.with_index(|session| {
        let context = TemplateContext::new(target.stem().to_string(), target.clone(), &clock);
        templates::render(session.ops(), &template, &context)
    })
}

/// Open today's daily note, creating it from the template if needed.
#[tauri::command]
pub fn open_daily_note(
    app: tauri::AppHandle,
    state: State<'_, SharedState>,
    day_offset: Option<i64>,
) -> CommandResult<DailyNote> {
    let clock = std::sync::Arc::clone(&state.host.clock);
    let result = state.with_session(|session| {
        let settings = session.settings().daily_notes.clone();
        let (path, created) =
            templates::ensure_daily_note(session.ops(), &settings, &clock, day_offset.unwrap_or(0))?;
        if created {
            session.reindex(&path)?;
        }
        Ok(DailyNote { path, created })
    })?;

    if result.created {
        use tauri::Emitter;
        let _ = app.emit("fileCreated", serde_json::json!({ "path": result.path }));
    }
    Ok(result)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyNote {
    pub path: VaultPath,
    pub created: bool,
}

/// Export notes to a folder outside the vault.
#[tauri::command]
pub fn export_notes(
    state: State<'_, SharedState>,
    paths: Vec<VaultPath>,
    destination: String,
    options: Option<ExportRequest>,
) -> CommandResult<ExportResult> {
    let options: ExportOptions = options.unwrap_or_default().into();
    let destination = std::path::PathBuf::from(destination);

    let (result, files) = state.with_index(|session| {
        ie_core::export::export_notes(session.connection(), session.ops(), &paths, &options)
    })?;

    // Writing happens here rather than in the core, which deliberately has no
    // notion of a destination outside the vault.
    for (relative, bytes) in files {
        let target = destination.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CommandError::refused(format!("{}: {e}", parent.display())))?;
        }
        state
            .host
            .fs
            .write_atomic(&target, &bytes)
            .map_err(CommandError::from)?;
    }

    Ok(result)
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub format: Option<ie_core::export::ExportFormat>,
    pub include_linked_notes: Option<bool>,
    pub include_attachments: Option<bool>,
    pub include_properties: Option<bool>,
    pub depth: Option<usize>,
}

impl From<ExportRequest> for ExportOptions {
    fn from(request: ExportRequest) -> Self {
        let defaults = ExportOptions::default();
        ExportOptions {
            format: request.format.unwrap_or(defaults.format),
            include_linked_notes: request
                .include_linked_notes
                .unwrap_or(defaults.include_linked_notes),
            include_attachments: request
                .include_attachments
                .unwrap_or(defaults.include_attachments),
            include_properties: request
                .include_properties
                .unwrap_or(defaults.include_properties),
            depth: request.depth.unwrap_or(defaults.depth),
        }
    }
}

/// Import external files into the vault.
#[tauri::command]
pub fn import_files(
    state: State<'_, SharedState>,
    folder: VaultPath,
    sources: Vec<String>,
) -> CommandResult<ie_core::export::ImportResult> {
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for source in &sources {
        let path = std::path::PathBuf::from(source);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "imported".to_string());
        match std::fs::read(&path) {
            Ok(bytes) => payloads.push((name, bytes)),
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "skipping unreadable import"),
        }
    }

    let result = state.with_session(|session| {
        let result = ie_core::export::import_files(session.ops(), &folder, &payloads)?;
        for item in &result.imported {
            session.reindex(&item.target)?;
        }
        Ok(result)
    })?;
    Ok(result)
}

/// Where the app keeps its own files, for the About screen.
#[tauri::command]
pub fn app_directories(state: State<'_, SharedState>) -> CommandResult<AppDirectories> {
    let dirs = &state.host.dirs;
    Ok(AppDirectories {
        config: dirs.config_dir().map(path_string).unwrap_or_default(),
        data: dirs.data_dir().map(path_string).unwrap_or_default(),
        logs: dirs.log_dir().map(path_string).unwrap_or_default(),
        cache: dirs.cache_dir().map(path_string).unwrap_or_default(),
        platform: state.host.platform.kind().to_string(),
    })
}

fn path_string(path: std::path::PathBuf) -> String {
    path.to_string_lossy().to_string()
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDirectories {
    pub config: String,
    pub data: String,
    pub logs: String,
    pub cache: String,
    pub platform: String,
}

/// Application-wide preferences, stored outside any vault.
#[tauri::command]
pub fn load_app_settings(state: State<'_, SharedState>) -> CommandResult<serde_json::Value> {
    let path = state
        .host
        .dirs
        .config_dir()
        .map_err(CommandError::from)?
        .join("settings.json");
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| CommandError::refused(format!("{}: {e}", path.display())))?;
    Ok(serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({})))
}

#[tauri::command]
pub fn save_app_settings(
    state: State<'_, SharedState>,
    settings: serde_json::Value,
) -> CommandResult<()> {
    let dir = state.host.dirs.config_dir().map_err(CommandError::from)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::refused(format!("{}: {e}", dir.display())))?;
    let json = serde_json::to_string_pretty(&settings)?;
    state
        .host
        .fs
        .write_atomic(&dir.join("settings.json"), json.as_bytes())
        .map_err(CommandError::from)
}

/// The tail of the log file, for the diagnostics panel.
#[tauri::command]
pub fn read_log_tail(
    state: State<'_, SharedState>,
    lines: Option<usize>,
) -> CommandResult<String> {
    let dir = state.host.dirs.log_dir().map_err(CommandError::from)?;
    Ok(ie_core::logging::tail(&dir, lines.unwrap_or(200)).unwrap_or_default())
}

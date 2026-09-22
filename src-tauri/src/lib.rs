//! The Tauri shell.
//!
//! Deliberately thin. It owns the window, the IPC surface and the worker
//! threads, and nothing else: every decision about what a vault is, how
//! Markdown parses or how links resolve lives in `ie-core`, which does not
//! depend on Tauri and can therefore be reused by a future command-line tool,
//! sync engine or headless indexer.

mod commands;
mod error;
mod events;
mod state;
mod window_state;

use std::sync::Arc;

use ie_core::logging::LogLevel;
use ie_platform::HostServices;
use state::{AppState, SharedState};
use tauri::{Manager, WindowEvent};

/// Build and run the application.
pub fn run() {
    let host = match HostServices::for_current_platform() {
        Ok(host) => host,
        Err(error) => {
            // Without standard directories there is nowhere to put settings or
            // logs. Say so plainly rather than failing later in a confusing way.
            eprintln!("Inner Empire could not start: {error}");
            std::process::exit(1);
        }
    };

    if let Err(error) = host.dirs.ensure_all() {
        eprintln!("Inner Empire could not create its application directories: {error}");
        std::process::exit(1);
    }

    if let Ok(log_dir) = host.dirs.log_dir() {
        if let Err(error) = ie_core::logging::init(&log_dir, LogLevel::Info) {
            eprintln!("logging is unavailable: {error}");
        }
    }

    tracing::info!(
        platform = %host.platform.kind(),
        version = env!("CARGO_PKG_VERSION"),
        "starting"
    );

    let state: SharedState = Arc::new(AppState::new(host));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            // The window is created by the config, then moved back to wherever
            // the user last left it — before it is shown, so restoring is not
            // a visible jump.
            if let Some(webview) = app.get_webview_window("main") {
                window_state::restore(&webview.as_ref().window());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Geometry is written on the way out rather than on every drag:
            // a resize fires continuously, and a settings file is not a place
            // for that much churn.
            if matches!(
                event,
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
            ) {
                window_state::save(window);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::open_vault,
            commands::vault::create_vault,
            commands::vault::close_vault,
            commands::vault::is_vault_open,
            commands::vault::vault_info,
            commands::vault::update_vault_settings,
            commands::vault::recent_vaults,
            commands::vault::forget_vault,
            commands::vault::rebuild_index,
            commands::vault::index_status,
            commands::vault::vault_diagnostics,
            commands::files::list_folder,
            commands::files::read_note,
            commands::files::read_file_bytes,
            commands::files::resolve_asset_path,
            commands::files::save_note,
            commands::files::create_note,
            commands::files::create_note_from_link,
            commands::files::create_folder,
            commands::files::rename_entry,
            commands::files::preview_rename,
            commands::files::delete_entry,
            commands::files::duplicate_entry,
            commands::files::list_trash,
            commands::files::restore_from_trash,
            commands::files::purge_from_trash,
            commands::files::empty_trash,
            commands::files::set_properties,
            commands::files::import_attachment,
            commands::files::journal_unsaved,
            commands::files::clear_journal,
            commands::files::recoverable_notes,
            commands::files::prune_journal,
            commands::files::reveal_in_file_manager,
            commands::files::open_external,
            commands::index_queries::backlinks,
            commands::index_queries::outgoing_links,
            commands::index_queries::outline,
            commands::index_queries::note_blocks,
            commands::index_queries::unlinked_mentions,
            commands::index_queries::unresolved_links,
            commands::index_queries::ambiguous_links,
            commands::index_queries::resolve_link,
            commands::index_queries::all_tags,
            commands::index_queries::files_with_tag,
            commands::index_queries::property_keys,
            commands::index_queries::property_values,
            commands::index_queries::recent_files,
            commands::index_queries::graph,
            commands::index_queries::local_graph,
            commands::search::search_vault,
            commands::search::validate_query,
            commands::search::quick_switch,
            commands::search::complete_tags,
            commands::search::complete_headings,
            commands::search::complete_blocks,
            commands::workspace::load_workspace,
            commands::workspace::save_workspace,
            commands::workspace::reset_workspace,
            commands::workspace::save_workspace_as,
            commands::workspace::load_saved_workspace,
            commands::workspace::list_saved_workspaces,
            commands::workspace::delete_saved_workspace,
            commands::workspace::list_templates,
            commands::workspace::render_template,
            commands::workspace::open_daily_note,
            commands::workspace::export_notes,
            commands::workspace::import_files,
            commands::workspace::app_directories,
            commands::workspace::load_app_settings,
            commands::workspace::save_app_settings,
            commands::workspace::read_log_tail,
        ])
        .run(tauri::generate_context!())
        .expect("the application window could not be created");
}

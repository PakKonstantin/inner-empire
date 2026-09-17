//! Search and autocomplete.

use ie_core::search::{self, engine, FileMatch, SearchOptions, SearchResults};
use ie_core::vault::VaultPath;
use tauri::State;

use crate::error::{CommandError, CommandResult};
use crate::state::SharedState;

#[tauri::command]
pub fn search_vault(
    state: State<'_, SharedState>,
    query: String,
    limit: Option<usize>,
    offset: Option<usize>,
) -> CommandResult<SearchResults> {
    let parsed = search::parse(&query).map_err(CommandError::from)?;
    let options = SearchOptions {
        limit: limit.unwrap_or(100),
        offset: offset.unwrap_or(0),
        ..SearchOptions::default()
    };
    state.with_index(|session| engine::search(session.connection(), &parsed, &options))
}

/// Check a query without running it, so the search box can show a syntax error
/// as the user types rather than after they press enter.
#[tauri::command]
pub fn validate_query(query: String) -> CommandResult<()> {
    search::parse(&query)
        .map(|_| ())
        .map_err(CommandError::from)
}

#[tauri::command]
pub fn quick_switch(
    state: State<'_, SharedState>,
    needle: String,
    limit: Option<usize>,
) -> CommandResult<Vec<FileMatch>> {
    state.with_index(|session| {
        engine::quick_switch(session.connection(), &needle, limit.unwrap_or(50))
    })
}

#[tauri::command]
pub fn complete_tags(
    state: State<'_, SharedState>,
    needle: String,
    limit: Option<usize>,
) -> CommandResult<Vec<(String, usize)>> {
    state.with_index(|session| {
        engine::complete_tags(session.connection(), &needle, limit.unwrap_or(20))
    })
}

#[tauri::command]
pub fn complete_headings(
    state: State<'_, SharedState>,
    path: VaultPath,
    needle: String,
    limit: Option<usize>,
) -> CommandResult<Vec<String>> {
    state.with_index(|session| {
        engine::complete_headings(session.connection(), &path, &needle, limit.unwrap_or(20))
    })
}

/// Blocks in a note, for `[[Note#^` autocomplete.
#[tauri::command]
pub fn complete_blocks(
    state: State<'_, SharedState>,
    path: VaultPath,
    limit: Option<usize>,
) -> CommandResult<Vec<String>> {
    state.with_index(|session| {
        let mut ids: Vec<String> = ie_core::index::queries::blocks(session.connection(), &path)?
            .into_iter()
            .map(|b| b.id)
            .collect();
        ids.truncate(limit.unwrap_or(50));
        Ok(ids)
    })
}

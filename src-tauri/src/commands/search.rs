//! Search and autocomplete.

use ie_core::search::{self, engine, Clause, ClauseKind, FileMatch, SearchOptions, SearchResults};
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

/// Describe a query's clauses, for the chips above the search results.
///
/// The clauses come from the parser that runs the search rather than from a
/// second reading of the string in the frontend: one language, one definition
/// of what it means, and no chance of the chips saying something the results
/// disagree with.
#[tauri::command]
pub fn describe_query(query: String) -> CommandResult<Vec<QueryClause>> {
    search::describe(&query)
        .map(|clauses| clauses.iter().map(QueryClause::from).collect())
        .map_err(CommandError::from)
}

/// One clause, as the frontend sees it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryClause {
    /// Exactly the text this clause was written as, so removing its chip can
    /// cut it back out of the query.
    pub source: String,
    pub kind: String,
    pub label: String,
    pub negated: bool,
}

impl From<&Clause> for QueryClause {
    fn from(clause: &Clause) -> Self {
        Self {
            source: clause.source.clone(),
            kind: match clause.kind {
                ClauseKind::Text => "text",
                ClauseKind::Phrase => "phrase",
                ClauseKind::Tag => "tag",
                ClauseKind::Path => "path",
                ClauseKind::File => "file",
                ClauseKind::Extension => "extension",
                ClauseKind::Section => "section",
                ClauseKind::Property => "property",
                ClauseKind::Structural => "structural",
            }
            .to_string(),
            label: clause.label.clone(),
            negated: clause.negated,
        }
    }
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

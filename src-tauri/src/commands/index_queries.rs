//! Read-only queries against the index: links, tags, outline, graph.

use ie_core::index::queries::{self, GraphOptions, UnresolvedTarget};
use ie_core::links::{LinkResolution, LinkResolver};
use ie_core::model::{Backlink, Block, FileEntry, GraphData, Heading, ResolvedLink, TagSummary};
use ie_core::vault::VaultPath;
use tauri::State;

use crate::error::CommandResult;
use crate::state::SharedState;

#[tauri::command]
pub fn backlinks(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<Vec<Backlink>> {
    state.with_index(|session| queries::backlinks(session.connection(), &path))
}

#[tauri::command]
pub fn outgoing_links(
    state: State<'_, SharedState>,
    path: VaultPath,
) -> CommandResult<Vec<ResolvedLink>> {
    state.with_index(|session| queries::outgoing_links(session.connection(), &path))
}

#[tauri::command]
pub fn outline(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<Vec<Heading>> {
    state.with_index(|session| queries::headings(session.connection(), &path))
}

#[tauri::command]
pub fn note_blocks(state: State<'_, SharedState>, path: VaultPath) -> CommandResult<Vec<Block>> {
    state.with_index(|session| queries::blocks(session.connection(), &path))
}

/// Mentions of a note's title in other notes that are *not* links, so the user
/// can turn them into links.
///
/// Implemented as a phrase search for the title, minus the notes that already
/// link, rather than by scanning every file.
#[tauri::command]
pub fn unlinked_mentions(
    state: State<'_, SharedState>,
    path: VaultPath,
    limit: Option<usize>,
) -> CommandResult<Vec<ie_core::search::SearchHit>> {
    use ie_core::search::{
        engine,
        query::{Query, Term},
        SearchOptions,
    };

    state.with_index(|session| {
        let conn = session.connection();
        let title = queries::file(conn, &path)?
            .map(|f| f.title)
            .unwrap_or_else(|| path.stem().to_string());
        if title.trim().is_empty() {
            return Ok(Vec::new());
        }

        let linking: Vec<String> = queries::backlinks(conn, &path)?
            .into_iter()
            .map(|b| b.source_path.as_str().to_string())
            .collect();

        let query = Query {
            terms: vec![Term::Phrase(title)],
            filters: Vec::new(),
        };
        let results = engine::search(
            conn,
            &query,
            &SearchOptions {
                limit: limit.unwrap_or(50),
                ..SearchOptions::default()
            },
        )?;

        Ok(results
            .hits
            .into_iter()
            .filter(|hit| hit.path != path && !linking.contains(&hit.path.as_str().to_string()))
            .collect())
    })
}

#[tauri::command]
pub fn unresolved_links(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> CommandResult<Vec<UnresolvedTarget>> {
    state
        .with_index(|session| queries::unresolved_links(session.connection(), limit.unwrap_or(200)))
}

#[tauri::command]
pub fn ambiguous_links(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> CommandResult<Vec<UnresolvedTarget>> {
    state.with_index(|session| queries::ambiguous_links(session.connection(), limit.unwrap_or(200)))
}

/// Follow a link written in a note.
#[tauri::command]
pub fn resolve_link(
    state: State<'_, SharedState>,
    from: VaultPath,
    target: String,
) -> CommandResult<LinkResolution> {
    state.with_index(|session| LinkResolver::resolve_target(session.connection(), &from, &target))
}

#[tauri::command]
pub fn all_tags(state: State<'_, SharedState>) -> CommandResult<Vec<TagSummary>> {
    state.with_index(|session| queries::tag_summaries(session.connection()))
}

#[tauri::command]
pub fn files_with_tag(
    state: State<'_, SharedState>,
    tag: String,
    limit: Option<usize>,
) -> CommandResult<Vec<FileEntry>> {
    state.with_index(|session| {
        queries::files_with_tag(session.connection(), &tag, limit.unwrap_or(500))
    })
}

#[tauri::command]
pub fn property_keys(state: State<'_, SharedState>) -> CommandResult<Vec<(String, usize)>> {
    state.with_index(|session| queries::property_keys(session.connection()))
}

#[tauri::command]
pub fn property_values(
    state: State<'_, SharedState>,
    key: String,
    limit: Option<usize>,
) -> CommandResult<Vec<String>> {
    state.with_index(|session| {
        queries::property_values(session.connection(), &key, limit.unwrap_or(100))
    })
}

#[tauri::command]
pub fn recent_files(
    state: State<'_, SharedState>,
    limit: Option<usize>,
) -> CommandResult<Vec<FileEntry>> {
    state.with_index(|session| queries::recent_files(session.connection(), limit.unwrap_or(20)))
}

/// Options the graph view sends, mirroring `GraphOptions` but with every field
/// optional so the frontend can send a partial update.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRequest {
    pub include_attachments: Option<bool>,
    pub include_unresolved: Option<bool>,
    pub include_tags: Option<bool>,
    pub folder: Option<VaultPath>,
    pub max_nodes: Option<usize>,
}

impl From<GraphRequest> for GraphOptions {
    fn from(request: GraphRequest) -> Self {
        let defaults = GraphOptions::default();
        GraphOptions {
            include_attachments: request
                .include_attachments
                .unwrap_or(defaults.include_attachments),
            include_unresolved: request
                .include_unresolved
                .unwrap_or(defaults.include_unresolved),
            include_tags: request.include_tags.unwrap_or(defaults.include_tags),
            folder: request.folder,
            max_nodes: request.max_nodes.unwrap_or(defaults.max_nodes),
        }
    }
}

#[tauri::command]
pub fn graph(
    state: State<'_, SharedState>,
    options: Option<GraphRequest>,
) -> CommandResult<GraphData> {
    let options: GraphOptions = options.unwrap_or_default().into();
    state.with_index(|session| queries::graph(session.connection(), &options))
}

#[tauri::command]
pub fn local_graph(
    state: State<'_, SharedState>,
    path: VaultPath,
    depth: Option<usize>,
    options: Option<GraphRequest>,
) -> CommandResult<GraphData> {
    let options: GraphOptions = options.unwrap_or_default().into();
    state.with_index(|session| {
        queries::local_graph(session.connection(), &path, depth.unwrap_or(1), &options)
    })
}

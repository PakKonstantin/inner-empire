//! Reads against the index.
//!
//! Every query here is written so its work is proportional to the answer, not
//! to the size of the vault: backlinks hit `links_target_file`, the tag
//! explorer aggregates over `tags_fold`, and nothing does a full scan of
//! `files` unless the caller genuinely asked for every file.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Diagnostic, Result};
use crate::model::{
    Backlink, Block, FileEntry, FileKind, GraphData, GraphEdge, GraphNode, GraphNodeKind, Heading,
    LinkKind, ResolvedLink, TagSummary,
};
use crate::vault::path::VaultPath;

/// A file's id, or `None` if it is not indexed.
pub fn file_id(conn: &Connection, path: &VaultPath) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT id FROM files WHERE path = ?1", [path.as_str()], |r| {
            r.get(0)
        })
        .optional()?)
}

fn file_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileEntry> {
    let path = VaultPath::from_indexed(row.get::<_, String>(0)?);
    let title: Option<String> = row.get(4)?;
    Ok(FileEntry {
        name: path.file_name().to_string(),
        title: title.unwrap_or_else(|| path.stem().to_string()),
        kind: FileKind::parse(&row.get::<_, String>(1)?),
        size: row.get::<_, i64>(2)? as u64,
        modified_ms: row.get(3)?,
        path,
    })
}

const FILE_COLUMNS: &str = "path, kind, size, mtime_ms, title";

/// Every indexed file, newest first. Capped, because "show me everything" in a
/// 50k-note vault is a request the UI should paginate.
pub fn all_files(conn: &Connection, limit: usize) -> Result<Vec<FileEntry>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {FILE_COLUMNS} FROM files ORDER BY mtime_ms DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([limit as i64], file_entry_from_row)?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Files most recently modified, for the quick switcher's empty state.
pub fn recent_files(conn: &Connection, limit: usize) -> Result<Vec<FileEntry>> {
    all_files(conn, limit)
}

/// One file's indexed record.
pub fn file(conn: &Connection, path: &VaultPath) -> Result<Option<FileEntry>> {
    Ok(conn
        .query_row(
            &format!("SELECT {FILE_COLUMNS} FROM files WHERE path = ?1"),
            [path.as_str()],
            file_entry_from_row,
        )
        .optional()?)
}

/// Headings of a note, in document order. Drives the outline panel.
pub fn headings(conn: &Connection, path: &VaultPath) -> Result<Vec<Heading>> {
    let mut stmt = conn.prepare_cached(
        "SELECT h.level, h.text, h.slug, h.line, h.byte_start
         FROM headings h JOIN files f ON f.id = h.file_id
         WHERE f.path = ?1 ORDER BY h.ordinal",
    )?;
    let rows = stmt
        .query_map([path.as_str()], |row| {
            Ok(Heading {
                level: row.get::<_, i64>(0)? as u8,
                text: row.get(1)?,
                slug: row.get(2)?,
                line: row.get::<_, i64>(3)? as usize,
                byte_start: row.get::<_, i64>(4)? as usize,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// A note's identified blocks.
pub fn blocks(conn: &Connection, path: &VaultPath) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare_cached(
        "SELECT b.block_id, b.line, b.byte_start, b.byte_end
         FROM blocks b JOIN files f ON f.id = b.file_id
         WHERE f.path = ?1 ORDER BY b.line",
    )?;
    let rows = stmt
        .query_map([path.as_str()], |row| {
            Ok(Block {
                id: row.get(0)?,
                line: row.get::<_, i64>(1)? as usize,
                byte_start: row.get::<_, i64>(2)? as usize,
                byte_end: row.get::<_, i64>(3)? as usize,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Links a note contains, resolved where possible.
pub fn outgoing_links(conn: &Connection, path: &VaultPath) -> Result<Vec<ResolvedLink>> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.kind, l.raw, l.target_text, l.heading, l.block_id, l.alias,
                l.line, l.byte_start, l.byte_end, t.path
         FROM links l
         JOIN files f ON f.id = l.file_id
         LEFT JOIN files t ON t.id = l.target_file_id
         WHERE f.path = ?1
         ORDER BY l.byte_start",
    )?;
    let rows = stmt
        .query_map([path.as_str()], |row| {
            Ok(ResolvedLink {
                link: crate::model::Link {
                    kind: LinkKind::parse(&row.get::<_, String>(0)?),
                    raw: row.get(1)?,
                    target: row.get(2)?,
                    heading: row.get(3)?,
                    block_id: row.get(4)?,
                    alias: row.get(5)?,
                    line: row.get::<_, i64>(6)? as usize,
                    byte_start: row.get::<_, i64>(7)? as usize,
                    byte_end: row.get::<_, i64>(8)? as usize,
                },
                target_path: row
                    .get::<_, Option<String>>(9)?
                    .map(VaultPath::from_indexed),
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Notes that link to this one, with the line of context the panel shows.
///
/// The context is read from the index rather than from disk, so opening a note
/// with two hundred backlinks does not open two hundred files.
pub fn backlinks(conn: &Connection, path: &VaultPath) -> Result<Vec<Backlink>> {
    let mut stmt = conn.prepare_cached(
        "SELECT s.path, s.title, l.kind, l.line, l.alias, l.raw
         FROM links l
         JOIN files s ON s.id = l.file_id
         JOIN files t ON t.id = l.target_file_id
         WHERE t.path = ?1
         ORDER BY s.path, l.line",
    )?;
    let rows = stmt
        .query_map([path.as_str()], |row| {
            let source_path = VaultPath::from_indexed(row.get::<_, String>(0)?);
            let title: Option<String> = row.get(1)?;
            Ok(Backlink {
                source_title: title.unwrap_or_else(|| source_path.stem().to_string()),
                source_path,
                kind: LinkKind::parse(&row.get::<_, String>(2)?),
                line: row.get::<_, i64>(3)? as usize,
                alias: row.get(4)?,
                context: row.get(5)?,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

pub fn backlink_count(conn: &Connection, path: &VaultPath) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM links l
         JOIN files t ON t.id = l.target_file_id
         WHERE t.path = ?1",
        [path.as_str()],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

/// A link target nobody has created yet, and how often it is written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedTarget {
    pub target: String,
    pub count: usize,
    /// Notes that reference it, capped for display.
    pub sources: Vec<VaultPath>,
}

/// Every link target with no file behind it.
pub fn unresolved_links(conn: &Connection, limit: usize) -> Result<Vec<UnresolvedTarget>> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.target_text, count(*) AS uses,
                group_concat(DISTINCT f.path) AS sources
         FROM links l JOIN files f ON f.id = l.file_id
         WHERE l.target_file_id IS NULL AND l.kind <> 'external' AND l.target_text <> ''
         GROUP BY lower(l.target_text)
         ORDER BY uses DESC, l.target_text
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], |row| {
            let sources: Option<String> = row.get(2)?;
            Ok(UnresolvedTarget {
                target: row.get(0)?,
                count: row.get::<_, i64>(1)? as usize,
                sources: sources
                    .unwrap_or_default()
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .take(20)
                    .map(|s| VaultPath::from_indexed(s.to_string()))
                    .collect(),
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Links that resolved, but where more than one file answered equally well.
pub fn ambiguous_links(conn: &Connection, limit: usize) -> Result<Vec<UnresolvedTarget>> {
    let mut stmt = conn.prepare_cached(
        "SELECT l.target_text, count(*) AS uses, group_concat(DISTINCT f.path)
         FROM links l JOIN files f ON f.id = l.file_id
         WHERE l.ambiguous = 1
         GROUP BY lower(l.target_text)
         ORDER BY uses DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], |row| {
            let sources: Option<String> = row.get(2)?;
            Ok(UnresolvedTarget {
                target: row.get(0)?,
                count: row.get::<_, i64>(1)? as usize,
                sources: sources
                    .unwrap_or_default()
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| VaultPath::from_indexed(s.to_string()))
                    .collect(),
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Every tag with its own count and the count including nested children.
///
/// `#AI` reports its own uses plus those of `#AI/LLM` and `#AI/RAG`, which is
/// what makes the tag tree's numbers add up.
pub fn tag_summaries(conn: &Connection) -> Result<Vec<TagSummary>> {
    let mut stmt =
        conn.prepare_cached("SELECT tag, count(*) FROM tags GROUP BY tag_fold ORDER BY tag")?;
    let exact: Vec<(String, usize)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?
        .filter_map(std::result::Result::ok)
        .collect();

    let mut summaries: Vec<TagSummary> = Vec::with_capacity(exact.len());
    for (name, count) in &exact {
        let prefix = format!("{}/", name.to_lowercase());
        let total = exact
            .iter()
            .filter(|(other, _)| {
                other.to_lowercase() == name.to_lowercase()
                    || other.to_lowercase().starts_with(&prefix)
            })
            .map(|(_, c)| *c)
            .sum();
        summaries.push(TagSummary {
            name: name.clone(),
            count: *count,
            total_count: total,
        });
    }
    Ok(summaries)
}

/// Notes carrying a tag, including its nested children.
pub fn files_with_tag(conn: &Connection, tag: &str, limit: usize) -> Result<Vec<FileEntry>> {
    let fold = tag.trim_start_matches('#').to_lowercase();
    let prefix = format!("{fold}/");
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT DISTINCT {} FROM files f JOIN tags t ON t.file_id = f.id
         WHERE t.tag_fold = ?1 OR t.tag_fold LIKE ?2 ESCAPE '\\'
         ORDER BY f.mtime_ms DESC LIMIT ?3",
        FILE_COLUMNS.replace("path", "f.path")
    ))?;
    let rows = stmt
        .query_map(
            params![fold, format!("{}%", escape_like(&prefix)), limit as i64],
            file_entry_from_row,
        )?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Distinct property keys with how many notes use each, for autocomplete.
pub fn property_keys(conn: &Connection) -> Result<Vec<(String, usize)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT key, count(DISTINCT file_id) FROM properties GROUP BY key_fold ORDER BY key",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Distinct values seen for a property key, for autocomplete.
pub fn property_values(conn: &Connection, key: &str, limit: usize) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT text_value FROM properties
         WHERE key_fold = ?1 AND text_value IS NOT NULL AND text_value <> ''
         ORDER BY text_value LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![key.to_lowercase(), limit as i64], |row| row.get(0))?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Paths that differ only by capitalisation.
///
/// Legal on ext4 and destructive on NTFS, so a vault containing any is not
/// portable and the user needs to know.
pub fn case_conflicts(conn: &Connection) -> Result<Vec<Vec<VaultPath>>> {
    let mut stmt = conn.prepare_cached(
        "SELECT group_concat(path, char(10)) FROM files
         GROUP BY path_fold HAVING count(*) > 1",
    )?;
    let groups = stmt
        .query_map([], |row| {
            let joined: String = row.get(0)?;
            Ok(joined
                .split('\n')
                .map(|s| VaultPath::from_indexed(s.to_string()))
                .collect::<Vec<_>>())
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(groups)
}

/// Diagnostics recorded by the last scan.
pub fn diagnostics(conn: &Connection, limit: usize) -> Result<Vec<Diagnostic>> {
    let mut stmt =
        conn.prepare_cached("SELECT payload FROM diagnostics ORDER BY id LIMIT ?1")?;
    let rows = stmt
        .query_map([limit as i64], |row| row.get::<_, String>(0))?
        .filter_map(std::result::Result::ok)
        .filter_map(|json| serde_json::from_str(&json).ok())
        .collect();
    Ok(rows)
}

/// What the graph view should draw.
#[derive(Debug, Clone)]
pub struct GraphOptions {
    /// Include attachments as nodes.
    pub include_attachments: bool,
    /// Include link targets that do not exist.
    pub include_unresolved: bool,
    /// Render tags as nodes joined to the notes that carry them.
    pub include_tags: bool,
    /// Only notes inside this folder, when set.
    pub folder: Option<VaultPath>,
    /// Refuse to return more than this many nodes, so a huge vault degrades
    /// into a truncated graph rather than a frozen window.
    pub max_nodes: usize,
}

impl Default for GraphOptions {
    fn default() -> Self {
        Self {
            include_attachments: false,
            include_unresolved: true,
            include_tags: false,
            folder: None,
            max_nodes: 5_000,
        }
    }
}

/// Build the whole-vault graph.
pub fn graph(conn: &Connection, options: &GraphOptions) -> Result<GraphData> {
    let mut data = GraphData::default();
    let mut node_ids: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    let kind_filter = if options.include_attachments {
        "1 = 1"
    } else {
        "f.kind IN ('note', 'canvas')"
    };
    let folder_prefix = options
        .folder
        .as_ref()
        .filter(|f| !f.is_root())
        .map(|f| format!("{}/", f.as_str()));

    let mut stmt = conn.prepare(&format!(
        "SELECT f.path, f.kind, f.title,
                (SELECT count(*) FROM links l WHERE l.file_id = f.id AND l.target_file_id IS NOT NULL)
              + (SELECT count(*) FROM links l WHERE l.target_file_id = f.id) AS degree
         FROM files f
         WHERE {kind_filter}
         ORDER BY degree DESC
         LIMIT ?1"
    ))?;

    let rows = stmt.query_map([options.max_nodes as i64], |row| {
        let path = VaultPath::from_indexed(row.get::<_, String>(0)?);
        let kind = FileKind::parse(&row.get::<_, String>(1)?);
        let title: Option<String> = row.get(2)?;
        Ok((path, kind, title, row.get::<_, i64>(3)? as usize))
    })?;

    for row in rows.filter_map(std::result::Result::ok) {
        let (path, kind, title, degree) = row;
        if let Some(prefix) = &folder_prefix {
            if !path.as_str().starts_with(prefix.as_str()) {
                continue;
            }
        }
        let id = path.as_str().to_string();
        node_ids.insert(id.clone(), data.nodes.len());
        data.nodes.push(GraphNode {
            label: title.unwrap_or_else(|| path.stem().to_string()),
            folder: path.parent().as_str().to_string(),
            kind: match kind {
                FileKind::Note | FileKind::Canvas => GraphNodeKind::Note,
                _ => GraphNodeKind::Attachment,
            },
            path: Some(path),
            id,
            degree,
            tags: Vec::new(),
        });
    }

    let total_files: i64 = conn.query_row(
        &format!("SELECT count(*) FROM files f WHERE {kind_filter}"),
        [],
        |row| row.get(0),
    )?;
    data.truncated = total_files as usize > data.nodes.len();

    // Resolved edges.
    let mut edge_stmt = conn.prepare(
        "SELECT s.path, t.path, l.kind FROM links l
         JOIN files s ON s.id = l.file_id
         JOIN files t ON t.id = l.target_file_id",
    )?;
    for edge in edge_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                LinkKind::parse(&row.get::<_, String>(2)?),
            ))
        })?
        .filter_map(std::result::Result::ok)
    {
        let (source, target, kind) = edge;
        if node_ids.contains_key(&source) && node_ids.contains_key(&target) && source != target {
            data.edges.push(GraphEdge {
                source,
                target,
                kind,
            });
        }
    }

    if options.include_unresolved {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT s.path, l.target_text FROM links l
             JOIN files s ON s.id = l.file_id
             WHERE l.target_file_id IS NULL AND l.kind <> 'external' AND l.target_text <> ''",
        )?;
        for row in stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .filter_map(std::result::Result::ok)
        {
            let (source, target) = row;
            if !node_ids.contains_key(&source) {
                continue;
            }
            let node_id = format!("unresolved:{target}");
            if !node_ids.contains_key(&node_id) {
                node_ids.insert(node_id.clone(), data.nodes.len());
                data.nodes.push(GraphNode {
                    id: node_id.clone(),
                    path: None,
                    label: target.clone(),
                    kind: GraphNodeKind::Unresolved,
                    degree: 0,
                    tags: Vec::new(),
                    folder: String::new(),
                });
            }
            data.edges.push(GraphEdge {
                source,
                target: node_id,
                kind: LinkKind::WikiLink,
            });
        }
    }

    if options.include_tags {
        let mut stmt = conn.prepare("SELECT DISTINCT f.path, t.tag FROM tags t JOIN files f ON f.id = t.file_id")?;
        for row in stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .filter_map(std::result::Result::ok)
        {
            let (path, tag) = row;
            if !node_ids.contains_key(&path) {
                continue;
            }
            let node_id = format!("tag:{tag}");
            if !node_ids.contains_key(&node_id) {
                node_ids.insert(node_id.clone(), data.nodes.len());
                data.nodes.push(GraphNode {
                    id: node_id.clone(),
                    path: None,
                    label: format!("#{tag}"),
                    kind: GraphNodeKind::Tag,
                    degree: 0,
                    tags: Vec::new(),
                    folder: String::new(),
                });
            }
            data.edges.push(GraphEdge {
                source: path,
                target: node_id,
                kind: LinkKind::WikiLink,
            });
        }
    }

    // Degree is what sizes a node, so count the edges actually drawn rather
    // than every link in the vault.
    let mut degrees: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for edge in &data.edges {
        *degrees.entry(edge.source.as_str()).or_default() += 1;
        *degrees.entry(edge.target.as_str()).or_default() += 1;
    }
    let drawn: Vec<(String, usize)> = degrees
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    for (id, degree) in drawn {
        if let Some(index) = node_ids.get(&id) {
            data.nodes[*index].degree = degree;
        }
    }

    Ok(data)
}

/// The neighbourhood of one note, out to `depth` hops.
pub fn local_graph(
    conn: &Connection,
    center: &VaultPath,
    depth: usize,
    options: &GraphOptions,
) -> Result<GraphData> {
    let full = graph(conn, options)?;
    let center_id = center.as_str().to_string();
    if !full.nodes.iter().any(|n| n.id == center_id) {
        return Ok(GraphData::default());
    }

    // Breadth-first over the undirected graph: a note you link to and a note
    // that links to you are both neighbours.
    let mut adjacency: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for edge in &full.edges {
        adjacency
            .entry(edge.source.as_str())
            .or_default()
            .push(edge.target.as_str());
        adjacency
            .entry(edge.target.as_str())
            .or_default()
            .push(edge.source.as_str());
    }

    let mut keep: std::collections::HashSet<String> = std::collections::HashSet::new();
    keep.insert(center_id.clone());
    let mut frontier = vec![center_id.as_str()];
    for _ in 0..depth {
        let mut next = Vec::new();
        for node in frontier {
            for neighbour in adjacency.get(node).into_iter().flatten() {
                if keep.insert((*neighbour).to_string()) {
                    next.push(*neighbour);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    Ok(GraphData {
        nodes: full
            .nodes
            .into_iter()
            .filter(|n| keep.contains(&n.id))
            .collect(),
        edges: full
            .edges
            .into_iter()
            .filter(|e| keep.contains(&e.source) && keep.contains(&e.target))
            .collect(),
        truncated: full.truncated,
    })
}

/// Escape the wildcards SQL `LIKE` would otherwise interpret.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

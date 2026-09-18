//! Running a parsed query against the index.
//!
//! Shape of the work: filters narrow a candidate set using indexed columns,
//! then FTS5 ranks what is left with `bm25` and produces snippets. Doing it in
//! that order is what keeps `tag:AI neural` fast in a large vault — the
//! expensive text match sees only the notes that already passed the cheap
//! filters.

use rusqlite::{types::Value, Connection};

use crate::error::Result;
use crate::model::FileKind;
use crate::search::fuzzy;
use crate::search::query::{self, Query};
use crate::vault::path::VaultPath;

/// One hit.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: VaultPath,
    pub title: String,
    pub kind: FileKind,
    /// The matched text with surrounding context. Empty for a filter-only
    /// query, which has no text to highlight.
    pub snippet: String,
    /// Line the first match falls on, where known, so clicking a result lands
    /// in the right place.
    pub line: Option<usize>,
    pub modified_ms: i64,
    /// Lower is a better match. Exposed so a caller can merge result sets.
    pub rank: f64,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub offset: usize,
    /// Characters of context around a match.
    pub snippet_tokens: usize,
    pub highlight_open: String,
    pub highlight_close: String,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            offset: 0,
            snippet_tokens: 12,
            highlight_open: "<mark>".into(),
            highlight_close: "</mark>".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Total matches before paging, so the UI can say "1–100 of 4,312".
    pub total: usize,
    pub truncated: bool,
}

/// Run a query.
pub fn search(conn: &Connection, query: &Query, options: &SearchOptions) -> Result<SearchResults> {
    if query.is_empty() {
        return Ok(SearchResults::default());
    }

    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    for filter in &query.filters {
        let (sql, filter_params) = query::filter_sql(filter);
        conditions.push(sql);
        params.extend(filter_params);
    }

    match query.to_fts_expression() {
        Some(expression) => search_with_text(conn, &expression, conditions, params, options),
        None => search_filters_only(conn, conditions, params, options),
    }
}

fn search_with_text(
    conn: &Connection,
    expression: &str,
    filters: Vec<String>,
    filter_params: Vec<Value>,
    options: &SearchOptions,
) -> Result<SearchResults> {
    // Counting and selecting need the text match expressed differently: the
    // count has no FTS table to join against, so it uses a subquery, while the
    // select joins so `snippet()` and `bm25()` have a table to work on.
    let total: i64 = {
        let mut count_conditions = filters.clone();
        count_conditions.insert(
            0,
            "f.id IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?)".into(),
        );
        let mut count_params: Vec<Value> = vec![Value::Text(expression.to_string())];
        count_params.extend(filter_params.iter().cloned());
        let mut stmt = conn.prepare(&format!(
            "SELECT count(*) FROM files f WHERE {}",
            count_conditions.join(" AND ")
        ))?;
        stmt.query_row(rusqlite::params_from_iter(count_params.iter()), |row| {
            row.get(0)
        })?
    };

    let mut select_conditions = filters;
    // FTS5's auxiliary functions resolve their first argument as a table name,
    // not as a correlation name, so the join must not be aliased.
    select_conditions.push("notes_fts MATCH ?".into());

    // Parameters bind in the order their `?` appears in the statement text: the
    // three snippet arguments in the SELECT list, then the filters, then the
    // MATCH, then paging. Assembling the list right beside the statement it
    // belongs to is what keeps the two in step.
    let mut bound: Vec<Value> = vec![
        Value::Text(options.highlight_open.clone()),
        Value::Text(options.highlight_close.clone()),
        Value::Integer(options.snippet_tokens as i64),
    ];
    bound.extend(filter_params);
    bound.push(Value::Text(expression.to_string()));
    bound.push(Value::Integer(options.limit as i64));
    bound.push(Value::Integer(options.offset as i64));

    let mut stmt = conn.prepare(&format!(
        "SELECT f.path, f.title, f.kind, f.mtime_ms,
                snippet(notes_fts, 2, ?, ?, '…', ?) AS snip,
                bm25(notes_fts, 2.0, 8.0, 1.0) AS rank
         FROM files f
         JOIN notes_fts ON notes_fts.rowid = f.id
         WHERE {}
         ORDER BY rank
         LIMIT ? OFFSET ?",
        select_conditions.join(" AND ")
    ))?;

    let hits = stmt
        .query_map(rusqlite::params_from_iter(bound.iter()), |row| {
            let path = VaultPath::from_indexed(row.get::<_, String>(0)?);
            Ok(SearchHit {
                title: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| path.stem().to_string()),
                kind: FileKind::parse(&row.get::<_, String>(2)?),
                modified_ms: row.get(3)?,
                snippet: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                line: None,
                rank: row.get(5)?,
                path,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();

    Ok(SearchResults {
        truncated: total as usize > options.offset + hits.len(),
        total: total as usize,
        hits,
    })
}

fn search_filters_only(
    conn: &Connection,
    conditions: Vec<String>,
    params: Vec<Value>,
    options: &SearchOptions,
) -> Result<SearchResults> {
    let where_clause = if conditions.is_empty() {
        "1 = 1".to_string()
    } else {
        conditions.join(" AND ")
    };

    let total: i64 = {
        let mut stmt = conn.prepare(&format!(
            "SELECT count(*) FROM files f WHERE {where_clause}"
        ))?;
        stmt.query_row(rusqlite::params_from_iter(params.iter()), |row| row.get(0))?
    };

    let mut stmt = conn.prepare(&format!(
        "SELECT f.path, f.title, f.kind, f.mtime_ms
         FROM files f WHERE {where_clause}
         ORDER BY f.mtime_ms DESC
         LIMIT ? OFFSET ?"
    ))?;

    let mut ordered = params.clone();
    ordered.push(Value::Integer(options.limit as i64));
    ordered.push(Value::Integer(options.offset as i64));

    let hits = stmt
        .query_map(rusqlite::params_from_iter(ordered.iter()), |row| {
            let path = VaultPath::from_indexed(row.get::<_, String>(0)?);
            Ok(SearchHit {
                title: row
                    .get::<_, Option<String>>(1)?
                    .unwrap_or_else(|| path.stem().to_string()),
                kind: FileKind::parse(&row.get::<_, String>(2)?),
                modified_ms: row.get(3)?,
                path,
                snippet: String::new(),
                line: None,
                rank: 0.0,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();

    Ok(SearchResults {
        truncated: total as usize > options.offset + hits.len(),
        total: total as usize,
        hits,
    })
}

/// A quick-switcher hit.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatch {
    pub path: VaultPath,
    pub title: String,
    pub kind: FileKind,
    pub score: i32,
    /// Character offsets within `path` that matched, for highlighting.
    pub positions: Vec<usize>,
    pub modified_ms: i64,
}

/// Fuzzy-match filenames, for the "open note" palette.
///
/// An empty query returns the most recently modified files, which is the right
/// answer for an empty palette.
pub fn quick_switch(conn: &Connection, needle: &str, limit: usize) -> Result<Vec<FileMatch>> {
    let mut stmt = conn
        .prepare_cached("SELECT path, title, kind, mtime_ms FROM files ORDER BY mtime_ms DESC")?;
    let rows: Vec<(String, Option<String>, String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .filter_map(std::result::Result::ok)
        .collect();

    let mut matches: Vec<FileMatch> = Vec::new();
    for (path_text, title, kind, modified_ms) in rows {
        let hit = match fuzzy::score_path(needle, &path_text) {
            Some(hit) => hit,
            None => continue,
        };
        let path = VaultPath::from_indexed(path_text);
        matches.push(FileMatch {
            title: title.unwrap_or_else(|| path.stem().to_string()),
            kind: FileKind::parse(&kind),
            score: hit.score,
            positions: hit.positions,
            modified_ms,
            path,
        });
    }

    if needle.trim().is_empty() {
        matches.truncate(limit);
        return Ok(matches);
    }

    // Score first, recency second: two equally good name matches should offer
    // the one worked on most recently.
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.modified_ms.cmp(&a.modified_ms))
            .then_with(|| a.path.as_str().cmp(b.path.as_str()))
    });
    matches.truncate(limit);
    Ok(matches)
}

/// Tags whose name contains `needle`, for autocomplete in the editor.
pub fn complete_tags(
    conn: &Connection,
    needle: &str,
    limit: usize,
) -> Result<Vec<(String, usize)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT tag, count(*) AS uses FROM tags
         WHERE tag_fold LIKE ? ESCAPE '\\'
         GROUP BY tag_fold ORDER BY uses DESC, tag LIMIT ?",
    )?;
    let pattern = format!(
        "%{}%",
        needle
            .trim_start_matches('#')
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

/// Headings in one note matching `needle`, for `[[Note#` autocomplete.
pub fn complete_headings(
    conn: &Connection,
    path: &VaultPath,
    needle: &str,
    limit: usize,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT h.text FROM headings h JOIN files f ON f.id = h.file_id
         WHERE f.path = ? AND lower(h.text) LIKE ? ESCAPE '\\'
         ORDER BY h.ordinal LIMIT ?",
    )?;
    let pattern = format!(
        "%{}%",
        needle
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows = stmt
        .query_map(
            rusqlite::params![path.as_str(), pattern, limit as i64],
            |row| row.get(0),
        )?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

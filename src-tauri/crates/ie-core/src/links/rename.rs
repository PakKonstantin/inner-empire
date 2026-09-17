//! Keeping links correct when a note is renamed or moved.
//!
//! This is the feature that makes a wiki-style vault trustworthy: if renaming
//! `My Project.md` silently breaks every `[[My Project]]` in the vault, the
//! links stop being worth writing. So a rename produces a *plan* — every file
//! that must change, and the exact byte ranges within it — which the caller
//! can show the user before anything is written.
//!
//! Only links that actually pointed at the renamed file are rewritten. A link
//! whose text happens to match but which resolved elsewhere is left alone,
//! which is why the plan is built from the index's resolution rather than from
//! string matching.

use rusqlite::Connection;

use crate::error::Result;
use crate::markdown::transform::{Edit, MarkdownTransformer};
use crate::model::{Link, LinkKind};
use crate::vault::path::VaultPath;
use crate::vault::settings::LinkStyle;

/// One file's worth of changes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameEdit {
    pub path: VaultPath,
    /// How many links in this file change.
    pub link_count: usize,
}

/// Everything a rename implies.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePlan {
    pub from: VaultPath,
    pub to: VaultPath,
    /// Files whose links will be rewritten.
    pub edits: Vec<RenameEdit>,
    /// Total links affected across all files.
    pub total_links: usize,
}

impl RenamePlan {
    pub fn is_noop(&self) -> bool {
        self.edits.is_empty()
    }
}

/// What text a link to `to` should use, given the vault's link style.
///
/// `ShortestWikiLink` uses the bare title when exactly one file answers to it,
/// and falls back to the full path when several do — so shortening a link can
/// never make it ambiguous.
pub fn preferred_target_text(
    conn: &Connection,
    to: &VaultPath,
    style: LinkStyle,
    original_kind: LinkKind,
) -> Result<String> {
    // A Markdown link addresses a file, so it keeps the extension; a wiki link
    // addresses a note by title.
    if matches!(original_kind, LinkKind::Markdown | LinkKind::MarkdownImage) {
        return Ok(to.as_str().to_string());
    }

    let is_note = matches!(to.extension().as_deref(), Some("md" | "markdown"));
    let full = if is_note {
        // Drop the `.md`: `[[Folder/Note]]` is the conventional spelling.
        to.as_str()
            .strip_suffix(&format!(".{}", to.extension().unwrap_or_default()))
            .unwrap_or(to.as_str())
            .to_string()
    } else {
        to.as_str().to_string()
    };

    match style {
        LinkStyle::AbsoluteWikiLink | LinkStyle::MarkdownLink => Ok(full),
        LinkStyle::ShortestWikiLink => {
            let stem = to.stem();
            let competitors: i64 = conn.query_row(
                "SELECT count(*) FROM files WHERE stem_fold = ?1",
                [stem.to_lowercase()],
                |row| row.get(0),
            )?;
            // Only shorten when the short form is unambiguous.
            Ok(if competitors <= 1 {
                if is_note {
                    stem.to_string()
                } else {
                    to.file_name().to_string()
                }
            } else {
                full
            })
        }
    }
}

/// Work out which files reference `from`, and how many links each contains.
///
/// Reads from the index, so it costs one indexed query rather than a scan of
/// the vault.
pub fn plan_rename(conn: &Connection, from: &VaultPath, to: &VaultPath) -> Result<RenamePlan> {
    let mut stmt = conn.prepare(
        "SELECT s.path, count(*) FROM links l
         JOIN files s ON s.id = l.file_id
         JOIN files t ON t.id = l.target_file_id
         WHERE t.path = ?1 AND l.kind <> 'external'
         GROUP BY s.path
         ORDER BY s.path",
    )?;
    let edits: Vec<RenameEdit> = stmt
        .query_map([from.as_str()], |row| {
            Ok(RenameEdit {
                path: VaultPath::from_indexed(row.get::<_, String>(0)?),
                link_count: row.get::<_, i64>(1)? as usize,
            })
        })?
        .filter_map(std::result::Result::ok)
        .collect();

    let total_links = edits.iter().map(|e| e.link_count).sum();
    Ok(RenamePlan {
        from: from.clone(),
        to: to.clone(),
        edits,
        total_links,
    })
}

/// Rewrite the links in one file's source.
///
/// `targets` is the set of link-target strings that resolved to the renamed
/// file, taken from the index. Matching on that set rather than on the raw
/// text is what keeps a same-named but differently-resolved link untouched.
pub fn rewrite_source(
    source: &str,
    targets: &[String],
    new_wiki_target: &str,
    new_markdown_target: &str,
) -> Result<String> {
    let transformer = MarkdownTransformer::new();
    let matches =
        |link: &Link| -> bool { targets.iter().any(|t| t.eq_ignore_ascii_case(&link.target)) };

    let mut edits: Vec<Edit> = Vec::new();
    for (kinds, replacement) in [
        (vec![LinkKind::WikiLink, LinkKind::Embed], new_wiki_target),
        (
            vec![LinkKind::Markdown, LinkKind::MarkdownImage],
            new_markdown_target,
        ),
    ] {
        edits.extend(transformer.retarget_links(
            source,
            |link| kinds.contains(&link.kind) && matches(link),
            replacement,
        ));
    }

    MarkdownTransformer::apply(source, edits).map_err(|message| crate::error::CoreError::Refused {
        operation: "update links",
        reason: message,
    })
}

/// The exact link-target strings in `source_path` that resolved to `target`.
pub fn targets_pointing_at(
    conn: &Connection,
    source_path: &VaultPath,
    target: &VaultPath,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT l.target_text FROM links l
         JOIN files s ON s.id = l.file_id
         JOIN files t ON t.id = l.target_file_id
         WHERE s.path = ?1 AND t.path = ?2",
    )?;
    let rows = stmt
        .query_map([source_path.as_str(), target.as_str()], |row| row.get(0))?
        .filter_map(std::result::Result::ok)
        .collect();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::schema;

    fn db_with(files: &[(&str, &str)]) -> Connection {
        // A minimal index built by hand, so these tests exercise the rename
        // logic rather than the indexer.
        let conn = Connection::open_in_memory().unwrap();
        schema::initialize(&conn).unwrap();
        for (id, (path, _)) in files.iter().enumerate() {
            let vp = VaultPath::parse(path).unwrap();
            conn.execute(
                "INSERT INTO files(id, path, path_fold, name, name_fold, stem_fold, ext, kind, size, mtime_ms, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'note', 0, 0, 0)",
                rusqlite::params![
                    id as i64 + 1,
                    vp.as_str(),
                    vp.fold(),
                    vp.file_name(),
                    vp.file_name().to_lowercase(),
                    vp.stem().to_lowercase(),
                    vp.extension()
                ],
            )
            .unwrap();
        }
        conn
    }

    fn add_link(conn: &Connection, source_id: i64, target_id: Option<i64>, target_text: &str) {
        conn.execute(
            "INSERT INTO links(file_id, kind, raw, target_text, target_fold, target_tail, target_file_id, line, byte_start, byte_end)
             VALUES (?1, 'wikiLink', ?2, ?3, ?4, ?5, ?6, 0, 0, 0)",
            rusqlite::params![
                source_id,
                format!("[[{target_text}]]"),
                target_text,
                target_text.to_lowercase(),
                crate::index::resolve::target_tail(target_text),
                target_id
            ],
        )
        .unwrap();
    }

    #[test]
    fn the_plan_lists_every_file_that_links_to_the_renamed_note() {
        let conn = db_with(&[("Target.md", ""), ("A.md", ""), ("B.md", ""), ("C.md", "")]);
        add_link(&conn, 2, Some(1), "Target");
        add_link(&conn, 2, Some(1), "Target");
        add_link(&conn, 3, Some(1), "Target");
        add_link(&conn, 4, None, "Something Else");

        let plan = plan_rename(
            &conn,
            &VaultPath::parse("Target.md").unwrap(),
            &VaultPath::parse("Renamed.md").unwrap(),
        )
        .unwrap();

        assert_eq!(plan.edits.len(), 2);
        assert_eq!(plan.edits[0].path.as_str(), "A.md");
        assert_eq!(plan.edits[0].link_count, 2);
        assert_eq!(plan.total_links, 3);
        assert!(!plan.is_noop());
    }

    #[test]
    fn renaming_a_note_nothing_links_to_is_a_no_op_plan() {
        let conn = db_with(&[("Lonely.md", "")]);
        let plan = plan_rename(
            &conn,
            &VaultPath::parse("Lonely.md").unwrap(),
            &VaultPath::parse("Still Lonely.md").unwrap(),
        )
        .unwrap();
        assert!(plan.is_noop());
        assert_eq!(plan.total_links, 0);
    }

    #[test]
    fn the_shortest_style_uses_the_bare_title_when_it_is_unambiguous() {
        let conn = db_with(&[("Projects/Plan.md", "")]);
        let text = preferred_target_text(
            &conn,
            &VaultPath::parse("Projects/Plan.md").unwrap(),
            LinkStyle::ShortestWikiLink,
            LinkKind::WikiLink,
        )
        .unwrap();
        assert_eq!(text, "Plan");
    }

    #[test]
    fn the_shortest_style_falls_back_to_the_full_path_when_the_title_is_taken() {
        let conn = db_with(&[("Projects/Plan.md", ""), ("Archive/Plan.md", "")]);
        let text = preferred_target_text(
            &conn,
            &VaultPath::parse("Projects/Plan.md").unwrap(),
            LinkStyle::ShortestWikiLink,
            LinkKind::WikiLink,
        )
        .unwrap();
        assert_eq!(
            text, "Projects/Plan",
            "shortening must never introduce ambiguity"
        );
    }

    #[test]
    fn the_absolute_style_always_writes_the_full_path_without_the_extension() {
        let conn = db_with(&[("Projects/Plan.md", "")]);
        let text = preferred_target_text(
            &conn,
            &VaultPath::parse("Projects/Plan.md").unwrap(),
            LinkStyle::AbsoluteWikiLink,
            LinkKind::WikiLink,
        )
        .unwrap();
        assert_eq!(text, "Projects/Plan");
    }

    #[test]
    fn a_markdown_link_keeps_the_extension_because_it_addresses_a_file() {
        let conn = db_with(&[("Projects/Plan.md", "")]);
        let text = preferred_target_text(
            &conn,
            &VaultPath::parse("Projects/Plan.md").unwrap(),
            LinkStyle::ShortestWikiLink,
            LinkKind::Markdown,
        )
        .unwrap();
        assert_eq!(text, "Projects/Plan.md");
    }

    #[test]
    fn an_attachment_keeps_its_extension_in_a_wiki_link() {
        let conn = db_with(&[("Attachments/diagram.png", "")]);
        let text = preferred_target_text(
            &conn,
            &VaultPath::parse("Attachments/diagram.png").unwrap(),
            LinkStyle::ShortestWikiLink,
            LinkKind::WikiLink,
        )
        .unwrap();
        assert_eq!(text, "diagram.png");
    }

    #[test]
    fn rewriting_updates_only_the_links_that_pointed_at_the_renamed_note() {
        let source = "See [[Plan]] and [[Other Plan]] and [[Plan|the plan]].\n";
        let result = rewrite_source(source, &["Plan".into()], "New Plan", "New Plan.md").unwrap();
        assert_eq!(
            result,
            "See [[New Plan]] and [[Other Plan]] and [[New Plan|the plan]].\n"
        );
    }

    #[test]
    fn rewriting_handles_wiki_and_markdown_links_in_the_same_file() {
        let source = "[[Plan]] and [label](Plan.md) and ![[Plan]]\n";
        let result = rewrite_source(
            source,
            &["Plan".into(), "Plan.md".into()],
            "New Plan",
            "New Plan.md",
        )
        .unwrap();
        assert_eq!(
            result,
            "[[New Plan]] and [label](New%20Plan.md) and ![[New Plan]]\n"
        );
    }

    #[test]
    fn rewriting_preserves_headings_blocks_and_aliases() {
        let source = "[[Plan#Scope]] [[Plan#^key]] [[Plan#Scope|see]]\n";
        let result = rewrite_source(source, &["Plan".into()], "New", "New.md").unwrap();
        assert_eq!(result, "[[New#Scope]] [[New#^key]] [[New#Scope|see]]\n");
    }

    #[test]
    fn rewriting_leaves_links_inside_code_untouched() {
        let source = "Real [[Plan]] and `[[Plan]]`\n";
        let result = rewrite_source(source, &["Plan".into()], "New", "New.md").unwrap();
        assert_eq!(result, "Real [[New]] and `[[Plan]]`\n");
    }

    #[test]
    fn rewriting_matches_link_text_case_insensitively() {
        let source = "[[plan]] and [[PLAN]]\n";
        let result = rewrite_source(source, &["Plan".into()], "New", "New.md").unwrap();
        assert_eq!(result, "[[New]] and [[New]]\n");
    }

    #[test]
    fn the_targets_query_returns_the_spellings_actually_used() {
        let conn = db_with(&[("Projects/Plan.md", ""), ("A.md", "")]);
        add_link(&conn, 2, Some(1), "Plan");
        add_link(&conn, 2, Some(1), "Projects/Plan");

        let mut targets = targets_pointing_at(
            &conn,
            &VaultPath::parse("A.md").unwrap(),
            &VaultPath::parse("Projects/Plan.md").unwrap(),
        )
        .unwrap();
        targets.sort();
        assert_eq!(targets, vec!["Plan", "Projects/Plan"]);
    }
}

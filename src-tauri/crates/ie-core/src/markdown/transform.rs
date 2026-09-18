//! `MarkdownTransformer` — edits to a note's source, guided by its parse.
//!
//! Every method here rewrites exact byte ranges the parser identified. None of
//! them reformats, re-indents or re-serialises the document. That restraint is
//! the whole point: a rename must change the link and nothing else, so a user
//! who diffs their vault afterwards sees one changed line.

use std::ops::Range;

use crate::links::reference::LinkTarget;
use crate::markdown::{frontmatter, MarkdownParser};
use crate::model::{Link, LinkKind, Property};

/// One replacement, in source byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

#[derive(Debug, Clone, Default)]
pub struct MarkdownTransformer {
    parser: MarkdownParser,
}

impl MarkdownTransformer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply edits to a source string.
    ///
    /// Edits are applied back to front so that each range still refers to the
    /// text it was computed against. Overlapping edits are refused rather than
    /// silently producing garbage.
    pub fn apply(source: &str, mut edits: Vec<Edit>) -> std::result::Result<String, String> {
        edits.sort_by_key(|e| (e.range.start, e.range.end));
        for pair in edits.windows(2) {
            if pair[0].range.end > pair[1].range.start {
                return Err(format!(
                    "overlapping edits at {}..{} and {}..{}",
                    pair[0].range.start, pair[0].range.end, pair[1].range.start, pair[1].range.end
                ));
            }
        }
        if let Some(last) = edits.last() {
            if last.range.end > source.len() {
                return Err("an edit extends past the end of the document".into());
            }
        }

        let mut out = source.to_string();
        for edit in edits.into_iter().rev() {
            if !out.is_char_boundary(edit.range.start) || !out.is_char_boundary(edit.range.end) {
                return Err("an edit does not fall on a character boundary".into());
            }
            out.replace_range(edit.range, &edit.replacement);
        }
        Ok(out)
    }

    /// Rewrite every link whose target matches `from` so it points at `to`.
    ///
    /// Aliases, heading references and block references are preserved: renaming
    /// `Project` to `Game Project` turns `[[Project#Scope|the scope]]` into
    /// `[[Game Project#Scope|the scope]]` and leaves everything else alone.
    ///
    /// `matches` decides what counts as "the same target", which is where case
    /// sensitivity and shortest-unique-path policy live; this function only
    /// applies the decision.
    pub fn retarget_links<F>(&self, source: &str, matches: F, new_target: &str) -> Vec<Edit>
    where
        F: Fn(&Link) -> bool,
    {
        let document = self.parser.parse(source);
        document
            .metadata
            .links
            .iter()
            .filter(|link| link.kind.is_internal() && matches(link))
            .filter_map(|link| self.retarget_one(link, new_target))
            .collect()
    }

    fn retarget_one(&self, link: &Link, new_target: &str) -> Option<Edit> {
        let replacement = match link.kind {
            LinkKind::WikiLink | LinkKind::Embed => {
                let target = LinkTarget {
                    path: new_target.to_string(),
                    heading: link.heading.clone(),
                    block_id: link.block_id.clone(),
                    alias: link.alias.clone(),
                };
                target.to_wikilink(link.kind == LinkKind::Embed)
            }
            LinkKind::Markdown | LinkKind::MarkdownImage => {
                // Keep the display text exactly as written; only the
                // destination changes.
                let label = markdown_link_label(&link.raw)?;
                let mut destination = encode_destination(new_target);
                if let Some(block) = &link.block_id {
                    destination.push_str(&format!("#^{block}"));
                } else if let Some(heading) = &link.heading {
                    destination.push_str(&format!("#{heading}"));
                }
                let bang = if link.kind == LinkKind::MarkdownImage {
                    "!"
                } else {
                    ""
                };
                format!("{bang}[{label}]({destination})")
            }
            LinkKind::External => return None,
        };

        if replacement == link.raw {
            return None;
        }
        Some(Edit {
            range: link.byte_start..link.byte_end,
            replacement,
        })
    }

    /// Replace the frontmatter block, leaving the body untouched.
    pub fn set_properties(&self, source: &str, properties: &[Property]) -> String {
        frontmatter::replace(source, properties)
    }

    /// Set a single property, adding it if absent and preserving the order of
    /// the others.
    pub fn set_property(
        &self,
        source: &str,
        key: &str,
        value: crate::model::PropertyValue,
    ) -> String {
        let (mut properties, _) = frontmatter::parse(source);
        match properties
            .iter_mut()
            .find(|p| p.key.eq_ignore_ascii_case(key))
        {
            Some(existing) => existing.value = value,
            None => properties.push(Property {
                key: key.to_string(),
                value,
            }),
        }
        frontmatter::replace(source, &properties)
    }

    /// Remove a property. Removing the last one removes the block.
    pub fn remove_property(&self, source: &str, key: &str) -> String {
        let (mut properties, _) = frontmatter::parse(source);
        properties.retain(|p| !p.key.eq_ignore_ascii_case(key));
        frontmatter::replace(source, &properties)
    }

    /// Append a block identifier to the line containing `offset`, or return the
    /// existing one if that line already has a marker.
    ///
    /// This is what "copy link to this block" needs: a reference is only usable
    /// once the block has a stable id, and creating one must not disturb the
    /// text.
    pub fn ensure_block_id(
        &self,
        source: &str,
        offset: usize,
        generate_id: impl FnOnce() -> String,
    ) -> (String, String, Option<Edit>) {
        let document = self.parser.parse(source);
        let lines = crate::markdown::text::LineIndex::new(source);
        let line = lines.line_of(offset.min(source.len()));

        if let Some(existing) = document
            .metadata
            .blocks
            .iter()
            .find(|b| b.line == line || (b.byte_start <= offset && offset <= b.byte_end))
        {
            return (source.to_string(), existing.id.clone(), None);
        }

        let range = lines.line_range(line);
        let text = lines.line_text(source, line);
        let trimmed_len = text.trim_end().len();
        let id = generate_id();
        let edit = Edit {
            range: (range.start + trimmed_len)..(range.start + trimmed_len),
            replacement: format!(" ^{id}"),
        };
        let updated =
            Self::apply(source, vec![edit.clone()]).unwrap_or_else(|_| source.to_string());
        (updated, id, Some(edit))
    }
}

/// Pull the label out of `[label](dest)` or `![label](dest)`.
fn markdown_link_label(raw: &str) -> Option<String> {
    let start = raw.find('[')? + 1;
    let mut depth = 1usize;
    for (offset, ch) in raw[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(raw[start..start + offset].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Escape the characters that would end a Markdown link destination early.
fn encode_destination(target: &str) -> String {
    target
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PropertyValue;

    fn retarget(source: &str, from: &str, to: &str) -> String {
        let transformer = MarkdownTransformer::new();
        let edits = transformer.retarget_links(source, |link| link.target == from, to);
        MarkdownTransformer::apply(source, edits).unwrap()
    }

    #[test]
    fn a_plain_wiki_link_is_retargeted() {
        assert_eq!(
            retarget("See [[My Project]] today.", "My Project", "My Game Project"),
            "See [[My Game Project]] today."
        );
    }

    #[test]
    fn an_alias_survives_the_rename() {
        assert_eq!(
            retarget("[[My Project|the plan]]", "My Project", "My Game Project"),
            "[[My Game Project|the plan]]"
        );
    }

    #[test]
    fn heading_and_block_references_survive_the_rename() {
        assert_eq!(
            retarget("[[Project#Scope]]", "Project", "Game Project"),
            "[[Game Project#Scope]]"
        );
        assert_eq!(
            retarget("[[Project#^key-point]]", "Project", "Game Project"),
            "[[Game Project#^key-point]]"
        );
        assert_eq!(
            retarget("[[Project#Scope|see scope]]", "Project", "Game Project"),
            "[[Game Project#Scope|see scope]]"
        );
    }

    #[test]
    fn an_embed_stays_an_embed() {
        assert_eq!(
            retarget("![[Project]]", "Project", "Game Project"),
            "![[Game Project]]"
        );
    }

    #[test]
    fn markdown_links_are_retargeted_with_their_label_intact() {
        assert_eq!(
            retarget("[the plan](Project.md)", "Project.md", "Game Project.md"),
            "[the plan](Game%20Project.md)"
        );
        assert_eq!(
            retarget("![a diagram](old.png)", "old.png", "new.png"),
            "![a diagram](new.png)"
        );
    }

    #[test]
    fn external_links_are_never_touched() {
        let source = "[site](https://example.org/Project)";
        assert_eq!(retarget(source, "https://example.org/Project", "x"), source);
    }

    #[test]
    fn links_inside_code_are_not_retargeted() {
        let source = "Real [[Project]] and `[[Project]]` and\n\n```\n[[Project]]\n```\n";
        let result = retarget(source, "Project", "Renamed");
        assert!(result.contains("Real [[Renamed]]"));
        assert!(result.contains("`[[Project]]`"), "{result}");
        assert!(result.contains("```\n[[Project]]\n```"), "{result}");
    }

    #[test]
    fn everything_outside_the_links_is_byte_identical() {
        let source = "Line one.\n\nSee [[Old]] here.   \n\n\tindented\n";
        let result = retarget(source, "Old", "New");
        assert_eq!(result, "Line one.\n\nSee [[New]] here.   \n\n\tindented\n");
    }

    #[test]
    fn several_links_on_one_line_are_all_retargeted() {
        assert_eq!(retarget("[[A]] [[A]] [[A]]", "A", "B"), "[[B]] [[B]] [[B]]");
    }

    #[test]
    fn a_no_op_rename_produces_no_edits() {
        let transformer = MarkdownTransformer::new();
        let edits = transformer.retarget_links("[[Same]]", |l| l.target == "Same", "Same");
        assert!(edits.is_empty());
    }

    #[test]
    fn overlapping_edits_are_refused_rather_than_corrupting_the_file() {
        let result = MarkdownTransformer::apply(
            "abcdef",
            vec![
                Edit {
                    range: 0..3,
                    replacement: "X".into(),
                },
                Edit {
                    range: 2..5,
                    replacement: "Y".into(),
                },
            ],
        );
        assert!(result.is_err());
    }

    #[test]
    fn independent_edits_apply_in_any_order() {
        let result = MarkdownTransformer::apply(
            "one two three",
            vec![
                Edit {
                    range: 8..13,
                    replacement: "THREE".into(),
                },
                Edit {
                    range: 0..3,
                    replacement: "ONE".into(),
                },
            ],
        )
        .unwrap();
        assert_eq!(result, "ONE two THREE");
    }

    #[test]
    fn setting_a_property_preserves_the_others_and_the_body() {
        let transformer = MarkdownTransformer::new();
        let source = "---\ntitle: A\nstatus: draft\n---\n# Body\n\ntext\n";
        let updated =
            transformer.set_property(source, "status", PropertyValue::Text("active".into()));
        assert!(updated.ends_with("# Body\n\ntext\n"));
        let (properties, _) = frontmatter::parse(&updated);
        assert_eq!(
            frontmatter::get(&properties, "title").unwrap().as_text(),
            "A"
        );
        assert_eq!(
            frontmatter::get(&properties, "status").unwrap().as_text(),
            "active"
        );
    }

    #[test]
    fn setting_a_new_property_appends_it() {
        let transformer = MarkdownTransformer::new();
        let updated = transformer.set_property("Body\n", "rating", PropertyValue::Number(8.0));
        let (properties, _) = frontmatter::parse(&updated);
        assert_eq!(properties.len(), 1);
        assert!(updated.ends_with("Body\n"));
    }

    #[test]
    fn removing_the_last_property_removes_the_block() {
        let transformer = MarkdownTransformer::new();
        assert_eq!(
            transformer.remove_property("---\nonly: 1\n---\nBody\n", "only"),
            "Body\n"
        );
    }

    #[test]
    fn a_block_id_is_appended_to_the_line_the_cursor_is_on() {
        let transformer = MarkdownTransformer::new();
        let source = "First line.\nSecond line.\n";
        let (updated, id, edit) =
            transformer.ensure_block_id(source, 14, || "generated".to_string());
        assert_eq!(id, "generated");
        assert!(edit.is_some());
        assert_eq!(updated, "First line.\nSecond line. ^generated\n");
    }

    #[test]
    fn an_existing_block_id_is_reused_rather_than_duplicated() {
        let transformer = MarkdownTransformer::new();
        let source = "A line. ^already-there\n";
        let (updated, id, edit) = transformer.ensure_block_id(source, 2, || "new".to_string());
        assert_eq!(id, "already-there");
        assert_eq!(updated, source);
        assert!(edit.is_none());
    }

    #[test]
    fn a_link_label_containing_brackets_is_extracted_correctly() {
        assert_eq!(
            markdown_link_label("[see [the plan]](old.md)").as_deref(),
            Some("see [the plan]")
        );
    }
}

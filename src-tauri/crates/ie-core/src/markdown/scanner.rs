//! The app's Markdown extensions: wiki links, embeds, tags and block
//! identifiers.
//!
//! These are not CommonMark, so a CommonMark parser cannot find them. What it
//! *can* do is tell us where code, HTML and ordinary links are, and this
//! scanner runs only outside those regions. That is the difference between
//! "parsing Markdown with regular expressions" — which the brief rules out,
//! rightly — and scanning plain prose for a small, unambiguous grammar after a
//! real parser has carved out everything else.

use std::ops::Range;

use crate::links::reference::LinkTarget;
use crate::markdown::text::{ExclusionZones, LineIndex};
use crate::model::{Block, Link, LinkKind, Tag};

/// Find `[[wiki links]]` and `![[embeds]]` outside the excluded regions.
pub fn scan_wikilinks(source: &str, zones: &ExclusionZones, lines: &LineIndex) -> Vec<Link> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while cursor + 1 < bytes.len() {
        let Some(found) = find_from(bytes, cursor, b"[[") else {
            break;
        };

        // `![[` is an embed; the `!` is part of the link's source text and must
        // be included so a rename rewrites the whole construct.
        let is_embed = found > 0 && bytes[found - 1] == b'!';
        let raw_start = if is_embed { found - 1 } else { found };

        if zones.covers(raw_start) {
            cursor = found + 2;
            continue;
        }

        let Some(close) = find_from(bytes, found + 2, b"]]") else {
            break;
        };
        let inner = &source[found + 2..close];

        // A wiki link never spans lines, and never contains another one.
        if inner.contains('\n') || inner.contains("[[") {
            cursor = found + 2;
            continue;
        }

        let raw_end = close + 2;
        let target = LinkTarget::parse(inner);

        // `[[]]` is not a link; neither is `[[|alias]]` with nothing to point at.
        if target.path.is_empty() && target.heading.is_none() && target.block_id.is_none() {
            cursor = raw_end;
            continue;
        }

        out.push(Link {
            kind: if is_embed {
                LinkKind::Embed
            } else {
                LinkKind::WikiLink
            },
            raw: source[raw_start..raw_end].to_string(),
            target: target.path,
            heading: target.heading,
            block_id: target.block_id,
            alias: target.alias,
            line: lines.line_of(raw_start),
            byte_start: raw_start,
            byte_end: raw_end,
        });

        cursor = raw_end;
    }

    out
}

/// Characters that may follow `#` in a tag.
fn is_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/'
}

/// A `#` only starts a tag when what precedes it is not word-like. This is
/// what keeps `C#`, `issue#42` and the `#` of a `##` heading from becoming
/// tags.
fn can_start_tag(previous: Option<char>) -> bool {
    match previous {
        None => true,
        Some(ch) => !(ch.is_alphanumeric() || ch == '_' || ch == '#' || ch == '/' || ch == '\\'),
    }
}

/// Find `#tags`, including `#nested/tags`.
pub fn scan_tags(source: &str, zones: &ExclusionZones, lines: &LineIndex) -> Vec<Tag> {
    let mut out = Vec::new();
    let mut previous: Option<char> = None;

    let mut chars = source.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        if ch != '#' {
            previous = Some(ch);
            continue;
        }
        if zones.covers(offset) || !can_start_tag(previous) {
            previous = Some(ch);
            continue;
        }

        // Consume the tag body.
        let body_start = offset + 1;
        let mut end = body_start;
        while let Some((next_offset, next_ch)) = chars.peek().copied() {
            if is_tag_char(next_ch) {
                end = next_offset + next_ch.len_utf8();
                chars.next();
            } else {
                break;
            }
        }

        let mut name = &source[body_start..end];
        // A trailing separator is punctuation, not part of the tag.
        name = name.trim_end_matches(['/', '-']);

        // `#` alone, `#123` and `#---` are not tags: requiring a letter avoids
        // turning issue references and horizontal rules into taxonomy.
        let usable = !name.is_empty()
            && name.chars().any(|c| c.is_alphabetic())
            && !name.contains("//");

        if usable {
            let byte_end = body_start + name.len();
            out.push(Tag {
                name: name.to_string(),
                line: lines.line_of(offset),
                byte_start: offset,
                byte_end,
            });
        }

        previous = source[..end].chars().next_back();
    }

    out
}

/// Byte ranges of the tags found, so a later scan can skip them.
pub fn tag_ranges(tags: &[Tag]) -> Vec<Range<usize>> {
    tags.iter().map(|t| t.byte_start..t.byte_end).collect()
}

/// Byte ranges of the links found.
pub fn link_ranges(links: &[Link]) -> Vec<Range<usize>> {
    links.iter().map(|l| l.byte_start..l.byte_end).collect()
}

fn is_block_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'
}

/// Find trailing `^block-id` markers and the text they identify.
///
/// Two placements are recognised, matching how people actually write them:
///
/// * at the end of a line — the marker identifies that line's paragraph;
/// * alone on a line — the marker identifies the paragraph above it.
///
/// In both cases the block's recorded range covers the whole contiguous run of
/// non-blank lines, excluding the marker itself, so embedding a block yields
/// the paragraph rather than a fragment.
pub fn scan_blocks(source: &str, zones: &ExclusionZones, lines: &LineIndex) -> Vec<Block> {
    let mut out = Vec::new();

    for line_no in 0..lines.line_count() {
        let range = lines.line_range(line_no);
        let text = lines.line_text(source, line_no);
        let Some((id, marker_offset_in_line)) = parse_trailing_block_id(text) else {
            continue;
        };
        let marker_start = range.start + marker_offset_in_line;
        if zones.covers(marker_start) {
            continue;
        }

        let standalone = text[..marker_offset_in_line].trim().is_empty();

        let (content_start, content_end) = if standalone {
            // Walk back over blank lines, then take the paragraph above.
            let mut last = match line_no.checked_sub(1) {
                Some(prev) => prev,
                None => continue,
            };
            while lines.line_text(source, last).trim().is_empty() {
                match last.checked_sub(1) {
                    Some(prev) => last = prev,
                    None => break,
                }
            }
            if lines.line_text(source, last).trim().is_empty() {
                continue;
            }
            let first = first_line_of_paragraph(source, lines, last);
            (lines.line_range(first).start, lines.line_range(last).end)
        } else {
            let first = first_line_of_paragraph(source, lines, line_no);
            (
                lines.line_range(first).start,
                range.start + text[..marker_offset_in_line].trim_end().len(),
            )
        };

        if content_start >= content_end {
            continue;
        }

        out.push(Block {
            id,
            line: line_no,
            byte_start: content_start,
            byte_end: content_end,
        });
    }

    out
}

/// Walk up from `line` while the lines above are non-blank.
fn first_line_of_paragraph(source: &str, lines: &LineIndex, line: usize) -> usize {
    let mut first = line;
    while first > 0 && !lines.line_text(source, first - 1).trim().is_empty() {
        first -= 1;
    }
    first
}

/// Recognise a `^id` marker at the end of a line, returning the id and the
/// byte offset of the `^` within the line.
fn parse_trailing_block_id(line: &str) -> Option<(String, usize)> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let caret = trimmed.rfind('^')?;
    let id = &trimmed[caret + 1..];
    if id.is_empty() || !id.chars().all(is_block_id_char) {
        return None;
    }
    // The marker must be its own token: preceded by whitespace, or at the very
    // start of the line's content. Otherwise `a^b` in prose would register.
    let before = trimmed[..caret].chars().next_back();
    match before {
        None => Some((id.to_string(), caret)),
        Some(ch) if ch.is_whitespace() => Some((id.to_string(), caret)),
        _ => None,
    }
}

fn find_from(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|idx| from + idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_links_in(source: &str) -> Vec<Link> {
        let lines = LineIndex::new(source);
        scan_wikilinks(source, &ExclusionZones::default(), &lines)
    }

    fn scan_tags_in(source: &str) -> Vec<String> {
        let lines = LineIndex::new(source);
        scan_tags(source, &ExclusionZones::default(), &lines)
            .into_iter()
            .map(|t| t.name)
            .collect()
    }

    #[test]
    fn finds_every_wiki_link_form() {
        let source = "[[Note]] and [[Note|Alias]] and [[Note#Heading]] and [[Note#Heading|Alias]] and [[Note#^block]]";
        let links = scan_links_in(source);
        assert_eq!(links.len(), 5);
        assert_eq!(links[0].target, "Note");
        assert_eq!(links[1].alias.as_deref(), Some("Alias"));
        assert_eq!(links[2].heading.as_deref(), Some("Heading"));
        assert_eq!(links[3].alias.as_deref(), Some("Alias"));
        assert_eq!(links[4].block_id.as_deref(), Some("block"));
        assert!(links.iter().all(|l| l.kind == LinkKind::WikiLink));
    }

    #[test]
    fn an_embed_includes_its_exclamation_mark_in_the_raw_text() {
        let links = scan_links_in("![[Note]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::Embed);
        assert_eq!(links[0].raw, "![[Note]]");
        assert_eq!(links[0].byte_start, 0);
        assert_eq!(links[0].byte_end, 9);
    }

    #[test]
    fn byte_ranges_address_exactly_the_link_text() {
        let source = "before [[Target|Shown]] after";
        let links = scan_links_in(source);
        assert_eq!(&source[links[0].byte_start..links[0].byte_end], "[[Target|Shown]]");
    }

    #[test]
    fn line_numbers_are_recorded_for_each_link() {
        let links = scan_links_in("first\n[[A]]\n\n[[B]]\n");
        assert_eq!(links[0].line, 1);
        assert_eq!(links[1].line, 3);
    }

    #[test]
    fn links_inside_code_are_ignored() {
        let source = "real [[Yes]] and code [[No]]";
        let lines = LineIndex::new(source);
        // Pretend the second link sits inside a code span.
        let zones = ExclusionZones::new(vec![22..28]);
        let links = scan_wikilinks(source, &zones, &lines);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Yes");
    }

    #[test]
    fn an_unterminated_link_is_not_a_link() {
        assert!(scan_links_in("[[never closed").is_empty());
    }

    #[test]
    fn a_link_may_not_span_lines() {
        assert!(scan_links_in("[[Note\nstill]]").is_empty());
    }

    #[test]
    fn empty_brackets_are_not_a_link() {
        assert!(scan_links_in("[[]] and [[|alias]]").is_empty());
    }

    #[test]
    fn finds_flat_and_nested_tags() {
        assert_eq!(
            scan_tags_in("#AI and #GameDesign/Mechanics and #AI/LLM"),
            vec!["AI", "GameDesign/Mechanics", "AI/LLM"]
        );
    }

    #[test]
    fn a_hash_inside_a_word_is_not_a_tag() {
        assert!(scan_tags_in("C# and issue#42 and a#b").is_empty());
    }

    #[test]
    fn headings_are_not_tags() {
        assert!(scan_tags_in("# Heading\n## Subheading\n").is_empty());
    }

    #[test]
    fn a_purely_numeric_hash_reference_is_not_a_tag() {
        assert!(scan_tags_in("see #42 and #2026").is_empty());
    }

    #[test]
    fn tags_may_follow_punctuation() {
        assert_eq!(scan_tags_in("(#AI) [#Research]"), vec!["AI", "Research"]);
    }

    #[test]
    fn trailing_punctuation_is_not_swallowed_into_the_tag() {
        assert_eq!(scan_tags_in("tagged #AI, and #Research."), vec!["AI", "Research"]);
        assert_eq!(scan_tags_in("#AI/"), vec!["AI"]);
    }

    #[test]
    fn tags_inside_excluded_regions_are_ignored() {
        let source = "#Real and #Fake";
        let lines = LineIndex::new(source);
        let zones = ExclusionZones::new(vec![10..15]);
        let names: Vec<_> = scan_tags(source, &zones, &lines)
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["Real"]);
    }

    #[test]
    fn tag_positions_point_at_the_hash() {
        let source = "text #AI more";
        let lines = LineIndex::new(source);
        let tags = scan_tags(source, &ExclusionZones::default(), &lines);
        assert_eq!(&source[tags[0].byte_start..tags[0].byte_end], "#AI");
    }

    #[test]
    fn a_trailing_marker_identifies_its_own_paragraph() {
        let source = "Intro line.\n\nThe important point. ^important-point\n\nAfter.\n";
        let lines = LineIndex::new(source);
        let blocks = scan_blocks(source, &ExclusionZones::default(), &lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, "important-point");
        assert_eq!(&source[blocks[0].byte_start..blocks[0].byte_end], "The important point.");
    }

    #[test]
    fn a_standalone_marker_identifies_the_paragraph_above() {
        let source = "A paragraph\nspanning two lines.\n^spanning\n";
        let lines = LineIndex::new(source);
        let blocks = scan_blocks(source, &ExclusionZones::default(), &lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            &source[blocks[0].byte_start..blocks[0].byte_end],
            "A paragraph\nspanning two lines."
        );
    }

    #[test]
    fn a_multi_line_paragraph_with_a_trailing_marker_is_captured_whole() {
        let source = "Line one\nline two ^id\n";
        let lines = LineIndex::new(source);
        let blocks = scan_blocks(source, &ExclusionZones::default(), &lines);
        assert_eq!(&source[blocks[0].byte_start..blocks[0].byte_end], "Line one\nline two");
    }

    #[test]
    fn a_caret_inside_a_word_is_not_a_block_marker() {
        let source = "the expression a^b is maths\n";
        let lines = LineIndex::new(source);
        assert!(scan_blocks(source, &ExclusionZones::default(), &lines).is_empty());
    }

    #[test]
    fn block_ids_reject_characters_that_would_break_a_link() {
        let source = "text ^has spaces\ntext ^has/slash\n";
        let lines = LineIndex::new(source);
        assert!(scan_blocks(source, &ExclusionZones::default(), &lines).is_empty());
    }

    #[test]
    fn markers_inside_code_blocks_are_ignored() {
        let source = "```\nlet x = y ^2\n```\n";
        let lines = LineIndex::new(source);
        let zones = ExclusionZones::new(vec![0..source.len()]);
        assert!(scan_blocks(source, &zones, &lines).is_empty());
    }
}

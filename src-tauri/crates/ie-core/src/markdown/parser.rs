//! `MarkdownParser` — source text to everything the rest of the app needs.
//!
//! The pipeline is deliberately two-stage:
//!
//! 1. A real CommonMark parser (`pulldown-cmark`) walks the document and
//!    reports its structure: headings, code, HTML, links, images. This is what
//!    tells us which byte ranges are *prose* and which are not.
//! 2. The extension scanner runs over the prose only, finding the constructs
//!    CommonMark has no opinion about: `[[wiki links]]`, `![[embeds]]`, `#tags`
//!    and `^block-ids`.
//!
//! Doing it in that order is what keeps a shell script containing `#!/bin/sh`
//! from contributing a tag, and a code sample containing `[[x]]` from becoming
//! a link.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::links::reference::slugify;
use crate::markdown::text::{count_words, ExclusionZones, LineIndex};
use crate::markdown::{frontmatter, scanner};
use crate::model::{Heading, Link, LinkKind, NoteMetadata};

/// Tunable parsing behaviour. Defaults match what the editor and the indexer
/// both want; export overrides nothing today but has a place to.
#[derive(Debug, Clone, Copy)]
pub struct ParserOptions {
    pub tables: bool,
    pub footnotes: bool,
    pub strikethrough: bool,
    pub task_lists: bool,
    /// Record `[text](https://…)` links. They never resolve to a file, but the
    /// graph can show which notes reach outward.
    pub track_external_links: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            tables: true,
            footnotes: true,
            strikethrough: true,
            task_lists: true,
            track_external_links: true,
        }
    }
}

impl ParserOptions {
    fn to_cmark(self) -> Options {
        let mut options = Options::empty();
        if self.tables {
            options.insert(Options::ENABLE_TABLES);
        }
        if self.footnotes {
            options.insert(Options::ENABLE_FOOTNOTES);
        }
        if self.strikethrough {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }
        if self.task_lists {
            options.insert(Options::ENABLE_TASKLISTS);
        }
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        options
    }
}

/// The result of parsing one note.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedDocument {
    pub metadata: NoteMetadata,
    /// Byte offset where the body starts, after any frontmatter.
    pub body_offset: usize,
    /// Set when the frontmatter is present but not valid YAML. The note is
    /// still fully parsed; only its properties are missing.
    pub frontmatter_error: Option<String>,
}

/// The app's Markdown front end.
#[derive(Debug, Clone, Default)]
pub struct MarkdownParser {
    options: ParserOptions,
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: ParserOptions) -> Self {
        Self { options }
    }

    /// Parse a note completely: frontmatter, structure and extensions.
    pub fn parse(&self, source: &str) -> ParsedDocument {
        let lines = LineIndex::new(source);
        let (properties, frontmatter_error) = frontmatter::parse(source);
        let body_offset = frontmatter::body_offset(source);

        let structure = self.walk_structure(source, body_offset);

        // Everything before the body is off limits: a `#` in a YAML comment is
        // not a tag, and `---` is not a heading.
        let mut excluded = structure.excluded.clone();
        if body_offset > 0 {
            excluded.push(0..body_offset);
        }
        let zones = ExclusionZones::new(excluded);

        let mut links = scanner::scan_wikilinks(source, &zones, &lines);

        // Tags must not be found inside a link that was already recognised,
        // otherwise `[[Note#Heading]]` yields a phantom `#Heading` tag and
        // `[a](http://x/#frag)` yields `#frag`.
        let tag_zones = zones.extended(
            scanner::link_ranges(&links)
                .into_iter()
                .chain(structure.link_ranges.iter().cloned()),
        );
        let tags = scanner::scan_tags(source, &tag_zones, &lines);
        let blocks = scanner::scan_blocks(source, &zones, &lines);

        links.extend(structure.markdown_links);
        links.sort_by_key(|l| l.byte_start);

        let title = frontmatter::get(&properties, "title")
            .map(|v| v.as_text())
            .filter(|t| !t.trim().is_empty())
            .or_else(|| {
                structure
                    .headings
                    .iter()
                    .find(|h| h.level == 1)
                    .map(|h| h.text.clone())
            });

        ParsedDocument {
            metadata: NoteMetadata {
                title,
                properties,
                links,
                tags,
                headings: structure.headings,
                blocks,
                frontmatter_bytes: body_offset,
                word_count: count_words(&source[body_offset..]),
            },
            body_offset,
            frontmatter_error,
        }
    }

    /// Parse only far enough to answer "what does this note link to and tag?".
    /// Same work today; kept as a separate entry point so the indexer's needs
    /// can diverge from the editor's without changing call sites.
    pub fn parse_metadata(&self, source: &str) -> NoteMetadata {
        self.parse(source).metadata
    }

    fn walk_structure(&self, source: &str, body_offset: usize) -> Structure {
        let body = &source[body_offset..];
        let lines = LineIndex::new(source);
        let shift = |range: Range<usize>| (range.start + body_offset)..(range.end + body_offset);

        let mut structure = Structure::default();
        let mut heading_in_progress: Option<(u8, usize, String)> = None;

        let parser = Parser::new_ext(body, self.options.to_cmark());
        for (event, range) in parser.into_offset_iter() {
            match event {
                Event::Start(Tag::Heading { level, .. }) => {
                    heading_in_progress =
                        Some((heading_level_number(level), range.start + body_offset, String::new()));
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some((level, byte_start, text)) = heading_in_progress.take() {
                        let text = text.trim().to_string();
                        structure.headings.push(Heading {
                            level,
                            slug: slugify(&text),
                            text,
                            line: lines.line_of(byte_start),
                            byte_start,
                        });
                    }
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    structure.excluded.push(shift(range.clone()));
                    if let CodeBlockKind::Fenced(_) = kind {
                        // The info string is part of the fence and already inside
                        // the range above; nothing further to exclude.
                    }
                }
                Event::Code(ref code) => {
                    structure.excluded.push(shift(range.clone()));
                    if let Some((_, _, text)) = heading_in_progress.as_mut() {
                        text.push_str(code);
                    }
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    structure.excluded.push(shift(range.clone()));
                }
                Event::Start(Tag::Link {
                    ref dest_url,
                    ref title,
                    ..
                }) => {
                    let shifted = shift(range.clone());
                    structure.link_ranges.push(shifted.clone());
                    if let Some(link) =
                        self.build_markdown_link(source, dest_url, title, shifted, &lines, false)
                    {
                        structure.markdown_links.push(link);
                    }
                }
                Event::Start(Tag::Image {
                    ref dest_url,
                    ref title,
                    ..
                }) => {
                    let shifted = shift(range.clone());
                    structure.link_ranges.push(shifted.clone());
                    if let Some(link) =
                        self.build_markdown_link(source, dest_url, title, shifted, &lines, true)
                    {
                        structure.markdown_links.push(link);
                    }
                }
                Event::Text(ref text) => {
                    if let Some((_, _, heading_text)) = heading_in_progress.as_mut() {
                        heading_text.push_str(text);
                    }
                }
                _ => {}
            }
        }

        structure
    }

    fn build_markdown_link(
        &self,
        source: &str,
        dest_url: &str,
        title: &str,
        range: Range<usize>,
        lines: &LineIndex,
        is_image: bool,
    ) -> Option<Link> {
        let _ = title;
        let external = is_external_url(dest_url);
        if external && !self.options.track_external_links {
            return None;
        }

        let kind = match (external, is_image) {
            (true, _) => LinkKind::External,
            (false, true) => LinkKind::MarkdownImage,
            (false, false) => LinkKind::Markdown,
        };

        let (target, heading, block_id) = if external {
            (dest_url.to_string(), None, None)
        } else {
            split_internal_destination(&percent_decode(dest_url))
        };

        if target.is_empty() && heading.is_none() && block_id.is_none() {
            return None;
        }

        Some(Link {
            kind,
            raw: source.get(range.clone()).unwrap_or_default().to_string(),
            target,
            heading,
            block_id,
            alias: None,
            line: lines.line_of(range.start),
            byte_start: range.start,
            byte_end: range.end,
        })
    }
}

#[derive(Debug, Default)]
struct Structure {
    headings: Vec<Heading>,
    /// Ranges the extension scanner must not enter.
    excluded: Vec<Range<usize>>,
    /// Ranges of Markdown links and images, excluded from tag scanning only.
    link_ranges: Vec<Range<usize>>,
    markdown_links: Vec<Link>,
}

fn heading_level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Does this destination point outside the vault?
///
/// Anything with a URL scheme does. Protocol-relative `//host/path` counts too,
/// because a browser would treat it as remote.
pub fn is_external_url(dest: &str) -> bool {
    if dest.starts_with("//") {
        return true;
    }
    match dest.find(':') {
        Some(idx) if idx > 0 => {
            let scheme = &dest[..idx];
            let rest = &dest[idx + 1..];
            // A single letter is a Windows drive, not a scheme. A colon
            // followed by a space is prose (`Note: A Subtitle`), not a URL,
            // because no scheme may be followed by whitespace.
            scheme.len() > 1
                && !rest.starts_with(' ')
                && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        }
        _ => false,
    }
}

/// Split `Folder/Note.md#Heading` into its parts.
fn split_internal_destination(dest: &str) -> (String, Option<String>, Option<String>) {
    match dest.find('#') {
        Some(idx) => {
            let path = dest[..idx].to_string();
            let fragment = &dest[idx + 1..];
            if let Some(block) = fragment.strip_prefix('^') {
                (path, None, Some(block.to_string()).filter(|s| !s.is_empty()))
            } else {
                (
                    path,
                    Some(fragment.to_string()).filter(|s| !s.is_empty()),
                    None,
                )
            }
        }
        None => (dest.to_string(), None, None),
    }
}

/// Decode `%20`-style escapes that editors insert into link destinations.
/// Invalid escapes are left alone rather than dropped.
pub fn percent_decode(input: &str) -> String {
    if !input.contains('%') {
        return input.to_string();
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &input[i + 1..i + 3];
            match u8::from_str_radix(hex, 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                Err(_) => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> ParsedDocument {
        MarkdownParser::new().parse(source)
    }

    #[test]
    fn collects_headings_with_levels_slugs_and_lines() {
        let doc = parse("# Top\n\nText\n\n## Getting Started\n\n### Deep\n");
        let headings = &doc.metadata.headings;
        assert_eq!(headings.len(), 3);
        assert_eq!((headings[0].level, headings[0].text.as_str()), (1, "Top"));
        assert_eq!(headings[1].slug, "getting-started");
        assert_eq!(headings[1].line, 4);
        assert_eq!(headings[2].level, 3);
    }

    #[test]
    fn heading_text_includes_inline_formatting_content() {
        let doc = parse("## A *bold* `code` heading\n");
        assert_eq!(doc.metadata.headings[0].text, "A bold code heading");
    }

    #[test]
    fn title_comes_from_frontmatter_first() {
        let doc = parse("---\ntitle: From Properties\n---\n# From Heading\n");
        assert_eq!(doc.metadata.title.as_deref(), Some("From Properties"));
    }

    #[test]
    fn title_falls_back_to_the_first_level_one_heading() {
        let doc = parse("## Not this\n\n# This one\n\n# Nor this\n");
        assert_eq!(doc.metadata.title.as_deref(), Some("This one"));
    }

    #[test]
    fn a_note_without_a_title_reports_none_so_the_caller_can_use_the_filename() {
        assert_eq!(parse("Just text.\n").metadata.title, None);
    }

    #[test]
    fn wiki_links_inside_fenced_code_blocks_are_not_links() {
        let doc = parse("Real [[Yes]]\n\n```md\n[[No]]\n#nottag\n```\n");
        let targets: Vec<_> = doc
            .metadata
            .links
            .iter()
            .map(|l| l.target.as_str())
            .collect();
        assert_eq!(targets, vec!["Yes"]);
        assert!(doc.metadata.tags.is_empty());
    }

    #[test]
    fn wiki_links_inside_inline_code_are_not_links() {
        let doc = parse("Use `[[Literal]]` to write a link to [[Real]].\n");
        let targets: Vec<_> = doc
            .metadata
            .links
            .iter()
            .map(|l| l.target.as_str())
            .collect();
        assert_eq!(targets, vec!["Real"]);
    }

    #[test]
    fn a_shebang_in_a_code_block_is_not_a_tag() {
        let doc = parse("```sh\n#!/bin/sh\necho hi\n```\n\n#RealTag\n");
        let names: Vec<_> = doc.metadata.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["RealTag"]);
    }

    #[test]
    fn an_indented_code_block_is_excluded_too() {
        let doc = parse("Text\n\n    [[Indented]]\n    #indentedtag\n\n[[Real]]\n");
        let targets: Vec<_> = doc
            .metadata
            .links
            .iter()
            .map(|l| l.target.as_str())
            .collect();
        assert_eq!(targets, vec!["Real"]);
        assert!(doc.metadata.tags.is_empty());
    }

    #[test]
    fn a_url_fragment_is_not_a_tag() {
        let doc = parse("See [docs](https://example.org/guide#installation).\n");
        assert!(doc.metadata.tags.is_empty());
    }

    #[test]
    fn a_heading_reference_inside_a_wiki_link_is_not_a_tag() {
        let doc = parse("[[Note#Heading]] and [[Other#^block]]\n");
        assert!(doc.metadata.tags.is_empty());
        assert_eq!(doc.metadata.links.len(), 2);
    }

    #[test]
    fn frontmatter_is_never_scanned_for_tags_or_links() {
        let doc = parse("---\n# a yaml comment\nnote: \"see [[Ghost]]\"\n---\n\n[[Real]]\n");
        let targets: Vec<_> = doc
            .metadata
            .links
            .iter()
            .map(|l| l.target.as_str())
            .collect();
        assert_eq!(targets, vec!["Real"]);
        assert!(doc.metadata.tags.is_empty());
        assert_eq!(doc.metadata.headings.len(), 0);
    }

    #[test]
    fn markdown_links_are_classified_as_internal_or_external() {
        let doc = parse("[a](Notes/Other.md) [b](https://example.org) ![c](img/pic.png)\n");
        let kinds: Vec<_> = doc.metadata.links.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![LinkKind::Markdown, LinkKind::External, LinkKind::MarkdownImage]
        );
        assert_eq!(doc.metadata.links[0].target, "Notes/Other.md");
        assert_eq!(doc.metadata.links[2].target, "img/pic.png");
    }

    #[test]
    fn a_markdown_link_destination_is_percent_decoded() {
        let doc = parse("[a](Notes/My%20Note.md)\n");
        assert_eq!(doc.metadata.links[0].target, "Notes/My Note.md");
    }

    #[test]
    fn a_markdown_link_fragment_becomes_a_heading_or_block_reference() {
        let doc = parse("[a](Other.md#Section) [b](Other.md#^blk)\n");
        assert_eq!(doc.metadata.links[0].heading.as_deref(), Some("Section"));
        assert_eq!(doc.metadata.links[1].block_id.as_deref(), Some("blk"));
    }

    #[test]
    fn links_are_returned_in_source_order() {
        let doc = parse("[md](a.md) then [[wiki]] then ![[embed]]\n");
        let starts: Vec<_> = doc.metadata.links.iter().map(|l| l.byte_start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert_eq!(doc.metadata.links.len(), 3);
    }

    #[test]
    fn block_identifiers_are_collected() {
        let doc = parse("A point worth referencing. ^important\n");
        assert_eq!(doc.metadata.blocks.len(), 1);
        assert_eq!(doc.metadata.blocks[0].id, "important");
    }

    #[test]
    fn word_count_excludes_frontmatter() {
        let doc = parse("---\ntitle: A Very Long Title Indeed\n---\none two three\n");
        assert_eq!(doc.metadata.word_count, 3);
    }

    #[test]
    fn frontmatter_byte_length_is_reported_for_body_addressing() {
        let source = "---\na: 1\n---\nbody\n";
        let doc = parse(source);
        assert_eq!(&source[doc.body_offset..], "body\n");
        assert_eq!(doc.metadata.frontmatter_bytes, doc.body_offset);
    }

    #[test]
    fn a_malformed_frontmatter_block_still_yields_links_and_tags() {
        let doc = parse("---\nbad: [unclosed\n---\n\n[[Note]] #tag\n");
        assert!(doc.frontmatter_error.is_some());
        assert_eq!(doc.metadata.links.len(), 1);
        assert_eq!(doc.metadata.tags.len(), 1);
    }

    #[test]
    fn tables_and_task_lists_parse_without_disturbing_extensions() {
        let source = "| a | b |\n| - | - |\n| [[X]] | #tag |\n\n- [ ] todo\n- [x] done\n";
        let doc = parse(source);
        assert_eq!(doc.metadata.links.len(), 1);
        assert_eq!(doc.metadata.tags.len(), 1);
    }

    #[test]
    fn external_url_detection_handles_schemes_and_lookalikes() {
        assert!(is_external_url("https://example.org"));
        assert!(is_external_url("mailto:a@b.c"));
        assert!(is_external_url("//cdn.example.org/x.png"));
        assert!(!is_external_url("Notes/Other.md"));
        assert!(!is_external_url("Note: A Subtitle.md"));
        assert!(!is_external_url("./relative.md"));
    }

    #[test]
    fn percent_decoding_leaves_invalid_escapes_alone() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%done"), "100%done");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn parsing_is_stable_for_windows_line_endings() {
        let unix = parse("# Title\r\n\r\n[[Link]] #tag\r\n");
        assert_eq!(unix.metadata.headings.len(), 1);
        assert_eq!(unix.metadata.links.len(), 1);
        assert_eq!(unix.metadata.tags.len(), 1);
    }
}

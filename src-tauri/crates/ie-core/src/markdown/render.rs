//! `MarkdownRenderer` — structure to HTML.
//!
//! Used by export (§35) and by anything server-side that needs a rendered
//! note. The editor's live preview does *not* go through here: it decorates
//! the CodeMirror document directly so the cursor stays in the real source.
//!
//! Wiki links and embeds are not CommonMark, so they are rewritten into
//! ordinary Markdown links before the CommonMark renderer sees them. That way
//! a single HTML generator handles everything, and a link that resolves to a
//! file and one that does not differ only by a CSS class.

use std::collections::HashMap;

use pulldown_cmark::{html, Options, Parser};

use crate::links::reference::LinkTarget;
use crate::markdown::frontmatter;
use crate::markdown::parser::{MarkdownParser, ParserOptions};
use crate::model::{Link, LinkKind};
use crate::vault::path::VaultPath;

/// How a resolved link should be addressed in the output.
pub type LinkRewriter<'a> = dyn Fn(&Link) -> Option<String> + 'a;

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub parser: ParserOptions,
    /// Emit the frontmatter as a definition list ahead of the body.
    pub include_properties: bool,
    /// Wrap the output in a full HTML document with the theme's stylesheet.
    pub standalone: bool,
    pub document_title: String,
    /// Extension appended to internal link targets, e.g. `html` for export.
    pub internal_link_extension: Option<String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            parser: ParserOptions::default(),
            include_properties: true,
            standalone: false,
            document_title: String::new(),
            internal_link_extension: None,
        }
    }
}

pub struct MarkdownRenderer {
    options: RenderOptions,
    /// Maps a link's target text to the path it resolved to. Supplied by the
    /// caller because resolution needs the index, which this module must not
    /// depend on.
    resolutions: HashMap<String, VaultPath>,
}

impl MarkdownRenderer {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            resolutions: HashMap::new(),
        }
    }

    /// Tell the renderer what each link target resolves to.
    pub fn with_resolutions(mut self, resolutions: HashMap<String, VaultPath>) -> Self {
        self.resolutions = resolutions;
        self
    }

    /// Render a note to HTML.
    pub fn render(&self, source: &str) -> String {
        let body_offset = frontmatter::body_offset(source);
        let (properties, _) = frontmatter::parse(source);
        let normalized = self.rewrite_extensions(source, body_offset);

        let mut body_html = String::with_capacity(normalized.len() * 2);
        html::push_html(
            &mut body_html,
            Parser::new_ext(&normalized, self.cmark_options()),
        );

        let mut out = String::new();
        if self.options.include_properties && !properties.is_empty() {
            out.push_str("<dl class=\"ie-properties\">\n");
            for property in &properties {
                out.push_str("  <dt>");
                push_escaped(&mut out, &property.key);
                out.push_str("</dt>\n  <dd>");
                push_escaped(&mut out, &property.value.as_text());
                out.push_str("</dd>\n");
            }
            out.push_str("</dl>\n");
        }
        out.push_str(&body_html);

        if self.options.standalone {
            wrap_document(&self.options.document_title, &out)
        } else {
            out
        }
    }

    fn cmark_options(&self) -> Options {
        let mut options = Options::empty();
        if self.options.parser.tables {
            options.insert(Options::ENABLE_TABLES);
        }
        if self.options.parser.footnotes {
            options.insert(Options::ENABLE_FOOTNOTES);
        }
        if self.options.parser.strikethrough {
            options.insert(Options::ENABLE_STRIKETHROUGH);
        }
        if self.options.parser.task_lists {
            options.insert(Options::ENABLE_TASKLISTS);
        }
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        options
    }

    /// Replace `[[wiki links]]` and `![[embeds]]` with CommonMark equivalents,
    /// working from the end of the document backwards so earlier byte offsets
    /// stay valid as the text changes length.
    ///
    /// The links come from `MarkdownParser`, not from a second scan, so what
    /// the renderer treats as a link is exactly what the indexer recorded —
    /// including its refusal to see links inside code.
    fn rewrite_extensions(&self, source: &str, body_offset: usize) -> String {
        let document = MarkdownParser::with_options(self.options.parser).parse(source);

        let mut out = source[body_offset..].to_string();
        for link in document
            .metadata
            .links
            .iter()
            .filter(|l| matches!(l.kind, LinkKind::WikiLink | LinkKind::Embed))
            .rev()
        {
            let start = link.byte_start - body_offset;
            let end = link.byte_end - body_offset;
            let replacement = self.render_wikilink(link);
            out.replace_range(start..end, &replacement);
        }
        out
    }

    fn render_wikilink(&self, link: &Link) -> String {
        let target = LinkTarget {
            path: link.target.clone(),
            heading: link.heading.clone(),
            block_id: link.block_id.clone(),
            alias: link.alias.clone(),
        };
        let display = target.default_display();
        let resolved = self.resolutions.get(&link.target);

        let href = match resolved {
            Some(path) => {
                // Only notes are re-extensioned on export: an attachment keeps
                // its own extension, or `diagram.png` would become
                // `diagram.html` and the image would not load.
                let is_note =
                    matches!(path.extension().as_deref(), Some("md" | "markdown" | "mdx"));
                let base = match (&self.options.internal_link_extension, is_note) {
                    (Some(ext), true) => path
                        .with_extension(Some(ext))
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_else(|_| path.as_str().to_string()),
                    _ => path.as_str().to_string(),
                };
                let fragment = fragment_for(link);
                format!("{}{}", encode_href(&base), fragment)
            }
            // An unresolved link still renders, marked so a stylesheet can grey
            // it out. Hiding it would lose information the writer typed.
            None => format!("#{}", encode_href(&link.target)),
        };

        match link.kind {
            LinkKind::Embed => {
                if is_image_target(&link.target) {
                    format!("![{}]({})", escape_bracket(&display), href)
                } else {
                    // A note embed cannot be expanded here: expansion needs the
                    // vault, which this module deliberately cannot reach. The
                    // marker is what the export pass replaces.
                    format!(
                        "<div class=\"ie-embed\" data-target=\"{}\">{}</div>",
                        escape_attribute(&link.target),
                        escape_html(&display)
                    )
                }
            }
            _ => {
                let class = if resolved.is_some() {
                    "ie-internal-link"
                } else {
                    "ie-unresolved-link"
                };
                format!(
                    "<a class=\"{class}\" href=\"{}\">{}</a>",
                    escape_attribute(&href),
                    escape_html(&display)
                )
            }
        }
    }
}

fn fragment_for(link: &Link) -> String {
    if let Some(block) = &link.block_id {
        format!("#{}", encode_href(&format!("^{block}")))
    } else if let Some(heading) = &link.heading {
        format!("#{}", crate::links::reference::slugify(heading))
    } else {
        String::new()
    }
}

fn is_image_target(target: &str) -> bool {
    let lowered = target.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".avif",
    ]
    .iter()
    .any(|ext| lowered.ends_with(ext))
}

/// Percent-encode the characters that would otherwise break an href.
fn encode_href(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn escape_bracket(input: &str) -> String {
    input.replace('[', "\\[").replace(']', "\\]")
}

pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    push_escaped(&mut out, input);
    out
}

fn push_escaped(out: &mut String, input: &str) {
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
}

fn escape_attribute(input: &str) -> String {
    escape_html(input)
}

/// Wrap rendered HTML in a self-contained document.
///
/// The stylesheet is inlined and uses the same custom-property names as the
/// app's themes, so an exported page looks like what the user was reading and
/// needs no network access to render.
fn wrap_document(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{}</title>
<style>
:root {{
  --background-primary: #ffffff;
  --background-secondary: #f6f7f9;
  --text-normal: #1f2328;
  --text-muted: #6b7280;
  --accent: #3b6fd4;
  --border: #e2e5ea;
  --code-background: #f2f4f7;
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --background-primary: #16181d;
    --background-secondary: #1d2027;
    --text-normal: #e6e8eb;
    --text-muted: #9aa3ae;
    --accent: #7aa2f7;
    --border: #2b2f38;
    --code-background: #21252d;
  }}
}}
body {{
  margin: 0 auto; padding: 3rem 1.5rem; max-width: 46rem;
  background: var(--background-primary); color: var(--text-normal);
  font: 16px/1.7 -apple-system, "Segoe UI", Roboto, "Helvetica Neue", sans-serif;
}}
h1, h2, h3, h4, h5, h6 {{ line-height: 1.25; margin-top: 2em; }}
a {{ color: var(--accent); }}
a.ie-unresolved-link {{ color: var(--text-muted); text-decoration: underline dotted; }}
code {{ background: var(--code-background); padding: 0.15em 0.35em; border-radius: 4px; }}
pre {{ background: var(--code-background); padding: 1rem; border-radius: 8px; overflow-x: auto; }}
pre code {{ background: none; padding: 0; }}
blockquote {{ margin: 1rem 0; padding-left: 1rem; border-left: 3px solid var(--border); color: var(--text-muted); }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid var(--border); padding: 0.4rem 0.6rem; text-align: left; }}
img {{ max-width: 100%; height: auto; }}
hr {{ border: none; border-top: 1px solid var(--border); margin: 2rem 0; }}
.ie-properties {{ background: var(--background-secondary); border: 1px solid var(--border);
  border-radius: 8px; padding: 0.75rem 1rem; margin-bottom: 2rem; font-size: 0.9em; }}
.ie-properties dt {{ color: var(--text-muted); font-weight: 600; }}
.ie-properties dd {{ margin: 0 0 0.5rem 0; }}
.ie-embed {{ border-left: 3px solid var(--border); padding-left: 1rem; color: var(--text-muted); }}
</style>
</head>
<body>
{}
</body>
</html>
"#,
        escape_html(title),
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str) -> String {
        MarkdownRenderer::new(RenderOptions::default()).render(source)
    }

    fn render_with(source: &str, resolutions: &[(&str, &str)]) -> String {
        let map = resolutions
            .iter()
            .map(|(k, v)| ((*k).to_string(), VaultPath::parse(v).unwrap()))
            .collect();
        MarkdownRenderer::new(RenderOptions {
            internal_link_extension: Some("html".into()),
            ..RenderOptions::default()
        })
        .with_resolutions(map)
        .render(source)
    }

    #[test]
    fn renders_standard_markdown() {
        let html = render("# Title\n\n**bold** and *italic* and `code`\n\n- one\n- two\n");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("<li>one</li>"));
    }

    #[test]
    fn renders_gfm_extensions() {
        let html = render("~~gone~~\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n");
        assert!(html.contains("<del>gone</del>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("checked"));
    }

    #[test]
    fn a_resolved_wiki_link_becomes_an_anchor_to_the_exported_file() {
        let html = render_with(
            "See [[Other Note]].",
            &[("Other Note", "Notes/Other Note.md")],
        );
        assert!(html.contains("ie-internal-link"), "{html}");
        assert!(html.contains("Notes/Other%20Note.html"), "{html}");
        assert!(html.contains(">Other Note</a>"), "{html}");
    }

    #[test]
    fn an_alias_is_what_the_reader_sees() {
        let html = render_with("[[Target|Shown]]", &[("Target", "Target.md")]);
        assert!(html.contains(">Shown</a>"), "{html}");
        assert!(!html.contains(">Target<"), "{html}");
    }

    #[test]
    fn a_heading_reference_becomes_a_fragment() {
        let html = render_with("[[Note#Getting Started]]", &[("Note", "Note.md")]);
        assert!(html.contains("Note.html#getting-started"), "{html}");
    }

    #[test]
    fn an_unresolved_link_still_renders_and_is_marked() {
        let html = render("[[Future Project]]");
        assert!(html.contains("ie-unresolved-link"), "{html}");
        assert!(html.contains("Future Project"), "{html}");
    }

    #[test]
    fn an_image_embed_becomes_an_img_tag() {
        let html = render_with(
            "![[diagram.png]]",
            &[("diagram.png", "Attachments/diagram.png")],
        );
        assert!(html.contains("<img"), "{html}");
        assert!(html.contains("Attachments/diagram.png"), "{html}");
    }

    #[test]
    fn a_note_embed_becomes_a_marker_the_export_pass_can_expand() {
        let html = render("![[Other Note]]");
        assert!(html.contains("ie-embed"), "{html}");
        assert!(html.contains("data-target=\"Other Note\""), "{html}");
    }

    #[test]
    fn frontmatter_is_rendered_as_properties_not_as_a_horizontal_rule() {
        let html = render("---\ntitle: Example\nstatus: active\n---\n\nBody text.\n");
        assert!(html.contains("ie-properties"), "{html}");
        assert!(html.contains("<dt>status</dt>"), "{html}");
        assert!(html.contains("<p>Body text.</p>"), "{html}");
        assert!(
            !html.contains("<hr"),
            "frontmatter leaked as a rule:\n{html}"
        );
    }

    #[test]
    fn properties_can_be_omitted() {
        let renderer = MarkdownRenderer::new(RenderOptions {
            include_properties: false,
            ..RenderOptions::default()
        });
        let html = renderer.render("---\ntitle: Example\n---\nBody\n");
        assert!(!html.contains("ie-properties"));
        assert!(html.contains("Body"));
    }

    #[test]
    fn wiki_links_inside_code_are_left_alone() {
        let html = render("Write `[[Link]]` like this.\n");
        assert!(html.contains("<code>[[Link]]</code>"), "{html}");
    }

    #[test]
    fn html_in_note_text_is_escaped_in_link_labels() {
        let html = render("[[<script>alert(1)</script>]]");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn a_standalone_document_is_self_contained() {
        let renderer = MarkdownRenderer::new(RenderOptions {
            standalone: true,
            document_title: "My Note".into(),
            ..RenderOptions::default()
        });
        let html = renderer.render("# Hi\n");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>My Note</title>"));
        assert!(html.contains("--background-primary"));
        assert!(
            !html.contains("http://"),
            "an export must not need the network"
        );
    }

    #[test]
    fn multiple_links_on_one_line_all_rewrite_correctly() {
        let html = render_with(
            "[[A]] then [[B]] then [[C]]",
            &[("A", "A.md"), ("B", "B.md"), ("C", "C.md")],
        );
        for name in ["A.html", "B.html", "C.html"] {
            assert!(html.contains(name), "missing {name} in {html}");
        }
    }
}

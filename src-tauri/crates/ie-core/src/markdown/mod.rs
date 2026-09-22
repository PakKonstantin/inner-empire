//! Markdown: parsing, rendering and source transformation.
//!
//! The three abstractions the brief calls for map onto three modules:
//!
//! * [`MarkdownParser`] — source to structure and metadata
//! * [`render`] — structure to HTML, for preview and export
//! * [`transform`] — structure-guided edits back to source, for renames and
//!   property updates
//!
//! They share [`text`] for byte and line bookkeeping, and [`scanner`] for the
//! non-CommonMark extensions.

pub mod frontmatter;
pub mod parser;
pub mod render;
pub mod scanner;
pub mod text;
pub mod transform;

pub use parser::{MarkdownParser, ParsedDocument, ParserOptions};
pub use render::{MarkdownRenderer, RenderOptions};
pub use text::{ExclusionZones, LineIndex};
pub use transform::MarkdownTransformer;

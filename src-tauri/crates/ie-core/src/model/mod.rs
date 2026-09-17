//! The domain vocabulary.
//!
//! Every type here is `serde`-serialisable with camelCase field names, because
//! these are the exact shapes that cross the IPC boundary into TypeScript.
//! `src/types/domain.ts` mirrors them, and a conformance test keeps the two
//! definitions honest.

pub mod property;
pub mod workspace;

pub use property::{Property, PropertyKind, PropertyValue};
pub use workspace::{
    PaneLayout, PaneNode, SplitDirection, TabState, Workspace, WorkspaceSidebar,
};

use crate::vault::path::VaultPath;

/// What kind of thing a vault entry is, decided by extension.
///
/// The distinction drives indexing (only notes get parsed), the explorer's
/// icons, and what an embed does with a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    /// A Markdown note: parsed, indexed, linkable.
    Note,
    /// A canvas board, stored as JSON.
    Canvas,
    Image,
    Pdf,
    Audio,
    Video,
    /// Anything else: tracked so links to it resolve, but not parsed.
    Other,
}

impl FileKind {
    pub fn from_extension(extension: Option<&str>) -> Self {
        match extension {
            Some("md" | "markdown" | "mdx") => FileKind::Note,
            Some("canvas") => FileKind::Canvas,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "avif" | "ico") => {
                FileKind::Image
            }
            Some("pdf") => FileKind::Pdf,
            Some("mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" | "opus") => FileKind::Audio,
            Some("mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v") => FileKind::Video,
            _ => FileKind::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Note => "note",
            FileKind::Canvas => "canvas",
            FileKind::Image => "image",
            FileKind::Pdf => "pdf",
            FileKind::Audio => "audio",
            FileKind::Video => "video",
            FileKind::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "note" => FileKind::Note,
            "canvas" => FileKind::Canvas,
            "image" => FileKind::Image,
            "pdf" => FileKind::Pdf,
            "audio" => FileKind::Audio,
            "video" => FileKind::Video,
            _ => FileKind::Other,
        }
    }

    /// Can this be embedded inline in a note?
    pub fn is_embeddable(self) -> bool {
        matches!(
            self,
            FileKind::Note | FileKind::Image | FileKind::Pdf | FileKind::Audio | FileKind::Video
        )
    }
}

/// A file as the index knows it. Deliberately excludes content: listing ten
/// thousand of these must stay cheap.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: VaultPath,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified_ms: i64,
    /// Title from frontmatter if present, otherwise the filename stem.
    pub title: String,
}

/// A folder in the explorer tree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub path: VaultPath,
    pub name: String,
    pub child_file_count: usize,
    pub child_folder_count: usize,
}

/// One level of the explorer, fetched on demand so the tree never loads a
/// whole vault at once.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryListing {
    pub path: VaultPath,
    pub folders: Vec<FolderEntry>,
    pub files: Vec<FileEntry>,
}

/// How a link was written in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    /// `[[Note]]`
    WikiLink,
    /// `![[Note]]` — renders the target inside this note.
    Embed,
    /// `[text](target.md)`
    Markdown,
    /// `![alt](image.png)`
    MarkdownImage,
    /// `[text](https://…)` — recorded so the graph can show external reach,
    /// never followed automatically.
    External,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::WikiLink => "wikiLink",
            LinkKind::Embed => "embed",
            LinkKind::Markdown => "markdown",
            LinkKind::MarkdownImage => "markdownImage",
            LinkKind::External => "external",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "embed" => LinkKind::Embed,
            "markdown" => LinkKind::Markdown,
            "markdownImage" => LinkKind::MarkdownImage,
            "external" => LinkKind::External,
            _ => LinkKind::WikiLink,
        }
    }

    pub fn is_internal(self) -> bool {
        !matches!(self, LinkKind::External)
    }

    pub fn is_embed(self) -> bool {
        matches!(self, LinkKind::Embed | LinkKind::MarkdownImage)
    }
}

/// A link found in a note, with enough position information to jump to it and
/// to rewrite it during a rename.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub kind: LinkKind,
    /// The exact source text, e.g. `[[Note#Heading|Alias]]`.
    pub raw: String,
    /// The target portion before any `#`, `^` or `|`.
    pub target: String,
    pub heading: Option<String>,
    pub block_id: Option<String>,
    pub alias: Option<String>,
    /// Zero-based line in the source.
    pub line: usize,
    /// Byte offsets of `raw` within the source, for surgical rewriting.
    pub byte_start: usize,
    pub byte_end: usize,
}

impl Link {
    /// What the reader sees: the alias if given, otherwise the target.
    pub fn display_text(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.target)
    }
}

/// A link that resolved to a file, as stored in the index.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLink {
    #[serde(flatten)]
    pub link: Link,
    /// `None` when the target does not exist: an unresolved link, which the
    /// UI offers to turn into a new note.
    pub target_path: Option<VaultPath>,
}

/// An incoming reference, with the surrounding text for the backlinks panel.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Backlink {
    pub source_path: VaultPath,
    pub source_title: String,
    pub kind: LinkKind,
    pub line: usize,
    /// The source line, for context in the panel.
    pub context: String,
    pub alias: Option<String>,
}

/// A mention of a note's title in another note's body that is *not* a link.
/// Offered alongside backlinks so the user can convert it into one.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlinkedMention {
    pub source_path: VaultPath,
    pub source_title: String,
    pub line: usize,
    pub context: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// A tag occurrence. `nested/tags` are stored whole; the hierarchy is derived
/// by splitting on `/` at query time.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// Without the leading `#`.
    pub name: String,
    pub line: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// A tag with its vault-wide usage count, for the tag explorer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagSummary {
    pub name: String,
    /// Occurrences of exactly this tag.
    pub count: usize,
    /// Occurrences of this tag and everything nested beneath it.
    pub total_count: usize,
}

/// A heading, used by the outline panel and by `[[Note#Heading]]` resolution.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// URL-safe identifier, also the HTML anchor used by export.
    pub slug: String,
    pub line: usize,
    pub byte_start: usize,
}

/// A block identified by a trailing `^id`, referenceable as `[[Note#^id]]`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: String,
    pub line: usize,
    /// Byte range of the block's text, excluding the `^id` marker itself.
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Everything derived from one note's source, produced in a single parse.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMetadata {
    pub title: Option<String>,
    pub properties: Vec<Property>,
    pub links: Vec<Link>,
    pub tags: Vec<Tag>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<Block>,
    /// Byte length of the frontmatter block including its delimiters, so the
    /// body can be addressed without re-parsing.
    pub frontmatter_bytes: usize,
    pub word_count: usize,
}

/// A note: its identity, its metadata and (only when asked for) its text.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub path: VaultPath,
    pub title: String,
    pub content: String,
    pub metadata: NoteMetadata,
    pub modified_ms: i64,
}

/// A node in the graph view.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub path: Option<VaultPath>,
    pub label: String,
    pub kind: GraphNodeKind,
    /// Total degree, used for node size.
    pub degree: usize,
    pub tags: Vec<String>,
    pub folder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphNodeKind {
    Note,
    Attachment,
    /// A link target with no file behind it.
    Unresolved,
    /// A tag rendered as a node, when tag display is enabled.
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: LinkKind,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// True when the result was capped; the UI says so rather than quietly
    /// showing a partial graph.
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_kind_is_decided_by_extension_case_insensitively() {
        assert_eq!(FileKind::from_extension(Some("md")), FileKind::Note);
        assert_eq!(FileKind::from_extension(Some("png")), FileKind::Image);
        assert_eq!(FileKind::from_extension(Some("pdf")), FileKind::Pdf);
        assert_eq!(FileKind::from_extension(Some("canvas")), FileKind::Canvas);
        assert_eq!(FileKind::from_extension(Some("zip")), FileKind::Other);
        assert_eq!(FileKind::from_extension(None), FileKind::Other);
    }

    #[test]
    fn file_kind_round_trips_through_its_index_representation() {
        for kind in [
            FileKind::Note,
            FileKind::Canvas,
            FileKind::Image,
            FileKind::Pdf,
            FileKind::Audio,
            FileKind::Video,
            FileKind::Other,
        ] {
            assert_eq!(FileKind::parse(kind.as_str()), kind);
        }
    }

    #[test]
    fn link_kind_round_trips_and_classifies() {
        for kind in [
            LinkKind::WikiLink,
            LinkKind::Embed,
            LinkKind::Markdown,
            LinkKind::MarkdownImage,
            LinkKind::External,
        ] {
            assert_eq!(LinkKind::parse(kind.as_str()), kind);
        }
        assert!(LinkKind::Embed.is_embed());
        assert!(!LinkKind::External.is_internal());
    }

    #[test]
    fn a_link_displays_its_alias_when_it_has_one() {
        let mut link = Link {
            kind: LinkKind::WikiLink,
            raw: "[[Note]]".into(),
            target: "Note".into(),
            heading: None,
            block_id: None,
            alias: None,
            line: 0,
            byte_start: 0,
            byte_end: 8,
        };
        assert_eq!(link.display_text(), "Note");
        link.alias = Some("Display".into());
        assert_eq!(link.display_text(), "Display");
    }
}

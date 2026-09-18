//! Mirrors of the core's domain types, shaped for the FFI boundary.
//!
//! These exist rather than exporting `ie-core`'s types directly for two
//! reasons, one forced and one chosen.
//!
//! The forced one: `usize` has no FFI representation, and the core is full of
//! it — every line number, byte offset and count. Something has to choose a
//! width, and doing it here keeps the choice out of the core.
//!
//! The chosen one: adding `#[derive(uniffi::Record)]` to `ie-core` would put a
//! bridge concern inside the layer that is meant to know nothing about its
//! hosts, and would add a dependency the CI check deliberately forbids. A
//! mirror plus a `From` is mechanical, and the compiler checks both directions.
//!
//! A test in `tests/mirrors.rs` walks the core's public model types and fails
//! when one has no mirror here, so a field added to `Note` cannot be silently
//! dropped on the way to Swift.

use ie_core::model as core_model;
use ie_core::vault::VaultPath;

use crate::error::FfiError;

// Paths cross as strings and are re-parsed on arrival.
//
// That re-parse is not ceremony: `VaultPath::parse` is what refuses `..`,
// rejects a reserved Windows name, and normalises to NFC. Swift therefore
// cannot hand the core a path that escapes the vault, however it built the
// string.
uniffi::custom_type!(VaultPath, String, {
    remote,
    lower: |path| path.as_str().to_string(),
    try_lift: |text| VaultPath::parse(&text).map_err(|e| anyhow::anyhow!(e.to_string())),
});

/// `usize` is not an FFI type, and 4 billion lines is not a vault.
fn u32_of(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------- kinds ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FileKind {
    Note,
    Canvas,
    Image,
    Pdf,
    Audio,
    Video,
    Other,
}

impl From<core_model::FileKind> for FileKind {
    fn from(kind: core_model::FileKind) -> Self {
        match kind {
            core_model::FileKind::Note => FileKind::Note,
            core_model::FileKind::Canvas => FileKind::Canvas,
            core_model::FileKind::Image => FileKind::Image,
            core_model::FileKind::Pdf => FileKind::Pdf,
            core_model::FileKind::Audio => FileKind::Audio,
            core_model::FileKind::Video => FileKind::Video,
            core_model::FileKind::Other => FileKind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkKind {
    WikiLink,
    Embed,
    Markdown,
    MarkdownImage,
    External,
}

impl From<core_model::LinkKind> for LinkKind {
    fn from(kind: core_model::LinkKind) -> Self {
        match kind {
            core_model::LinkKind::WikiLink => LinkKind::WikiLink,
            core_model::LinkKind::Embed => LinkKind::Embed,
            core_model::LinkKind::Markdown => LinkKind::Markdown,
            core_model::LinkKind::MarkdownImage => LinkKind::MarkdownImage,
            core_model::LinkKind::External => LinkKind::External,
        }
    }
}

// ----------------------------------------------------------- properties ----

/// A frontmatter value, with its type preserved.
///
/// `Object` carries a list of entries rather than a map: the core uses a
/// `BTreeMap` so YAML keys keep a stable order, and a map across the FFI
/// boundary would lose that ordering on the Swift side. A list keeps it, and
/// keeps round-tripping a note's frontmatter byte-stable.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum PropertyValue {
    Text {
        value: String,
    },
    Number {
        value: f64,
    },
    Checkbox {
        value: bool,
    },
    /// A calendar date with no time, `2026-09-17`.
    Date {
        value: String,
    },
    /// An instant, RFC 3339.
    DateTime {
        value: String,
    },
    List {
        values: Vec<PropertyValue>,
    },
    Object {
        entries: Vec<Property>,
    },
    Null,
}

impl From<core_model::PropertyValue> for PropertyValue {
    fn from(value: core_model::PropertyValue) -> Self {
        use core_model::PropertyValue as V;
        match value {
            V::Text(value) => PropertyValue::Text { value },
            V::Number(value) => PropertyValue::Number { value },
            V::Checkbox(value) => PropertyValue::Checkbox { value },
            V::Date(value) => PropertyValue::Date { value },
            V::DateTime(value) => PropertyValue::DateTime { value },
            V::List(values) => PropertyValue::List {
                values: values.into_iter().map(Into::into).collect(),
            },
            V::Object(map) => PropertyValue::Object {
                entries: map
                    .into_iter()
                    .map(|(key, value)| Property {
                        key,
                        value: value.into(),
                    })
                    .collect(),
            },
            V::Null => PropertyValue::Null,
        }
    }
}

impl From<PropertyValue> for core_model::PropertyValue {
    fn from(value: PropertyValue) -> Self {
        use core_model::PropertyValue as V;
        match value {
            PropertyValue::Text { value } => V::Text(value),
            PropertyValue::Number { value } => V::Number(value),
            PropertyValue::Checkbox { value } => V::Checkbox(value),
            PropertyValue::Date { value } => V::Date(value),
            PropertyValue::DateTime { value } => V::DateTime(value),
            PropertyValue::List { values } => V::List(values.into_iter().map(Into::into).collect()),
            PropertyValue::Object { entries } => V::Object(
                entries
                    .into_iter()
                    .map(|entry| (entry.key, entry.value.into()))
                    .collect(),
            ),
            PropertyValue::Null => V::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Property {
    pub key: String,
    pub value: PropertyValue,
}

impl From<core_model::Property> for Property {
    fn from(property: core_model::Property) -> Self {
        Self {
            key: property.key,
            value: property.value.into(),
        }
    }
}

impl From<Property> for core_model::Property {
    fn from(property: Property) -> Self {
        Self {
            key: property.key,
            value: property.value.into(),
        }
    }
}

// ---------------------------------------------------------------- notes ----

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Link {
    pub kind: LinkKind,
    /// The source text exactly as written, e.g. `[[Note#Heading|Alias]]`.
    pub raw: String,
    pub target: String,
    pub heading: Option<String>,
    pub block_id: Option<String>,
    pub alias: Option<String>,
    pub line: u32,
    /// Byte offsets of `raw` in the source, so an edit can be surgical.
    pub byte_start: u32,
    pub byte_end: u32,
}

impl From<core_model::Link> for Link {
    fn from(link: core_model::Link) -> Self {
        Self {
            kind: link.kind.into(),
            raw: link.raw,
            target: link.target,
            heading: link.heading,
            block_id: link.block_id,
            alias: link.alias,
            line: u32_of(link.line),
            byte_start: u32_of(link.byte_start),
            byte_end: u32_of(link.byte_end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ResolvedLink {
    pub link: Link,
    /// `None` for an unresolved link, which the UI offers to turn into a note.
    pub target_path: Option<VaultPath>,
}

impl From<core_model::ResolvedLink> for ResolvedLink {
    fn from(resolved: core_model::ResolvedLink) -> Self {
        Self {
            link: resolved.link.into(),
            target_path: resolved.target_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Backlink {
    pub source_path: VaultPath,
    pub source_title: String,
    pub kind: LinkKind,
    pub line: u32,
    pub context: String,
    pub alias: Option<String>,
}

impl From<core_model::Backlink> for Backlink {
    fn from(backlink: core_model::Backlink) -> Self {
        Self {
            source_path: backlink.source_path,
            source_title: backlink.source_title,
            kind: backlink.kind.into(),
            line: u32_of(backlink.line),
            context: backlink.context,
            alias: backlink.alias,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct UnlinkedMention {
    pub source_path: VaultPath,
    pub source_title: String,
    pub line: u32,
    pub context: String,
    pub byte_start: u32,
    pub byte_end: u32,
}

impl From<core_model::UnlinkedMention> for UnlinkedMention {
    fn from(mention: core_model::UnlinkedMention) -> Self {
        Self {
            source_path: mention.source_path,
            source_title: mention.source_title,
            line: u32_of(mention.line),
            context: mention.context,
            byte_start: u32_of(mention.byte_start),
            byte_end: u32_of(mention.byte_end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Tag {
    /// Without the leading `#`. Nested tags are stored whole.
    pub name: String,
    pub line: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

impl From<core_model::Tag> for Tag {
    fn from(tag: core_model::Tag) -> Self {
        Self {
            name: tag.name,
            line: u32_of(tag.line),
            byte_start: u32_of(tag.byte_start),
            byte_end: u32_of(tag.byte_end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TagSummary {
    pub name: String,
    pub count: u32,
    /// This tag plus everything nested beneath it.
    pub total_count: u32,
}

impl From<core_model::TagSummary> for TagSummary {
    fn from(summary: core_model::TagSummary) -> Self {
        Self {
            name: summary.name,
            count: u32_of(summary.count),
            total_count: u32_of(summary.total_count),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    /// URL-safe identifier, also the anchor export writes.
    pub slug: String,
    pub line: u32,
    pub byte_start: u32,
}

impl From<core_model::Heading> for Heading {
    fn from(heading: core_model::Heading) -> Self {
        Self {
            level: heading.level,
            text: heading.text,
            slug: heading.slug,
            line: u32_of(heading.line),
            byte_start: u32_of(heading.byte_start),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Block {
    pub id: String,
    pub line: u32,
    /// The block's text, excluding the `^id` marker.
    pub byte_start: u32,
    pub byte_end: u32,
}

impl From<core_model::Block> for Block {
    fn from(block: core_model::Block) -> Self {
        Self {
            id: block.id,
            line: u32_of(block.line),
            byte_start: u32_of(block.byte_start),
            byte_end: u32_of(block.byte_end),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NoteMetadata {
    pub title: Option<String>,
    pub properties: Vec<Property>,
    pub links: Vec<Link>,
    pub tags: Vec<Tag>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<Block>,
    /// Length of the frontmatter block including delimiters, so the body can be
    /// addressed without re-parsing.
    pub frontmatter_bytes: u32,
    pub word_count: u32,
}

impl From<core_model::NoteMetadata> for NoteMetadata {
    fn from(metadata: core_model::NoteMetadata) -> Self {
        Self {
            title: metadata.title,
            properties: metadata.properties.into_iter().map(Into::into).collect(),
            links: metadata.links.into_iter().map(Into::into).collect(),
            tags: metadata.tags.into_iter().map(Into::into).collect(),
            headings: metadata.headings.into_iter().map(Into::into).collect(),
            blocks: metadata.blocks.into_iter().map(Into::into).collect(),
            frontmatter_bytes: u32_of(metadata.frontmatter_bytes),
            word_count: u32_of(metadata.word_count),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Note {
    pub path: VaultPath,
    pub title: String,
    pub content: String,
    pub metadata: NoteMetadata,
    /// When the file was last written.
    ///
    /// The host records this when it opens a buffer and hands it back on save;
    /// a mismatch is what makes an external change impossible to overwrite by
    /// accident.
    pub modified_ms: i64,
}

impl From<core_model::Note> for Note {
    fn from(note: core_model::Note) -> Self {
        Self {
            path: note.path,
            title: note.title,
            content: note.content,
            metadata: note.metadata.into(),
            modified_ms: note.modified_ms,
        }
    }
}

// ------------------------------------------------------------ the tree ----

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FileEntry {
    pub path: VaultPath,
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    pub modified_ms: i64,
    /// Frontmatter title if there is one, otherwise the filename stem.
    pub title: String,
}

impl From<core_model::FileEntry> for FileEntry {
    fn from(entry: core_model::FileEntry) -> Self {
        Self {
            path: entry.path,
            name: entry.name,
            kind: entry.kind.into(),
            size: entry.size,
            modified_ms: entry.modified_ms,
            title: entry.title,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FolderEntry {
    pub path: VaultPath,
    pub name: String,
    pub child_file_count: u32,
    pub child_folder_count: u32,
}

impl From<core_model::FolderEntry> for FolderEntry {
    fn from(entry: core_model::FolderEntry) -> Self {
        Self {
            path: entry.path,
            name: entry.name,
            child_file_count: u32_of(entry.child_file_count),
            child_folder_count: u32_of(entry.child_folder_count),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DirectoryListing {
    pub path: VaultPath,
    pub folders: Vec<FolderEntry>,
    pub files: Vec<FileEntry>,
}

impl From<core_model::DirectoryListing> for DirectoryListing {
    fn from(listing: core_model::DirectoryListing) -> Self {
        Self {
            path: listing.path,
            folders: listing.folders.into_iter().map(Into::into).collect(),
            files: listing.files.into_iter().map(Into::into).collect(),
        }
    }
}

// --------------------------------------------------------------- search ----

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchHit {
    pub path: VaultPath,
    pub title: String,
    pub kind: FileKind,
    /// The match with context. Empty for a filter-only query.
    pub snippet: String,
    pub line: Option<u32>,
    pub modified_ms: i64,
    /// Lower is better, so two result sets can be merged.
    pub rank: f64,
}

impl From<ie_core::search::SearchHit> for SearchHit {
    fn from(hit: ie_core::search::SearchHit) -> Self {
        Self {
            path: hit.path,
            title: hit.title,
            kind: hit.kind.into(),
            snippet: hit.snippet,
            line: hit.line.map(u32_of),
            modified_ms: hit.modified_ms,
            rank: hit.rank,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// Matches before paging, so the UI can say "1–100 of 4,312".
    pub total: u32,
    pub truncated: bool,
}

impl From<ie_core::search::SearchResults> for SearchResults {
    fn from(results: ie_core::search::SearchResults) -> Self {
        Self {
            hits: results.hits.into_iter().map(Into::into).collect(),
            total: u32_of(results.total),
            truncated: results.truncated,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchOptions {
    #[uniffi(default = 100)]
    pub limit: u32,
    #[uniffi(default = 0)]
    pub offset: u32,
    /// Tokens of context either side of a match.
    #[uniffi(default = 12)]
    pub snippet_tokens: u32,
    /// What wraps a match in `snippet`. The host decides, because what the
    /// desktop wants (`<mark>`) is not what an attributed string wants.
    #[uniffi(default = "\u{2}")]
    pub highlight_open: String,
    #[uniffi(default = "\u{3}")]
    pub highlight_close: String,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 100,
            offset: 0,
            snippet_tokens: 12,
            // Control characters rather than HTML: a native client builds an
            // `AttributedString`, and `<mark>` in a note's own text would be
            // indistinguishable from a highlight.
            highlight_open: "\u{2}".into(),
            highlight_close: "\u{3}".into(),
        }
    }
}

impl From<SearchOptions> for ie_core::search::SearchOptions {
    fn from(options: SearchOptions) -> Self {
        Self {
            limit: options.limit as usize,
            offset: options.offset as usize,
            snippet_tokens: options.snippet_tokens as usize,
            highlight_open: options.highlight_open,
            highlight_close: options.highlight_close,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FileMatch {
    pub path: VaultPath,
    pub title: String,
    pub kind: FileKind,
    pub score: i32,
    /// Character offsets in `path` that matched, for highlighting.
    pub positions: Vec<u32>,
    pub modified_ms: i64,
}

impl From<ie_core::search::FileMatch> for FileMatch {
    fn from(matched: ie_core::search::FileMatch) -> Self {
        Self {
            path: matched.path,
            title: matched.title,
            kind: matched.kind.into(),
            score: matched.score,
            positions: matched.positions.into_iter().map(u32_of).collect(),
            modified_ms: matched.modified_ms,
        }
    }
}

// ---------------------------------------------------------------- graph ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GraphNodeKind {
    Note,
    Attachment,
    /// A link target with no file behind it.
    Unresolved,
    Tag,
}

impl From<core_model::GraphNodeKind> for GraphNodeKind {
    fn from(kind: core_model::GraphNodeKind) -> Self {
        match kind {
            core_model::GraphNodeKind::Note => GraphNodeKind::Note,
            core_model::GraphNodeKind::Attachment => GraphNodeKind::Attachment,
            core_model::GraphNodeKind::Unresolved => GraphNodeKind::Unresolved,
            core_model::GraphNodeKind::Tag => GraphNodeKind::Tag,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GraphNode {
    pub id: String,
    pub path: Option<VaultPath>,
    pub label: String,
    pub kind: GraphNodeKind,
    /// Total degree, which is what sizes the node.
    pub degree: u32,
    pub tags: Vec<String>,
    pub folder: String,
}

impl From<core_model::GraphNode> for GraphNode {
    fn from(node: core_model::GraphNode) -> Self {
        Self {
            id: node.id,
            path: node.path,
            label: node.label,
            kind: node.kind.into(),
            degree: u32_of(node.degree),
            tags: node.tags,
            folder: node.folder,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: LinkKind,
}

impl From<core_model::GraphEdge> for GraphEdge {
    fn from(edge: core_model::GraphEdge) -> Self {
        Self {
            source: edge.source,
            target: edge.target,
            kind: edge.kind.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// True when the result was capped. The UI says so rather than quietly
    /// showing a partial graph.
    pub truncated: bool,
}

impl From<core_model::GraphData> for GraphData {
    fn from(data: core_model::GraphData) -> Self {
        Self {
            nodes: data.nodes.into_iter().map(Into::into).collect(),
            edges: data.edges.into_iter().map(Into::into).collect(),
            truncated: data.truncated,
        }
    }
}

// ------------------------------------------------- vault-level outcomes ----

/// A non-fatal problem found while scanning. Collected and reported together,
/// because a vault with a hundred case collisions deserves one report.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Diagnostic {
    /// Two names differing only by case. Fine on ext4; on the volume iOS ships
    /// with, one of them shadows the other.
    CaseConflict { paths: Vec<VaultPath> },
    /// A name Windows would reject, in a vault authored elsewhere.
    UnportableName { path: VaultPath, reason: String },
    /// A leftover `.ie-tmp-*`: a previous write was interrupted.
    InterruptedWrite { path: VaultPath },
    /// Unreadable, so left untouched and excluded rather than treated as empty.
    UnreadableFile { path: VaultPath, message: String },
    /// Frontmatter that is not valid YAML. Still indexed for text and links.
    MalformedFrontmatter { path: VaultPath, message: String },
    /// A symlink pointing out of the vault. Not followed.
    EscapingSymlink { path: VaultPath },
}

impl From<ie_core::error::Diagnostic> for Diagnostic {
    fn from(diagnostic: ie_core::error::Diagnostic) -> Self {
        use ie_core::error::Diagnostic as D;
        match diagnostic {
            D::CaseConflict { paths } => Diagnostic::CaseConflict { paths },
            D::UnportableName { path, reason } => Diagnostic::UnportableName { path, reason },
            D::InterruptedWrite { path } => Diagnostic::InterruptedWrite { path },
            D::UnreadableFile { path, message } => Diagnostic::UnreadableFile { path, message },
            D::MalformedFrontmatter { path, message } => {
                Diagnostic::MalformedFrontmatter { path, message }
            }
            D::EscapingSymlink { path } => Diagnostic::EscapingSymlink { path },
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScanReport {
    pub files_seen: u32,
    pub files_indexed: u32,
    /// Unchanged since the last scan, so not re-parsed. On a warm launch this
    /// should be nearly all of them.
    pub files_unchanged: u32,
    pub files_removed: u32,
    pub duration_ms: u64,
    pub diagnostics: Vec<Diagnostic>,
}

impl From<ie_core::index::ScanReport> for ScanReport {
    fn from(report: ie_core::index::ScanReport) -> Self {
        Self {
            files_seen: u32_of(report.files_seen),
            files_indexed: u32_of(report.files_indexed),
            files_unchanged: u32_of(report.files_unchanged),
            files_removed: u32_of(report.files_removed),
            duration_ms: report.duration_ms,
            diagnostics: report.diagnostics.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct IndexProgress {
    pub scanned: u32,
    pub indexed: u32,
    /// `None` until the walk has finished counting, so a progress bar knows to
    /// stay indeterminate rather than guessing.
    pub total: Option<u32>,
    pub current: Option<VaultPath>,
}

impl From<ie_core::index::IndexProgress> for IndexProgress {
    fn from(progress: ie_core::index::IndexProgress) -> Self {
        Self {
            scanned: u32_of(progress.scanned),
            indexed: u32_of(progress.indexed),
            total: progress.total.map(u32_of),
            current: progress.current,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventOutcome {
    pub indexed: Vec<VaultPath>,
    pub removed: Vec<VaultPath>,
    /// The watcher lost events; only a full rescan can be trusted now.
    pub needs_full_scan: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl From<ie_core::index::EventOutcome> for EventOutcome {
    fn from(outcome: ie_core::index::EventOutcome) -> Self {
        Self {
            indexed: outcome.indexed,
            removed: outcome.removed,
            needs_full_scan: outcome.needs_full_scan,
            diagnostics: outcome.diagnostics.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OpenOutcome {
    /// An existing, usable index was reused.
    Reused,
    Created,
    RebuiltForSchemaChange,
    RebuiltAfterCorruption,
}

impl OpenOutcome {
    pub fn needs_full_scan(self) -> bool {
        !matches!(self, OpenOutcome::Reused)
    }
}

impl From<ie_core::index::OpenOutcome> for OpenOutcome {
    fn from(outcome: ie_core::index::OpenOutcome) -> Self {
        use ie_core::index::OpenOutcome as O;
        match outcome {
            O::Reused => OpenOutcome::Reused,
            O::Created => OpenOutcome::Created,
            O::RebuiltForSchemaChange => OpenOutcome::RebuiltForSchemaChange,
            O::RebuiltAfterCorruption => OpenOutcome::RebuiltAfterCorruption,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OpenReport {
    /// The vault's location, for logging only. The host addresses the vault by
    /// bookmark and must never persist this.
    pub root: String,
    pub name: String,
    pub vault_id: String,
    pub index: OpenOutcome,
    /// The app's own folder had to be created: this is a vault being opened
    /// for the first time.
    pub created: bool,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrashEntry {
    pub id: String,
    /// Where it was, so restore can put it back.
    pub original_path: VaultPath,
    pub trashed_ms: i64,
    pub stored_as: String,
    pub is_dir: bool,
    pub size: u64,
}

impl From<ie_core::vault::TrashEntry> for TrashEntry {
    fn from(entry: ie_core::vault::TrashEntry) -> Self {
        Self {
            id: entry.id,
            original_path: entry.original_path,
            trashed_ms: entry.trashed_ms,
            stored_as: entry.stored_as,
            is_dir: entry.is_dir,
            size: entry.size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RenameEdit {
    pub path: VaultPath,
    pub link_count: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RenamePlan {
    pub from: VaultPath,
    pub to: VaultPath,
    /// Files whose links would be rewritten. Shown before the rename, so
    /// "this will touch 43 other notes" is something the user can decline.
    pub edits: Vec<RenameEdit>,
    pub total_links: u32,
}

impl From<ie_core::links::RenamePlan> for RenamePlan {
    fn from(plan: ie_core::links::RenamePlan) -> Self {
        Self {
            from: plan.from,
            to: plan.to,
            edits: plan
                .edits
                .into_iter()
                .map(|edit| RenameEdit {
                    path: edit.path,
                    link_count: u32_of(edit.link_count),
                })
                .collect(),
            total_links: u32_of(plan.total_links),
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RenameOutcome {
    pub from: VaultPath,
    pub to: VaultPath,
    pub files_updated: u32,
    pub links_updated: u32,
    /// Files whose links could not be rewritten. The rename still happened;
    /// these are reported rather than swallowed.
    pub failures: Vec<String>,
}

impl From<ie_core::session::RenameOutcome> for RenameOutcome {
    fn from(outcome: ie_core::session::RenameOutcome) -> Self {
        Self {
            from: outcome.from,
            to: outcome.to,
            files_updated: u32_of(outcome.files_updated),
            links_updated: u32_of(outcome.links_updated),
            failures: outcome.failures,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RecoveryCandidate {
    pub path: VaultPath,
    pub content: String,
    pub saved_ms: i64,
    /// The journal's text already matches the file: nothing to recover.
    pub already_saved: bool,
    /// The file is newer than the journal, so restoring would go backwards.
    /// Offered anyway, but the UI must say so.
    pub file_is_newer: bool,
}

impl From<ie_core::recovery::RecoveryCandidate> for RecoveryCandidate {
    fn from(candidate: ie_core::recovery::RecoveryCandidate) -> Self {
        Self {
            path: candidate.path,
            content: candidate.content,
            saved_ms: candidate.saved_ms,
            already_saved: candidate.already_saved,
            file_is_newer: candidate.file_is_newer,
        }
    }
}

/// What to do when a new note's name is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Collision {
    /// Refuse, so the caller can prompt.
    Fail,
    /// Append ` (1)`, ` (2)`, … until a free name is found.
    Rename,
    /// Replace. Only for a caller that has already asked.
    Overwrite,
}

impl From<Collision> for ie_core::vault::Collision {
    fn from(collision: Collision) -> Self {
        match collision {
            Collision::Fail => ie_core::vault::Collision::Fail,
            Collision::Rename => ie_core::vault::Collision::Rename,
            Collision::Overwrite => ie_core::vault::Collision::Overwrite,
        }
    }
}

/// A filesystem change observed by the host.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FsEvent {
    Created {
        path: String,
    },
    Modified {
        path: String,
    },
    Deleted {
        path: String,
    },
    Renamed {
        from: String,
        to: String,
    },
    /// The host lost track — it was suspended, or a bookmark resolved stale.
    /// The only safe response is a rescan.
    Rescan,
}

impl FsEvent {
    /// Translate to the core's vocabulary, resolving against the vault root.
    ///
    /// The host reports paths relative to the vault so it never has to hold or
    /// hand over an absolute one, which is the same discipline that keeps
    /// absolute paths out of the vault's own files.
    pub fn to_core(
        &self,
        root: &std::path::Path,
    ) -> std::result::Result<ie_platform::FsEvent, FfiError> {
        let resolve = |text: &str| -> std::result::Result<std::path::PathBuf, FfiError> {
            VaultPath::parse(text)
                .map(|p| p.to_fs_path(root))
                .map_err(|e| FfiError::InvalidArgument {
                    message: e.to_string(),
                })
        };
        Ok(match self {
            FsEvent::Created { path } => ie_platform::FsEvent::Created(resolve(path)?),
            FsEvent::Modified { path } => ie_platform::FsEvent::Modified(resolve(path)?),
            FsEvent::Deleted { path } => ie_platform::FsEvent::Deleted(resolve(path)?),
            FsEvent::Renamed { from, to } => ie_platform::FsEvent::Renamed {
                from: resolve(from)?,
                to: resolve(to)?,
            },
            FsEvent::Rescan => ie_platform::FsEvent::Rescan {
                root: root.to_path_buf(),
            },
        })
    }
}

/// A note that has just been written, with the base a save can compare against.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CreatedNote {
    /// Where it landed. Not necessarily where it was asked for:
    /// `Collision::Rename` appends ` (1)` until a free name is found.
    pub path: VaultPath,
    pub modified_ms: i64,
}

/// A daily note, and whether this call is what brought it into being.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DailyNote {
    pub path: VaultPath,
    /// True when it was just created from the template, so the UI can say
    /// "new" rather than silently opening an empty note.
    pub created: bool,
}

/// Work out what a typed-in property value means.
///
/// The user types `2026-09-17` into a property field. Is that a date or a
/// string? YAML says a date, the desktop already agrees, and iOS must too —
/// otherwise the same note round-tripping between them would gain and lose
/// quotes, and a `date:` filter would match on one platform and not the other.
///
/// So the inference is the core's, exposed rather than reimplemented.
#[uniffi::export]
pub fn infer_property_value(raw: String) -> PropertyValue {
    core_model::PropertyValue::infer_from_scalar(&raw).into()
}

/// Render a property value the way it would be typed back in.
#[uniffi::export]
pub fn property_value_as_text(value: PropertyValue) -> String {
    core_model::PropertyValue::from(value).as_text()
}

/// What kind of editor a property needs: a switch, a date picker, a field.
#[uniffi::export]
pub fn property_value_kind(value: PropertyValue) -> PropertyKind {
    core_model::PropertyValue::from(value).kind().into()
}

/// The kinds a property value can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PropertyKind {
    Text,
    Number,
    Checkbox,
    Date,
    DateTime,
    List,
    Object,
    Null,
}

impl From<core_model::PropertyKind> for PropertyKind {
    fn from(kind: core_model::PropertyKind) -> Self {
        use core_model::PropertyKind as K;
        match kind {
            K::Text => PropertyKind::Text,
            K::Number => PropertyKind::Number,
            K::Checkbox => PropertyKind::Checkbox,
            K::Date => PropertyKind::Date,
            K::DateTime => PropertyKind::DateTime,
            K::List => PropertyKind::List,
            K::Object => PropertyKind::Object,
            K::Null => PropertyKind::Null,
        }
    }
}

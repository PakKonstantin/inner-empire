//! Keeping the bridge's types in step with the core's.
//!
//! `types.rs` mirrors the core's domain types rather than exporting them
//! directly, and a mirror is a thing that can fall behind. These tests are the
//! check: adding a field to `Note` without adding it here has to fail
//! something, or the field silently never reaches Swift.
//!
//! The mechanism is exhaustive destructuring. `let Core { a, b, c } = value;`
//! stops compiling the moment the core grows a `d`, and the error names it.
//! That is a compile-time guarantee, which is stronger than any runtime check
//! and needs no reflection.

use ie_core::model as core_model;
use ie_core::vault::VaultPath;
use ie_ffi::types::*;

fn path(text: &str) -> VaultPath {
    VaultPath::parse(text).unwrap()
}

/// Every field of every mirrored type, named once.
///
/// If this stops compiling, the core gained a field. Add it to the mirror in
/// `types.rs`, add it to its `From`, and add it here — in that order.
#[test]
fn every_core_field_is_accounted_for() {
    let core_model::Link {
        kind: _,
        raw: _,
        target: _,
        heading: _,
        block_id: _,
        alias: _,
        line: _,
        byte_start: _,
        byte_end: _,
    } = core_model::Link {
        kind: core_model::LinkKind::WikiLink,
        raw: "[[x]]".into(),
        target: "x".into(),
        heading: None,
        block_id: None,
        alias: None,
        line: 0,
        byte_start: 0,
        byte_end: 5,
    };

    let core_model::Backlink {
        source_path: _,
        source_title: _,
        kind: _,
        line: _,
        context: _,
        alias: _,
    } = core_model::Backlink {
        source_path: path("a.md"),
        source_title: "a".into(),
        kind: core_model::LinkKind::WikiLink,
        line: 0,
        context: String::new(),
        alias: None,
    };

    let core_model::Tag {
        name: _,
        line: _,
        byte_start: _,
        byte_end: _,
    } = core_model::Tag {
        name: "t".into(),
        line: 0,
        byte_start: 0,
        byte_end: 2,
    };

    let core_model::TagSummary {
        name: _,
        count: _,
        total_count: _,
    } = core_model::TagSummary {
        name: "t".into(),
        count: 1,
        total_count: 1,
    };

    let core_model::Heading {
        level: _,
        text: _,
        slug: _,
        line: _,
        byte_start: _,
    } = core_model::Heading {
        level: 1,
        text: "H".into(),
        slug: "h".into(),
        line: 0,
        byte_start: 0,
    };

    let core_model::Block {
        id: _,
        line: _,
        byte_start: _,
        byte_end: _,
    } = core_model::Block {
        id: "b".into(),
        line: 0,
        byte_start: 0,
        byte_end: 1,
    };

    let core_model::NoteMetadata {
        title: _,
        properties: _,
        links: _,
        tags: _,
        headings: _,
        blocks: _,
        frontmatter_bytes: _,
        word_count: _,
    } = core_model::NoteMetadata::default();

    let core_model::Note {
        path: _,
        title: _,
        content: _,
        metadata: _,
        modified_ms: _,
    } = core_model::Note {
        path: path("a.md"),
        title: "a".into(),
        content: String::new(),
        metadata: core_model::NoteMetadata::default(),
        modified_ms: 0,
    };

    let core_model::FileEntry {
        path: _,
        name: _,
        kind: _,
        size: _,
        modified_ms: _,
        title: _,
    } = core_model::FileEntry {
        path: path("a.md"),
        name: "a.md".into(),
        kind: core_model::FileKind::Note,
        size: 0,
        modified_ms: 0,
        title: "a".into(),
    };

    let core_model::FolderEntry {
        path: _,
        name: _,
        child_file_count: _,
        child_folder_count: _,
    } = core_model::FolderEntry {
        path: path("f"),
        name: "f".into(),
        child_file_count: 0,
        child_folder_count: 0,
    };

    let core_model::GraphNode {
        id: _,
        path: _,
        label: _,
        kind: _,
        degree: _,
        tags: _,
        folder: _,
    } = core_model::GraphNode {
        id: "1".into(),
        path: None,
        label: "n".into(),
        kind: core_model::GraphNodeKind::Note,
        degree: 0,
        tags: Vec::new(),
        folder: String::new(),
    };

    let core_model::GraphEdge {
        source: _,
        target: _,
        kind: _,
    } = core_model::GraphEdge {
        source: "1".into(),
        target: "2".into(),
        kind: core_model::LinkKind::WikiLink,
    };

    let ie_core::search::SearchHit {
        path: _,
        title: _,
        kind: _,
        snippet: _,
        line: _,
        modified_ms: _,
        rank: _,
    } = ie_core::search::SearchHit {
        path: path("a.md"),
        title: "a".into(),
        kind: core_model::FileKind::Note,
        snippet: String::new(),
        line: None,
        modified_ms: 0,
        rank: 0.0,
    };

    let ie_core::search::FileMatch {
        path: _,
        title: _,
        kind: _,
        score: _,
        positions: _,
        modified_ms: _,
    } = ie_core::search::FileMatch {
        path: path("a.md"),
        title: "a".into(),
        kind: core_model::FileKind::Note,
        score: 0,
        positions: Vec::new(),
        modified_ms: 0,
    };

    let ie_core::index::ScanReport {
        files_seen: _,
        files_indexed: _,
        files_unchanged: _,
        files_removed: _,
        duration_ms: _,
        diagnostics: _,
    } = ie_core::index::ScanReport::default();

    let ie_core::index::IndexProgress {
        scanned: _,
        indexed: _,
        total: _,
        current: _,
    } = ie_core::index::IndexProgress {
        scanned: 0,
        indexed: 0,
        total: None,
        current: None,
    };

    let ie_core::index::EventOutcome {
        indexed: _,
        removed: _,
        needs_full_scan: _,
        diagnostics: _,
    } = ie_core::index::EventOutcome::default();

    let ie_core::vault::TrashEntry {
        id: _,
        original_path: _,
        trashed_ms: _,
        stored_as: _,
        is_dir: _,
        size: _,
    } = ie_core::vault::TrashEntry {
        id: "1".into(),
        original_path: path("a.md"),
        trashed_ms: 0,
        stored_as: "1-a.md".into(),
        is_dir: false,
        size: 0,
    };

    let ie_core::links::RenamePlan {
        from: _,
        to: _,
        edits: _,
        total_links: _,
    } = ie_core::links::RenamePlan::default();

    let ie_core::links::RenameEdit {
        path: _,
        link_count: _,
    } = ie_core::links::RenameEdit {
        path: path("a.md"),
        link_count: 0,
    };

    let ie_core::session::RenameOutcome {
        from: _,
        to: _,
        files_updated: _,
        links_updated: _,
        failures: _,
    } = ie_core::session::RenameOutcome {
        from: path("a.md"),
        to: path("b.md"),
        files_updated: 0,
        links_updated: 0,
        failures: Vec::new(),
    };

    let ie_core::recovery::RecoveryCandidate {
        path: _,
        content: _,
        saved_ms: _,
        already_saved: _,
        file_is_newer: _,
    } = ie_core::recovery::RecoveryCandidate {
        path: path("a.md"),
        content: String::new(),
        saved_ms: 0,
        already_saved: false,
        file_is_newer: false,
    };
}

/// Every variant of every mirrored enum, matched exhaustively.
///
/// A `match` with no wildcard fails to compile when the core adds a variant,
/// which is what stops a new `FileKind` quietly becoming `Other` on iOS.
#[test]
fn every_core_variant_is_accounted_for() {
    fn file_kind(kind: core_model::FileKind) -> FileKind {
        match kind {
            core_model::FileKind::Note
            | core_model::FileKind::Canvas
            | core_model::FileKind::Image
            | core_model::FileKind::Pdf
            | core_model::FileKind::Audio
            | core_model::FileKind::Video
            | core_model::FileKind::Other => kind.into(),
        }
    }

    fn link_kind(kind: core_model::LinkKind) -> LinkKind {
        match kind {
            core_model::LinkKind::WikiLink
            | core_model::LinkKind::Embed
            | core_model::LinkKind::Markdown
            | core_model::LinkKind::MarkdownImage
            | core_model::LinkKind::External => kind.into(),
        }
    }

    fn node_kind(kind: core_model::GraphNodeKind) -> GraphNodeKind {
        match kind {
            core_model::GraphNodeKind::Note
            | core_model::GraphNodeKind::Attachment
            | core_model::GraphNodeKind::Unresolved
            | core_model::GraphNodeKind::Tag => kind.into(),
        }
    }

    fn property_value(value: core_model::PropertyValue) -> PropertyValue {
        match value {
            core_model::PropertyValue::Text(_)
            | core_model::PropertyValue::Number(_)
            | core_model::PropertyValue::Checkbox(_)
            | core_model::PropertyValue::Date(_)
            | core_model::PropertyValue::DateTime(_)
            | core_model::PropertyValue::List(_)
            | core_model::PropertyValue::Object(_)
            | core_model::PropertyValue::Null => value.into(),
        }
    }

    fn diagnostic(d: ie_core::error::Diagnostic) -> Diagnostic {
        match d {
            ie_core::error::Diagnostic::CaseConflict { .. }
            | ie_core::error::Diagnostic::UnportableName { .. }
            | ie_core::error::Diagnostic::InterruptedWrite { .. }
            | ie_core::error::Diagnostic::UnreadableFile { .. }
            | ie_core::error::Diagnostic::MalformedFrontmatter { .. }
            | ie_core::error::Diagnostic::EscapingSymlink { .. } => d.into(),
        }
    }

    fn open_outcome(o: ie_core::index::OpenOutcome) -> OpenOutcome {
        match o {
            ie_core::index::OpenOutcome::Reused
            | ie_core::index::OpenOutcome::Created
            | ie_core::index::OpenOutcome::RebuiltForSchemaChange
            | ie_core::index::OpenOutcome::RebuiltAfterCorruption => o.into(),
        }
    }

    assert_eq!(file_kind(core_model::FileKind::Pdf), FileKind::Pdf);
    assert_eq!(link_kind(core_model::LinkKind::Embed), LinkKind::Embed);
    assert_eq!(
        node_kind(core_model::GraphNodeKind::Tag),
        GraphNodeKind::Tag
    );
    assert_eq!(
        property_value(core_model::PropertyValue::Null),
        PropertyValue::Null
    );
    assert!(matches!(
        diagnostic(ie_core::error::Diagnostic::InterruptedWrite { path: path("a.md") }),
        Diagnostic::InterruptedWrite { .. }
    ));
    assert_eq!(
        open_outcome(ie_core::index::OpenOutcome::Reused),
        OpenOutcome::Reused
    );
}

#[test]
fn property_values_round_trip_through_the_bridge_unchanged() {
    use std::collections::BTreeMap;

    let nested = core_model::PropertyValue::Object(BTreeMap::from([
        (
            "alpha".to_string(),
            core_model::PropertyValue::Text("one".into()),
        ),
        (
            "beta".to_string(),
            core_model::PropertyValue::List(vec![
                core_model::PropertyValue::Number(1.0),
                core_model::PropertyValue::Checkbox(false),
                core_model::PropertyValue::Null,
            ]),
        ),
        (
            "gamma".to_string(),
            core_model::PropertyValue::Date("2026-09-17".into()),
        ),
    ]));

    let original = core_model::Property {
        key: "meta".into(),
        value: nested,
    };

    let there: Property = original.clone().into();
    let back: core_model::Property = there.clone().into();
    assert_eq!(back, original, "a property must survive the round trip");

    // And the map's ordering survives, which is what keeps rewriting a note's
    // frontmatter from reshuffling it.
    let PropertyValue::Object { entries } = &there.value else {
        panic!("expected an object");
    };
    let keys: Vec<_> = entries.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn a_count_too_large_for_the_bridge_saturates_rather_than_wrapping() {
    // Not a vault anyone has, but wrapping would turn "four billion links" into
    // "three", which is worse than "as many as we can say".
    let summary: TagSummary = core_model::TagSummary {
        name: "t".into(),
        count: usize::MAX,
        total_count: usize::MAX,
    }
    .into();
    assert_eq!(summary.count, u32::MAX);
}

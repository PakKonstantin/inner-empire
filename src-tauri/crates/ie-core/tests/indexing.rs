//! End-to-end tests for scanning a vault and keeping the index in step.

mod common;

use common::TestVault;
use ie_core::error::Diagnostic;
use ie_core::index::{queries, GraphOptions};
use ie_core::model::LinkKind;

#[test]
fn a_full_scan_indexes_notes_and_skips_app_internals() {
    let mut vault = TestVault::new();
    vault.write("Notes/Alpha.md", "# Alpha\n\nLinks to [[Beta]].\n");
    vault.write("Notes/Beta.md", "# Beta\n\n#research\n");
    vault.write("Attachments/diagram.png", "not really a png");
    vault.write(".inner-empire/workspace.json", "{}");

    let report = vault.scan();

    assert_eq!(report.files_indexed, 3, "{report:?}");
    assert_eq!(vault.db.file_count().unwrap(), 3);
    assert!(
        queries::file(
            vault.db.connection(),
            &TestVault::path(".inner-empire/workspace.json")
        )
        .unwrap()
        .is_none(),
        "app-internal files must never be indexed"
    );
}

#[test]
fn metadata_from_a_note_reaches_every_table() {
    let mut vault = TestVault::new();
    vault.write(
        "Note.md",
        "---\ntitle: The Title\nstatus: active\nrating: 8\ntags:\n  - AI\n---\n\
         # Heading One\n\nBody with [[Other]] and #inline/tag.\n\nA key point. ^kp\n\n## Heading Two\n",
    );
    vault.write("Other.md", "# Other\n");
    vault.scan();

    let conn = vault.db.connection();
    let path = TestVault::path("Note.md");

    let file = queries::file(conn, &path).unwrap().unwrap();
    assert_eq!(file.title, "The Title");

    let headings = queries::headings(conn, &path).unwrap();
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[0].text, "Heading One");
    assert_eq!(headings[1].level, 2);

    let blocks = queries::blocks(conn, &path).unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].id, "kp");

    let links = queries::outgoing_links(conn, &path).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target_path.as_ref().unwrap().as_str(), "Other.md");

    let tags: Vec<String> = queries::tag_summaries(conn)
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert!(tags.contains(&"inline/tag".to_string()), "{tags:?}");
    assert!(
        tags.contains(&"AI".to_string()),
        "frontmatter tags count too: {tags:?}"
    );

    let keys: Vec<String> = queries::property_keys(conn)
        .unwrap()
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    assert!(keys.contains(&"status".to_string()), "{keys:?}");
    assert!(keys.contains(&"rating".to_string()), "{keys:?}");
}

#[test]
fn backlinks_point_back_at_the_referring_note() {
    let mut vault = TestVault::new();
    vault.write("Target.md", "# Target\n");
    vault.write("A.md", "See [[Target]].\n");
    vault.write("B.md", "Also [[Target|the target]].\n");
    vault.write("C.md", "Unrelated.\n");
    vault.scan();

    let backlinks =
        queries::backlinks(vault.db.connection(), &TestVault::path("Target.md")).unwrap();
    let sources: Vec<String> = backlinks
        .iter()
        .map(|b| b.source_path.as_str().to_string())
        .collect();
    assert_eq!(sources, vec!["A.md", "B.md"]);
    assert_eq!(backlinks[1].alias.as_deref(), Some("the target"));
    assert_eq!(
        queries::backlink_count(vault.db.connection(), &TestVault::path("Target.md")).unwrap(),
        2
    );
}

#[test]
fn an_unresolved_link_resolves_as_soon_as_its_target_is_created() {
    let mut vault = TestVault::new();
    vault.write("A.md", "Planning [[Future Project]].\n");
    vault.scan();

    let unresolved = queries::unresolved_links(vault.db.connection(), 10).unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].target, "Future Project");

    // Creating the note must light up the existing link without re-scanning
    // the note that contains it.
    let created = vault.write("Future Project.md", "# Future Project\n");
    let indexer = vault.indexer();
    let root = vault.root().to_path_buf();
    indexer.index_file(&mut vault.db, &root, &created).unwrap();

    assert!(queries::unresolved_links(vault.db.connection(), 10)
        .unwrap()
        .is_empty());
    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(
        links[0].target_path.as_ref().unwrap().as_str(),
        "Future Project.md"
    );
}

#[test]
fn deleting_a_target_leaves_the_link_visible_but_unresolved() {
    let mut vault = TestVault::new();
    vault.write("Target.md", "# Target\n");
    vault.write("A.md", "See [[Target]].\n");
    vault.scan();

    let indexer = vault.indexer();
    indexer
        .remove_file(&mut vault.db, &TestVault::path("Target.md"))
        .unwrap();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links.len(), 1, "the link text is still in the note");
    assert!(links[0].target_path.is_none(), "but it points nowhere now");

    let unresolved = queries::unresolved_links(vault.db.connection(), 10).unwrap();
    assert_eq!(unresolved.len(), 1);
}

#[test]
fn deleting_one_of_two_same_named_notes_repoints_the_link_at_the_survivor() {
    let mut vault = TestVault::new();
    vault.write("Plan.md", "# Shallow\n");
    vault.write("Deep/Plan.md", "# Deep\n");
    vault.write("A.md", "See [[Plan]].\n");
    vault.scan();

    // The shallower one wins while both exist.
    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links[0].target_path.as_ref().unwrap().as_str(), "Plan.md");

    let indexer = vault.indexer();
    indexer
        .remove_file(&mut vault.db, &TestVault::path("Plan.md"))
        .unwrap();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(
        links[0].target_path.as_ref().unwrap().as_str(),
        "Deep/Plan.md",
        "the link should fall back to the remaining candidate"
    );
}

#[test]
fn rebuilding_the_index_from_scratch_reproduces_it_exactly() {
    let mut vault = TestVault::new();
    vault.write(
        "A.md",
        "---\nstatus: active\n---\n# A\n\n[[B]] #tag\n\nPoint. ^p\n",
    );
    vault.write("B.md", "# B\n\n[[A]]\n");
    vault.write("img.png", "binary");
    vault.scan();

    let before = snapshot(&vault);

    // Simulate the index file being deleted: clear every derived row and scan
    // again. Nothing was read from the old index, so anything that survives
    // came from the Markdown files.
    vault.db.clear().unwrap();
    assert_eq!(vault.db.file_count().unwrap(), 0);
    vault.scan();

    assert_eq!(before, snapshot(&vault));
}

/// A comparable summary of everything the index derived.
fn snapshot(vault: &TestVault) -> Vec<String> {
    let conn = vault.db.connection();
    let mut out = Vec::new();
    for file in queries::all_files(conn, 1000).unwrap() {
        out.push(format!(
            "file {} {} {}",
            file.path,
            file.kind.as_str(),
            file.title
        ));
        for heading in queries::headings(conn, &file.path).unwrap() {
            out.push(format!("  heading {} {}", heading.level, heading.text));
        }
        for block in queries::blocks(conn, &file.path).unwrap() {
            out.push(format!("  block {}", block.id));
        }
        for link in queries::outgoing_links(conn, &file.path).unwrap() {
            out.push(format!(
                "  link {} -> {}",
                link.link.target,
                link.target_path
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_else(|| "?".into())
            ));
        }
        for backlink in queries::backlinks(conn, &file.path).unwrap() {
            out.push(format!("  backlink from {}", backlink.source_path));
        }
    }
    for tag in queries::tag_summaries(conn).unwrap() {
        out.push(format!(
            "tag {} {} {}",
            tag.name, tag.count, tag.total_count
        ));
    }
    for (key, count) in queries::property_keys(conn).unwrap() {
        out.push(format!("property {key} {count}"));
    }
    out.sort();
    out
}

#[test]
fn an_unchanged_file_is_not_re_indexed_on_a_second_scan() {
    let mut vault = TestVault::new();
    vault.write("A.md", "# A\n");
    vault.write("B.md", "# B\n");

    let first = vault.scan();
    assert_eq!(first.files_indexed, 2);
    assert_eq!(first.files_unchanged, 0);

    let second = vault.scan();
    assert_eq!(second.files_indexed, 0, "nothing changed, nothing to do");
    assert_eq!(second.files_unchanged, 2);
}

#[test]
fn a_rewrite_with_identical_content_does_not_cost_a_reparse() {
    let mut vault = TestVault::new();
    vault.write("A.md", "# A\n");
    vault.scan();

    // Same bytes, new modification time: what a sync tool or a save-without-
    // changes produces. The content hash is what catches it.
    vault.write("A.md", "# A\n");
    let report = vault.scan();
    assert_eq!(report.files_indexed, 0, "{report:?}");
    assert_eq!(report.files_unchanged, 1);
}

#[test]
fn a_real_edit_is_re_indexed() {
    let mut vault = TestVault::new();
    vault.write("A.md", "# A\n");
    vault.scan();

    vault.write("A.md", "# A\n\nNow with [[B]].\n");
    let report = vault.scan();
    assert_eq!(report.files_indexed, 1);

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links.len(), 1);
}

#[test]
fn a_file_deleted_while_the_app_was_closed_is_dropped_on_the_next_scan() {
    let mut vault = TestVault::new();
    vault.write("A.md", "# A\n");
    vault.write("B.md", "# B\n");
    vault.scan();

    vault.delete("B.md");
    let report = vault.scan();

    assert_eq!(report.files_removed, 1);
    assert_eq!(vault.db.file_count().unwrap(), 1);
}

#[test]
fn case_colliding_names_are_reported_as_a_portability_problem() {
    // Only reproducible on a case-sensitive filesystem, which is exactly the
    // situation the diagnostic exists to warn about.
    let mut vault = TestVault::with_case_sensitivity(true);
    vault.write("MyNote.md", "# One\n");
    vault.write("mynote.md", "# Two\n");
    let report = vault.scan();

    let conflicts: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| matches!(d, Diagnostic::CaseConflict { .. }))
        .collect();
    assert_eq!(conflicts.len(), 1, "{:?}", report.diagnostics);
}

#[test]
fn a_vault_authored_on_linux_opens_on_a_case_insensitive_filesystem() {
    // The same note, the same links, a filesystem that folds case. Links that
    // were written with one capitalisation must still resolve.
    let mut vault = TestVault::with_case_sensitivity(false);
    vault.write("MyNote.md", "# My Note\n");
    vault.write("Other.md", "See [[mynote]] and [[MYNOTE]].\n");
    vault.scan();

    let links =
        queries::outgoing_links(vault.db.connection(), &TestVault::path("Other.md")).unwrap();
    assert_eq!(links.len(), 2);
    for link in links {
        assert!(
            link.target_path.is_some(),
            "a differently-cased link should still resolve: {:?}",
            link.link.raw
        );
    }
}

#[test]
fn malformed_frontmatter_is_reported_without_losing_the_note() {
    let mut vault = TestVault::new();
    vault.write(
        "Bad.md",
        "---\nbroken: [unclosed\n---\n\n# Still Indexed\n\n[[Other]]\n",
    );
    vault.write("Other.md", "# Other\n");
    let report = vault.scan();

    assert!(report
        .diagnostics
        .iter()
        .any(|d| matches!(d, Diagnostic::MalformedFrontmatter { .. })));

    // The note is fully usable despite the bad YAML.
    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("Bad.md")).unwrap();
    assert_eq!(links.len(), 1);
    assert!(links[0].target_path.is_some());
    let headings = queries::headings(vault.db.connection(), &TestVault::path("Bad.md")).unwrap();
    assert_eq!(headings.len(), 1);
}

#[test]
fn nested_tags_roll_up_into_their_parents() {
    let mut vault = TestVault::new();
    vault.write("A.md", "#AI\n");
    vault.write("B.md", "#AI/LLM\n");
    vault.write("C.md", "#AI/RAG #AI/LLM\n");
    vault.scan();

    let summaries = queries::tag_summaries(vault.db.connection()).unwrap();
    let ai = summaries.iter().find(|t| t.name == "AI").unwrap();
    assert_eq!(ai.count, 1, "one note uses the bare tag");
    assert_eq!(ai.total_count, 4, "plus three nested uses");

    let files = queries::files_with_tag(vault.db.connection(), "AI", 10).unwrap();
    assert_eq!(files.len(), 3, "the parent tag lists nested notes too");

    let llm = queries::files_with_tag(vault.db.connection(), "AI/LLM", 10).unwrap();
    assert_eq!(llm.len(), 2);
}

#[test]
fn the_graph_contains_a_node_per_note_and_an_edge_per_link() {
    let mut vault = TestVault::new();
    vault.write("A.md", "[[B]] and [[C]]\n");
    vault.write("B.md", "[[C]]\n");
    vault.write("C.md", "# C\n");
    vault.scan();

    let graph = queries::graph(vault.db.connection(), &GraphOptions::default()).unwrap();
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 3);
    assert!(!graph.truncated);

    let c = graph.nodes.iter().find(|n| n.id == "C.md").unwrap();
    assert_eq!(c.degree, 2, "C is linked from both A and B");
}

#[test]
fn the_graph_can_show_link_targets_that_do_not_exist_yet() {
    let mut vault = TestVault::new();
    vault.write("A.md", "[[Ghost]]\n");
    vault.scan();

    let with = queries::graph(vault.db.connection(), &GraphOptions::default()).unwrap();
    assert_eq!(with.nodes.len(), 2);
    assert!(with.nodes.iter().any(|n| n.label == "Ghost"));

    let without = queries::graph(
        vault.db.connection(),
        &GraphOptions {
            include_unresolved: false,
            ..GraphOptions::default()
        },
    )
    .unwrap();
    assert_eq!(without.nodes.len(), 1);
}

#[test]
fn the_local_graph_reaches_exactly_as_far_as_its_depth() {
    let mut vault = TestVault::new();
    vault.write("Center.md", "[[One]]\n");
    vault.write("One.md", "[[Two]]\n");
    vault.write("Two.md", "[[Three]]\n");
    vault.write("Three.md", "# Three\n");
    vault.scan();

    let options = GraphOptions::default();
    let center = TestVault::path("Center.md");

    let depth1 = queries::local_graph(vault.db.connection(), &center, 1, &options).unwrap();
    let labels: Vec<_> = depth1.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(depth1.nodes.len(), 2, "{labels:?}");

    let depth2 = queries::local_graph(vault.db.connection(), &center, 2, &options).unwrap();
    assert_eq!(depth2.nodes.len(), 3);

    let depth9 = queries::local_graph(vault.db.connection(), &center, 9, &options).unwrap();
    assert_eq!(depth9.nodes.len(), 4, "the whole connected component");
}

#[test]
fn embeds_are_recorded_as_links_with_their_own_kind() {
    let mut vault = TestVault::new();
    vault.write("A.md", "![[B]] and [[B]]\n");
    vault.write("B.md", "# B\n");
    vault.scan();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].link.kind, LinkKind::Embed);
    assert_eq!(links[1].link.kind, LinkKind::WikiLink);
    assert!(links.iter().all(|l| l.target_path.is_some()));
}

#[test]
fn a_leftover_temporary_file_is_reported_as_an_interrupted_write() {
    let mut vault = TestVault::new();
    vault.write("A.md", "# A\n");
    vault.write(".ie-tmp-999-0-A.md", "half written");
    let report = vault.scan();

    assert!(report
        .diagnostics
        .iter()
        .any(|d| matches!(d, Diagnostic::InterruptedWrite { .. })));
    assert_eq!(
        vault.db.file_count().unwrap(),
        1,
        "the temporary file must not be indexed as a note"
    );
}

#[test]
fn an_ambiguous_link_still_resolves_but_is_flagged() {
    let mut vault = TestVault::new();
    vault.write("Alpha/Plan.md", "# One\n");
    vault.write("Beta/Plan.md", "# Two\n");
    vault.write("A.md", "See [[Plan]].\n");
    vault.scan();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert!(
        links[0].target_path.is_some(),
        "the link still goes somewhere"
    );

    let ambiguous = queries::ambiguous_links(vault.db.connection(), 10).unwrap();
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].target, "Plan");
}

#[test]
fn a_markdown_link_to_a_file_resolves_like_a_wiki_link() {
    let mut vault = TestVault::new();
    vault.write("Notes/Other.md", "# Other\n");
    vault.write(
        "A.md",
        "[label](Notes/Other.md) and [enc](Notes/Other.md#Section)\n",
    );
    vault.scan();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links.len(), 2);
    assert!(links.iter().all(|l| l.target_path.is_some()), "{links:?}");
    assert_eq!(links[1].link.heading.as_deref(), Some("Section"));
}

#[test]
fn external_links_are_recorded_but_never_resolve_to_a_file() {
    let mut vault = TestVault::new();
    vault.write("A.md", "[site](https://example.org)\n");
    vault.scan();

    let links = queries::outgoing_links(vault.db.connection(), &TestVault::path("A.md")).unwrap();
    assert_eq!(links[0].link.kind, LinkKind::External);
    assert!(links[0].target_path.is_none());

    // And they must not pollute the unresolved-links panel.
    assert!(queries::unresolved_links(vault.db.connection(), 10)
        .unwrap()
        .is_empty());
}

//! Search behaviour, exercised against a real index.

mod common;

use common::TestVault;
use ie_core::search::{self, engine, SearchOptions};

fn fixture() -> TestVault {
    let mut vault = TestVault::new();
    vault.write(
        "Research/Neural Networks.md",
        "---\nstatus: active\nrating: 9\n---\n\
         # Neural Networks\n\n\
         Notes on machine learning and deep neural networks. #AI/LLM\n\n\
         ## Design notes\n\nGradient descent is the workhorse.\n",
    );
    vault.write(
        "Research/Transformers.md",
        "---\nstatus: draft\nrating: 6\n---\n\
         # Transformers\n\nAttention is all you need. #AI\n\n[[Neural Networks]]\n",
    );
    vault.write(
        "Projects/Roadmap.md",
        "---\nstatus: active\nrating: 3\n---\n\
         # Roadmap\n\nShip the machine by Friday. #planning\n",
    );
    vault.write("Attachments/diagram.png", "png bytes");
    vault.write("Orphan.md", "# Orphan\n\nNothing links here and it links to [[Nowhere]].\n");
    vault.scan();
    vault
}

fn run(vault: &TestVault, query: &str) -> Vec<String> {
    let parsed = search::parse(query).unwrap();
    engine::search(vault.db.connection(), &parsed, &SearchOptions::default())
        .unwrap()
        .hits
        .into_iter()
        .map(|h| h.path.as_str().to_string())
        .collect()
}

#[test]
fn a_bare_word_searches_the_full_text() {
    let vault = fixture();
    let hits = run(&vault, "gradient");
    assert_eq!(hits, vec!["Research/Neural Networks.md"]);
}

#[test]
fn several_words_must_all_appear() {
    let vault = fixture();
    assert_eq!(run(&vault, "machine learning").len(), 1);
    // "machine" alone appears in two notes.
    assert_eq!(run(&vault, "machine").len(), 2);
}

#[test]
fn a_quoted_phrase_matches_only_that_sequence() {
    let vault = fixture();
    assert_eq!(run(&vault, "\"machine learning\"").len(), 1);
    assert_eq!(
        run(&vault, "\"learning machine\"").len(),
        0,
        "the words are present but not in this order"
    );
}

#[test]
fn a_negated_word_excludes_its_notes() {
    let vault = fixture();
    let all = run(&vault, "machine");
    assert_eq!(all.len(), 2);
    let filtered = run(&vault, "machine -friday");
    assert_eq!(filtered, vec!["Research/Neural Networks.md"]);
}

#[test]
fn tag_filters_include_nested_tags() {
    let vault = fixture();
    let ai = run(&vault, "tag:AI");
    assert_eq!(ai.len(), 2, "#AI and #AI/LLM both count: {ai:?}");

    let llm = run(&vault, "tag:AI/LLM");
    assert_eq!(llm, vec!["Research/Neural Networks.md"]);
}

#[test]
fn path_filters_narrow_to_a_folder() {
    let vault = fixture();
    let hits = run(&vault, "path:Research");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|p| p.starts_with("Research/")));
}

#[test]
fn file_and_extension_filters_work_on_attachments_too() {
    let vault = fixture();
    assert_eq!(run(&vault, "ext:png"), vec!["Attachments/diagram.png"]);
    assert_eq!(run(&vault, "file:diagram"), vec!["Attachments/diagram.png"]);
}

#[test]
fn property_equality_filters_notes_by_frontmatter() {
    let vault = fixture();
    let active = run(&vault, "status:active");
    assert_eq!(active.len(), 2);
    assert_eq!(run(&vault, "status:draft").len(), 1);
}

#[test]
fn numeric_property_comparisons_compare_as_numbers() {
    let vault = fixture();
    // String ordering would put "9" below "10"; this must not do that.
    assert_eq!(run(&vault, "rating>5").len(), 2);
    assert_eq!(run(&vault, "rating>=9").len(), 1);
    assert_eq!(run(&vault, "rating<5").len(), 1);
}

#[test]
fn a_section_filter_finds_notes_by_heading() {
    let vault = fixture();
    assert_eq!(
        run(&vault, "section:\"Design notes\""),
        vec!["Research/Neural Networks.md"]
    );
}

#[test]
fn structural_filters_find_notes_by_their_link_shape() {
    let vault = fixture();

    let unresolved = run(&vault, "is:unresolved");
    assert_eq!(unresolved, vec!["Orphan.md"]);

    let orphans = run(&vault, "is:orphan");
    assert!(orphans.contains(&"Orphan.md".to_string()), "{orphans:?}");
    assert!(
        !orphans.contains(&"Research/Neural Networks.md".to_string()),
        "that note is linked from Transformers"
    );

    let untagged = run(&vault, "is:untagged");
    assert!(untagged.contains(&"Orphan.md".to_string()));
}

#[test]
fn filters_and_text_combine() {
    let vault = fixture();
    let hits = run(&vault, "tag:AI attention");
    assert_eq!(hits, vec!["Research/Transformers.md"]);

    assert!(
        run(&vault, "tag:planning attention").is_empty(),
        "both must hold"
    );
}

#[test]
fn a_negated_filter_excludes_matching_notes() {
    let vault = fixture();
    let hits = run(&vault, "path:Research -status:draft");
    assert_eq!(hits, vec!["Research/Neural Networks.md"]);
}

#[test]
fn results_carry_a_highlighted_snippet_of_the_match() {
    let vault = fixture();
    let parsed = search::parse("gradient").unwrap();
    let results =
        engine::search(vault.db.connection(), &parsed, &SearchOptions::default()).unwrap();
    let snippet = &results.hits[0].snippet;
    assert!(snippet.contains("<mark>"), "{snippet}");
    assert!(snippet.to_lowercase().contains("gradient"), "{snippet}");
}

#[test]
fn matches_are_counted_beyond_the_page_that_is_returned() {
    let mut vault = TestVault::new();
    for i in 0..25 {
        vault.write(&format!("Note {i}.md"), "the word appears here\n");
    }
    vault.scan();

    let parsed = search::parse("appears").unwrap();
    let results = engine::search(
        vault.db.connection(),
        &parsed,
        &SearchOptions {
            limit: 10,
            ..SearchOptions::default()
        },
    )
    .unwrap();

    assert_eq!(results.hits.len(), 10);
    assert_eq!(results.total, 25);
    assert!(results.truncated);
}

#[test]
fn paging_walks_through_the_whole_result_set_without_repeats() {
    let mut vault = TestVault::new();
    for i in 0..12 {
        vault.write(&format!("Note {i:02}.md"), "findme\n");
    }
    vault.scan();

    let parsed = search::parse("findme").unwrap();
    let mut seen: Vec<String> = Vec::new();
    for page in 0..3 {
        let results = engine::search(
            vault.db.connection(),
            &parsed,
            &SearchOptions {
                limit: 5,
                offset: page * 5,
                ..SearchOptions::default()
            },
        )
        .unwrap();
        seen.extend(results.hits.into_iter().map(|h| h.path.as_str().to_string()));
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 12);
}

#[test]
fn frontmatter_text_is_not_searched_as_body_text() {
    let mut vault = TestVault::new();
    vault.write("A.md", "---\nsummary: pineapple\n---\n\nThe body says nothing.\n");
    vault.scan();

    assert!(
        run(&vault, "pineapple").is_empty(),
        "frontmatter is reachable through property filters, not full text"
    );
    assert_eq!(run(&vault, "summary:pineapple").len(), 1);
}

#[test]
fn searching_is_diacritic_insensitive() {
    let mut vault = TestVault::new();
    vault.write("A.md", "A note about the café on the corner.\n");
    vault.scan();

    assert_eq!(run(&vault, "cafe").len(), 1);
    assert_eq!(run(&vault, "café").len(), 1);
}

#[test]
fn an_empty_query_returns_nothing_rather_than_everything() {
    let vault = fixture();
    let parsed = search::parse("   ").unwrap();
    let results =
        engine::search(vault.db.connection(), &parsed, &SearchOptions::default()).unwrap();
    assert!(results.hits.is_empty());
    assert_eq!(results.total, 0);
}

#[test]
fn fts_operators_typed_by_a_user_are_searched_literally() {
    let mut vault = TestVault::new();
    vault.write("A.md", "this AND that\n");
    vault.write("B.md", "only this\n");
    vault.scan();

    // Without quoting, FTS5 would read AND as an operator and match both.
    let hits = run(&vault, "\"this AND that\"");
    assert_eq!(hits, vec!["A.md"]);
}

#[test]
fn the_quick_switcher_matches_filenames_fuzzily() {
    let vault = fixture();
    let matches = engine::quick_switch(vault.db.connection(), "neunet", 10).unwrap();
    assert_eq!(
        matches.first().map(|m| m.path.as_str()),
        Some("Research/Neural Networks.md"),
        "{matches:?}"
    );
}

#[test]
fn an_empty_quick_switcher_query_offers_recent_files() {
    let vault = fixture();
    let matches = engine::quick_switch(vault.db.connection(), "", 3).unwrap();
    assert_eq!(matches.len(), 3);
}

#[test]
fn tag_autocomplete_ranks_by_how_often_a_tag_is_used() {
    let mut vault = TestVault::new();
    vault.write("A.md", "#AI #AI/LLM\n");
    vault.write("B.md", "#AI\n");
    vault.write("C.md", "#AI\n");
    vault.scan();

    let completions = engine::complete_tags(vault.db.connection(), "ai", 10).unwrap();
    assert_eq!(completions[0].0, "AI");
    assert_eq!(completions[0].1, 3);
    assert!(completions.iter().any(|(name, _)| name == "AI/LLM"));
}

#[test]
fn heading_autocomplete_is_scoped_to_one_note() {
    let vault = fixture();
    let headings = engine::complete_headings(
        vault.db.connection(),
        &TestVault::path("Research/Neural Networks.md"),
        "design",
        10,
    )
    .unwrap();
    assert_eq!(headings, vec!["Design notes"]);

    let elsewhere = engine::complete_headings(
        vault.db.connection(),
        &TestVault::path("Projects/Roadmap.md"),
        "design",
        10,
    )
    .unwrap();
    assert!(elsewhere.is_empty());
}

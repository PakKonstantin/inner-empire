//! The Rust half of the Markdown conformance corpus.
//!
//! The TypeScript half is `src/markdown/conformance.test.ts`, reading the same
//! fixtures and asserting the same expectations. Rust is authoritative — it is
//! what the index, the backlinks and a rename act on — but the frontend must
//! agree with it, and a grammar implemented twice drifts unless something
//! holds it together. This is that something.

use std::path::PathBuf;

use ie_core::markdown::MarkdownParser;
use ie_core::model::LinkKind;

/// The corpus lives at the repository root, beside the frontend that also
/// reads it.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("markdown")
}

#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ExpectedLink {
    kind: String,
    target: String,
    heading: Option<String>,
    block_id: Option<String>,
    alias: Option<String>,
}

#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ExpectedHeading {
    level: u8,
    text: String,
    slug: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    links: Vec<ExpectedLink>,
    tags: Vec<String>,
    headings: Vec<ExpectedHeading>,
    blocks: Vec<String>,
}

fn kind_name(kind: LinkKind) -> &'static str {
    kind.as_str()
}

#[test]
fn every_fixture_matches_the_shared_expectation() {
    let dir = corpus_dir();
    let parser = MarkdownParser::new();

    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("could not read the corpus at {}: {e}", dir.display()))
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "md")
                && path.file_name().is_some_and(|name| name != "README.md")
        })
        .collect();
    fixtures.sort();

    // A corpus that quietly became empty would make this test pass while
    // checking nothing.
    assert!(
        fixtures.len() > 5,
        "expected a populated corpus at {}, found {} fixtures",
        dir.display(),
        fixtures.len()
    );

    for fixture in fixtures {
        let name = fixture.file_stem().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", fixture.display()));
        let expected_json = std::fs::read_to_string(fixture.with_extension("expected.json"))
            .unwrap_or_else(|e| panic!("{name} has no matching .expected.json: {e}"));
        let expected: Expected = serde_json::from_str(&expected_json)
            .unwrap_or_else(|e| panic!("{name}.expected.json is not valid: {e}"));

        let metadata = parser.parse(&source).metadata;

        let actual_links: Vec<ExpectedLink> = metadata
            .links
            .iter()
            .map(|link| ExpectedLink {
                kind: kind_name(link.kind).to_string(),
                target: link.target.clone(),
                heading: link.heading.clone(),
                block_id: link.block_id.clone(),
                alias: link.alias.clone(),
            })
            .collect();
        assert_eq!(actual_links, expected.links, "links differ in {name}");

        let actual_tags: Vec<String> = metadata.tags.iter().map(|tag| tag.name.clone()).collect();
        assert_eq!(actual_tags, expected.tags, "tags differ in {name}");

        let actual_headings: Vec<ExpectedHeading> = metadata
            .headings
            .iter()
            .map(|heading| ExpectedHeading {
                level: heading.level,
                text: heading.text.clone(),
                slug: heading.slug.clone(),
            })
            .collect();
        assert_eq!(
            actual_headings, expected.headings,
            "headings differ in {name}"
        );

        let actual_blocks: Vec<String> = metadata
            .blocks
            .iter()
            .map(|block| block.id.clone())
            .collect();
        assert_eq!(actual_blocks, expected.blocks, "blocks differ in {name}");
    }
}

#[test]
fn every_fixture_has_an_expectation_and_the_reverse() {
    let dir = corpus_dir();
    let entries: Vec<String> = std::fs::read_dir(&dir)
        .expect("the corpus directory should exist")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    for entry in entries
        .iter()
        .filter(|name| name.ends_with(".md") && *name != "README.md")
    {
        let expectation = entry.replace(".md", ".expected.json");
        assert!(
            entries.contains(&expectation),
            "{entry} has no {expectation}; a fixture without an expectation tests nothing"
        );
    }

    for entry in entries
        .iter()
        .filter(|name| name.ends_with(".expected.json"))
    {
        let fixture = entry.replace(".expected.json", ".md");
        assert!(
            entries.contains(&fixture),
            "{entry} has no {fixture}; a stale expectation is never checked"
        );
    }
}

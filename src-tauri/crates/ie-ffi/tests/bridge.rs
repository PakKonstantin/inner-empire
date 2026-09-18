//! The bridge, exercised end to end against a real temporary directory.
//!
//! These run on any host. What they check is not that `ie-core` works — it has
//! 385 tests of its own — but that nothing is lost or mistranslated on the way
//! across: that a path cannot escape the vault, that an error keeps the shape
//! Swift needs to branch on, and above all that a save cannot silently
//! overwrite a change made elsewhere.

use std::sync::{Arc, Mutex};

use ie_ffi::error::FfiError;
use ie_ffi::handle::{OpenReportSink, ScanProgress, VaultHandle};
use ie_ffi::host::{HostConfig, StorageKind};
use ie_ffi::types::*;

struct Fixture {
    _container: tempfile::TempDir,
    vault: tempfile::TempDir,
    config: HostConfig,
}

impl Fixture {
    fn new() -> Self {
        let container = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let config = HostConfig {
            library_dir: container
                .path()
                .join("Library")
                .to_string_lossy()
                .to_string(),
            utc_offset_seconds: 0,
            storage: StorageKind::LocalFolder,
            watch_debounce_ms: 20,
        };
        Self {
            _container: container,
            vault,
            config,
        }
    }

    fn open(&self) -> Arc<VaultHandle> {
        VaultHandle::open(
            self.config.clone(),
            self.vault.path().to_string_lossy().to_string(),
            None,
        )
        .unwrap()
    }

    fn create(&self) -> Arc<VaultHandle> {
        VaultHandle::create(
            self.config.clone(),
            self.vault.path().to_string_lossy().to_string(),
            "Test Vault".into(),
            None,
        )
        .unwrap()
    }

    /// Write straight to disk, behind the app's back — which is what a sync
    /// client, the Files app, or a desktop sharing the folder would do.
    fn write_externally(&self, relative: &str, contents: &str) {
        let path = self.vault.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

#[derive(Default)]
struct ProgressLog(Mutex<Vec<IndexProgress>>);

impl ScanProgress for ProgressLog {
    fn report(&self, progress: IndexProgress) {
        self.0.lock().unwrap().push(progress);
    }
}

#[test]
fn creating_a_vault_gives_it_the_conventional_shape() {
    let fixture = Fixture::new();
    let handle = fixture.create();

    let report = handle.scan(None).unwrap();
    assert!(report.files_seen >= 1, "the welcome note should be there");

    let listing = handle.list_directory(String::new()).unwrap();
    let folders: Vec<_> = listing.folders.iter().map(|f| f.name.as_str()).collect();
    for expected in ["Notes", "Projects", "Attachments", "Templates", "Daily"] {
        assert!(
            folders.contains(&expected),
            "missing {expected} in {folders:?}"
        );
    }
}

#[test]
fn the_open_report_says_what_happened_to_the_index() {
    let fixture = Fixture::new();

    let sink = OpenReportSink::new();
    let _handle = VaultHandle::open_reporting(
        fixture.config.clone(),
        fixture.vault.path().to_string_lossy().to_string(),
        None,
        Arc::clone(&sink),
    )
    .unwrap();

    let report = sink.take().expect("a report");
    assert!(report.created, "a fresh folder is a first open");
    assert_eq!(report.index, OpenOutcome::Created);
    assert!(report.index.needs_full_scan());
    assert!(!report.vault_id.is_empty(), "the vault must have an id");
    assert!(sink.take().is_none(), "the report is taken once");
}

#[test]
fn a_note_survives_the_round_trip_with_its_metadata() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    let source =
        "---\ntitle: Round Trip\ntags:\n  - alpha\n  - nested/beta\ncount: 3\ndone: true\n---\n\n\
                  # Round Trip\n\nA link to [[Other Note|an alias]] and #inline-tag.\n\n\
                  Some text. ^block-one\n";
    handle
        .create_note("Notes/Round Trip.md".into(), source.into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();

    let note = handle.read_note("Notes/Round Trip.md".into()).unwrap();
    assert_eq!(note.content, source, "the bytes must not change");
    assert_eq!(note.title, "Round Trip");

    let keys: Vec<_> = note
        .metadata
        .properties
        .iter()
        .map(|p| p.key.as_str())
        .collect();
    assert_eq!(keys, vec!["title", "tags", "count", "done"]);

    // Types survive: a number is a number, not the string "3".
    let count = note
        .metadata
        .properties
        .iter()
        .find(|p| p.key == "count")
        .unwrap();
    assert_eq!(count.value, PropertyValue::Number { value: 3.0 });
    let done = note
        .metadata
        .properties
        .iter()
        .find(|p| p.key == "done")
        .unwrap();
    assert_eq!(done.value, PropertyValue::Checkbox { value: true });

    // A YAML list keeps its order.
    let tags = note
        .metadata
        .properties
        .iter()
        .find(|p| p.key == "tags")
        .unwrap();
    assert_eq!(
        tags.value,
        PropertyValue::List {
            values: vec![
                PropertyValue::Text {
                    value: "alpha".into()
                },
                PropertyValue::Text {
                    value: "nested/beta".into()
                },
            ]
        }
    );

    assert_eq!(note.metadata.links.len(), 1);
    assert_eq!(note.metadata.links[0].kind, LinkKind::WikiLink);
    assert_eq!(note.metadata.links[0].target, "Other Note");
    assert_eq!(note.metadata.links[0].alias.as_deref(), Some("an alias"));

    assert_eq!(note.metadata.headings.len(), 1);
    assert_eq!(note.metadata.blocks.len(), 1);
    assert_eq!(note.metadata.blocks[0].id, "block-one");

    let tag_names: Vec<_> = note.metadata.tags.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(tag_names, vec!["inline-tag"]);
}

#[test]
fn a_save_refuses_when_the_file_changed_underneath_it() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Note.md".into(), "original\n".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();
    let opened = handle.read_note("Note.md".into()).unwrap();

    // A sync client, the Files app, or a Mac sharing the folder.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fixture.write_externally("Note.md", "changed elsewhere\n");
    handle.scan(None).unwrap();

    let error = handle
        .save_note("Note.md".into(), "my edit\n".into(), opened.modified_ms)
        .unwrap_err();

    match &error {
        FfiError::ExternalModification {
            path,
            opened_modified_ms,
            current_modified_ms,
        } => {
            assert_eq!(path, "Note.md");
            assert_eq!(*opened_modified_ms, opened.modified_ms);
            assert_ne!(*current_modified_ms, *opened_modified_ms);
        }
        other => panic!("expected ExternalModification, got {other:?}"),
    }
    assert_eq!(error.code(), "external_modification");

    // And the other edit is still there, which is the whole point.
    assert_eq!(
        std::fs::read_to_string(fixture.vault.path().join("Note.md")).unwrap(),
        "changed elsewhere\n"
    );
}

#[test]
fn an_unchanged_file_saves_and_reports_its_new_timestamp() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Note.md".into(), "original\n".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();
    let opened = handle.read_note("Note.md".into()).unwrap();

    let saved_ms = handle
        .save_note("Note.md".into(), "edited\n".into(), opened.modified_ms)
        .unwrap();

    assert_eq!(
        handle.read_note("Note.md".into()).unwrap().content,
        "edited\n"
    );
    // The returned stamp is what the host must carry into the next save.
    let reopened = handle.read_note("Note.md".into()).unwrap();
    assert!(
        saved_ms == reopened.modified_ms || saved_ms == opened.modified_ms,
        "the save must report a usable base for the next one"
    );
}

#[test]
fn forcing_a_save_is_the_only_way_past_the_check() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Note.md".into(), "original\n".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(1100));
    fixture.write_externally("Note.md", "changed elsewhere\n");
    handle.scan(None).unwrap();

    // The user has seen the other version and chosen to keep theirs.
    handle
        .force_save_note("Note.md".into(), "mine wins\n".into())
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.vault.path().join("Note.md")).unwrap(),
        "mine wins\n"
    );
}

#[test]
fn creating_a_note_hands_back_a_base_to_save_against() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    // Without this the host would have an open buffer and no base, and the
    // only safe thing to do with no base is refuse to save.
    let created = handle
        .create_note("Fresh.md".into(), "first\n".into(), Collision::Fail)
        .unwrap();
    assert_eq!(created.path.as_str(), "Fresh.md");
    assert!(created.modified_ms > 0);

    handle
        .save_note("Fresh.md".into(), "second\n".into(), created.modified_ms)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.vault.path().join("Fresh.md")).unwrap(),
        "second\n"
    );
}

#[test]
fn a_stale_base_is_refused_even_before_the_first_scan() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    // The file exists but the index has never seen it. Checking the index
    // rather than the filesystem here would wave this straight through and
    // destroy whatever is on disk.
    fixture.write_externally("Unseen.md", "written by something else\n");

    let error = handle
        .save_note("Unseen.md".into(), "mine\n".into(), 0)
        .unwrap_err();
    assert!(
        matches!(error, FfiError::ExternalModification { .. }),
        "got {error:?}"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.vault.path().join("Unseen.md")).unwrap(),
        "written by something else\n"
    );
}

#[test]
fn a_renaming_collision_reports_where_the_note_actually_landed() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Taken.md".into(), "first\n".into(), Collision::Fail)
        .unwrap();
    let second = handle
        .create_note("Taken.md".into(), "second\n".into(), Collision::Rename)
        .unwrap();

    assert_ne!(second.path.as_str(), "Taken.md");
    assert_eq!(
        handle
            .read_note(second.path.as_str().into())
            .unwrap()
            .content,
        "second\n"
    );
    // And the original is untouched.
    assert_eq!(
        handle.read_note("Taken.md".into()).unwrap().content,
        "first\n"
    );
}

#[test]
fn a_path_containing_dot_dot_is_refused_outright() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    // Not resolved and then checked — refused. A link containing `..` is
    // either a mistake or an escape attempt, and both deserve to be visible.
    for escape in [
        "../outside.md",
        "Notes/../../outside.md",
        "..",
        "a/../../b.md",
    ] {
        let error = handle.read_note(escape.into()).unwrap_err();
        assert!(
            matches!(error, FfiError::InvalidName { .. }),
            "{escape} should have been refused, got {error:?}"
        );
    }
}

#[test]
fn an_absolute_looking_path_is_contained_rather_than_followed() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    // A leading slash makes it vault-relative, not host-absolute: whatever
    // string the host builds, it addresses something inside the vault.
    handle
        .create_note(
            "etc/passwd".into(),
            "inside the vault\n".into(),
            Collision::Fail,
        )
        .unwrap();

    let note = handle.read_note("/etc/passwd".into()).unwrap();
    assert_eq!(note.path.as_str(), "etc/passwd");
    assert_eq!(note.content, "inside the vault\n");
    assert!(fixture.vault.path().join("etc/passwd").exists());
}

#[test]
fn errors_keep_the_shape_the_host_branches_on() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    let missing = handle.read_note("Nowhere.md".into()).unwrap_err();
    assert!(matches!(missing, FfiError::NotFound { .. }));
    assert_eq!(missing.code(), "not_found");

    handle
        .create_note("Taken.md".into(), "x".into(), Collision::Fail)
        .unwrap();
    let taken = handle
        .create_note("Taken.md".into(), "y".into(), Collision::Fail)
        .unwrap_err();
    assert!(matches!(taken, FfiError::AlreadyExists { .. }));
    assert_eq!(taken.code(), "already_exists");

    let bad_query = handle
        .search("tag:\"unterminated".into(), SearchOptions::default())
        .unwrap_err();
    assert!(matches!(bad_query, FfiError::InvalidQuery { .. }));
}

#[test]
fn a_collision_that_only_differs_by_case_is_refused() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Note.md".into(), "one".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();

    // Legal on ext4, destructive on the volume iOS ships with, so refused on
    // both — a vault must mean the same thing wherever it is opened.
    let error = handle
        .create_note("note.md".into(), "two".into(), Collision::Fail)
        .unwrap_err();
    assert!(
        matches!(
            error,
            FfiError::CaseCollision { .. } | FfiError::AlreadyExists { .. }
        ),
        "got {error:?}"
    );
}

#[test]
fn renaming_rewrites_the_links_that_pointed_at_it() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Target.md".into(), "# Target\n".into(), Collision::Fail)
        .unwrap();
    handle
        .create_note(
            "Referrer.md".into(),
            "See [[Target]] and [[Target|again]].\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    let plan = handle
        .plan_rename("Target.md".into(), "Renamed.md".into())
        .unwrap();
    assert_eq!(plan.total_links, 2);
    assert_eq!(plan.edits.len(), 1);
    assert_eq!(plan.edits[0].path.as_str(), "Referrer.md");

    let outcome = handle
        .rename("Target.md".into(), "Renamed.md".into())
        .unwrap();
    assert_eq!(outcome.files_updated, 1);
    assert_eq!(outcome.links_updated, 2);
    assert!(outcome.failures.is_empty());

    let referrer = handle.read_note("Referrer.md".into()).unwrap();
    assert!(referrer.content.contains("[[Renamed]]"));
    assert!(referrer.content.contains("[[Renamed|again]]"));
}

#[test]
fn search_finds_by_text_tag_and_property() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note(
            "Alpha.md".into(),
            "---\nstatus: active\n---\n\nThe quick brown fox. #animals\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle
        .create_note(
            "Beta.md".into(),
            "---\nstatus: done\n---\n\nA lazy dog sleeps.\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    let by_text = handle
        .search("brown".into(), SearchOptions::default())
        .unwrap();
    assert_eq!(by_text.hits.len(), 1);
    assert_eq!(by_text.hits[0].path.as_str(), "Alpha.md");

    let by_tag = handle
        .search("tag:animals".into(), SearchOptions::default())
        .unwrap();
    assert_eq!(by_tag.hits.len(), 1);
    assert_eq!(by_tag.hits[0].path.as_str(), "Alpha.md");

    let by_property = handle
        .search("status:done".into(), SearchOptions::default())
        .unwrap();
    assert_eq!(by_property.hits.len(), 1);
    assert_eq!(by_property.hits[0].path.as_str(), "Beta.md");
}

#[test]
fn the_snippet_delimiters_are_the_hosts_to_choose() {
    let fixture = Fixture::new();
    let handle = fixture.open();
    handle
        .create_note(
            "Note.md".into(),
            "the quick brown fox\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    let results = handle
        .search(
            "brown".into(),
            SearchOptions {
                highlight_open: "«".into(),
                highlight_close: "»".into(),
                ..SearchOptions::default()
            },
        )
        .unwrap();
    assert!(
        results.hits[0].snippet.contains("«brown»"),
        "got {:?}",
        results.hits[0].snippet
    );

    // A native client wants something it can turn into an AttributedString,
    // not HTML that might collide with a note's own text.
    assert_eq!(SearchOptions::default().highlight_open, "\u{2}");
}

#[test]
fn backlinks_and_outgoing_links_agree_with_each_other() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Hub.md".into(), "# Hub\n".into(), Collision::Fail)
        .unwrap();
    handle
        .create_note(
            "Spoke.md".into(),
            "Points at [[Hub]] and at [[Nowhere]].\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    let backlinks = handle.backlinks("Hub.md".into()).unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_path.as_str(), "Spoke.md");

    let outgoing = handle.outgoing_links("Spoke.md".into()).unwrap();
    assert_eq!(outgoing.len(), 2);
    let resolved: Vec<_> = outgoing
        .iter()
        .map(|l| l.target_path.as_ref().map(|p| p.as_str().to_string()))
        .collect();
    assert!(resolved.contains(&Some("Hub.md".to_string())));
    // An unresolved link is reported as such, not hidden: it is what the UI
    // offers to turn into a new note.
    assert!(resolved.contains(&None));
}

#[test]
fn tags_roll_up_their_nested_children() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("A.md".into(), "#project\n".into(), Collision::Fail)
        .unwrap();
    handle
        .create_note("B.md".into(), "#project/alpha\n".into(), Collision::Fail)
        .unwrap();
    handle
        .create_note("C.md".into(), "#project/beta\n".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();

    let tags = handle.tags().unwrap();
    let project = tags.iter().find(|t| t.name == "project").unwrap();
    assert_eq!(project.count, 1, "only A writes the bare tag");
    assert_eq!(project.total_count, 3, "the subtree is three notes");
}

#[test]
fn deleting_moves_to_the_trash_and_restore_puts_it_back() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note(
            "Notes/Doomed.md".into(),
            "still here\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    let entry = handle.delete("Notes/Doomed.md".into()).unwrap();
    assert_eq!(entry.original_path.as_str(), "Notes/Doomed.md");
    assert!(!fixture.vault.path().join("Notes/Doomed.md").exists());

    let restored = handle.restore(entry.id).unwrap();
    assert_eq!(restored.as_str(), "Notes/Doomed.md");
    assert_eq!(
        handle.read_note("Notes/Doomed.md".into()).unwrap().content,
        "still here\n"
    );
}

#[test]
fn properties_can_be_set_without_disturbing_the_body() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note(
            "Note.md".into(),
            "---\nstatus: draft\n---\n\n# Body\n\nUntouched.\n".into(),
            Collision::Fail,
        )
        .unwrap();
    handle.scan(None).unwrap();

    handle
        .set_properties(
            "Note.md".into(),
            vec![
                Property {
                    key: "status".into(),
                    value: PropertyValue::Text {
                        value: "published".into(),
                    },
                },
                Property {
                    key: "rating".into(),
                    value: PropertyValue::Number { value: 5.0 },
                },
            ],
        )
        .unwrap();

    let note = handle.read_note("Note.md".into()).unwrap();
    assert!(note.content.contains("# Body\n\nUntouched.\n"));
    let by_key: std::collections::HashMap<_, _> = note
        .metadata
        .properties
        .iter()
        .map(|p| (p.key.as_str(), p.value.clone()))
        .collect();
    assert_eq!(
        by_key["status"],
        PropertyValue::Text {
            value: "published".into()
        }
    );
    assert_eq!(by_key["rating"], PropertyValue::Number { value: 5.0 });
}

#[test]
fn the_recovery_journal_survives_the_vault_being_closed() {
    let fixture = Fixture::new();

    {
        let handle = fixture.open();
        handle
            .create_note("Draft.md".into(), "saved\n".into(), Collision::Fail)
            .unwrap();
        handle.scan(None).unwrap();
        handle
            .journal_unsaved("Draft.md".into(), "typed but never saved\n".into())
            .unwrap();
        // The app is killed here — no clear_journal, no save.
    }

    let handle = fixture.open();
    let candidates = handle.recoverable().unwrap();
    let draft = candidates
        .iter()
        .find(|c| c.path.as_str() == "Draft.md")
        .expect("the unsaved buffer should be offered back");
    assert_eq!(draft.content, "typed but never saved\n");
    assert!(!draft.already_saved);
}

#[test]
fn the_journal_lives_outside_the_vault() {
    let fixture = Fixture::new();
    let handle = fixture.open();
    handle
        .create_note("Draft.md".into(), "saved\n".into(), Collision::Fail)
        .unwrap();
    handle
        .journal_unsaved("Draft.md".into(), "private scratch\n".into())
        .unwrap();

    // A half-typed paragraph is machine-local. If it were in the vault it
    // would sync to every other device the moment it was typed.
    let mut found = Vec::new();
    let mut stack = vec![fixture.vault.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if std::fs::read_to_string(&path)
                .map(|t| t.contains("private scratch"))
                .unwrap_or(false)
            {
                found.push(path);
            }
        }
    }
    assert!(
        found.is_empty(),
        "the journal leaked into the vault: {found:?}"
    );
}

#[test]
fn the_index_cache_is_not_inside_the_vault() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    handle
        .create_note("Note.md".into(), "content\n".into(), Collision::Fail)
        .unwrap();
    handle.scan(None).unwrap();

    let index = handle.index_cache_path();
    assert!(
        !index.starts_with(&fixture.vault.path().to_string_lossy().to_string()),
        "{index} is inside the vault"
    );
    assert!(index.contains("Caches"), "{index} should be purgeable");
    assert!(index.contains(&handle.vault_id()), "keyed by the vault id");
    assert!(
        std::path::Path::new(&index).exists(),
        "{index} was not created"
    );

    // And it is genuinely not in the vault, not merely reported that way.
    // SQLite wants advisory locks and `-wal` siblings that a synced folder does
    // not guarantee, and the FTS5 table holds note content that must not end up
    // uploaded by a sync client.
    let mut stray = Vec::new();
    let mut stack = vec![fixture.vault.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(".db"))
            {
                stray.push(path);
            }
        }
    }
    assert!(
        stray.is_empty(),
        "a database was left in the vault: {stray:?}"
    );
}

#[test]
fn a_scan_reports_progress_and_then_costs_nothing_to_repeat() {
    let fixture = Fixture::new();
    let handle = fixture.open();

    // Written behind the app's back, so the scan is what discovers them —
    // `create_note` indexes as it goes and would leave nothing for it to do.
    for n in 0..20 {
        fixture.write_externally(
            &format!("Note {n}.md"),
            &format!("# Note {n}\n\nLinks to [[Note {}]].\n", (n + 1) % 20),
        );
    }

    let log = Arc::new(ProgressLog::default());
    let first = handle
        .scan(Some(Arc::clone(&log) as Arc<dyn ScanProgress>))
        .unwrap();
    assert_eq!(first.files_indexed, 20);
    assert!(
        !log.0.lock().unwrap().is_empty(),
        "progress was never reported"
    );

    // The requirement that a launch must not rescan the vault: a second pass
    // sees the same files and re-parses none of them.
    let second = handle.scan(None).unwrap();
    assert_eq!(second.files_seen, 20);
    assert_eq!(second.files_indexed, 0);
    assert_eq!(second.files_unchanged, 20);
}

#[test]
fn deleting_the_index_loses_nothing() {
    let fixture = Fixture::new();

    let expected = {
        let handle = fixture.open();
        handle
            .create_note(
                "Note.md".into(),
                "---\ntag: x\n---\n\n[[Other]] #here\n".into(),
                Collision::Fail,
            )
            .unwrap();
        handle
            .create_note("Other.md".into(), "# Other\n".into(), Collision::Fail)
            .unwrap();
        handle.scan(None).unwrap();
        (
            handle.backlinks("Other.md".into()).unwrap(),
            handle.tags().unwrap(),
        )
    };

    // What a cache purge under storage pressure does.
    let handle = fixture.open();
    std::fs::remove_file(handle.index_cache_path()).unwrap();
    drop(handle);

    let handle = fixture.open();
    handle.scan(None).unwrap();
    assert_eq!(handle.backlinks("Other.md".into()).unwrap(), expected.0);
    assert_eq!(handle.tags().unwrap(), expected.1);
}

#[test]
fn a_file_provider_vault_without_a_host_is_refused() {
    let fixture = Fixture::new();
    let config = HostConfig {
        storage: StorageKind::FileProvider,
        ..fixture.config.clone()
    };

    // Falling back to POSIX here would make an evicted note read as empty and
    // be indexed as empty — data loss noticed much later, if at all.
    let error = VaultHandle::open(
        config,
        fixture.vault.path().to_string_lossy().to_string(),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error, FfiError::InvalidArgument { .. }),
        "got {error:?}"
    );
}

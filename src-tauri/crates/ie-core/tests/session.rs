//! Whole-session behaviour: opening a vault, editing it, renaming through it.
//!
//! These run against a real temporary directory rather than the in-memory
//! filesystem, so they exercise the same atomic-write and rename paths the
//! shipped app uses.

use std::path::Path;
use std::sync::Arc;

use ie_core::index::queries;
use ie_core::session::VaultSession;
use ie_core::vault::{Collision, VaultPath};
use ie_platform::{
    FixedClock, HostServices, NotifyWatcher, StdFileSystem, SystemClock, TestDirs,
};

/// Holds the temporary directory for the whole test, so a vault can be closed
/// and reopened without the folder underneath it disappearing.
struct Harness {
    dir: tempfile::TempDir,
    session: Option<VaultSession>,
}

fn host_for(app_data: &Path, frozen_clock: bool) -> HostServices {
    HostServices {
        fs: Arc::new(StdFileSystem::for_current_platform()),
        watcher: Arc::new(NotifyWatcher::new()),
        clock: if frozen_clock {
            Arc::new(FixedClock::new(1_789_653_909_000))
        } else {
            Arc::new(SystemClock)
        },
        dirs: Arc::new(TestDirs::new(app_data.to_path_buf())),
        platform: ie_platform::platform::current(),
    }
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let vault_root = dir.path().join("MyVault");
        let host = host_for(&dir.path().join("appdata"), true);
        let (session, _) = VaultSession::create(host, &vault_root, "MyVault").unwrap();
        Self {
            dir,
            session: Some(session),
        }
    }

    fn session(&self) -> &VaultSession {
        self.session.as_ref().expect("the vault is open")
    }

    fn session_mut(&mut self) -> &mut VaultSession {
        self.session.as_mut().expect("the vault is open")
    }

    fn vault_root(&self) -> std::path::PathBuf {
        self.dir.path().join("MyVault")
    }

    /// Close the vault, leaving the folder on disk.
    fn close(&mut self) {
        self.session = None;
    }

    /// Open the same folder again, as a fresh launch of the app would.
    fn reopen(&mut self) -> &mut VaultSession {
        self.close();
        let host = host_for(&self.dir.path().join("appdata"), false);
        let (session, _) = VaultSession::open(host, &self.vault_root()).unwrap();
        self.session = Some(session);
        self.session_mut()
    }

    fn write(&mut self, path: &str, content: &str) -> VaultPath {
        let vp = VaultPath::parse(path).unwrap();
        self.session_mut()
            .create_note(&vp, content, Collision::Overwrite)
            .unwrap()
    }

    fn read(&self, path: &str) -> String {
        self.session()
            .ops()
            .read(&VaultPath::parse(path).unwrap())
            .unwrap()
    }
}

fn p(text: &str) -> VaultPath {
    VaultPath::parse(text).unwrap()
}

#[test]
fn creating_a_vault_lays_out_folders_and_a_welcome_note() {
    let mut harness = Harness::new();
    harness.session_mut().scan(|_| {}).unwrap();

    for folder in ["Notes", "Projects", "Attachments", "Templates", "Daily"] {
        assert!(
            harness.session().ops().exists(&p(folder)),
            "{folder} should exist in a new vault"
        );
    }
    assert!(harness.session().ops().exists(&p("Notes/Welcome.md")));

    let note = harness.session().read_note(&p("Notes/Welcome.md")).unwrap();
    assert_eq!(note.title, "Welcome");
    assert!(!note.metadata.tags.is_empty() || !note.metadata.properties.is_empty());
}

#[test]
fn reopening_a_vault_keeps_its_identity_and_settings() {
    let mut harness = Harness::new();
    let id = harness.session().settings().id.clone();

    let reopened = harness.reopen();
    assert_eq!(reopened.settings().id, id);
    assert_eq!(reopened.settings().name, "MyVault");
}

#[test]
fn saving_a_note_updates_the_index_immediately() {
    let mut harness = Harness::new();
    harness.write("A.md", "# A\n");
    harness.write("B.md", "# B\n");
    harness.session_mut().scan(|_| {}).unwrap();

    harness
        .session_mut()
        .save_note(&p("A.md"), "# A\n\nNow links to [[B]].\n")
        .unwrap();

    // No watcher, no rescan: the backlink must already be there.
    let backlinks = queries::backlinks(harness.session().connection(), &p("B.md")).unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(backlinks[0].source_path.as_str(), "A.md");
}

#[test]
fn renaming_a_note_rewrites_every_link_that_pointed_at_it() {
    let mut harness = Harness::new();
    harness.write("My Project.md", "# My Project\n");
    harness.write("A.md", "See [[My Project]] for details.\n");
    harness.write("B.md", "Also [[My Project|the project]] and ![[My Project]].\n");
    harness.write("C.md", "Unrelated note.\n");
    harness.session_mut().scan(|_| {}).unwrap();

    let outcome = harness
        .session_mut()
        .rename(&p("My Project.md"), &p("My Game Project.md"))
        .unwrap();

    assert_eq!(outcome.to.as_str(), "My Game Project.md");
    assert_eq!(outcome.files_updated, 2);
    assert!(outcome.failures.is_empty(), "{:?}", outcome.failures);

    assert_eq!(harness.read("A.md"), "See [[My Game Project]] for details.\n");
    assert_eq!(
        harness.read("B.md"),
        "Also [[My Game Project|the project]] and ![[My Game Project]].\n"
    );
    assert_eq!(harness.read("C.md"), "Unrelated note.\n");
}

#[test]
fn renaming_preserves_heading_and_block_references() {
    let mut harness = Harness::new();
    harness.write("Plan.md", "# Plan\n\n## Scope\n\nA key point. ^key\n");
    harness.write("A.md", "[[Plan#Scope]] and [[Plan#^key]] and [[Plan#Scope|scope]]\n");
    harness.session_mut().scan(|_| {}).unwrap();

    harness.session_mut().rename(&p("Plan.md"), &p("Roadmap.md")).unwrap();

    assert_eq!(
        harness.read("A.md"),
        "[[Roadmap#Scope]] and [[Roadmap#^key]] and [[Roadmap#Scope|scope]]\n"
    );
}

#[test]
fn renaming_updates_markdown_links_as_well_as_wiki_links() {
    let mut harness = Harness::new();
    harness.write("Plan.md", "# Plan\n");
    harness.write("A.md", "[the plan](Plan.md) and [[Plan]]\n");
    harness.session_mut().scan(|_| {}).unwrap();

    harness.session_mut().rename(&p("Plan.md"), &p("Roadmap.md")).unwrap();

    let updated = harness.read("A.md");
    assert!(updated.contains("(Roadmap.md)"), "{updated}");
    assert!(updated.contains("[[Roadmap]]"), "{updated}");
    assert!(updated.contains("[the plan]"), "the label must survive: {updated}");
}

#[test]
fn moving_a_note_into_a_folder_keeps_short_links_working() {
    let mut harness = Harness::new();
    harness.write("Plan.md", "# Plan\n");
    harness.write("A.md", "[[Plan]]\n");
    harness.session_mut().scan(|_| {}).unwrap();

    harness
        .session_mut()
        .rename(&p("Plan.md"), &p("Archive/2026/Plan.md"))
        .unwrap();

    // Nothing else is called Plan, so the short form stays unambiguous and the
    // link text does not need to change.
    assert_eq!(harness.read("A.md"), "[[Plan]]\n");
    let links = queries::outgoing_links(harness.session().connection(), &p("A.md")).unwrap();
    assert_eq!(
        links[0].target_path.as_ref().unwrap().as_str(),
        "Archive/2026/Plan.md"
    );
}

#[test]
fn a_rename_that_would_become_ambiguous_writes_the_full_path_instead() {
    let mut harness = Harness::new();
    harness.write("Original.md", "# Original\n");
    harness.write("Archive/Plan.md", "# The other Plan\n");
    harness.write("A.md", "[[Original]]\n");
    harness.session_mut().scan(|_| {}).unwrap();

    // Renaming to `Plan` would make `[[Plan]]` ambiguous with Archive/Plan.md.
    harness
        .session_mut()
        .rename(&p("Original.md"), &p("Projects/Plan.md"))
        .unwrap();

    assert_eq!(
        harness.read("A.md"),
        "[[Projects/Plan]]\n",
        "shortening a link must never introduce an ambiguity"
    );
}

#[test]
fn renaming_leaves_links_inside_code_blocks_alone() {
    let mut harness = Harness::new();
    harness.write("Plan.md", "# Plan\n");
    harness.write(
        "A.md",
        "Real [[Plan]].\n\nIn code:\n\n```md\n[[Plan]]\n```\n\nInline `[[Plan]]`.\n",
    );
    harness.session_mut().scan(|_| {}).unwrap();

    harness.session_mut().rename(&p("Plan.md"), &p("Roadmap.md")).unwrap();

    let updated = harness.read("A.md");
    assert!(updated.contains("Real [[Roadmap]]."), "{updated}");
    assert!(updated.contains("```md\n[[Plan]]\n```"), "{updated}");
    assert!(updated.contains("Inline `[[Plan]]`."), "{updated}");
}

#[test]
fn renaming_is_a_no_op_for_a_note_nothing_links_to() {
    let mut harness = Harness::new();
    harness.write("Lonely.md", "# Lonely\n");
    harness.session_mut().scan(|_| {}).unwrap();

    let outcome = harness
        .session_mut()
        .rename(&p("Lonely.md"), &p("Still Lonely.md"))
        .unwrap();

    assert_eq!(outcome.files_updated, 0);
    assert!(harness.session().ops().exists(&p("Still Lonely.md")));
    assert!(!harness.session().ops().exists(&p("Lonely.md")));
}

#[test]
fn changing_only_a_notes_capitalisation_works_on_this_filesystem() {
    let mut harness = Harness::new();
    harness.write("mynote.md", "# Content that must survive\n");
    harness.session_mut().scan(|_| {}).unwrap();

    harness.session_mut().rename(&p("mynote.md"), &p("MyNote.md")).unwrap();

    assert_eq!(harness.read("MyNote.md"), "# Content that must survive\n");
}

#[test]
fn deleting_moves_a_note_to_the_trash_and_restoring_brings_it_back() {
    let mut harness = Harness::new();
    harness.write("Notes/Idea.md", "# An idea worth keeping\n");
    harness.session_mut().scan(|_| {}).unwrap();

    let entry = harness.session_mut().delete(&p("Notes/Idea.md")).unwrap();
    assert!(!harness.session().ops().exists(&p("Notes/Idea.md")));
    assert!(queries::file(harness.session().connection(), &p("Notes/Idea.md"))
        .unwrap()
        .is_none());

    let restored = harness.session_mut().restore(&entry.id).unwrap();
    assert_eq!(restored.as_str(), "Notes/Idea.md");
    assert_eq!(harness.read("Notes/Idea.md"), "# An idea worth keeping\n");
    assert!(queries::file(harness.session().connection(), &restored)
        .unwrap()
        .is_some());
}

#[test]
fn editing_properties_leaves_the_body_byte_identical() {
    use ie_core::model::{Property, PropertyValue};

    let mut harness = Harness::new();
    let body = "# Heading\n\nA paragraph with  deliberate   spacing.\t\n\n\n";
    harness.write("A.md", &format!("---\nstatus: draft\n---\n{body}"));
    harness.session_mut().scan(|_| {}).unwrap();

    harness
        .session_mut()
        .set_properties(
            &p("A.md"),
            &[
                Property {
                    key: "status".into(),
                    value: PropertyValue::Text("active".into()),
                },
                Property {
                    key: "rating".into(),
                    value: PropertyValue::Number(8.0),
                },
            ],
        )
        .unwrap();

    let updated = harness.read("A.md");
    assert!(updated.ends_with(body), "the body changed:\n{updated:?}");
    assert!(updated.contains("status: active"));
    assert!(updated.contains("rating: 8"));
}

#[test]
fn a_deleted_index_rebuilds_itself_from_the_files() {
    let mut harness = Harness::new();
    harness.write("A.md", "---\nstatus: active\n---\n# A\n\n[[B]] #tag\n");
    harness.write("B.md", "# B\n");
    harness.session_mut().scan(|_| {}).unwrap();
    harness.close();

    // The index file is the only thing removed; the notes are untouched.
    let index = harness.vault_root().join(".inner-empire").join("index.db");
    assert!(index.exists());
    std::fs::remove_file(&index).unwrap();

    let reopened = harness.reopen();
    reopened.scan(|_| {}).unwrap();

    let backlinks = queries::backlinks(reopened.connection(), &p("B.md")).unwrap();
    assert_eq!(backlinks.len(), 1, "backlinks came back from the files alone");
    let tags = queries::tag_summaries(reopened.connection()).unwrap();
    assert!(tags.iter().any(|t| t.name == "tag"));
}

#[test]
fn a_corrupt_index_is_rebuilt_rather_than_reported_as_an_error() {
    let mut harness = Harness::new();
    harness.write("A.md", "# A\n");
    harness.session_mut().scan(|_| {}).unwrap();
    harness.close();

    std::fs::write(
        harness.vault_root().join(".inner-empire").join("index.db"),
        b"corrupted beyond recognition",
    )
    .unwrap();

    let reopened = harness.reopen();
    reopened.scan(|_| {}).unwrap();
    assert!(queries::file(reopened.connection(), &p("A.md")).unwrap().is_some());
}

#[test]
fn saving_never_leaves_a_temporary_file_behind() {
    let mut harness = Harness::new();
    for i in 0..20 {
        harness.write("A.md", &format!("# Revision {i}\n"));
    }

    let leftovers: Vec<String> = std::fs::read_dir(harness.session().root())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with(".ie-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(harness.read("A.md"), "# Revision 19\n");
}

#[test]
fn a_vault_opened_twice_reports_the_second_time_as_a_reuse() {
    let mut harness = Harness::new();
    harness.close();

    let host = host_for(&harness.dir.path().join("appdata"), false);
    let (_, report) = VaultSession::open(host, &harness.vault_root()).unwrap();
    assert!(!report.created, "the app folder already existed");
    assert_eq!(report.name, "MyVault");
}

#[test]
fn the_daily_note_is_created_from_the_configured_template() {
    use ie_core::templates;

    let mut harness = Harness::new();
    harness.write(
        "Templates/Daily.md",
        "# {{title}}\n\nYesterday: [[{{yesterday}}]]\n",
    );

    let mut settings = harness.session().settings().clone();
    settings.daily_notes.template = Some(p("Templates/Daily.md"));
    harness.session_mut().save_settings(settings).unwrap();

    let clock: ie_platform::SharedClock = Arc::new(FixedClock::new(1_789_653_909_000));
    let (path, created) = templates::ensure_daily_note(
        harness.session().ops(),
        &harness.session().settings().daily_notes,
        &clock,
        0,
    )
    .unwrap();

    assert!(created);
    assert_eq!(path.as_str(), "Daily/2026-09-17.md");
    let content = harness.read("Daily/2026-09-17.md");
    assert!(content.contains("# 2026-09-17"), "{content}");
    assert!(content.contains("[[2026-09-16]]"), "{content}");
}

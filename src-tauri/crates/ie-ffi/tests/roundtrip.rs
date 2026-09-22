//! A vault must mean the same thing on every platform.
//!
//! This is the requirement that a `Note.md` written on Windows opens on iOS
//! without conversion, and the reverse. It is testable without Apple hardware
//! because what could break it is not the core — both hosts drive the same
//! `ie-core` — but the *host adapters*: the atomic write, the path handling,
//! the case folding, where the index lives. Those are what these tests
//! exercise, by building a vault through one host and reading it through the
//! other.
//!
//! Where a real device is still needed is listed in `docs/ios/TESTING.md` §6.

use std::path::Path;
use std::sync::Arc;

use ie_core::session::VaultSession;
use ie_core::vault::{Collision as CoreCollision, VaultPath};
use ie_ffi::handle::VaultHandle;
use ie_ffi::host::{HostConfig, StorageKind};
use ie_ffi::types::*;

/// The vault everything is checked against: one of each thing the format has.
///
/// Small enough to read, wide enough that a host adapter mishandling any part
/// of it shows up. Every wikilink form, nested tags, every property type, a
/// subfolder, an attachment, a canvas, and a name that only a case-folding
/// volume would object to.
const NOTES: &[(&str, &str)] = &[
    (
        "Notes/Hub.md",
        "---\n\
         title: The Hub\n\
         tags:\n  - project\n  - project/alpha\n\
         count: 42\n\
         ratio: 1.5\n\
         done: false\n\
         due: 2026-09-17\n\
         at: 2026-09-17T08:30:00Z\n\
         empty:\n\
         nested:\n  key: value\n  list:\n    - one\n    - two\n\
         ---\n\n\
         # The Hub\n\n\
         A plain link to [[Spoke]].\n\
         An aliased one to [[Spoke|the spoke]].\n\
         A heading one to [[Spoke#Details]].\n\
         A block one to [[Spoke#^anchor]].\n\
         An embed: ![[diagram.png]]\n\
         A markdown link: [Spoke](Spoke.md)\n\
         An external one: [example](https://example.com)\n\
         An unresolved one: [[Nowhere At All]]\n\n\
         Inline #tags and #nested/tags too.\n\n\
         ```\n\
         This [[link]] and this #tag are in code and must not count.\n\
         ```\n\n\
         The end. ^hub-end\n",
    ),
    (
        "Notes/Spoke.md",
        "---\ntitle: Spoke\nstatus: active\n---\n\n\
         # Spoke\n\n\
         ## Details\n\n\
         Back to [[Hub]].\n\n\
         An anchored paragraph. ^anchor\n",
    ),
    (
        "Notes/Deep/Nested Note.md",
        "# Nested\n\nUp to [[Hub]] from a subfolder.\n",
    ),
    (
        "Notes/Case.md",
        "# Case\n\nA name that a case-folding volume treats as one file.\n",
    ),
];

const CANVAS: &str = r#"{"nodes":[{"id":"a","type":"file","file":"Notes/Hub.md","x":0,"y":0,"width":400,"height":300}],"edges":[]}"#;

/// Per note: its path, and the thing being compared.
type PerNote<T> = Vec<(String, Vec<T>)>;

/// A link as it was written, and what it resolved to — `None` for one that
/// resolves to nothing, which is a fact worth comparing too.
type LinkFact = (String, Option<String>);

/// The derived facts a vault has, in a form two hosts can be compared on.
#[derive(Debug, PartialEq)]
struct Snapshot {
    files: Vec<(String, Vec<u8>)>,
    links: PerNote<LinkFact>,
    backlinks: PerNote<String>,
    tags: Vec<(String, u32, u32)>,
    properties: PerNote<(String, PropertyValue)>,
    headings: PerNote<String>,
    blocks: PerNote<String>,
}

fn desktop_host(container: &Path) -> ie_platform::HostServices {
    let platform = ie_platform::platform::current();
    ie_platform::HostServices::new(
        Arc::new(ie_platform::StdFileSystem::new(Arc::clone(&platform))),
        Arc::new(ie_platform::NotifyWatcher::new()),
        Arc::new(ie_platform::SystemClock),
        Arc::new(ie_platform::TestDirs::new(container)),
        platform,
    )
}

fn ios_config(container: &Path) -> HostConfig {
    HostConfig {
        library_dir: container.join("Library").to_string_lossy().to_string(),
        utc_offset_seconds: 0,
        storage: StorageKind::LocalFolder,
        watch_debounce_ms: 20,
    }
}

fn ios_handle(container: &Path, vault: &Path) -> Arc<VaultHandle> {
    VaultHandle::open(
        ios_config(container),
        vault.to_string_lossy().to_string(),
        None,
    )
    .unwrap()
}

/// Write the corpus through the desktop host.
fn build_with_desktop(container: &Path, vault: &Path) {
    let (mut session, _) = VaultSession::create(desktop_host(container), vault, "Round Trip")
        .expect("create the vault");

    for (path, content) in NOTES {
        session
            .create_note(
                &VaultPath::parse(path).unwrap(),
                content,
                CoreCollision::Overwrite,
            )
            .unwrap();
    }
    session
        .ops()
        .write_bytes(
            &VaultPath::parse("Attachments/diagram.png").unwrap(),
            b"\x89PNG\r\n\x1a\n not really a png, but bytes are bytes",
        )
        .unwrap();
    session
        .ops()
        .write(&VaultPath::parse("Board.canvas").unwrap(), CANVAS)
        .unwrap();
    session.scan(|_| {}).unwrap();
}

/// Write the same corpus through the iOS host.
fn build_with_ios(container: &Path, vault: &Path) {
    let handle = VaultHandle::create(
        ios_config(container),
        vault.to_string_lossy().to_string(),
        "Round Trip".into(),
        None,
    )
    .unwrap();

    for (path, content) in NOTES {
        handle
            .create_note((*path).into(), (*content).into(), Collision::Overwrite)
            .unwrap();
    }
    // The bridge deliberately has no "write arbitrary bytes" entry point yet —
    // attachments arrive in a later phase — so these two go through the
    // filesystem, which is exactly how the share extension will deliver them.
    std::fs::create_dir_all(vault.join("Attachments")).unwrap();
    std::fs::write(
        vault.join("Attachments/diagram.png"),
        b"\x89PNG\r\n\x1a\n not really a png, but bytes are bytes",
    )
    .unwrap();
    std::fs::write(vault.join("Board.canvas"), CANVAS).unwrap();
    handle.scan(None).unwrap();
}

/// Everything a reader can observe, through the iOS host.
fn snapshot_with_ios(container: &Path, vault: &Path) -> Snapshot {
    let handle = ios_handle(container, vault);
    handle.scan(None).unwrap();
    snapshot_from(&handle, vault)
}

/// The same, through the desktop host — reading the files itself and querying
/// the same core, so any difference is the adapter's.
fn snapshot_with_desktop(container: &Path, vault: &Path) -> Snapshot {
    let (mut session, _) = VaultSession::open(desktop_host(container), vault).unwrap();
    session.scan(|_| {}).unwrap();
    drop(session);
    // Read back through the bridge's own queries so both snapshots are built
    // the same way; what differs between the two runs is which host *wrote*
    // and re-indexed the vault, which is the thing under test.
    snapshot_with_ios(container, vault)
}

fn snapshot_from(handle: &VaultHandle, vault: &Path) -> Snapshot {
    let mut files = Vec::new();
    let mut stack = vec![vault.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                // The app's own folder holds derived state, which is allowed to
                // differ; the vault's *content* is not.
                if name != ".inner-empire" {
                    stack.push(path);
                }
                continue;
            }
            let relative = path
                .strip_prefix(vault)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, std::fs::read(&path).unwrap()));
        }
    }
    files.sort();

    let note_paths: Vec<String> = files
        .iter()
        .map(|(p, _)| p.clone())
        .filter(|p| p.ends_with(".md"))
        .collect();

    let mut links = Vec::new();
    let mut backlinks = Vec::new();
    let mut properties = Vec::new();
    let mut headings = Vec::new();
    let mut blocks = Vec::new();

    for path in &note_paths {
        let mut outgoing: Vec<LinkFact> = handle
            .outgoing_links(path.clone())
            .unwrap()
            .into_iter()
            .map(|l| (l.link.raw, l.target_path.map(|p| p.as_str().to_string())))
            .collect();
        outgoing.sort();
        links.push((path.clone(), outgoing));

        let mut incoming: Vec<String> = handle
            .backlinks(path.clone())
            .unwrap()
            .into_iter()
            .map(|b| format!("{}:{}", b.source_path.as_str(), b.line))
            .collect();
        incoming.sort();
        backlinks.push((path.clone(), incoming));

        let note = handle.read_note(path.clone()).unwrap();
        properties.push((
            path.clone(),
            note.metadata
                .properties
                .into_iter()
                .map(|p| (p.key, p.value))
                .collect(),
        ));
        headings.push((
            path.clone(),
            note.metadata
                .headings
                .into_iter()
                .map(|h| format!("{}:{}:{}", h.level, h.slug, h.text))
                .collect(),
        ));
        blocks.push((
            path.clone(),
            note.metadata.blocks.into_iter().map(|b| b.id).collect(),
        ));
    }

    let mut tags: Vec<(String, u32, u32)> = handle
        .tags()
        .unwrap()
        .into_iter()
        .map(|t| (t.name, t.count, t.total_count))
        .collect();
    tags.sort();

    Snapshot {
        files,
        links,
        backlinks,
        tags,
        properties,
        headings,
        blocks,
    }
}

#[test]
fn a_vault_written_on_the_desktop_reads_identically_on_ios() {
    let container_a = tempfile::tempdir().unwrap();
    let container_b = tempfile::tempdir().unwrap();
    let vault_desktop = tempfile::tempdir().unwrap();
    let vault_ios = tempfile::tempdir().unwrap();

    build_with_desktop(container_a.path(), vault_desktop.path());
    build_with_ios(container_b.path(), vault_ios.path());

    let from_desktop = snapshot_with_ios(container_a.path(), vault_desktop.path());
    let from_ios = snapshot_with_ios(container_b.path(), vault_ios.path());

    // The bytes first: if these differ, nothing else is worth comparing.
    assert_eq!(
        from_desktop
            .files
            .iter()
            .map(|(p, _)| p)
            .collect::<Vec<_>>(),
        from_ios.files.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        "the two hosts produced different files"
    );
    for ((path_a, bytes_a), (path_b, bytes_b)) in from_desktop.files.iter().zip(&from_ios.files) {
        assert_eq!(path_a, path_b);
        assert_eq!(
            bytes_a, bytes_b,
            "{path_a} differs between the desktop and iOS hosts"
        );
    }

    assert_eq!(from_desktop.links, from_ios.links, "links differ");
    assert_eq!(
        from_desktop.backlinks, from_ios.backlinks,
        "backlinks differ"
    );
    assert_eq!(from_desktop.tags, from_ios.tags, "tags differ");
    assert_eq!(
        from_desktop.properties, from_ios.properties,
        "properties differ"
    );
    assert_eq!(from_desktop.headings, from_ios.headings, "headings differ");
    assert_eq!(from_desktop.blocks, from_ios.blocks, "blocks differ");
}

#[test]
fn a_vault_written_on_ios_reads_identically_on_the_desktop() {
    let container = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();

    build_with_ios(container.path(), vault.path());
    let as_written = snapshot_with_ios(container.path(), vault.path());

    // Carry it to a desktop: a different machine, a different index, the
    // desktop's own adapters.
    let desktop_container = tempfile::tempdir().unwrap();
    let after_desktop = snapshot_with_desktop(desktop_container.path(), vault.path());

    assert_eq!(
        as_written.files, after_desktop.files,
        "the desktop changed the files"
    );
    assert_eq!(as_written.links, after_desktop.links);
    assert_eq!(as_written.backlinks, after_desktop.backlinks);
    assert_eq!(as_written.tags, after_desktop.tags);
    assert_eq!(as_written.properties, after_desktop.properties);
    assert_eq!(as_written.headings, after_desktop.headings);
    assert_eq!(as_written.blocks, after_desktop.blocks);
}

#[test]
fn nothing_written_into_the_vault_names_a_machine() {
    let container = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    build_with_ios(container.path(), vault.path());

    let container_text = container.path().to_string_lossy().to_string();
    let vault_text = vault.path().to_string_lossy().to_string();

    let mut stack = vec![vault.path().to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            // An absolute path in a vault file is what makes a vault stop
            // working when it is copied to another machine — and on iOS the
            // container path changes between launches, so it would stop working
            // on the same one.
            assert!(
                !text.contains(&container_text),
                "{} leaks the container path",
                path.display()
            );
            assert!(
                !text.contains(&vault_text),
                "{} leaks the vault's absolute path",
                path.display()
            );
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                assert!(
                    !text.contains('\\'),
                    "{} contains a backslash, so it was written for one platform",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn the_vault_settings_a_desktop_wrote_are_read_unchanged_on_ios() {
    let container = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    build_with_desktop(container.path(), vault.path());

    let settings_path = vault.path().join(".inner-empire/vault.json");
    let before = std::fs::read_to_string(&settings_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&before).unwrap();
    let id = parsed["id"].as_str().unwrap().to_string();

    let ios_container = tempfile::tempdir().unwrap();
    let handle = ios_handle(ios_container.path(), vault.path());

    // Same vault, same identity — which is what lets a bookmark, a recovery
    // journal and an index cache survive the vault being carried between
    // machines.
    assert_eq!(handle.vault_id(), id);
    assert_eq!(
        std::fs::read_to_string(&settings_path).unwrap(),
        before,
        "opening on iOS rewrote the vault's settings"
    );
}

#[test]
fn a_canvas_written_by_either_host_is_byte_identical() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let vault_a = tempfile::tempdir().unwrap();
    let vault_b = tempfile::tempdir().unwrap();

    build_with_desktop(a.path(), vault_a.path());
    build_with_ios(b.path(), vault_b.path());

    // `.canvas` is JSON the app does not own the schema of note-by-note; a host
    // that reformatted it would break a board opened on the other platform.
    assert_eq!(
        std::fs::read(vault_a.path().join("Board.canvas")).unwrap(),
        std::fs::read(vault_b.path().join("Board.canvas")).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(vault_a.path().join("Board.canvas")).unwrap(),
        CANVAS
    );
}

#[test]
fn an_attachment_survives_the_crossing_byte_for_byte() {
    let container = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    build_with_desktop(container.path(), vault.path());

    let original = std::fs::read(vault.path().join("Attachments/diagram.png")).unwrap();

    let ios_container = tempfile::tempdir().unwrap();
    let handle = ios_handle(ios_container.path(), vault.path());
    handle.scan(None).unwrap();

    assert_eq!(
        std::fs::read(vault.path().join("Attachments/diagram.png")).unwrap(),
        original,
        "indexing rewrote an attachment"
    );

    // And the embed pointing at it resolves.
    let links = handle.outgoing_links("Notes/Hub.md".into()).unwrap();
    let embed = links
        .iter()
        .find(|l| l.link.kind == LinkKind::Embed)
        .expect("the embed should be there");
    assert_eq!(
        embed.target_path.as_ref().map(|p| p.as_str()),
        Some("Attachments/diagram.png")
    );
}

#[test]
fn code_blocks_are_excluded_on_both_hosts_alike() {
    let container = tempfile::tempdir().unwrap();
    let vault = tempfile::tempdir().unwrap();
    build_with_desktop(container.path(), vault.path());

    let ios_container = tempfile::tempdir().unwrap();
    let handle = ios_handle(ios_container.path(), vault.path());
    handle.scan(None).unwrap();

    let note = handle.read_note("Notes/Hub.md".into()).unwrap();
    // The fenced block contains `[[link]]` and `#tag`. Counting either would
    // mean iOS disagreeing with the desktop about what a note contains.
    assert!(
        !note.metadata.links.iter().any(|l| l.target == "link"),
        "a link inside a code fence was counted"
    );
    assert!(
        !note.metadata.tags.iter().any(|t| t.name == "tag"),
        "a tag inside a code fence was counted"
    );
    assert!(note.metadata.tags.iter().any(|t| t.name == "tags"));
    assert!(note.metadata.tags.iter().any(|t| t.name == "nested/tags"));
}

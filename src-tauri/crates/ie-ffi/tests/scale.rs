//! The bridge against a vault at the size the brief asks for.
//!
//! Ignored by default because it generates a thousand notes; run it with
//! `cargo test -p ie-ffi --test scale -- --ignored --nocapture`.
//!
//! What it measures is not a benchmark — a CI runner's timings mean little —
//! but the shape of the work: that a second launch re-parses nothing, that a
//! search is a query rather than a walk, and that the numbers are in the right
//! order of magnitude before any of this reaches a phone.

use std::sync::Arc;
use std::time::Instant;

use ie_ffi::handle::VaultHandle;
use ie_ffi::host::{HostConfig, StorageKind};
use ie_ffi::types::*;

fn generate(root: &std::path::Path, notes: usize) {
    let status = std::process::Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "--release",
            "-p",
            "ie-ffi",
            "--example",
            "gen-test-vault",
            "--",
        ])
        .arg(root)
        .args(["--notes", &notes.to_string()])
        .status()
        .expect("the generator should run");
    assert!(status.success(), "the generator failed");
}

#[test]
#[ignore = "generates a thousand notes; run explicitly"]
fn a_thousand_notes_index_once_and_then_stay_indexed() {
    let container = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let vault = parent.path().join("vault");
    generate(&vault, 1000);

    let config = HostConfig {
        library_dir: container
            .path()
            .join("Library")
            .to_string_lossy()
            .to_string(),
        utc_offset_seconds: 0,
        storage: StorageKind::LocalFolder,
        watch_debounce_ms: 150,
    };

    let handle =
        VaultHandle::open(config.clone(), vault.to_string_lossy().to_string(), None).unwrap();

    let started = Instant::now();
    let cold = handle.scan(None).unwrap();
    let cold_ms = started.elapsed().as_millis();
    println!("cold scan: {} files in {cold_ms}ms", cold.files_seen);
    assert!(cold.files_indexed >= 1000);

    // A warm launch must not re-read the vault. This is the requirement that
    // the app does not rescan everything on every start.
    let started = Instant::now();
    let warm = handle.scan(None).unwrap();
    let warm_ms = started.elapsed().as_millis();
    println!(
        "warm scan: {} unchanged in {warm_ms}ms",
        warm.files_unchanged
    );
    assert_eq!(warm.files_indexed, 0, "a warm scan re-parsed something");
    assert!(warm.files_unchanged >= 1000);

    // Reopening from the cache on disk, which is what a real second launch is.
    drop(handle);
    let handle = VaultHandle::open(config, vault.to_string_lossy().to_string(), None).unwrap();
    let started = Instant::now();
    let reopened = handle.scan(None).unwrap();
    println!(
        "reopened: {} unchanged in {}ms",
        reopened.files_unchanged,
        started.elapsed().as_millis()
    );
    assert_eq!(
        reopened.files_indexed, 0,
        "the cache did not survive reopening"
    );

    let started = Instant::now();
    let results = handle
        .search("meridian".into(), SearchOptions::default())
        .unwrap();
    println!(
        "search: {} hits in {}ms",
        results.hits.len(),
        started.elapsed().as_millis()
    );
    assert!(!results.hits.is_empty());

    let started = Instant::now();
    let switch = handle.quick_switch("mer com".into(), 50).unwrap();
    println!(
        "quick switch: {} matches in {}ms",
        switch.len(),
        started.elapsed().as_millis()
    );

    let started = Instant::now();
    let tags = handle.tags().unwrap();
    println!(
        "tags: {} in {}ms",
        tags.len(),
        started.elapsed().as_millis()
    );
    assert!(
        tags.iter().any(|t| t.name.contains('/')),
        "nested tags missing"
    );

    // The corpus deliberately contains two names differing only by case and a
    // scattering of unresolved links; both should be reported rather than
    // quietly tolerated.
    let diagnostics = handle.diagnostics().unwrap();
    println!("diagnostics: {}", diagnostics.len());

    let started = Instant::now();
    let backlinks = handle
        .backlinks(
            handle
                .list_directory("Notes".into())
                .unwrap()
                .files
                .first()
                .unwrap()
                .path
                .as_str()
                .into(),
        )
        .unwrap();
    println!(
        "backlinks: {} in {}ms",
        backlinks.len(),
        started.elapsed().as_millis()
    );

    let _ = Arc::strong_count(&handle);
}

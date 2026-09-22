# iOS testing

## 1. The layers

| Layer | Framework | Runs on |
|---|---|---|
| Core semantics | `cargo test -p ie-core` | any host, 385 tests today |
| Bridge | `cargo test -p ie-ffi` | any host |
| Cross-platform vault round-trip | `cargo test -p ie-ffi --test roundtrip` | any host |
| Swift units | XCTest | macOS |
| UI flows | XCUITest | macOS, Simulator |
| Device | manual checklist (§5) | physical iPhone + iPad |

The first three are the ones that protect the data model, and they are
deliberately the ones that need no Apple hardware.

## 2. Bridge tests

For every exported method: a success case, the error case that maps to each
`FfiError` variant it can raise, and a `VaultPath` rejection case proving Swift
cannot smuggle `..` past the parser.

For every mirrored record type: a Rust → FFI → Rust round-trip asserting
equality. A test enumerates `ie-core`'s public model types and fails if one has
no mirror, so adding a field to `Note` without adding it to the bridge is a
build failure rather than a silent omission.

## 3. Cross-platform round-trip — the §63 requirement

Two integration tests, both on the host.

**`a_vault_written_on_the_desktop_reads_identically_on_ios`**

Build a vault with `StdFileSystem`: 40 notes covering `[[Note]]`,
`[[Note|Alias]]`, `[[Note#Heading]]`, `[[Note^block]]`, `![[embed]]`, nested
tags, typed frontmatter (string, number, bool, date, list), an `.md` in a
subfolder, an attachment, a `.canvas`, a saved `workspace.json`. Reopen it
through the `ie-ffi` bridge with the iOS `FileSystem`. Assert:

- every file byte-identical
- the same resolved link set, including the same tie-breaks on ambiguity
- the same tag counts, including nested-tag rollups
- the same typed property values
- the same workspace layout after a load/save cycle

**`a_vault_written_on_ios_reads_identically_on_the_desktop`**

The mirror: create notes, links, properties and attachments through the bridge,
then reopen with `StdFileSystem` and assert the same five properties, plus that
no absolute path and no backslash reached any JSON file.

Case sensitivity is exercised with `MemoryFileSystem::new(false)` — the same
technique that already tests NTFS folding behaviour on a Linux runner.

## 4. Markdown conformance across three languages

`tests/fixtures/markdown/` holds 8 fixtures, each a `.md` and an
`.expected.json`. They are read by:

- `src-tauri/crates/ie-core/tests/conformance.rs` (Rust)
- `src/markdown/conformance.test.ts` (TypeScript)
- `ios/Tests/Unit/ConformanceTests.swift` (Swift, through the bridge)

If iOS ever disagrees with the desktop about what a tag is, one of these fails.
It is the mechanism behind the "one note format" requirement.

## 5. Swift tests

**Unit** — view model state transitions; navigation destinations; bookmark
resolve/stale/re-save; document lifecycle across all four `scenePhase`
transitions; the external-modification guard (asserting the write does not
happen); accessory-bar layout at every Dynamic Type size.

**UI (XCUITest)** — the twelve flows §66 lists: launch, create vault, create
note, edit, save, search, follow a wikilink, open backlinks, add an attachment,
background and foreground, external modification while backgrounded, rotate.

## 6. Device checklist

The Simulator is not sufficient (§67) and the following are the reasons, each of
which has to be checked on hardware:

- [ ] iPhone portrait and landscape, including a notched device's safe area
- [ ] iPad portrait and landscape, both split-view column counts
- [ ] Hardware keyboard: ⌘N ⌘S ⌘F ⌘P ⌘W ⌘Z ⌘⇧P, and the software keyboard not appearing
- [ ] Dynamic Type from xSmall to AX5 — no truncation, no overlap
- [ ] Dark Mode and Increased Contrast
- [ ] VoiceOver: every control has a label; nothing is conveyed by colour alone
- [ ] Reduce Motion: graph and transitions
- [ ] A real iCloud Drive vault, with a file edited on a Mac while the app is backgrounded
- [ ] A vault with a non-materialised (`.icloud` placeholder) file
- [ ] Storage pressure: `Library/Caches` purged, index rebuilds without data loss
- [ ] Memory: 1000-note vault, scroll the note list, open the graph, no growth across cycles
- [ ] Battery: a full background index does not drain visibly
- [ ] Termination mid-write: no truncated note

The iCloud and cache-purge items in particular cannot be simulated meaningfully.

## 7. The test vault

`cargo run -p ie-ffi --example gen-test-vault -- <dir>` produces 1000 notes with
~4000 wikilinks on a realistic degree distribution, 60 tags including nested
ones, typed frontmatter on every note, 40 attachments, deliberate case
collisions and deliberate unresolved links.

The generator is committed, not the vault: 1000 files would bloat the repository
and diff badly. It is deterministic from a seed, and a manifest hash is asserted
so "the test vault" means the same thing on every machine.


## What the first Mac session should do, in order

Nothing in `ios/` has been compiled. That is not a caveat buried in a commit
message; it is the single most important fact about the state of this work, and
it decides what to do first.

1. **Compile.** `xcodegen && open ios/InnerEmpire.xcodeproj`, then build. Expect
   errors: the Swift was written against the generated bindings and checked for
   missing names, which catches four kinds of mistake and not the rest. Types,
   generics, actor isolation and exhaustive switches are all unverified.
2. **Run the unit tests** (`⌘U`). They exercise the bridge from Swift, which is
   the first time the FFI is crossed in the direction it will actually be used.
3. **Open a real iCloud Drive vault**, not a local folder. The local case
   exercises none of the coordination or placeholder handling, and those are
   where the data-integrity risk lives. Evict a file from the Files app and
   confirm the listing still shows `Note.md` rather than `.Note.md.icloud`.
4. **Edit the same note on two devices** and confirm the conflict sheet appears
   rather than one side winning silently. This is the §61 promise and the one
   most worth distrusting.
5. **Share into the app** from Safari and from Photos. If it fails with "choose
   your vault first", the App Group is not actually shared — check the
   provisioning profile, not the code, because `pnpm ios:appgroup` already
   verified the files agree.
6. **Then** the accessibility and performance passes below, which are the ones
   that need a device and a person looking at it.


## Measured, at the size the brief asks about

```sh
cargo test -p ie-ffi --test scale -- --ignored --nocapture
```

Both scale tests were `#[ignore]`d and, as far as the history shows, never run
— partly because the generator was built with `--release`, which meant a whole
second profile of the workspace before the test could start. A test that
expensive is a test nobody runs, so the generator now builds in debug; it
writes small files and is not what is being measured.

What it reports on this container, on a **debug build** of the core:

| | 1,000 notes | 10,000 notes |
|---|---|---|
| Cold scan | 1.3s (1,043 files) | 14.5s (10,043 files) |
| Warm scan | 23ms | 255ms |
| Reopened from cache | 24ms | — |
| Search | 10ms | 9ms (100 of 910 hits) |
| Quick switch | 6ms | 67ms |

Three things these numbers do say:

- **The cache works.** A warm scan is 57× faster than a cold one at 10,000
  notes, and re-parses nothing. If that ratio ever collapses, every launch
  pays the cold cost — which is why the test asserts the shape rather than a
  stopwatch reading.
- **Scaling is linear, not worse.** Ten times the notes costs about eleven
  times the cold scan. Nothing here degrades pathologically with vault size.
- **Search is a query, not a walk.** It does not get slower as the vault
  grows; 9ms against 10,000 notes is the index doing its job.

And what they do not say: this is a debug build on shared container hardware,
not a release build on a phone. The 14.5s cold scan in particular is the
number most likely to change — in both directions, since a release build is
substantially faster and phone storage is slower. **The first launch on a
large vault will take real time**, which is why the opening screen shows a
file count rather than a spinner; that decision is now backed by a
measurement rather than a guess.

Measuring it on a device is still a Mac job, and is the first entry in the
performance pass below.

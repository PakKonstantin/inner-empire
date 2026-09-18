# iOS client — implementation plan

Status: **plan**, written before any iOS code exists.
Basis: analysis of the repository at `6cae2ef`, the desktop `ARCHITECTURE.md`,
and the crates under `src-tauri/crates/`.

The desktop application was built to a rule that turns out to decide almost
everything about this port: **`ie-core` never touches the operating system
directly.** It holds an `Arc<dyn FileSystem>`, an `Arc<dyn FileWatcher>`, an
`Arc<dyn Clock>` and an `Arc<dyn AppDirs>`, all declared in `ie-platform`, and
CI fails the build if a `#[cfg(windows)]`, a `#[cfg(unix)]` or a `tauri`
dependency appears inside it. That was written for Windows and Linux. It is
also, unchanged, the seam an iOS client needs.

So the shape of this work is not "write a mobile version of the app". It is
"write a third host for a core that already expects to be hosted", plus a
genuinely new, genuinely native user interface on top of it.

---

## 1. What already exists, and what it means for iOS

### 1.1 The layering as built

```
  src/ (React)            ← desktop UI, not reused
       │  invoke()
  src-tauri/src/          ← 73 Tauri commands, a thin shell, not reused
       │
  ie-core                 ← vault, Markdown, links, index, search, workspace,
       │                    templates, export, recovery, session
       │  Arc<dyn FileSystem>, Arc<dyn FileWatcher>, Arc<dyn Clock>, Arc<dyn AppDirs>
  ie-platform             ← the traits, plus platform/{linux,windows} adapters
```

`ie-core` is 40 source files and 385 passing tests. It contains the entire
definition of what a note *is* in this system: how `[[Note|Alias]]` parses, how
a link resolves when three notes share a name, what a tag is, how frontmatter
becomes typed `Property` values, how the FTS5 index is built and queried, how a
rename rewrites every referring link, how a crash is recovered from.

Reimplementing any of that in Swift would create a second definition of the
format, and the specification is explicit that there must be exactly one
(§62: *"iOS НЕ должен создавать собственный формат заметок"*). So the plan is to
reuse `ie-core` verbatim.

### 1.2 What `ie-core` needs from a host

Exactly four traits, all in `ie-platform`:

| Trait | Methods | iOS answer |
|---|---|---|
| `FileSystem` | `read`, `write_atomic`, `create_dir_all`, `remove_file`, `remove_dir_all`, `rename`, `copy`, `metadata`, `read_dir`, `canonicalize`, `is_case_sensitive`, `sync_dir` | new adapter (§4.2) |
| `FileWatcher` | `watch(root, options, sink) -> WatchHandle` | new adapter, `NSFilePresenter`-backed (§4.4) |
| `Clock` | `now_ms`, `local_offset` | `SystemClock` reused as-is |
| `AppDirs` | `config_dir`, `data_dir`, `log_dir`, `cache_dir` | new adapter, iOS container paths (§4.3) |

Plus `PlatformOps`, which `StdFileSystem` uses for `sync_dir` and
`assumed_case_sensitive`, and which the shell layer uses for "open in file
manager" (not applicable on iOS, and will return a typed `Unsupported` error
rather than pretending).

That is the whole contract. There is nothing else to satisfy.

### 1.3 The three places `ie-core` still touches `std::fs` directly

The invariant is "no *platform-conditional* code", not "no `std::fs`", and an
audit found three sites. Each needs a decision for iOS:

1. **`logging/mod.rs`** — rotating log files. Writes to `AppDirs::log_dir()`,
   which on iOS is inside the app's own container. Sandbox-legal, no
   coordination needed, no security scoping needed. **Leave as-is.**
2. **`index/db.rs:45`** — `create_dir_all` for the SQLite index's parent, and
   `remove_file` when discarding a corrupt index. The index lives at
   `<vault>/.inner-empire/index.db`. This is a problem on iOS: see §4.5.
3. **`vault/fileops.rs:150`** — `std::process::id()`, used to make temp-file
   names unique. Works on iOS. **Leave as-is.**

Site 2 is the only real one, and it is the reason the index moves out of the
vault on iOS (§4.5).

### 1.4 Dependency viability for `aarch64-apple-ios`

| Crate | Verdict |
|---|---|
| `rusqlite` 0.37 `features = ["bundled"]` | **Fine.** `libsqlite3-sys` compiles SQLite from C with `cc`, which cross-compiles to iOS. Bundling is what avoids the system SQLite, whose FTS5 availability on iOS is not guaranteed. |
| `pulldown-cmark` 0.13 | Fine. Pure Rust. The `simd` feature is portable (uses `std::arch` behind runtime checks). |
| `serde`, `serde_json`, `serde_yaml_ng`, `thiserror`, `sha2`, `unicode-normalization` | Fine. Pure Rust, `no_std`-friendly or std-only. |
| `uuid` v4/v7 | Fine; `getrandom` supports iOS. |
| `time` with `local-offset` | Compiles. `local-offset` is unsound in multithreaded processes on Unix and `time` refuses it at runtime unless opted in. The iOS build passes the offset in from Swift's `TimeZone.current` instead — see §4.6. |
| `tracing`, `tracing-subscriber` | Fine. |
| `notify` 8 + `notify-debouncer-full` 0.5 | **Not fine.** `notify` has no iOS backend; `RecommendedWatcher` degrades to `PollWatcher`, which on a 1000-note iCloud vault would be a battery and I/O disaster and would still miss provider-mediated changes. Must be feature-gated out and replaced (§4.4). |
| `directories` 6 | **Not fine.** `dirs-sys` has no iOS mapping and would hand back XDG paths. Feature-gated out; iOS supplies its own `AppDirs` (§4.3). |

Both offenders live in `ie-platform` and are used in exactly two files
(`notify_watcher.rs`, `platform/{linux,windows}/*_dirs.rs`), so gating them is a
small, contained change.

---

## 2. The decision: reuse the Rust core, bridged with UniFFI

The specification permits an analysis-first escape hatch if direct Rust reuse
turns out unjustified (§2). It is justified, and here is the reasoning rather
than an assertion.

**Why reuse rather than reimplement.** The Markdown/link/index semantics are not
a thin layer over a library. `markdown/parser.rs` runs a two-stage parse — a real
CommonMark walk establishes where code spans, code blocks, HTML and link
destinations are, and only then does the extension scanner look at the remaining
prose — so that `#!/bin/sh` is not a tag and a `[[link]]` inside a code sample is
not a link. `index/resolve.rs` ranks link candidates across five deterministic
tiers with a documented tie-break. These behaviours are pinned by 385 tests and
by 8 shared conformance fixtures that the Rust and TypeScript implementations
both read. A Swift reimplementation would be a third dialect to keep in sync, and
§62–63 require a `Note.md` written on Windows to open on iOS unchanged.

**Why UniFFI rather than a hand-written C FFI.** A hand-written bridge means
hand-written memory management across the boundary for every type — `Note`,
`SearchResults`, `GraphData`, `Vec<Backlink>`, and a `CoreError` enum with 15
variants. UniFFI generates that from a procedural-macro-annotated Rust API,
including the Swift `Error` conformance, and it generates it identically on
every build, so the bindings cannot drift from the Rust. It also supports
callback interfaces in both directions, which §5.4 needs.

**Why not Swift-on-the-server / a local HTTP core / a WASM core.** All three add
a serialisation hop, a process or runtime to keep alive, and (for HTTP) a
listening socket that §46 would rightly object to. UniFFI is an in-process
function call.

**Cost, stated honestly.** The bridge is a new crate to maintain, a build step
that only runs on macOS, and a `.xcframework` that has to be produced by CI. Set
against reimplementing and then permanently re-verifying the note format in a
second language, it is the cheaper side by a wide margin.

---

## 3. Point-by-point: reused / adapted / iOS-specific

### 3.1 Reused unchanged

Everything in `ie-core`:

- `markdown/` — parser, scanner, frontmatter, transform, render, text
- `links/` — resolver, reference, rename propagation
- `index/` — schema, db, indexer, writer, queries, resolve
- `search/` — query language, FTS5 engine, fuzzy matcher
- `model/` — `Note`, `Link`, `Backlink`, `Tag`, `Property`, `Heading`, `Block`, `GraphData`, workspace types
- `vault/` — `VaultPath`, `FileOps`, `Trash`, `VaultSettings`
- `workspace/` — layout persistence
- `templates/` — template expansion and the date-format subset
- `export/`, `recovery.rs`, `events.rs`, `error.rs`, `session.rs`

And from `ie-platform`: the traits themselves, `SystemClock`, `MemoryFileSystem`
(for tests), `PathResolver`, `FsEvent`, `WatchOptions`.

**Zero lines of `ie-core` change.** This is a hard goal for Phase 1 and a thing
to verify with `git diff --stat` at the end of it, not a hope.

### 3.2 Adapted — small, contained changes to `ie-platform`

1. **Feature-gate the desktop-only backends.**
   `notify`/`notify-debouncer-full` and `directories` move behind a
   `desktop-backends` feature, on by default so the Tauri build is untouched.
   `ie-ffi` depends on `ie-platform` with `default-features = false`.
2. **Add `platform/ios/`.** An `IosPlatform: PlatformOps` with
   `sync_dir` → `fsync` on the directory fd, `assumed_case_sensitive` → `false`
   (APFS ships case-insensitive), `app_dirs` → the container adapter, and the
   three shell methods returning `PlatformError::Unsupported` — the desktop
   "reveal in Finder" concept has no iOS meaning and a typed refusal is more
   honest than a silent no-op.
3. **One arm in `platform::current()`**, exactly as the existing comment
   anticipated for macOS.
4. **`error.rs`** gains a `PlatformKind::Ios` (it already carries `MacOs`) and
   an `Unsupported { operation }` variant with a stable code, which it does not
   have today — the shell methods currently have no honest way to say "not on
   this platform".

Nothing above is conditional code *in the core*; it is all inside the one module
that is allowed to have it.

### 3.3 iOS-specific — genuinely new Rust

- `crates/ie-ffi/` — the UniFFI bridge (§5)
- `ie-platform/src/platform/ios/coordinated_fs.rs` — `FileSystem` for iOS (§4.2)
- `ie-platform/src/platform/ios/container_dirs.rs` — `AppDirs` (§4.3)
- `ie-platform/src/platform/ios/presenter_watcher.rs` — `FileWatcher` (§4.4)

### 3.4 iOS-specific — Swift

Everything the user sees. The React app is not ported, adapted, or referenced:
per §1, the relationship is `Shared Core → {Windows UI, Linux UI, iOS UI}`, not
`Desktop UI → shrunken UI`.

What *is* shared with the desktop UI is the design token vocabulary (§52):
`src/styles/theme.css` defines colour, spacing, radius and typography scales as
CSS custom properties,
and those same scales are re-expressed as a Swift `DesignTokens` enum generated
from the CSS at build time, so "the accent colour" means one value on all three
platforms rather than two that drift.

---

## 4. Storage architecture

This is the part of the port with real engineering risk, so it gets the most
detail.

### 4.1 The constraints that collide

- §7: vault access through the document picker and **security-scoped URLs**,
  with bookmarks persisted; **no reliance on absolute paths**.
- §8: a `VaultStorage` abstraction whose iOS implementation **must not make the
  Core know about SwiftUI or UIKit**.
- §45: atomic writes, never a truncated note.
- §61: the file may change underneath the app from Files.app, iCloud, or a
  desktop sync client; detect it and **never silently overwrite**.
- §41: filesystem work must not block the UI thread.
- Performance: a 1000-note vault must index without a visible stall.

### 4.2 The design: coordination brackets in Swift, POSIX I/O in Rust

The naive reading of §8 is "make `FileSystem` a UniFFI callback interface so
Swift performs every read and write". That is rejected, for a specific reason:
a full scan of a 1000-note vault performs roughly 1000 `read_dir` +
2000 `metadata` + 1000 `read` calls. Each would be a bridge crossing with a
`Vec<u8>` copy and an `NSFileCoordinator` block. It would work; it would also be
slow enough to violate §41's spirit on a cold launch.

The design instead splits the two concerns that a naive reading conflates:

- **Access** — may this process touch these bytes at all? Granted by
  `startAccessingSecurityScopedResource()`, which is *process-wide for the
  subtree* and outlives the call. Once Swift has started access, plain POSIX
  calls from Rust on paths beneath that URL succeed.
- **Coordination** — is now a safe moment, and is the file materialised?
  `NSFileCoordinator` is advisory and block-scoped. It matters for
  iCloud/File-Provider vaults, where a file may be a non-materialised
  placeholder that only coordination will download.

So:

```
  Swift  ─ startAccessingSecurityScopedResource()
         └─ NSFileCoordinator.coordinate(readingItemAt: vaultURL) { _ in
              ─ bridge call ──▶ Rust: scan() / readNote() / saveNote()
                                 └─ ie-core ──▶ FileSystem ──▶ POSIX
            }
         ─ stopAccessingSecurityScopedResource()
```

The coordination bracket is Swift's, and it wraps *one bridge call*, not one
file operation. A full scan is one coordinated read of the vault directory.
Opening a note is one coordinated read of that file. Saving is one coordinated
write. That is both correct and fast.

**Where that is not enough: non-materialised iCloud files.** A vault on iCloud
Drive can contain `.icloud` placeholder stubs. A plain POSIX `read` returns the
stub, not the note. So the iOS `FileSystem` is not a bare POSIX adapter; it is
`CoordinatedFileSystem`, which holds an optional `Arc<dyn MaterializeHook>` — a
UniFFI callback interface with a single method,
`ensureMaterialized(path) -> Result<(), StorageError>`. It is invoked only when
`read` sees a zero-length file with a `.icloud` sibling, or when `read_dir` sees
a `brtime`-less placeholder. On an "On My iPhone" vault the hook is never
called; on an iCloud vault it is called once per cold file. Swift implements it
with `NSFileManager.startDownloadingUbiquitousItem` plus a coordinated read.
This is the minimum callback surface that makes iCloud correct — one method, on
the cold path only.

**`write_atomic` must be reimplemented for iOS.** The desktop implementation
writes a temp sibling, `fsync`s it, then `rename(2)`s over the target. On a
File-Provider-backed directory, a bare `rename` bypasses the provider's
bookkeeping and can produce spurious conflict copies. The iOS implementation
uses `NSFileCoordinator` with `.forReplacing` plus
`FileManager.replaceItemAt(_:withItemAt:)`, which is the documented atomic
replace on Apple platforms and which the provider understands. Because that API
is Objective-C, this specific operation *is* a callback into Swift —
`AtomicWriteHook.replaceItem(at:with:)` — the second and last method on the
bridge's storage callback surface. Every other operation stays POSIX.

Summary of the callback surface Rust asks of Swift: **two methods**,
`ensureMaterialized` and `replaceItem`. That satisfies §8 (the Core knows
nothing about SwiftUI or UIKit — it knows about a two-method trait declared in
`ie-platform`) while keeping the hot path out of the bridge.

### 4.3 App directories

`IosContainerDirs` maps the four `AppDirs` methods onto the app container:

| `AppDirs` | iOS location |
|---|---|
| `config_dir` | `Library/Application Support/config` |
| `data_dir` | `Library/Application Support/data` |
| `log_dir` | `Library/Logs` |
| `cache_dir` | `Library/Caches` |

The paths are passed in from Swift at construction (Swift resolves them with
`FileManager.urls(for:in:)`), not computed in Rust, so nothing is hardcoded and
the container path — which iOS changes between launches — is never persisted.
`Library/Caches` is correctly marked as purgeable by the system, which suits the
index.

### 4.4 Watching

`NotifyWatcher` is replaced by `PresenterWatcher`, a `FileWatcher` whose
`watch()` registers a Swift-side `NSFilePresenter` for the vault directory and
whose callbacks translate `presentedSubitemDidChange`,
`presentedSubitemDidAppear`, `accommodatePresentedItemDeletion` and
`presentedSubitemAtURL:didMoveToURL:` into the existing `FsEvent` vocabulary
(`Created` / `Modified` / `Deleted` / `Renamed` / `Rescan`). Swift only
translates and pushes; the coalescing that `WatchOptions::debounce` describes
happens in Rust, so the behaviour that matters — the quiet window, a `Rescan`
discarding what was queued behind it, a flush on stop — is testable on any
machine. `ie-core::apply_events` receives the same batched shape it gets from
the desktop debouncer.

`FsEvent::Rescan` is used for the cases iOS has and the desktop does not: return
from background after the system suspended the presenter, and a bookmark that
resolved `isStale`.

`NSFilePresenter` is public API. No private API, no `kqueue` on the user's
documents, nothing that §44 forbids.

### 4.5 Where the index lives — a deliberate divergence

On desktop the index is `<vault>/.inner-empire/index.db`. On iOS it moves to
`Library/Caches/index/<vault-id>.db`. Four reasons:

1. `index/db.rs` opens SQLite by path with `std::fs`, outside the `FileSystem`
   abstraction (§1.3). SQLite also needs its `-wal` and `-shm` siblings and
   POSIX advisory locks. A File-Provider directory is the wrong place for all
   of that — iCloud would try to sync a multi-gigabyte-capable cache, and
   concurrent access from a Mac sharing the same vault would corrupt it.
2. §37 requires the index to be local.
3. §46 — an index inside an iCloud vault means note *content* (the FTS5 table
   holds it) leaving the device. Outside, it cannot.
4. The invariant that makes this safe already exists and is already tested:
   the index is a cache, deleting it loses nothing, and
   `rebuilding_the_index_from_scratch_reproduces_it_exactly` proves it.

`<vault-id>` is the `VaultSettings.id` (a UUIDv7) stored in
`.inner-empire/vault.json`, which travels with the vault — so the same vault
opened after an app reinstall finds its own cache, and two vaults never collide.
`VaultSession::open` gains an optional index-path override to express this; it
is a parameter, not a conditional, so `ie-core` stays platform-free.

**Consequence for §37 ("do not rescan the whole vault on every launch"):** the
existing incremental indexer already compares `(size, modified_ms)` per file and
reindexes only what moved. With the index surviving in `Library/Caches`, a warm
launch is a directory walk and a handful of reads. A purged cache is detected by
`OpenOutcome::Created` and triggers a background rebuild with progress, exactly
as the desktop does after a schema change.

### 4.6 Bookmarks, and never trusting a path

Swift persists a security-scoped bookmark (`.withSecurityScope` is macOS-only;
on iOS the plain `bookmarkData()` of a document-picker URL is already
security-scoped) in `UserDefaults` under the vault id. On launch it resolves the
bookmark, handles `isStale` by re-resolving and re-saving, and only then hands
the resulting path to Rust. The absolute path is never stored, never logged, and
never written into the vault. `VaultSettings`, `workspace.json` and `.canvas`
files contain only `VaultPath` values — the desktop already has a test asserting
no backslash and no absolute path reaches them, and that test now protects iOS too.

Time zone: Swift passes `TimeZone.current.secondsFromGMT()` into the bridge at
session open and on `NSSystemTimeZoneDidChange`, and the iOS `Clock` returns it,
rather than `time`'s `local-offset` doing a `getenv`-based lookup that is unsound
in a multithreaded process.

---

## 5. Swift/Rust bridge architecture

### 5.1 Crate layout

```
src-tauri/crates/
  ie-platform/        (+ platform/ios/, + desktop-backends feature)
  ie-core/            (unchanged)
  ie-ffi/             NEW — uniffi, staticlib + cdylib
```

`ie-ffi` is added to the existing workspace rather than a new one, so
`cargo test -p ie-ffi` runs in the same CI job and `Cargo.lock` stays single.

### 5.2 What the bridge exposes

Not 73 fine-grained commands. The Tauri surface is chatty because IPC there is
cheap and the frontend is a browser; on iOS the right granularity is
**one object plus coarse operations**, so that each call is one coordination
bracket:

```rust
#[derive(uniffi::Object)]
pub struct VaultHandle { inner: Mutex<VaultSession>, … }

#[uniffi::export]
impl VaultHandle {
    #[uniffi::constructor]
    fn open(config: HostConfig, path: String) -> Result<Arc<Self>, FfiError>;
    fn scan(&self, progress: Arc<dyn ScanProgress>) -> Result<ScanReport, FfiError>;
    fn read_note(&self, path: String) -> Result<Note, FfiError>;
    fn save_note(&self, path: String, content: String) -> Result<(), FfiError>;
    fn create_note(&self, path: String, content: String) -> Result<String, FfiError>;
    fn rename(&self, from: String, to: String) -> Result<RenameOutcome, FfiError>;
    fn plan_rename(&self, from: String, to: String) -> Result<RenamePlan, FfiError>;
    fn delete(&self, path: String) -> Result<TrashEntry, FfiError>;
    fn search(&self, query: String, options: SearchOptions) -> Result<SearchResults, FfiError>;
    fn quick_switch(&self, needle: String, limit: u32) -> Result<Vec<FileMatch>, FfiError>;
    fn backlinks(&self, path: String) -> Result<Vec<Backlink>, FfiError>;
    fn outgoing_links(&self, path: String) -> Result<Vec<ResolvedLink>, FfiError>;
    fn tags(&self) -> Result<Vec<TagSummary>, FfiError>;
    fn graph(&self, options: GraphOptions) -> Result<GraphData, FfiError>;
    fn local_graph(&self, path: String, depth: u32) -> Result<GraphData, FfiError>;
    fn set_properties(&self, path: String, properties: Vec<Property>) -> Result<(), FfiError>;
    fn apply_events(&self, events: Vec<FsEventDto>) -> Result<EventOutcome, FfiError>;
    fn journal_unsaved(&self, path: String, content: String) -> Result<(), FfiError>;
    fn recoverable(&self) -> Result<Vec<RecoveryCandidate>, FfiError>;
    fn diagnostics(&self) -> Result<Vec<Diagnostic>, FfiError>;
}
```

Roughly 25 methods. `VaultSession` already is this object; the bridge wraps it
in a `Mutex` (matching the desktop's `AppState`, which does the same) and
converts types.

### 5.3 Type marshalling

The domain types already derive `Serialize`/`Deserialize` for Tauri. Rather than
adding `uniffi::Record` derives to `ie-core` — which would put a bridge concern
inside the core and break "no bridge dependency in `ie-core`" — `ie-ffi` defines
its own `#[derive(uniffi::Record)]` mirrors and `From` conversions. That is
mechanical code, it is compile-checked in both directions, and it keeps
`ie-core`'s dependency list unchanged. A test asserts every `ie-core` public
model type has a mirror, so a new field cannot be silently dropped.

`VaultPath` crosses as `String` (it is `#[serde(transparent)]` over a `String`
already) and is re-parsed on the Rust side, which is also where its invariants
get re-checked — Swift cannot hand the core a path containing `..`.

### 5.4 Errors

`CoreError` has 15 variants and a stable `code()`. `FfiError` mirrors it as a
`#[derive(uniffi::Error)]` enum with the same variants and payloads, so Swift
gets a real `enum FfiError: Error` it can `switch` over — `case .caseCollision`,
`case .ambiguousLink(target:candidates:)` — rather than a string. §69's
"never silently overwrite" depends on the UI being able to branch on
`ExternalModification`, so a new variant is added for it.

### 5.5 Threading

`VaultHandle` is `Send + Sync` (UniFFI requires it). All bridge calls are
synchronous Rust; Swift calls them from a detached `Task` on a background
executor, never from `@MainActor`. A Swift `actor VaultService` owns the handle
and serialises access, which gives §60's "never allow concurrent writes to the
same Markdown file" structurally rather than by discipline.

Long operations (`scan`) report progress through a `ScanProgress` callback
interface, which Swift implements and forwards to a `MainActor`-isolated
observable.

### 5.6 Build

`ie-ffi` builds for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and
`x86_64-apple-ios-sim`, is lipo'd into per-platform slices, and is packaged as
`InnerEmpireCore.xcframework` by `ios/Scripts/build-core.sh`. `uniffi-bindgen`
emits `InnerEmpireCore.swift` and the module map. **The generated Swift is
checked in** so a clean checkout is readable and diffable, and CI regenerates it
and fails on a diff.

---

## 6. Sync architecture

§22–24. The plan here is deliberately conservative: **define the protocol, ship
one provider, build the conflict machinery properly, and do not write a server.**

```swift
protocol SyncProvider: Sendable {
    func connect() async throws
    func disconnect() async
    func upload(_ change: LocalChange) async throws
    func download(_ ref: RemoteRef) async throws -> RemoteFile
    func getChanges(since: SyncCursor) async throws -> [RemoteChange]
    func sync() async throws -> SyncReport
    func resolveConflict(_ c: Conflict, with: Resolution) async throws
}
```

The provider shipped first is `FileProviderSync` — which is to say, *none*: a
vault on iCloud Drive or any Files.app provider is already synced by the system,
and the app's job is to notice and to not corrupt it. That is the honest default
for v1 and it is what most users will actually use.

What that still requires, and what gets built:

- **External-change detection.** `PresenterWatcher` reports a change to an open
  note. Before any save, the app compares the file's `modified_ms` against the
  `base_modified_ms` recorded when it was opened. A mismatch means the file
  moved underneath us.
- **The three-way prompt** required by §61: *Reload*, *Keep mine*, *Compare*.
  Never a silent overwrite, in either direction.
- **A real text diff.** §23 asks for a Markdown diff. A Myers diff over lines is
  implemented in Rust (in `ie-ffi`, not `ie-core` — it is a sync concern) and
  surfaced in a Swift `ConflictView` with per-hunk Keep Local / Keep Remote /
  Merge.
- **Conflict files are never deleted.** iCloud's own `.icloud`-conflict siblings
  are surfaced in the UI as resolvable items, not hidden.

`WebDAVSync` and `S3Sync` are specified in `docs/ios/SYNC.md` as conformances to
the same protocol, with the cursor and change-detection semantics written down,
but are not implemented. §24 says not to build a server without necessity, and
there is none.

---

## 7. Testing strategy

§64–66, and the constraint stated plainly in §12 below.

### 7.1 Rust — runs everywhere, including this Linux CI

- The existing 385 `ie-core` tests are unaffected and must stay green.
- `ie-ffi` gets its own tests, run against `MemoryFileSystem` on the host
  target: every bridge method, every error mapping, and a round-trip test per
  mirrored record type.
- **The conformance corpus grows a third consumer.** `tests/fixtures/markdown/`
  is already read by `ie-core/tests/conformance.rs` and
  `src/markdown/conformance.test.ts`. A Swift `ConformanceTests` reads the same
  8 fixtures through the bridge, so if the iOS client ever disagrees with the
  desktop about what a tag is, a test says so.

### 7.2 Cross-platform vault round-trip — the §63 requirement

Two directions, both as Rust integration tests that need no Apple hardware:

1. **Desktop → iOS.** Build a vault with `StdFileSystem` (notes, wikilinks with
   aliases and headings and block refs, nested tags, typed frontmatter,
   attachments, a `.canvas`, a saved workspace). Reopen it through the
   `ie-ffi` bridge with the iOS `FileSystem`. Assert byte-identical file
   contents, identical resolved link sets, identical tag counts, identical
   property values.
2. **iOS → desktop.** The mirror: create and edit through the bridge, reopen
   with the desktop host, assert the same.

Because both hosts drive the same `ie-core`, these tests are meaningful without
a Mac: what they verify is that the *host adapters* — atomic write, path
handling, case folding, index location — do not change what lands on disk.
Case-insensitivity is simulated with `MemoryFileSystem::new(false)`, the same
technique already used to test Windows behaviour on Linux runners.

### 7.3 Swift — requires macOS

`XCTest` targets for ViewModels, navigation state, `VaultStorage` bookmark
resolution, document lifecycle; `XCUITest` for the 12 flows §66 lists (launch,
create vault, create note, edit, save, search, follow a link, backlinks,
attachments, background/foreground, external modification, rotation).

### 7.4 The 1000-note test vault

Generated by a Rust binary, `ie-ffi/examples/gen-test-vault.rs`, so it is
reproducible and diffable rather than committed as 1000 files: 1000 notes,
~4000 wikilinks with a realistic degree distribution, 60 tags including nested
ones, typed frontmatter on every note, 40 attachments, deliberate case
collisions and deliberate unresolved links. Committed as the generator plus a
manifest hash, run on demand.

---

## 8. Build strategy

```
ios/
  InnerEmpire.xcodeproj/          generated by XcodeGen from project.yml
  App/                            entry point, scene, lifecycle
  Features/{Notes,Search,Graph,Settings,Sync,Capture}/
  Editor/                         UITextView-backed Markdown editor + accessory bar
  Services/                       VaultService actor, bridge wrappers, indexing
  Storage/                        VaultStorage, bookmarks, coordination, hooks
  Extensions/{Share,Widgets,Intents}/
  Resources/                      assets, Info.plist, entitlements
  Scripts/build-core.sh           cargo → lipo → xcframework → uniffi-bindgen
  Tests/{Unit,UI}/
  project.yml                     XcodeGen spec — the project file is generated
```

**The Xcode project is generated from `project.yml` via XcodeGen.** A checked-in
`.pbxproj` is unreviewable and merge-hostile; a YAML spec is neither, and it can
be written and validated here. Signing is expressed as
`DEVELOPMENT_TEAM = $(IE_DEVELOPMENT_TEAM)` read from an untracked
`Config/Signing.xcconfig`, with `Signing.xcconfig.example` committed — §72's
"no real signing credentials in Git", satisfied structurally.

Bundle identifier, display name and API endpoints come from `.xcconfig` per
configuration (Debug / Release), not from literals in the plist.

Entitlements are the minimum §73 permits: no camera until the scanner ships and
then `NSCameraUsageDescription` only, `NSPhotoLibraryUsageDescription` for the
picker, `UIFileSharingEnabled` / `LSSupportsOpeningDocumentsInPlace` for Files
integration, an App Group for the share extension, and background modes limited
to `processing` for indexing. No background fetch, no location, no contacts, no
network entitlement beyond what a future sync provider needs.

---

## 9. Phase order

Phases follow the specification's §74, subject to its own priority order —
**data integrity → cross-platform compatibility → native iOS UX → performance →
features → polish**.

| Phase | Deliverable | Verifiable here? |
|---|---|---|
| 0 | This plan, `docs/ios/ARCHITECTURE.md`, `STORAGE.md`, `SYNC.md`, `DEVELOPMENT.md`, `TESTING.md`, `APP_STORE.md` | yes |
| 1 | `ie-platform` iOS adapter + feature gates; `ie-ffi` crate; generated Swift bindings; bridge tests | yes — `cargo test` on host |
| 2 | Cross-platform round-trip tests (§7.2); test-vault generator | yes |
| 3 | `VaultStorage`, bookmarks, coordination, document picker; app skeleton, TabView/NavigationSplitView | Swift compiles only on macOS |
| 4 | Notes list, editor, keyboard accessory, autosave, lifecycle | no |
| 5 | Wikilinks, autocomplete, backlinks, outline | no |
| 6 | Tags, Properties, recent, favourites | no |
| 7 | Search over the shared engine | no |
| 8 | Attachments, PhotosPicker, PDFKit, scanner | no |
| 9 | iPad: split view, tabs, hardware keyboard, drag & drop | no |
| 10 | Graph, touch gestures | no |
| 11 | Sync detection, conflict UI, diff | diff engine: yes |
| 12 | Share extension, widgets, App Intents | no |
| 13 | Accessibility, performance, Release config, App Store readiness | no |

---

## 10. Plugin API

§56–57. The desktop plugin API in `src/plugins/api.ts` is already
capability-based, with a `Permission` union of `vault:read`, `vault:write`,
`metadata:read`, `workspace`, `ui`, `commands`, `settings`, `network`. That
model transfers; the *host* does not, because the desktop host executes
JavaScript, and App Store rules (2.5.2) forbid downloading and executing code.

The plan:

- Lift the permission vocabulary into a shared `CorePluginAPI` description —
  a JSON manifest schema, checked into `docs/plugin-api/`, that both hosts read.
- `DesktopPluginCapabilities` — the existing JS host, unchanged.
- `iOSPluginCapabilities` — a **declarative** host: plugins ship as manifests
  describing commands, templates, property schemas and view contributions, with
  no executable code. Anything needing real logic is a compiled-in extension
  point, not a download.
- `network` is absent from `iOSPluginCapabilities` entirely.

This is written up in Phase 12, not before. It is explicitly not a port.

---

## 11. Privacy

§46, and it is already structurally true rather than promised: `ie-core` has no
network dependency, there is no telemetry client anywhere in the workspace, and
the index moving to `Library/Caches` (§4.5) means note content cannot reach
iCloud even incidentally. The iOS target adds no networking library. If
analytics are ever added they are off by default, and the absence is asserted by
the same CI grep that already proves `ie-core` has no network client.

---

## 12. What this environment can and cannot verify

Stated plainly, because the specification asks for a build after each phase and
says the Simulator alone is not sufficient (§67).

This session runs on `x86_64-unknown-linux-gnu` with no Swift toolchain and no
Xcode:

```
$ which swift swiftc xcodebuild   →  none present
$ rustup target list --installed  →  x86_64-unknown-linux-gnu
```

**Can be genuinely built and tested here:** the `ie-platform` changes, the whole
`ie-ffi` crate and its tests, the UniFFI binding *generation* (`uniffi-bindgen`
is a pure-Rust program that emits Swift source as text and runs fine on Linux),
the cross-platform round-trip tests, the test-vault generator, the diff engine,
and every existing Rust and TypeScript suite.

**Cannot be built or tested here:** anything requiring `swiftc`, `xcodebuild`,
the iOS SDK, the Simulator, or a device. That includes compiling the generated
bindings, the app itself, and the XCTest/XCUITest targets. Cross-compiling
`ie-ffi` to `aarch64-apple-ios` also fails here, because `libsqlite3-sys`
bundles C that needs the iOS sysroot.

Consequently: Swift code will be written to compile, `project.yml` and the build
script will be written to work, and `docs/ios/DEVELOPMENT.md` will document
exactly what to run on a Mac to verify it — but **no claim will be made in any
report that an iOS build or an iOS test passed**, because none will have been
run. Phases 1 and 2, which are Rust, will be reported with real test counts.
Phases 3 onward will be reported as written-but-unverified, per phase, with the
verification steps a Mac needs.

---

## 13. Risks

| Risk | Mitigation |
|---|---|
| `ie-core` needs changes after all | Phase 1 ends with `git diff --stat src-tauri/crates/ie-core` expected empty. If it is not, the change and its justification go in this document before it is made. |
| iCloud placeholder handling is subtler than §4.2 assumes | The `MaterializeHook` boundary is one method; widening it is a contained change. `STORAGE.md` records the exact failure modes to test on a device. |
| UniFFI record mirroring drifts from `ie-core` | A test enumerates `ie-core`'s public model types and fails when one has no mirror. |
| `.xcframework` build only works on macOS | The script is written and documented; CI gains a macOS job that is the first thing to run once Apple hardware is available. |
| Bridge granularity turns out wrong for some screen | The bridge is additive; a screen needing a finer call gets one, and the coordination bracket moves with it. |

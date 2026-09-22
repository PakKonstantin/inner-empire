# Architecture

**Inner Empire** is a local-first knowledge management platform. It treats a
plain folder of Markdown files as the single source of truth and builds every
higher-level capability — links, backlinks, tags, properties, search, graph —
as a *derived, disposable* index on top of that folder.

This document describes the system as designed, the reasoning behind the major
decisions, and the invariants every contributor must preserve.

---

## 1. Guiding invariants

These are non-negotiable. Everything else in the codebase is an implementation
detail that may change.

| # | Invariant | Enforced by |
|---|---|---|
| I1 | The Markdown file on disk is the only source of truth. | `ie-core::index` never writes to the vault; only `ie-core::vault` does. |
| I2 | Deleting the index must never lose user data. Full functionality is restored by a rescan. | `index::rebuild`, integration test `index_rebuild_is_lossless`. |
| I3 | A vault is byte-identical and fully functional when moved between Windows and Linux. | `VaultPath` (§4), portable JSON workspace files, no absolute paths stored in the vault. |
| I4 | A crash, power loss or kill signal must never truncate or corrupt a user's note. | Atomic write protocol (§7). |
| I5 | Core logic contains zero platform-conditional code. | `ie-core` has no `#[cfg(windows)]`/`#[cfg(unix)]`; adapters live in `ie-platform`. |
| I6 | Nothing leaves the machine unless the user explicitly asks. | No network client in `ie-core`; Tauri CSP denies remote origins; no telemetry. |
| I7 | No silent failures. Every fallible operation returns a typed error that reaches the UI. | `ie-core::error::CoreError`, `Result` everywhere, no `unwrap()` on I/O paths. |

---

## 2. Layering

```
┌──────────────────────────────────────────────────────────────┐
│  Presentation            React 19 + TypeScript (strict)      │
│  src/app  components  editor  explorer  graph  canvas  …     │
└───────────────────────────┬──────────────────────────────────┘
                            │  typed IPC facade (src/services/api.ts)
                            │  — the ONLY place `invoke()` is called
┌───────────────────────────┴──────────────────────────────────┐
│  Application / IPC       src-tauri  (thin)                   │
│  command handlers · app state · event emission · adapters    │
└───────────────────────────┬──────────────────────────────────┘
                            │  plain Rust function calls
┌───────────────────────────┴──────────────────────────────────┐
│  CORE                    crates/ie-core   (no Tauri, no GUI) │
│                                                              │
│  vault · markdown · links · metadata · index · search        │
│  graph · watcher · workspace · templates · trash · export    │
└───────────────────────────┬──────────────────────────────────┘
                            │  trait objects only
┌───────────────────────────┴──────────────────────────────────┐
│  Platform abstractions   crates/ie-platform                  │
│  FileSystem PathResolver FileWatcher SystemDialog Clipboard  │
│  ShellIntegration ProcessManager Clock AppDirs               │
│        ┌───────────────┬───────────────┬──────────────┐      │
│        │  linux/       │  windows/     │  (macos/)    │      │
│        └───────────────┴───────────────┴──────────────┘      │
└──────────────────────────────────────────────────────────────┘
```

### Why a separate `ie-core` crate rather than modules inside `src-tauri`?

The brief requires the core to be reusable for a future CLI, MCP server,
headless indexer, RAG pipeline or mobile client. If the core lives inside the
Tauri binary crate it can only ever be linked by that binary, and every core
test pays for Tauri's build. As a library crate with **zero Tauri
dependencies**, `ie-core`:

* compiles and tests in ~seconds with plain `cargo test -p ie-core`,
* can be depended on by a future `ie-cli`, `ie-mcp` or `ie-server` crate,
* makes the "core must not know about the UI" rule a *compile-time* guarantee
  rather than a code-review convention.

The same reasoning produced `ie-platform`: because `ie-core` depends only on
its traits, a test can inject an in-memory filesystem and a fake clock, and the
absence of `#[cfg(windows)]` inside `ie-core` is mechanically checkable (there
is a test that greps for it).

---

## 3. Crate and package layout

```
inner-empire/
├── package.json                  pnpm workspace root
├── vite.config.ts
├── index.html
│
├── src/                          React / TypeScript frontend
│   ├── app/                      root component, providers, layout, routing
│   ├── components/               reusable presentational primitives
│   ├── editor/                   CodeMirror 6 integration, extensions
│   ├── markdown/                 remark pipeline, renderer, TS AST types
│   ├── explorer/                 virtualized file tree
│   ├── search/                   search panel + query input
│   ├── graph/                    global + local graph, force simulation
│   ├── canvas/                   canvas board
│   ├── workspace/                tabs, panes, layout state
│   ├── commands/                 command registry, palette, hotkeys
│   ├── settings/                 settings UI + schema
│   ├── plugins/                  plugin host, sandbox, public API surface
│   ├── services/                 IPC facade, event bus, logger
│   ├── state/                    zustand stores
│   ├── types/                    shared domain types (mirror of Rust)
│   └── styles/                   CSS variables, themes, layout
│
├── src-tauri/                    Tauri desktop shell (thin)
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs                builder, plugin registration
│   │   ├── state.rs              AppState: Mutex<Option<VaultSession>>
│   │   ├── events.rs             core event → Tauri event bridge
│   │   └── commands/             one module per subsystem, all #[tauri::command]
│   ├── crates/
│   │   ├── ie-core/              ← the platform (no Tauri)
│   │   └── ie-platform/          ← OS abstractions + adapters
│   ├── Cargo.toml                workspace manifest
│   └── tauri.conf.json
│
├── tests/                        Playwright E2E + shared fixtures
├── docs/                         user + developer documentation
└── .github/workflows/            CI matrix: ubuntu-latest, windows-latest
```

### `ie-core` module map

```
ie-core/src/
├── lib.rs
├── error.rs          CoreError, Diagnostic, Result — typed, with stable codes
├── session.rs        VaultSession: the application's unit of work
├── events.rs         CoreEvent, EventSink
├── recovery.rs       the unsaved-work journal (§7)
├── model/
│   ├── mod.rs        FileKind Link Backlink Tag Heading Block Note Graph…
│   ├── property.rs   PropertyValue and its typing rules (§6)
│   └── workspace.rs  the pane tree, tabs, sidebars
├── vault/
│   ├── path.rs       VaultPath (§4)
│   ├── settings.rs   per-vault settings, stored in the vault
│   ├── fileops.rs    create/rename/move/delete, case-collision refusal
│   └── trash.rs      the in-vault trash with its manifest (§8)
├── markdown/
│   ├── parser.rs     MarkdownParser: CommonMark walk, then extension scan
│   ├── scanner.rs    wiki links, embeds, tags, block identifiers
│   ├── frontmatter.rs  YAML → PropertyValue, and surgical writing back
│   ├── text.rs       LineIndex and ExclusionZones
│   ├── render.rs     MarkdownRenderer: source → HTML, for export
│   └── transform.rs  MarkdownTransformer: byte-range edits back to source
├── links/
│   ├── reference.rs  the `[[Note#H^b|alias]]` grammar, in one place
│   ├── resolver.rs   the typed resolution surface the app calls
│   └── rename.rs     rename planning and link rewriting (§47 of the brief)
├── index/
│   ├── schema.rs     DDL, pragmas, migration-by-rebuild
│   ├── db.rs         opening, integrity checking, discarding and recreating
│   ├── resolve.rs    the link-resolution ranking policy
│   ├── writer.rs     one file's derived rows, inside a transaction
│   ├── indexer.rs    full scan, incremental update, watcher events
│   └── queries.rs    backlinks, tags, outline, graph, diagnostics
├── search/
│   ├── query.rs      the query language and its SQL
│   ├── engine.rs     FTS5 execution, ranking, snippets, quick switch
│   └── fuzzy.rs      filename matching for the quick switcher
├── templates/
│   ├── mod.rs        variable expansion, daily notes
│   └── datefmt.rs    the small date-format language
├── workspace/mod.rs  workspace persistence (portable JSON)
├── export/mod.rs     HTML and Markdown export, import
└── logging/mod.rs    rotating file logs, never the UI
```

There is no `graph/` module: graph construction is two queries in
`index/queries.rs`, because a graph is a projection of the links table and
giving it its own module would have meant moving the SQL away from the schema
it depends on. There is no `watcher/` module either: normalising the backends'
event streams is the platform layer's job, and coalescing them is four lines
in `index/indexer.rs`.

## 4. `VaultPath` — the cross-platform keystone

Every derived record, link, tab, workspace entry and canvas node refers to
files by `VaultPath`, never by an OS path.

```rust
/// A location inside a vault.
///
/// Invariants (checked at construction, upheld by every constructor):
///   * relative to the vault root — never absolute, no drive prefix
///   * separated by '/' on every platform
///   * no '.' or '..' components
///   * no empty components, no trailing slash
///   * NFC-normalized
pub struct VaultPath(String);
```

Consequences:

* **Nothing in the codebase splits a path on `'\\'` or `'/'`.** Conversion to
  and from `std::path::Path` goes through `Components`, so Windows `\` and
  POSIX `/` are both handled by the standard library.
* The SQLite index, `workspace.json`, `.canvas` files and every link stored on
  disk contain only `/`-separated relative paths, so a vault copied from
  `C:\Users\me\Vault` to `/home/me/Vault` needs no rewriting (I3).
* Escaping the vault root is impossible by construction, which is also the
  sandbox boundary for plugins.

### Case sensitivity

`ext4` distinguishes `MyNote.md` from `mynote.md`; NTFS normally does not. The
vault therefore tracks both the exact path and a case-folded key:

* `files.path` — exact, as it exists on disk.
* `files.path_fold` — `path.to_lowercase()`, indexed.

At scan time, two distinct `path` values sharing a `path_fold` raise a
`Diagnostic::CaseConflict`, surfaced in the UI as a warning with a rename
affordance. Link resolution tries an exact match first, then a unique
case-insensitive match; a case-insensitive match with more than one candidate
resolves to `Ambiguous` and is reported rather than guessed. File creation
refuses a name that folds onto an existing file, so a vault authored on Linux
cannot silently break when opened on Windows.

### Reserved names and illegal characters

`vault::path::validate_filename` rejects the union of Windows and POSIX
restrictions on every platform — `< > : " | ? * \ /`, control characters,
trailing dot or space, and the reserved device names (`CON`, `PRN`, `AUX`,
`NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`). Applying the stricter rule everywhere is
what makes a Linux-authored vault portable to Windows.

---

## 5. Markdown: one semantic parser, two renderers

The brief forbids regex-driven Markdown handling (§51) and requires a
parser/renderer/transformer abstraction. Two runtimes need Markdown, for
different reasons:

| Consumer | Needs | Implementation |
|---|---|---|
| Indexer (Rust) | links, tags, headings, blocks, frontmatter — fast over 50k files | `pulldown-cmark` events + `ie-core::markdown::extract` |
| Editor & preview (TS) | rendered HTML, syntax decorations, live preview | `remark`/`unified` + custom micromark extensions |

Two parsers means a divergence risk, so the *semantic* surface — what counts as
a wikilink, a tag, a block id, an embed — is pinned by a **shared conformance
corpus** at `tests/fixtures/markdown/`. Each fixture is a `.md` input plus a
`.expected.json` of extracted entities. The Rust suite and the Vitest suite
both run the same corpus and must produce identical output. A change to the
extension grammar that is implemented in only one language fails CI.

Authority is unambiguous: the **Rust extraction is authoritative** for
everything persisted (index, backlinks, graph, rename propagation). The TS
parser is display-only. Where they could disagree, the UI asks the core.

### App-level Markdown extensions

```
[[Note]]                      wikilink
[[Note|Display]]              aliased
[[Note#Heading]]              heading anchor
[[Note#Heading|Display]]
[[Note#^block-id]]            block reference
![[Note]]                     embed (transclusion)
![[Note#Heading]]             partial embed
![[image.png|400]]            sized attachment embed
#tag  #nested/tag             tags (not inside code or links)
^block-id                     trailing block identifier
---\n…\n---                   YAML frontmatter (first bytes only)
```

Embeds are expanded with a depth cap and an ancestor set, so `A` embedding `B`
embedding `A` renders a "circular embed" placeholder rather than hanging.

---

## 6. Properties (frontmatter) typing

YAML frontmatter is parsed into a typed `PropertyValue`:

```rust
enum PropertyValue {
    Text(String), Number(f64), Bool(bool),
    Date(NaiveDate), DateTime(DateTime<FixedOffset>),
    List(Vec<PropertyValue>), Object(BTreeMap<String, PropertyValue>),
    Null,
}
```

Scalars are classified by shape: `YYYY-MM-DD` → `Date`, RFC 3339 →
`DateTime`, otherwise the YAML-native type. The index stores a flattened
`(file, key, kind, text, num, list_index)` row per scalar so property queries
(`status:active`, `rating>7`) become ordinary SQL with an index on
`(key, text)` and `(key, num)`.

Writing properties back is **surgical**: `MarkdownTransformer` replaces only
the byte range of the frontmatter block, leaving the body untouched byte for
byte (I1). Key order and comments in the untouched portion are preserved.

---

## 7. Durability: the atomic write protocol

Every write to a user file follows the same sequence in
`vault::fileops::write_atomic`:

1. Write bytes to `<dir>/.<name>.ie-tmp-<pid>-<counter>`.
2. `flush()` then `sync_all()` the temp file.
3. `fs::rename(temp, target)` — atomic replacement on both NTFS and ext4/btrfs
   (Rust maps this to `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` on Windows).
4. Ask the platform adapter to `sync_dir(parent)`; the Linux adapter opens the
   directory and `fsync`s it, the Windows adapter is a documented no-op because
   NTFS metadata ordering already guarantees the rename is durable.

A reader therefore only ever observes the complete old content or the complete
new content (I4). Leftover `.ie-tmp-*` files from a crash are detected at vault
open, reported as an interrupted write, and offered for recovery rather than
being deleted silently.

Autosave in the UI debounces 800 ms after the last keystroke and also flushes
on blur, tab switch and window close. Unsaved buffers are additionally
journalled to `<appdata>/recovery/<vault-id>/<hash>.json` every 5 s so a hard
kill loses at most a few seconds of typing; the journal is replayed at next
open and offered as a diff.

---

## 8. Trash

Deletion never calls `remove_file` on a user file by default. Instead the file
moves to `.inner-empire/trash/` with a manifest entry:

```json
{ "id": "01J…", "originalPath": "Notes/Idea.md",
  "trashedAt": "2026-09-17T10:31:02Z", "storedAs": "01J….md", "kind": "file" }
```

Restore puts it back at `originalPath` (or a de-duplicated sibling name if
something now occupies that path). Permanent delete is an explicit, confirmed
action. Because the trash is inside the vault it is portable, needs no OS
integration, and works identically on GNOME, KDE and Windows — the brief
explicitly rules out depending on the Recycle Bin or a desktop-specific trash
spec from core (§36). Optional OS-trash integration can later be added as a
`ShellIntegration` method without touching core.

---

## 9. Index design

SQLite (via `rusqlite`, `bundled` feature so no system library is required) at
`<vault>/.inner-empire/index.db`. It is a **cache**: deleting it costs a
rescan, nothing else (I2).

```sql
files(id, path, path_fold, name, name_fold, ext, kind, size, mtime_ms,
      content_hash, indexed_at, frontmatter_json)
headings(file_id, level, text, slug, line, byte_start, ordinal)
blocks(file_id, block_id, line, byte_start, byte_end)
links(file_id, kind, raw, target_text, target_file_id, heading, block,
      alias, line, byte_start, byte_end, resolved)
tags(file_id, tag, tag_fold, line, byte_start)
properties(file_id, key, kind, text_value, num_value, list_index)
notes_fts(path, title, body)          -- FTS5, external-content
meta(key, value)                       -- schema_version, vault_id, …
```

Performance-critical indexes: `files.path_fold` (unique), `files.name_fold`,
`links.target_file_id` (backlinks), `links.target_text` + `resolved=0`
(unresolved links), `tags.tag_fold`, `properties(key, text_value)`,
`properties(key, num_value)`.

### Incremental indexing

* Initial scan walks the vault, skipping `.inner-empire/`, dotfiles and
  user-configured ignores, and indexes in batches inside one transaction per
  batch so progress is observable and a failure mid-scan is not corrupting.
* A file is re-indexed only when `(size, mtime_ms)` differs from the stored
  row; a content hash settles the ambiguous case where mtime granularity hides
  a change.
* The watcher coalesces events per path over a 150 ms debounce window, so the
  five inotify events an editor emits for one save become one re-index.
* Rename/move is detected by matching a delete and a create with an identical
  content hash in the same debounce window, so a move re-parents the row
  instead of destroying and recreating it (which would drop its backlinks for a
  frame).
* Indexing runs on a dedicated worker thread with a bounded channel; the UI
  thread never blocks on it and receives `IndexProgress` events.

### Corruption handling

If SQLite reports corruption or the schema version is unreadable, the index
file is renamed aside and rebuilt from the vault (I2, §54). The user sees a
notification, never a crash.

---

## 10. Search

A hand-written query parser (`search::query`) produces:

```
Query { terms: Vec<Term>, filters: Vec<Filter>, mode: And | Or }

Term   ::= Word(String) | Phrase(String) | Not(Box<Term>)
Filter ::= Tag(String)        # tag:AI, tag:AI/LLM (prefix-matches children)
         | Path(String)       # path:Projects
         | File(String)       # file:readme
         | Prop(key, Op, val) # status:active, rating>7
         | Ext(String)        # ext:md
         | Section(String)    # section:"Design notes"
```

Execution: filters narrow a candidate set with indexed SQL, then FTS5 `MATCH`
ranks the remainder with `bm25()`, then snippets come from `snippet()`. A
filename-only query additionally runs the fuzzy matcher for the quick switcher.
Both paths are capped and paginated so a 50k-note vault returns the first page
in tens of milliseconds.

---

## 11. Frontend architecture

* **State**: `zustand` stores split by concern — `vaultStore`, `workspaceStore`,
  `editorStore`, `settingsStore`, `indexStore`. Note *content* is never held in
  a global store; only the open buffers are, keyed by tab.
* **IPC**: every `invoke()` lives in `src/services/api.ts`, typed against
  `src/types/`, which mirrors the serde types in `ie-core::model`. Nothing else
  in the app imports from `@tauri-apps/api/core`. That single choke point is
  what makes a future web or mobile transport a drop-in replacement.
* **Virtualization**: the explorer, search results and backlink lists render
  through a windowing hook, so a 50k-file vault costs a constant number of DOM
  nodes (§38).
* **Editor**: CodeMirror 6. Live preview is implemented as a `ViewPlugin` that
  decorates the *source* document — concealing markup on lines the cursor is
  not on — rather than swapping in a separate rendered view. Cursor position,
  undo history and selection therefore stay in the real document, which is what
  makes cursor behaviour sane (§7 of the brief).
* **Themes**: a flat set of CSS custom properties on `:root`, overridden under
  `[data-theme="dark"]`. A user theme is a CSS file dropped in
  `<vault>/.inner-empire/themes/`; it only ever redefines variables.

---

## 12. Plugin system

Plugins are JavaScript modules loaded from
`<vault>/.inner-empire/plugins/<id>/main.js` with a `manifest.json`.

```ts
interface Plugin {
  onload(ctx: PluginContext): void | Promise<void>;
  onunload?(): void | Promise<void>;
}

interface PluginContext {
  app: AppApi;            // vault, metadata, workspace, ui, commands, events
  manifest: PluginManifest;
  settings: PluginSettings;
  register(disposable: Disposable): void;  // auto-cleanup on unload
}
```

Isolation, in order of strength:

1. **Capability-based API.** A plugin receives only the object graph handed to
   `onload`. Vault access goes through a `VaultApi` bound to `VaultPath`, so a
   plugin cannot address anything outside the vault.
2. **Declared permissions.** The manifest lists `vault:read`, `vault:write`,
   `ui`, `commands`, `network`. Un-declared calls throw. `network` is denied by
   default and requires explicit user consent (I6).
3. **Error containment.** Every plugin entry point is wrapped; a throwing
   plugin is disabled with a notification and never takes the app down (§54).
4. **Lifecycle ownership.** Everything registered through `ctx.register` is
   disposed on unload, so plugins are hot-reloadable.

Full isolation (separate realm / worker) is the documented next step; the API
is already asynchronous everywhere so moving the host into a Worker is a
transport change, not a redesign.

---

## 13. Events

`ie-core::events::CoreEvent` is the canonical event type. `src-tauri/events.rs`
bridges it to the webview; `src/services/events.ts` re-emits it on the frontend
bus, which the plugin `EventBus` subscribes to.

```
vaultOpened vaultClosed
fileCreated fileModified fileDeleted fileRenamed
indexProgress indexCompleted indexError
activeFileChanged workspaceChanged
```

---

## 14. Configuration and data locations

Resolved by `ie-platform::AppDirs`, which wraps the `directories` crate.

| Purpose | Linux (XDG) | Windows |
|---|---|---|
| Settings, recent vaults | `~/.config/inner-empire/` | `%APPDATA%\InnerEmpire\config\` |
| Recovery journal, plugin registry | `~/.local/share/inner-empire/` | `%APPDATA%\InnerEmpire\data\` |
| Logs | `~/.local/share/inner-empire/logs/` | `%LOCALAPPDATA%\InnerEmpire\logs\` |
| Scratch cache | `~/.cache/inner-empire/` | `%LOCALAPPDATA%\InnerEmpire\cache\` |

Per-vault state that must travel with the vault (workspace layout, canvas,
templates config, index) lives in `<vault>/.inner-empire/` instead.

A `portable.txt` marker file next to the executable redirects all four to
`./data/`, so the app can run from a USB stick and leave nothing behind. It is
not Windows-specific.

---

## 15. Build and packaging strategy

| Target | Artifact | Toolchain |
|---|---|---|
| Linux x86_64 | `.AppImage`, `.deb`, `.rpm`, `.desktop` entry | `ubuntu-22.04` runner (oldest glibc we support), `libwebkit2gtk-4.1`, `libgtk-3` |
| Windows x86_64 | NSIS `.exe` installer, `.msi` | `windows-latest`, WebView2 bootstrapper (evergreen) |

CI runs a matrix of `ubuntu-latest` and `windows-latest` for
`cargo test --workspace`, `pnpm test`, `pnpm typecheck`, `pnpm lint` and
Playwright E2E; packaging runs on tags. Tauri's updater plugin is configured
but points at no endpoint, so auto-update is an endpoint + signing-key change
rather than an architectural one (§57).

Linux desktop integration is done through the freedesktop `.desktop` entry and
icon theme spec, which GNOME and KDE Plasma both consume — no
desktop-environment-specific code.

---

## 16. What is deliberately not built yet

Named here so each is a decision rather than an omission.

* **macOS.** The platform split and every abstraction are in place, and adding
  it means a `platform/macos` module and one arm in `platform::current()`.
  There is no adapter, no CI leg and no bundle.

* **An update server.** The client architecture accommodates one — packaging is
  already per-platform and signing keys are a configuration change — but no
  endpoint exists and nothing checks for updates. Building the client half
  against a server that does not exist would be inventing an interface.

* **Plugin realm isolation.** Capability-based access, declared permissions and
  error containment are implemented and tested. A plugin still runs in the same
  JavaScript context as the application, so the containment is real but is not
  a sandbox. `PLUGIN_API.md` says so plainly rather than implying more. The API
  is asynchronous throughout, so moving the host into a Worker is a transport
  change rather than a redesign.

* **Sync and collaboration.** The core exposes a content hash per file and an
  event stream, which is what a later sync engine would need. Nothing more.

* **AI, retrieval and embeddings.** `ie-core` is the natural host: it already
  owns chunkable structure — headings, blocks, properties — and a per-file
  content hash that makes incremental embedding possible. No code yet, and no
  half-built scaffolding pretending otherwise.

## 17. Where the seams are

Three places in this codebase are load-bearing, in the sense that getting them
wrong would be expensive to discover later. They are worth knowing before
changing anything.

**`VaultPath`** is why a vault opens identically on both platforms. Every
persisted reference goes through it, and its constructors are the only place
the invariants are established. Adding a way to build one without validation
would quietly remove the guarantee.

**The index-as-cache rule** is what makes every recovery path safe: a corrupt
index is thrown away, a schema change is a rebuild, and a bug in the indexer
costs a rescan rather than data. Storing anything in SQLite that is not also in
a Markdown file would end that, and an integration test exists to catch it.

**The two Markdown implementations** are held together by the conformance
corpus and nothing else. A construct added to one and not the other is a bug
that shows up as a link the index knows about and the editor does not, or the
reverse. Add the fixture first.

# iOS architecture

Companion to the root `ARCHITECTURE.md`, which describes the desktop. This
document covers only what is different on iOS. Where it is silent, the desktop
document applies — because the layer it describes is the same code.

## 1. The shape

```
             ┌──────────────────────────────────────────────┐
             │  SwiftUI views                               │
             │  Notes · Search · Graph · Settings · Capture  │
             └───────────────────┬──────────────────────────┘
                                 │  @Observable view models, @MainActor
             ┌───────────────────▼──────────────────────────┐
             │  iOS application layer                        │
             │  VaultService (actor) · navigation · editor   │
             │  VaultStorage · bookmarks · coordination      │
             └───────────────────┬──────────────────────────┘
                                 │  generated Swift bindings
             ┌───────────────────▼──────────────────────────┐
             │  ie-ffi  (UniFFI)                             │
             │  VaultHandle · record mirrors · FfiError      │
             └───────────────────┬──────────────────────────┘
                                 │  Rust
             ┌───────────────────▼──────────────────────────┐
             │  ie-core                                      │
             │  markdown · links · index · search · vault    │
             │  workspace · templates · recovery             │
             └───────────────────┬──────────────────────────┘
                                 │  Arc<dyn FileSystem> etc.
             ┌───────────────────▼──────────────────────────┐
             │  ie-platform :: platform::ios                 │
             │  CoordinatedFileSystem · ContainerDirs        │
             │  PresenterWatcher · IosPlatform               │
             └──────────────────────────────────────────────┘
```

The relationship to the desktop is horizontal, not vertical:

```
                    ┌─ Windows UI (React + Tauri)
  Shared core ──────┼─ Linux UI   (React + Tauri)
  (ie-core)         └─ iOS UI     (SwiftUI + UniFFI)
```

No SwiftUI view has a React ancestor. No React component has an iOS descendant.
The only thing they share is the core and the design token vocabulary.

## 2. Invariants

The desktop's seven invariants (I1–I7) hold unchanged, because they are
properties of `ie-core` and `ie-core` is the same code. Three iOS-specific ones
are added:

**I8 — the Core never learns about UIKit or SwiftUI.**
`ie-core` depends on `ie-platform` traits. `ie-platform::platform::ios` depends
on `libc` and on two callback traits. Neither links against any Apple framework;
the Objective-C calls are all on the Swift side of the bridge. Checked the same
way I5 is: CI greps `ie-core` for platform conditionals and finds none.

**I9 — no absolute path is ever persisted.**
Vault location is a security-scoped bookmark in `UserDefaults`. Everything
written *into* the vault — `vault.json`, `workspace.json`, `.canvas` — contains
only `VaultPath` values. The desktop already has the test that proves this; on
iOS it also protects against a container path that iOS reassigns between
launches.

**I10 — an external change is never overwritten silently.**
Every save compares the file's current `modified_ms` against the value recorded
when the buffer was opened. A mismatch raises `FfiError::ExternalModification`
and the UI must resolve it. There is no code path from "user taps Save" to
"bytes replaced" that skips this check.

## 3. Concurrency model

```
  @MainActor                    actor VaultService              Rust
  ─────────                     ──────────────────              ────
  view models         ──await──▶ serialised access  ──sync──▶  VaultHandle
  navigation state               to VaultHandle                 (Mutex<VaultSession>)
  editor buffer
```

`VaultService` is a Swift `actor`. Every bridge call goes through it, so two
saves to the same note cannot interleave — §60's requirement is met by the type
system rather than by a lock the caller has to remember.

Bridge calls are synchronous Rust executed on a cooperative-thread-pool task.
Nothing touching the filesystem runs on `@MainActor`. A full scan runs as a
detached `Task` reporting progress through the `ScanProgress` callback, which
hops back to `@MainActor` to update the observable.

The editor buffer is the one piece of state that lives on the main actor and is
not owned by `VaultService`: `UITextView` requires it. Autosave copies the
string and hands it to the actor.

## 4. Navigation

**iPhone** — `TabView` with four tabs, per §27's warning not to overload it:

| Tab | Root |
|---|---|
| Notes | `NavigationStack` over the file tree; notes push |
| Search | search field + results; a result pushes the note |
| Graph | full-vault graph; a node pushes the note |
| Settings | grouped form |

Backlinks, outline and properties are sheets or pushed destinations, not a
fifth tab.

**iPad** — `NavigationSplitView`:

- two columns under ~1000pt: Explorer | Editor
- three columns above: Explorer | Editor | Inspector (backlinks + outline)

Tabs exist on iPad (§34) as a document tab bar above the editor column. iPhone
has no tab bar; the navigation stack is the history.

## 5. What is deliberately not ported

**The command palette.** §55 says not to port it literally, and the reason is
sound: on desktop it exists because there is a keyboard and no room for 200
buttons. On iPhone the equivalent affordances are the search tab, context menus
and swipe actions. On iPad with a hardware keyboard, ⌘⇧P opens a command list —
but it is built on `UIKeyCommand` and the menu system, not on a re-implementation
of the desktop palette component.

**The plugin runtime.** §56. See `IMPLEMENTATION_PLAN.md` §10.

**Split panes as a general tree.** The desktop's `PaneNode` binary tree is
overkill for a screen that shows at most two documents. iOS reads and writes
`workspace.json` faithfully — a vault edited on iPhone and reopened on Windows
must not lose its layout — but the iOS UI only ever expresses the subset it can
display, and preserves the rest untouched.

## 6. Where the seams are

- **`ie-platform::FileSystem`** — the whole port hinges on this trait. If a
  future platform arrives, it implements this and nothing else changes.
- **The index-as-cache invariant** — it is what makes moving the index out of
  the vault (`STORAGE.md` §3) safe rather than a data-model change.
- **`VaultPath`** — every path that crosses the bridge is one of these, as a
  string, re-validated on arrival. Swift cannot construct a path the core will
  accept without going through the parser.
- **The conformance corpus** — `tests/fixtures/markdown/` now has three readers
  (Rust, TypeScript, Swift). It is the thing that makes "one note format" a
  test rather than a promise.

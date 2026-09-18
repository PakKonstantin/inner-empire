# iOS storage

How the app reaches a vault it does not own, keeps it intact, and stays inside
the sandbox.

## 1. Two kinds of location

| | Vault | App container |
|---|---|---|
| Owned by | the user, possibly a file provider | the app |
| Reached by | security-scoped bookmark | `FileManager.urls(for:in:)` |
| Contains | `.md`, attachments, `.canvas`, `.inner-empire/{vault.json,workspace.json,trash}` | index cache, logs, recovery journal, settings |
| Survives reinstall | yes | no |
| Syncs | possibly, by the system | never |
| Is the source of truth | **yes** | no |

Everything in the right column is regenerable. Losing it costs time, never data.

## 2. Getting access

### 2.1 First run

`fileImporter(isPresented:allowedContentTypes: [.folder])` presents
`UIDocumentPickerViewController` in folder mode. The user picks any directory —
On My iPhone, iCloud Drive, a third-party provider. There is no import step and
no proprietary container; a folder of Markdown files is a vault, which is the
same premise the desktop has.

The app immediately:

1. calls `startAccessingSecurityScopedResource()` on the returned URL,
2. creates `bookmarkData()` and stores it in `UserDefaults` under the vault id,
3. opens the vault through the bridge, which writes `.inner-empire/vault.json`
   if absent and mints the vault id,
4. re-keys the bookmark under that id.

The absolute path is never stored. Only the bookmark.

### 2.2 Subsequent launches

```swift
var stale = false
let url = try URL(resolvingBookmarkData: data,
                  bookmarkDataIsStale: &stale)
guard url.startAccessingSecurityScopedResource() else { throw .accessDenied }
defer { url.stopAccessingSecurityScopedResource() }
if stale { save(try url.bookmarkData()) }
```

`isStale` is normal, not an error: it happens when the file moves, when the
provider re-issues its identifiers, and after some OS updates. It is re-resolved
and re-saved, and the session then issues an `FsEvent::Rescan` because anything
could have changed while the app was not running.

If resolution fails outright — the folder was deleted, the provider was signed
out — the app says so and offers to pick again. It does **not** silently fall
back to a copy or to a default location.

### 2.3 Balanced access

Every `start` has a matching `stop`. `VaultService` holds exactly one active
access for the open vault, taken at open and released at close, rather than
one per operation — nesting `start` calls is legal but the count must balance,
and a single owner is easier to prove correct than a count spread across
call sites. Access is released in `scenePhase == .background` if the app is
about to be suspended for a long time and re-taken on `.active`.

## 3. Where the index lives, and why not in the vault

Desktop: `<vault>/.inner-empire/index.db`.
iOS: `Library/Caches/index/<vault-id>.db`.

This is the one on-disk divergence between the platforms, and it is deliberate:

1. **SQLite needs POSIX locking and sibling files.** `-wal` and `-shm` live
   beside the database, and SQLite takes advisory locks on it. A File Provider
   directory guarantees neither. A vault opened on both a Mac and an iPhone
   through the same iCloud folder would have two processes contending for one
   database file through a sync layer that does not understand locks.
2. **§46, privacy.** The FTS5 table contains note *content*. An index inside an
   iCloud-synced vault would upload every note's text a second time, in a form
   the user did not choose. Outside the vault, it cannot leave the device.
3. **§37, launch cost.** `Library/Caches` persists across launches, so a warm
   start is incremental. It is purgeable under storage pressure, which is
   correct for a cache and which the app detects and handles.
4. **`index/db.rs` uses `std::fs` directly** to create the parent directory and
   to discard a corrupt file — outside the `FileSystem` abstraction, and so
   outside any coordination the vault would need.

**Why this is safe rather than a compatibility break:** the index is a cache and
nothing else. `ie-core`'s invariant I2 — deleting the index loses nothing, a
rescan reproduces it exactly — is enforced by a test that snapshots derived
state, drops every row, rescans, and compares. A vault carried from Windows to
iPhone simply gets indexed on first open; a vault carried back is unaffected,
because nothing the desktop needs was ever in the iOS cache.

The desktop is unchanged. `.inner-empire/index.db` written by Windows is ignored
by iOS and vice versa; neither reads the other's.

## 4. Coordination

### 4.1 The rule

**Security scoping grants access. Coordination picks the moment.** They are
separate mechanisms and the app uses both.

Access is process-wide for the subtree once started, so Rust's POSIX calls work
inside it. Coordination is block-scoped and advisory, so it must bracket the
operation — and the operation it brackets is *one bridge call*, not one file
read:

```swift
try coordinator.coordinate(readingItemAt: vaultURL, options: []) { _ in
    report = try handle.scan(progress: sink)     // ~4000 file operations inside
}
```

One coordinated read for a whole scan. One for opening a note. One coordinated
write for a save. This is what keeps a 1000-note index off the bridge and out of
4000 Objective-C blocks.

### 4.2 The two exceptions

Two operations cannot be done with POSIX from Rust, and are the only callbacks
the bridge asks Swift to implement:

**`ensureMaterialized(path)`** — an iCloud vault can hold placeholder stubs
rather than files. Rust's `read` detects one (zero length with a `.icloud`
sibling, or a `read_dir` entry with no content) and calls the hook, which does
`startDownloadingUbiquitousItem` and a coordinated read to block until the file
is local. On a non-ubiquitous vault this is never called.

**`replaceItem(at:with:)`** — the desktop's atomic write is temp-file +
`fsync` + `rename(2)`. On a File Provider directory a bare `rename` bypasses the
provider's bookkeeping and can spawn spurious conflict copies. iOS instead uses
`NSFileCoordinator` with `.forReplacing` and
`FileManager.replaceItemAt(_:withItemAt:backupItemName:options:)`, which is the
documented atomic replace on Apple platforms. The temp file is still written by
Rust; only the swap crosses the bridge.

Both are declared as traits in `ie-platform::platform::ios` and exported as
UniFFI callback interfaces. `ie-core` sees neither; it sees `FileSystem`.

### 4.3 Watching

`PresenterWatcher` registers an `NSFilePresenter` for the vault directory.
Callbacks map to the existing `FsEvent` vocabulary:

| `NSFilePresenter` | `FsEvent` |
|---|---|
| `presentedSubitemDidAppear(at:)` | `Created` |
| `presentedSubitemDidChange(at:)` | `Modified` |
| `accommodatePresentedSubitemDeletion(at:)` | `Deleted` |
| `presentedSubitem(at:didMoveTo:)` | `Renamed { from, to }` |
| presenter relinquished for a long time; app resumed; bookmark was stale | `Rescan { root }` |

Coalescing is done in Rust, not Swift. The Swift presenter only translates and
pushes; `PresenterWatcher` holds the events until the vault has been quiet for
`WatchOptions::debounce` and then delivers one batch, so
`ie-core::apply_events` receives the same shape it gets from the desktop
debouncer. It lives in Rust because it is the part with actual behaviour — a
quiet window, a `Rescan` that discards everything queued behind it, a flush on
stop so a save made as the app backgrounds is not lost — and that is worth
having under test on any machine rather than only on a device.

## 5. Writing a note safely

```
  1. buffer.modifiedMs != file.modifiedMs?  →  ExternalModification, stop.
  2. Rust writes  .ie-tmp-<pid>-<n>  beside the target, fsync
  3. Swift: coordinate(writingItemAt: target, options: .forReplacing)
       → FileManager.replaceItemAt(target, withItemAt: temp)
  4. Rust: fsync the parent directory
  5. record the new modifiedMs on the buffer
```

Step 1 is I10 and has no bypass. Steps 2–4 are I4 — a crash or power loss at any
point leaves either the complete old file or the complete new one.

Leftover `.ie-tmp-*` files from an interrupted write are reported by
`session.diagnostics()` as `Diagnostic::InterruptedWrite`, the same as on
desktop, and shown in Settings → Vault rather than deleted behind the user's
back.

## 6. Crash recovery

`RecoveryJournal` lives in the app container (`Library/Application Support/data`),
keyed by vault id — not in the vault, because a half-typed paragraph is
machine-local and has no business syncing to a desktop.

Autosave writes the journal entry on a debounce and on
`scenePhase` leaving `.active`. On next launch, `recoverable()` returns
candidates with `already_saved` and `file_is_newer` computed against the current
file, and the UI offers to restore only the ones that would actually recover
something.

## 7. Lifecycle

| Transition | Action |
|---|---|
| `.active` → `.inactive` | flush the editor buffer to the recovery journal |
| `.inactive` → `.background` | finish in-flight writes, save workspace, suspend the watcher, release security-scoped access if suspension is likely |
| `.background` → `.active` | re-take access, re-resolve a stale bookmark, resume the watcher, issue `Rescan`, reconcile the open buffer against the file |
| termination | nothing to do — everything durable was written at `.inactive` |

Background work (§39) is limited to indexing and sync, registered with
`BGProcessingTask`, checkpointed per batch so the system can stop it at any
point without losing progress. The indexer already commits per batch on desktop
for the same reason.

## 8. What is not done

- **No copying the vault into the app.** §7. The app works in place.
- **No `UIDocumentBrowserViewController` as the root.** It is a good fit for
  single-document apps; this is a folder-scoped app with its own navigation.
- **No private API, no jailbreak API, no undocumented paths.** §44. Everything
  above is public framework surface.

# Sync

## 1. Position

The vault is a folder of Markdown files. If the user puts it in iCloud Drive,
Dropbox, or any Files.app provider, **the system already syncs it** and the
app's job is not to sync but to notice and to not corrupt. That is the shipped
behaviour, and for most users it is the whole story.

`SyncProvider` exists so that a future provider — WebDAV, S3-compatible, a
self-hosted endpoint — plugs in without the rest of the app changing. It is
defined now because retrofitting an abstraction is expensive; it is not
implemented beyond the system-provider case, because §24 says not to build a
server without necessity and there is none.

## 2. The protocol

```swift
struct SyncCursor: Codable, Sendable { let token: String }

enum Resolution: Sendable { case keepLocal, keepRemote, merged(String) }

protocol SyncProvider: Sendable {
    var identifier: String { get }
    var capabilities: SyncCapabilities { get }

    func connect() async throws
    func disconnect() async

    func upload(_ change: LocalChange) async throws
    func download(_ ref: RemoteRef) async throws -> RemoteFile
    func getChanges(since: SyncCursor) async throws -> ([RemoteChange], SyncCursor)
    func sync() async throws -> SyncReport
    func resolveConflict(_ conflict: Conflict, with: Resolution) async throws
}
```

Nothing in it names a vendor. `SyncCapabilities` declares whether the provider
supports server-side change tokens, partial downloads, and atomic replace, so
the engine can degrade rather than assume.

## 3. `FileProviderSync` — the shipped conformance

For a vault on iCloud Drive or any Files provider:

- `connect` / `disconnect` — start / stop security-scoped access and the
  `NSFilePresenter`.
- `getChanges` — `NSFilePresenter` callbacks, coalesced, translated to
  `RemoteChange`.
- `download` — `startDownloadingUbiquitousItem` plus a coordinated read; this is
  the same machinery as `ensureMaterialized` in `STORAGE.md` §4.2.
- `upload` — a no-op. The provider uploads; the app's coordinated write is the
  handoff.
- `sync` — reconcile: rescan, apply events, report.
- `resolveConflict` — see §5.

## 4. Detecting an external change

Every open buffer records the `modified_ms` of the file it was loaded from.
Before any write:

```
file.modified_ms == buffer.base_modified_ms   →  write
otherwise                                     →  FfiError::ExternalModification
```

There is no other path to a write. This is invariant I10 and it is the reason
§61 ("never silently overwrite external changes") is a structural property
rather than a code-review rule.

The watcher raises the same condition proactively, so the user usually learns
about it while reading rather than at the moment they press save.

## 5. Conflict resolution UI

Three actions, always all three, never a default that discards:

| Action | Effect |
|---|---|
| **Reload** | discard the local buffer, load the file |
| **Keep mine** | write the buffer over the file, after showing what is being replaced |
| **Compare** | open the diff view |

The diff is a Myers line diff computed in Rust (in `ie-ffi`, not `ie-core` — it
is a sync concern, not a note-format concern) and rendered per hunk with
Keep Local / Keep Remote for each. The merged result is written through the same
atomic path as any other save.

Two rules with no exceptions:

- **Nothing is discarded without the user choosing it.** "Keep mine" shows the
  remote content first.
- **Provider conflict files are never deleted.** iCloud writes
  `Note 2.md` / `Note (conflicted copy).md` siblings; the app surfaces them as
  resolvable items in the note's own header, and leaves them on disk until the
  user resolves them.

## 6. Not implemented

`WebDAVSync` and `S3Sync` — specified against the protocol above, with cursor
semantics (`getChanges` returns an opaque token; a provider without server-side
change tracking returns a content-hash manifest) and ETag-based optimistic
concurrency written down here, so the work is designed. No code, no server, no
network entitlement.

import Foundation
import OSLog

/// The one owner of an open vault.
///
/// Every call into the core goes through this actor, so two saves to the same
/// note cannot interleave — the requirement that writes never race is met by
/// the type system rather than by remembering to take a lock. It is also why
/// nothing here is `@MainActor`: filesystem work must not run on the thread
/// drawing the screen.
///
/// The actor holds the security-scoped access for as long as the vault is
/// open, rather than taking it per operation. One owner is easier to prove
/// balanced than a count spread across call sites, and an unbalanced access
/// leaks a sandbox extension for the life of the process.
actor VaultService {
    struct OpenVault: Sendable {
        let id: String
        let name: String
        let caseSensitive: Bool
        /// The index had to be built — a first open, or the cache was purged.
        /// The UI shows progress rather than pretending the vault is ready.
        let needsFullScan: Bool
    }

    private var handle: VaultHandle?
    private var access: VaultAccess?
    private var storage: CoordinatedVaultStorage?
    private var host: BridgeStorageHost?
    private var watchToken: VaultWatchToken?
    private var openVault: OpenVault?

    private let bookmarks: VaultBookmarkStore
    private let log = Logger(subsystem: Logging.subsystem, category: "vault")

    init(bookmarks: VaultBookmarkStore = VaultBookmarkStore()) {
        self.bookmarks = bookmarks
    }

    var current: OpenVault? { openVault }

    // MARK: - Opening

    /// Open the folder the document picker just returned.
    func open(pickedURL url: URL, createNamed name: String? = nil) async throws -> OpenVault {
        let access = try VaultAccess.begin(at: url)
        return try await open(access: access, bookmarkData: try? access.bookmarkData(), createNamed: name)
    }

    /// Reopen a vault the app has seen before.
    func reopen(_ bookmark: VaultBookmark) async throws -> OpenVault {
        var refreshed: Data?
        let access = try VaultAccess.resolve(bookmark) { refreshed = $0 }
        let vault = try await open(access: access, bookmarkData: refreshed ?? bookmark.data)

        if refreshed != nil {
            // A stale bookmark means the vault moved, or the provider re-issued
            // its identifiers, or the OS updated. Anything could have changed
            // while the app was not running, so do not trust the cache.
            log.notice("the bookmark was stale; rescanning")
            _ = try? await scan(reason: .staleBookmark)
        }
        return vault
    }

    private func open(
        access: VaultAccess,
        bookmarkData: Data?,
        createNamed name: String? = nil
    ) async throws -> OpenVault {
        await close()

        let kind = Self.storageKind(for: access.url)
        let storage = CoordinatedVaultStorage(root: access.url, kind: kind)
        let host = BridgeStorageHost(storage: storage)
        let config = try Self.hostConfig(kind: kind)

        let report = OpenReportSink()
        let path = access.url.path(percentEncoded: false)
        let handle: VaultHandle
        do {
            handle = try storage.coordinatingRead {
                if let name {
                    return try VaultHandle.create(
                        config: config, path: path, name: name,
                        storage: kind == .fileProvider ? host : nil
                    )
                }
                return try VaultHandle.openReporting(
                    config: config, path: path,
                    storage: kind == .fileProvider ? host : nil,
                    report: report
                )
            }
        } catch {
            access.stop()
            throw error
        }

        // One-shot: `take()` clears the sink, so read it once. `create` does
        // not fill it at all, which is why the fallbacks are not decoration.
        let openReport = report.take()
        let opened = OpenVault(
            id: handle.vaultId(),
            name: openReport?.name ?? access.url.lastPathComponent,
            caseSensitive: openReport?.caseSensitive ?? (try? handle.isCaseSensitive()) ?? false,
            // No report means `create`, and a vault that was just created has
            // an empty index. Assuming it is ready would skip the first scan.
            needsFullScan: openReport?.index.needsFullScan ?? true
        )

        self.handle = handle
        self.access = access
        self.storage = storage
        self.host = host
        self.openVault = opened

        if let bookmarkData {
            await bookmarks.save(
                VaultBookmark(
                    vaultID: opened.id,
                    name: opened.name,
                    data: bookmarkData,
                    lastOpened: .now
                )
            )
        }
        return opened
    }

    /// Close the vault, releasing access and flushing the watch.
    func close() async {
        watchToken?.stop()
        watchToken = nil
        handle?.stopWatch()
        handle = nil
        storage = nil
        host = nil
        openVault = nil
        access?.stop()
        access = nil
    }

    // MARK: - Indexing

    enum ScanReason: Sendable {
        case opening
        case returningToForeground
        case staleBookmark
        case watcherLostTrack
    }

    @discardableResult
    func scan(
        reason: ScanReason,
        progress: (@Sendable (IndexProgress) -> Void)? = nil
    ) throws -> ScanReport {
        let (handle, storage) = try requireOpen()
        log.debug("scanning: \(String(describing: reason), privacy: .public)")
        // One coordinated read for the whole walk. A bracket per file would be
        // correct too, and several thousand times slower.
        return try storage.coordinatingRead {
            try handle.scan(progress: progress.map(ScanProgressSink.init))
        }
    }

    // MARK: - Notes

    func note(at path: String) throws -> Note {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingRead { try handle.readNote(path: path) }
    }

    /// Save, refusing if the file changed since the buffer was opened.
    ///
    /// `baseModifiedMs` is what `readNote` or `createNote` returned. It is not
    /// bookkeeping: it is the only thing standing between a background sync
    /// and a silently overwritten edit. A mismatch throws
    /// `FfiError.ExternalModification`, and the caller must resolve it with
    /// the user rather than picking a winner.
    @discardableResult
    func save(_ content: String, to path: String, baseModifiedMs: Int64) throws -> Int64 {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingWrite(at: path) {
            try handle.saveNote(path: path, content: content, baseModifiedMs: baseModifiedMs)
        }
    }

    /// Save over whatever is on disk, after the user has seen it and chosen.
    /// The only path that skips the freshness check.
    @discardableResult
    func saveOverwriting(_ content: String, to path: String) throws -> Int64 {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingWrite(at: path) {
            try handle.forceSaveNote(path: path, content: content)
        }
    }

    func createNote(at path: String, content: String, collision: Collision = .rename) throws -> CreatedNote {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingWrite(at: path) {
            try handle.createNote(path: path, content: content, collision: collision)
        }
    }

    func delete(at path: String) throws -> TrashEntry {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingWrite(at: path) { try handle.delete(path: path) }
    }

    func planRename(from: String, to: String) throws -> RenamePlan {
        let (handle, _) = try requireOpen()
        return try handle.planRename(from: from, to: to)
    }

    func rename(from: String, to: String) throws -> RenameOutcome {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingWrite(at: to) { try handle.rename(from: from, to: to) }
    }

    func setProperties(_ properties: [Property], at path: String) throws {
        let (handle, storage) = try requireOpen()
        try storage.coordinatingWrite(at: path) {
            try handle.setProperties(path: path, properties: properties)
        }
    }

    // MARK: - Reading the index

    func listDirectory(at path: String) throws -> DirectoryListing {
        try requireOpen().handle.listDirectory(path: path)
    }

    func search(_ query: String, options: SearchOptions = SearchOptions()) throws -> SearchResults {
        try requireOpen().handle.search(query: query, options: options)
    }

    func quickSwitch(_ needle: String, limit: UInt32 = 50) throws -> [FileMatch] {
        try requireOpen().handle.quickSwitch(needle: needle, limit: limit)
    }

    func backlinks(to path: String) throws -> [Backlink] {
        try requireOpen().handle.backlinks(path: path)
    }

    func outgoingLinks(from path: String) throws -> [ResolvedLink] {
        try requireOpen().handle.outgoingLinks(path: path)
    }

    func tags() throws -> [TagSummary] {
        try requireOpen().handle.tags()
    }

    func diagnostics() throws -> [Diagnostic] {
        try requireOpen().handle.diagnostics()
    }

    // MARK: - Recovery

    /// Record an unsaved buffer so a crash or a termination does not lose it.
    /// Written to the app container, never into the vault: a half-typed
    /// paragraph is machine-local and has no business syncing anywhere.
    func journal(_ content: String, for path: String) throws {
        try requireOpen().handle.journalUnsaved(path: path, content: content)
    }

    func clearJournal(for path: String) throws {
        try requireOpen().handle.clearJournal(path: path)
    }

    func recoverable() throws -> [RecoveryCandidate] {
        try requireOpen().handle.recoverable()
    }

    // MARK: - Watching

    /// Start reporting changes made outside the app.
    ///
    /// Two hops on purpose: the presenter pushes raw events into the core,
    /// which coalesces them and hands back batches. Doing the coalescing in
    /// Rust keeps one save from becoming four notifications, and keeps that
    /// logic under test without a device.
    func startWatching(_ onChange: @escaping @Sendable ([FsEvent]) -> Void) throws {
        let (handle, storage) = try requireOpen()
        guard watchToken == nil else { return }

        try handle.startWatch(observer: ChangeObserverSink(onChange))
        watchToken = try storage.watchChanges { [weak self] changes in
            let events = changes.map(Self.event(from:))
            Task { await self?.deliver(events) }
        }
    }

    func stopWatching() {
        watchToken?.stop()
        watchToken = nil
        handle?.stopWatch()
    }

    /// Bring the index up to date from a coalesced batch, without a full walk.
    ///
    /// This is the cheap path: a note saved in the Files app costs one reparse
    /// rather than a scan of the vault. A `.rescan` in the batch means the
    /// watcher lost track, and the caller should scan instead.
    @discardableResult
    func applyExternalEvents(_ events: [FsEvent]) throws -> EventOutcome {
        let (handle, storage) = try requireOpen()
        return try storage.coordinatingRead { try handle.applyEvents(events: events) }
    }

    private func deliver(_ events: [FsEvent]) {
        try? handle?.deliverEvents(events: events)
    }

    // MARK: - Lifecycle

    /// The time zone changed; the daily note must follow the device rather
    /// than the device as it was at launch.
    func timeZoneChanged() {
        handle?.setUtcOffsetSeconds(seconds: Int32(TimeZone.current.secondsFromGMT()))
    }

    // MARK: - Helpers

    private func requireOpen() throws -> (handle: VaultHandle, storage: CoordinatedVaultStorage) {
        guard let handle, let storage else { throw FfiError.NoVaultOpen }
        return (handle, storage)
    }

    private static func event(from change: VaultChange) -> FsEvent {
        switch change {
        case .created(let path): .created(path: path)
        case .modified(let path): .modified(path: path)
        case .deleted(let path): .deleted(path: path)
        case .moved(let from, let to): .renamed(from: from, to: to)
        case .lost: .rescan
        }
    }

    /// A vault behind a File Provider needs coordination and can hold evicted
    /// files; one in the app's own container needs neither.
    private static func storageKind(for url: URL) -> StorageKind {
        let values = try? url.resourceValues(forKeys: [.isUbiquitousItemKey])
        if values?.isUbiquitousItem == true { return .fileProvider }
        // Anything outside our container arrived through the document picker,
        // so assume a provider is involved: assuming a local folder when one
        // is not would mean an evicted note read as empty and indexed as
        // empty, which is data loss noticed much later, if at all.
        let home = URL(fileURLWithPath: NSHomeDirectory()).standardizedFileURL
            .path(percentEncoded: false)
        return url.standardizedFileURL.path(percentEncoded: false).hasPrefix(home)
            ? .localFolder
            : .fileProvider
    }

    private static func hostConfig(kind: StorageKind) throws -> HostConfig {
        guard let library = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first
        else {
            throw VaultStorageError.underlying("the app container has no Library directory")
        }
        return HostConfig(
            libraryDir: library.path(percentEncoded: false),
            utcOffsetSeconds: Int32(TimeZone.current.secondsFromGMT()),
            storage: kind,
            watchDebounceMs: 150
        )
    }
}

// MARK: - Callback adapters

/// Bridges the core's progress callback to a Swift closure.
private final class ScanProgressSink: ScanProgress, @unchecked Sendable {
    private let handler: @Sendable (IndexProgress) -> Void
    init(_ handler: @escaping @Sendable (IndexProgress) -> Void) { self.handler = handler }
    func report(progress: IndexProgress) { handler(progress) }
}

private final class ChangeObserverSink: ChangeObserver, @unchecked Sendable {
    private let handler: @Sendable ([FsEvent]) -> Void
    init(_ handler: @escaping @Sendable ([FsEvent]) -> Void) { self.handler = handler }
    func changed(events: [FsEvent]) { handler(events) }
}

extension OpenOutcome {
    /// Whether the index has to be built before the vault is usable.
    var needsFullScan: Bool { self != .reused }
}

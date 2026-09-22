import Foundation
import OSLog

/// Hearing about changes the app did not make.
///
/// A vault can be edited from the Files app, from a Mac syncing the same
/// folder, or by iCloud itself. `NSFilePresenter` is how iOS says so — and it
/// is public framework surface, unlike the directory-watching tricks that
/// would be needed otherwise.
///
/// Nothing here debounces. The core's watcher does that: it holds a batch
/// until the vault has been quiet, discards everything queued behind a `lost`,
/// and flushes on stop so a change made as the app backgrounds is not
/// dropped. That logic lives in Rust because it is the part with behaviour
/// worth testing on any machine, not just a device.
final class VaultPresenter: NSObject, NSFilePresenter, VaultWatchToken, @unchecked Sendable {
    let presentedItemURL: URL?
    let presentedItemOperationQueue: OperationQueue

    private let root: URL
    private let lock = NSLock()
    private var sink: (@Sendable ([VaultChange]) -> Void)?
    private var registered = false
    private let log = Logger(subsystem: Logging.subsystem, category: "presenter")

    init(root: URL) {
        self.root = root
        self.presentedItemURL = root
        self.presentedItemOperationQueue = {
            let queue = OperationQueue()
            queue.name = "vault.presenter"
            // Serial: the callbacks describe a sequence of changes, and
            // reordering them would turn a move into a delete and a create.
            queue.maxConcurrentOperationCount = 1
            queue.qualityOfService = .utility
            return queue
        }()
        super.init()
    }

    func start(_ onChange: @escaping @Sendable ([VaultChange]) -> Void) {
        lock.lock()
        sink = onChange
        let alreadyRegistered = registered
        registered = true
        lock.unlock()

        if !alreadyRegistered {
            NSFileCoordinator.addFilePresenter(self)
        }
    }

    func stop() {
        lock.lock()
        let wasRegistered = registered
        registered = false
        sink = nil
        lock.unlock()

        if wasRegistered {
            NSFileCoordinator.removeFilePresenter(self)
        }
    }

    deinit {
        if registered {
            NSFileCoordinator.removeFilePresenter(self)
        }
    }

    private func emit(_ changes: [VaultChange]) {
        lock.lock()
        let sink = self.sink
        lock.unlock()
        sink?(changes)
    }

    private func relative(_ url: URL) -> String? {
        VaultPathUtil.relative(url, under: root)
    }

    // MARK: - NSFilePresenter

    func presentedSubitemDidAppear(at url: URL) {
        guard let path = relative(url) else { return }
        emit([.created(path)])
    }

    func presentedSubitemDidChange(at url: URL) {
        guard let path = relative(url) else { return }
        // A provider reports a deletion this way too, so check rather than
        // assume: reporting a delete as a modification would leave the note in
        // the index pointing at nothing.
        if FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) {
            emit([.modified(path)])
        } else {
            emit([.deleted(path)])
        }
    }

    func accommodatePresentedSubitemDeletion(at url: URL) async throws {
        guard let path = relative(url) else { return }
        emit([.deleted(path)])
    }

    func presentedSubitem(at oldURL: URL, didMoveTo newURL: URL) {
        switch (relative(oldURL), relative(newURL)) {
        case let (from?, to?):
            emit([.moved(from: from, to: to)])
        case let (from?, nil):
            // Moved out of the vault: a delete, as far as this vault knows.
            emit([.deleted(from)])
        case let (nil, to?):
            emit([.created(to)])
        case (nil, nil):
            break
        }
    }

    func presentedItemDidChange() {
        // The vault directory itself changed in a way not attributed to a
        // subitem. Too coarse to act on precisely, so ask for a walk.
        emit([.lost])
    }

    func presentedItemDidMove(to newURL: URL) {
        // The vault moved. Bookmarks resolve to the new location, but every
        // path the index holds is now suspect.
        log.notice("the vault moved; a rescan is needed")
        emit([.lost])
    }

    func accommodatePresentedItemDeletion(completionHandler: @escaping (Error?) -> Void) {
        // The vault itself is going away. Say so and let the app decide; it is
        // not this type's business to close anything.
        emit([.lost])
        completionHandler(nil)
    }
}

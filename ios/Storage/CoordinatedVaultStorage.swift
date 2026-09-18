import Foundation
import OSLog

/// The real vault storage: POSIX inside a coordination bracket.
///
/// Two mechanisms are at work and they are not the same thing.
///
/// **Access** is granted by `startAccessingSecurityScopedResource` and is
/// process-wide for the subtree until the matching stop. That is why the Rust
/// core's ordinary file calls succeed inside it, and why a full scan does not
/// have to cross the bridge once per file.
///
/// **Coordination** is `NSFileCoordinator`, and it is advisory and
/// block-scoped. It matters for a provider-backed vault, where a file may be a
/// placeholder that only coordination will materialise, and where a bare
/// `rename(2)` bypasses bookkeeping the provider depends on.
///
/// So the bracket wraps *one call into the core*, not one file operation.
final class CoordinatedVaultStorage: VaultStorage, @unchecked Sendable {
    let root: URL
    private let kind: StorageKind
    private let coordinator: NSFileCoordinator
    private let presenter: VaultPresenter
    private let log = Logger(subsystem: Logging.subsystem, category: "storage")

    init(root: URL, kind: StorageKind) {
        self.root = root
        self.kind = kind
        self.coordinator = NSFileCoordinator(filePresenter: nil)
        self.presenter = VaultPresenter(root: root)
    }

    // MARK: - Coordination brackets

    func coordinatingRead<T>(_ body: () throws -> T) throws -> T {
        // A local folder has no provider to coordinate with, and taking the
        // bracket anyway would cost a round trip on every scan for nothing.
        guard kind == .fileProvider else { return try body() }

        var result: Result<T, Error> = .failure(VaultStorageError.underlying("the coordinated read never ran"))
        var coordinationError: NSError?
        coordinator.coordinate(readingItemAt: root, options: [], error: &coordinationError) { _ in
            result = Result { try body() }
        }
        if let coordinationError {
            throw VaultStorageError.underlying(coordinationError.localizedDescription)
        }
        return try result.get()
    }

    func coordinatingWrite<T>(at path: String, _ body: () throws -> T) throws -> T {
        guard kind == .fileProvider else { return try body() }
        let url = try VaultPathUtil.resolve(path, under: root)

        var result: Result<T, Error> = .failure(VaultStorageError.underlying("the coordinated write never ran"))
        var coordinationError: NSError?
        coordinator.coordinate(writingItemAt: url, options: [], error: &coordinationError) { _ in
            result = Result { try body() }
        }
        if let coordinationError {
            throw VaultStorageError.underlying(coordinationError.localizedDescription)
        }
        return try result.get()
    }

    // MARK: - Files

    func readFile(at path: String) throws -> Data {
        let url = try VaultPathUtil.resolve(path, under: root)
        do {
            return try Data(contentsOf: url)
        } catch {
            // Possibly evicted rather than missing.
            try materializeIfPlaceholder(at: path)
            do {
                return try Data(contentsOf: url)
            } catch {
                throw VaultStorageError.notFound(path)
            }
        }
    }

    func writeFile(_ data: Data, to path: String) throws {
        let url = try VaultPathUtil.resolve(path, under: root)
        try coordinatingWrite(at: path) {
            // `.atomic` writes a temporary and replaces, which is the same
            // protocol the core uses and the only one safe against a crash
            // mid-write.
            try data.write(to: url, options: .atomic)
        }
    }

    func createFile(_ data: Data, at path: String) throws {
        let url = try VaultPathUtil.resolve(path, under: root)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try writeFile(data, to: path)
    }

    func deleteFile(at path: String) throws {
        let url = try VaultPathUtil.resolve(path, under: root)
        var coordinationError: NSError?
        var thrown: Error?
        let remove: (URL) -> Void = { target in
            do {
                try FileManager.default.removeItem(at: target)
            } catch {
                thrown = error
            }
        }
        if kind == .fileProvider {
            coordinator.coordinate(writingItemAt: url, options: .forDeleting, error: &coordinationError, byAccessor: remove)
        } else {
            remove(url)
        }
        if let coordinationError { throw VaultStorageError.underlying(coordinationError.localizedDescription) }
        if let thrown { throw thrown }
    }

    func moveFile(from: String, to: String) throws {
        let source = try VaultPathUtil.resolve(from, under: root)
        let destination = try VaultPathUtil.resolve(to, under: root)
        try FileManager.default.createDirectory(
            at: destination.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )

        var coordinationError: NSError?
        var thrown: Error?
        let move: (URL, URL) -> Void = { source, destination in
            do {
                try FileManager.default.moveItem(at: source, to: destination)
            } catch {
                thrown = error
            }
        }
        if kind == .fileProvider {
            coordinator.coordinate(
                writingItemAt: source, options: .forMoving,
                writingItemAt: destination, options: .forReplacing,
                error: &coordinationError,
                byAccessor: move
            )
        } else {
            move(source, destination)
        }
        if let coordinationError { throw VaultStorageError.underlying(coordinationError.localizedDescription) }
        if let thrown { throw thrown }
    }

    func renameFile(at path: String, to newName: String) throws {
        let parent = (try VaultPathUtil.normalize(path) as NSString).deletingLastPathComponent
        let destination = parent.isEmpty ? newName : "\(parent)/\(newName)"
        try moveFile(from: path, to: destination)
    }

    func listDirectory(at path: String) throws -> [VaultEntry] {
        let url = try VaultPathUtil.resolve(path, under: root)
        let keys: [URLResourceKey] = [
            .isDirectoryKey, .fileSizeKey, .contentModificationDateKey, .nameKey,
            .isUbiquitousItemKey, .ubiquitousItemDownloadingStatusKey,
        ]
        let contents = try FileManager.default.contentsOfDirectory(
            at: url,
            includingPropertiesForKeys: keys,
            options: [.skipsSubdirectoryDescendants]
        )

        return contents.compactMap { child -> VaultEntry? in
            guard let relative = VaultPathUtil.relative(child, under: root) else { return nil }
            let values = try? child.resourceValues(forKeys: Set(keys))

            // An evicted file appears as `.Name.icloud`. Reporting that name
            // would make the note vanish from the list; reporting the real one
            // keeps it there, and reading it downloads it.
            let rawName = child.lastPathComponent
            let (name, placeholder) = Self.behindPlaceholder(rawName)
            let reportedPath = placeholder
                ? (relative as NSString).deletingLastPathComponent.isEmpty
                    ? name
                    : "\((relative as NSString).deletingLastPathComponent)/\(name)"
                : relative

            return VaultEntry(
                path: reportedPath,
                name: name,
                isDirectory: values?.isDirectory ?? false,
                size: Int64(values?.fileSize ?? 0),
                modified: values?.contentModificationDate,
                isPlaceholder: placeholder
                    || values?.ubiquitousItemDownloadingStatus == .notDownloaded
            )
        }
    }

    func fileExists(at path: String) -> Bool {
        guard let url = try? VaultPathUtil.resolve(path, under: root) else { return false }
        if FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) { return true }
        return placeholderURL(for: url).map {
            FileManager.default.fileExists(atPath: $0.path(percentEncoded: false))
        } ?? false
    }

    func watchChanges(_ onChange: @escaping @Sendable ([VaultChange]) -> Void) throws -> VaultWatchToken {
        presenter.start(onChange)
        return presenter
    }

    // MARK: - Placeholders

    /// `Note.md` → `.Note.md.icloud` when that placeholder is on disk.
    private func placeholderURL(for url: URL) -> URL? {
        let name = url.lastPathComponent
        guard !name.hasPrefix(".") else { return nil }
        return url.deletingLastPathComponent().appending(
            path: ".\(name).icloud", directoryHint: .notDirectory
        )
    }

    /// `.Note.md.icloud` → `("Note.md", true)`, anything else unchanged.
    private static func behindPlaceholder(_ name: String) -> (String, Bool) {
        guard name.hasPrefix("."), name.hasSuffix(".icloud") else { return (name, false) }
        let inner = String(name.dropFirst().dropLast(".icloud".count))
        return inner.isEmpty ? (name, false) : (inner, true)
    }

    /// Bring an evicted file onto the device, blocking until it is there.
    ///
    /// Called only when a read has already failed and a placeholder is
    /// present, so a local vault never reaches it.
    func materializeIfPlaceholder(at path: String) throws {
        let url = try VaultPathUtil.resolve(path, under: root)
        guard let placeholder = placeholderURL(for: url),
              FileManager.default.fileExists(atPath: placeholder.path(percentEncoded: false))
        else { return }

        log.debug("downloading an evicted file")
        do {
            try FileManager.default.startDownloadingUbiquitousItem(at: url)
        } catch {
            throw VaultStorageError.notDownloadable(path)
        }

        // A coordinated read blocks until the provider has materialised it,
        // which is the documented way to wait rather than polling.
        var coordinationError: NSError?
        coordinator.coordinate(readingItemAt: url, options: [], error: &coordinationError) { _ in }
        if let coordinationError {
            throw VaultStorageError.notDownloadable(path)
        }
        guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else {
            throw VaultStorageError.notDownloadable(path)
        }
    }

    /// The provider's own atomic replace.
    ///
    /// `rename(2)` would work and would also bypass the provider's
    /// bookkeeping, which is how spurious "conflicted copy" siblings appear.
    /// `replaceItemAt` under a `.forReplacing` bracket is the documented one,
    /// and the provider understands it.
    func replaceItem(at path: String, withTemporaryAt temporaryPath: String) throws {
        let target = try VaultPathUtil.resolve(path, under: root)
        let temporary = try VaultPathUtil.resolve(temporaryPath, under: root)

        var coordinationError: NSError?
        var thrown: Error?
        let swap: (URL) -> Void = { destination in
            do {
                if FileManager.default.fileExists(atPath: destination.path(percentEncoded: false)) {
                    _ = try FileManager.default.replaceItemAt(destination, withItemAt: temporary)
                } else {
                    try FileManager.default.moveItem(at: temporary, to: destination)
                }
            } catch {
                thrown = error
            }
        }

        if kind == .fileProvider {
            coordinator.coordinate(
                writingItemAt: target, options: .forReplacing,
                error: &coordinationError, byAccessor: swap
            )
        } else {
            swap(target)
        }

        // The temporary must never survive: a leftover `.ie-tmp-*` is reported
        // by the core as an interrupted write, and claiming one happened when
        // it did not would send the user looking for lost work.
        if thrown != nil || coordinationError != nil {
            try? FileManager.default.removeItem(at: temporary)
        }
        if let coordinationError { throw VaultStorageError.underlying(coordinationError.localizedDescription) }
        if let thrown { throw thrown }
    }
}

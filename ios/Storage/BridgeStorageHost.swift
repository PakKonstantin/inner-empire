import Foundation
import OSLog

/// The two operations the Rust core cannot do for itself.
///
/// Everything else in a vault — reads, directory walks, metadata, the
/// case-sensitivity probe — is POSIX, and the core does it directly inside the
/// coordination bracket the app holds. These two are Objective-C only:
///
/// 1. Materialising an evicted file. A provider may hold a note's contents in
///    the cloud, and only `startDownloadingUbiquitousItem` plus a coordinated
///    read will bring it back.
/// 2. Replacing a file atomically. `rename(2)` works and bypasses the
///    provider's bookkeeping, which is how spurious conflict copies appear.
///
/// The core calls these through a trait declared in `ie-platform`; it does not
/// know they are implemented in Swift, and it certainly does not know about
/// `NSFileCoordinator`. That is the §8 requirement, met by construction.
final class BridgeStorageHost: StorageHost, @unchecked Sendable {
    private let storage: CoordinatedVaultStorage
    private let log = Logger(subsystem: Logging.subsystem, category: "bridge")

    init(storage: CoordinatedVaultStorage) {
        self.storage = storage
    }

    func ensureMaterialized(relativePath: String) throws {
        do {
            try storage.materializeIfPlaceholder(at: relativePath)
        } catch {
            // The core turns this into a typed error the UI can act on. Losing
            // the reason here would leave the user with "could not read file"
            // and no idea that the fix is to go online.
            log.error("could not materialise a file: \(error.localizedDescription, privacy: .public)")
            throw error
        }
    }

    func replaceItem(relativeTarget: String, relativeSource: String) throws {
        try storage.replaceItem(at: relativeTarget, withTemporaryAt: relativeSource)
    }
}

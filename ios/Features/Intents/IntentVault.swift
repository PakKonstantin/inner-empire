import Foundation
import OSLog

/// Reaching the vault from an intent.
///
/// An intent can run with no app on screen, so it cannot assume the
/// `AppModel`'s service is already open. This opens one on demand from the
/// most recent bookmark and keeps it for the life of the process, because an
/// intent invoked three times in a row should not open the vault three times.
@MainActor
enum IntentVault {
    enum Failure: LocalizedError {
        case noVault

        var errorDescription: String? {
            String(localized: "Open Inner Empire and choose your vault first.")
        }
    }

    private static var shared: VaultService?
    private static let log = Logger(subsystem: Logging.subsystem, category: "intents")

    static func service() async throws -> VaultService {
        if let shared, shared.current != nil { return shared }

        let service = VaultService()
        let store = VaultBookmarkStore()
        guard let bookmark = await store.all().max(by: { $0.lastOpened < $1.lastOpened }) else {
            throw Failure.noVault
        }
        try await service.reopen(bookmark)
        shared = service
        log.notice("opened the vault for an intent")
        return service
    }

    /// Ask the app to show a note, for the intents that open it.
    ///
    /// A notification rather than a direct call: the app may not be running
    /// yet, and the scene picks this up when it appears.
    static func requestOpen(_ path: String) async {
        NotificationCenter.default.post(
            name: .init("ie.openNote"),
            object: nil,
            userInfo: ["path": path]
        )
    }
}

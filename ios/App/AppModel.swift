import Foundation
import OSLog
import SwiftUI

/// What the interface knows about.
///
/// Isolated to the main actor because it is view state; every call it makes
/// into `VaultService` is an `await` across an actor boundary, which is what
/// keeps filesystem work off the thread drawing the screen.
@MainActor
@Observable
final class AppModel {
    enum Stage: Equatable {
        case starting
        /// No vault yet, or the last one could not be found.
        case choosingVault(reason: String?)
        case opening(name: String)
        case indexing(progress: Double?, scanned: Int)
        case ready
        case failed(String)
    }

    private(set) var stage: Stage = .starting
    private(set) var vault: VaultService.OpenVault?
    private(set) var knownVaults: [VaultBookmark] = []
    /// Problems worth surfacing: case conflicts, names that would not survive
    /// a trip to Windows, an interrupted write. Shown in Settings rather than
    /// as an alert — a vault with a hundred of them deserves one report.
    private(set) var diagnostics: [Diagnostic] = []
    /// Buffers that were never saved, offered back on the next launch.
    private(set) var recoverable: [RecoveryCandidate] = []

    /// Raised when a file changed underneath an open buffer. Never resolved
    /// automatically: the user picks, or nothing happens.
    var pendingConflict: ExternalChange?

    let service = VaultService()
    private let bookmarks = VaultBookmarkStore()
    private let log = Logger(subsystem: Logging.subsystem, category: "app")

    struct ExternalChange: Identifiable, Equatable {
        let id = UUID()
        let path: String
        let openedModifiedMs: Int64
        let currentModifiedMs: Int64
        /// What the user typed, held so that "keep mine" still can.
        let localContent: String
    }

    // MARK: - Launch

    func start() async {
        knownVaults = await bookmarks.all()
        guard let mostRecent = knownVaults.first else {
            stage = .choosingVault(reason: nil)
            return
        }
        await reopen(mostRecent)
    }

    func reopen(_ bookmark: VaultBookmark) async {
        stage = .opening(name: bookmark.name)
        do {
            let opened = try await service.reopen(bookmark)
            await finishOpening(opened)
        } catch {
            log.error("could not reopen a vault: \(error.localizedDescription, privacy: .public)")
            // Not a dead end: the folder may have moved or its provider signed
            // out, and picking it again is the fix.
            knownVaults = await bookmarks.all()
            stage = .choosingVault(reason: error.localizedDescription)
        }
    }

    func open(pickedURL url: URL) async {
        stage = .opening(name: url.lastPathComponent)
        do {
            await finishOpening(try await service.open(pickedURL: url))
        } catch {
            stage = .choosingVault(reason: error.localizedDescription)
        }
    }

    func createVault(at url: URL, named name: String) async {
        stage = .opening(name: name)
        do {
            await finishOpening(try await service.open(pickedURL: url, createNamed: name))
        } catch {
            stage = .choosingVault(reason: error.localizedDescription)
        }
    }

    private func finishOpening(_ opened: VaultService.OpenVault) async {
        vault = opened
        knownVaults = await bookmarks.all()

        if opened.needsFullScan {
            stage = .indexing(progress: nil, scanned: 0)
        }
        await runScan(reason: .opening)

        // Only after the index exists: asking about unsaved work before the
        // vault is readable would be asking about something we cannot compare.
        recoverable = (try? await service.recoverable()) ?? []
        diagnostics = (try? await service.diagnostics()) ?? []

        await startWatching()
        stage = .ready
    }

    private func runScan(reason: VaultService.ScanReason) async {
        let progress = ScanProgressRelay { [weak self] update in
            Task { @MainActor in
                guard let self else { return }
                let fraction = update.total.map { total in
                    total > 0 ? Double(update.scanned) / Double(total) : 0
                }
                self.stage = .indexing(progress: fraction, scanned: Int(update.scanned))
            }
        }
        do {
            _ = try await service.scan(reason: reason, progress: progress.report)
        } catch {
            log.error("the scan failed: \(error.localizedDescription, privacy: .public)")
            stage = .failed(error.localizedDescription)
        }
    }

    // MARK: - Watching

    private func startWatching() async {
        do {
            try await service.startWatching { [weak self] events in
                Task { @MainActor in await self?.applyExternalEvents(events) }
            }
        } catch {
            // Not fatal: the vault is still readable, it just will not notice
            // a change made elsewhere until the next foreground scan. Worth
            // saying rather than silently degrading.
            log.error("watching is unavailable: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func applyExternalEvents(_ events: [FsEvent]) async {
        guard !events.isEmpty else { return }
        if events.contains(.rescan) {
            await runScan(reason: .watcherLostTrack)
            stage = .ready
            return
        }
        _ = try? await service.applyExternalEvents(events)
    }

    // MARK: - Lifecycle

    func scenePhaseChanged(to phase: ScenePhase) async {
        switch phase {
        case .inactive:
            // The last moment guaranteed before suspension: whatever is in the
            // editor goes to the recovery journal now.
            NotificationCenter.default.post(name: .vaultShouldFlushBuffers, object: nil)
        case .background:
            await service.stopWatching()
        case .active:
            guard case .ready = stage else { return }
            await service.timeZoneChanged()
            // Anything could have changed while suspended, so do not trust the
            // cache; the scan is incremental and costs a directory walk.
            await runScan(reason: .returningToForeground)
            await startWatching()
            stage = .ready
        @unknown default:
            break
        }
    }

    func forget(_ bookmark: VaultBookmark) async {
        await bookmarks.forget(vaultID: bookmark.vaultID)
        knownVaults = await bookmarks.all()
    }
}

extension Notification.Name {
    /// Sent when the app is about to lose the foreground. Anything holding
    /// unsaved text writes it to the recovery journal.
    static let vaultShouldFlushBuffers = Notification.Name("vault.flushBuffers")
}

/// Hops scan progress back to the main actor without the service knowing about it.
private final class ScanProgressRelay: @unchecked Sendable {
    private let handler: @Sendable (IndexProgress) -> Void
    init(_ handler: @escaping @Sendable (IndexProgress) -> Void) { self.handler = handler }
    var report: @Sendable (IndexProgress) -> Void { handler }
}

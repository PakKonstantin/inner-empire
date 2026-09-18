import Foundation
import OSLog

/// Writing what was shared into the vault.
///
/// Opens the vault, writes, closes. The extension has a much smaller memory
/// limit than the app, and holding an open index across a share is what gets
/// it killed — so this does the least it can and lets the app reindex later.
///
/// It reaches the vault through the same bookmark and the same host config the
/// app uses, both of which live in the shared App Group. Without that it could
/// not find the vault at all.
enum ShareWriter {
    enum Failure: LocalizedError {
        case noVault
        case bookmarkUnusable

        var errorDescription: String? {
            switch self {
            case .noVault:
                String(localized: "Open Inner Empire and choose your vault first.")
            case .bookmarkUnusable:
                // Only the app can renew a bookmark, because only the app can
                // ask. Saying so beats failing with something unactionable.
                String(localized: "Open Inner Empire once to reconnect your vault, then try again.")
            }
        }
    }

    private static let log = Logger(subsystem: Logging.subsystem, category: "share")

    static func save(
        text: String,
        attachments: [SharedAttachment],
        destination: ShareView.Destination
    ) async throws {
        let store = VaultBookmarkStore()
        guard let bookmark = await store.all()
            .max(by: { $0.lastOpened < $1.lastOpened })
        else {
            throw Failure.noVault
        }

        let access: VaultAccess
        do {
            access = try VaultAccess.resolve(bookmark) { refreshed in
                // Renewed while access is held, which is the only time it can
                // be. Storing it saves the app a re-pick later.
                Task {
                    await store.save(
                        VaultBookmark(
                            vaultID: bookmark.vaultID,
                            name: bookmark.name,
                            data: refreshed,
                            lastOpened: bookmark.lastOpened
                        )
                    )
                }
            }
        } catch {
            throw Failure.bookmarkUnusable
        }
        defer { access.stop() }

        // The same verdict and the same config the app reaches, from the same
        // code: one process coordinating its reads while the other did not
        // would be a data race nobody could reproduce.
        let kind = VaultService.storageKind(for: access.url)
        let storage = CoordinatedVaultStorage(root: access.url, kind: kind)
        let config = try VaultService.hostConfig(kind: kind)

        let report = OpenReportSink()
        let handle = try storage.coordinatingRead {
            try VaultHandle.openReporting(
                config: config,
                path: access.url.path(percentEncoded: false),
                storage: kind == .fileProvider ? BridgeStorageHost(storage: storage) : nil,
                report: report
            )
        }
        // No explicit close: the handle is released when this scope ends, and
        // the extension never starts a watch that would outlive it.

        var body = text.trimmingCharacters(in: .whitespacesAndNewlines)

        // Attachments first, so their embeds go into the same write and the
        // note never references a file that is not there yet.
        for attachment in attachments {
            guard case .file(let url) = attachment.payload else { continue }
            let bytes = try Data(contentsOf: url)
            let stored = try storage.coordinatingRead {
                try handle.importAttachment(
                    fileName: url.lastPathComponent,
                    bytes: bytes,
                    note: nil
                )
            }
            let embed = try handle.embedFor(attachment: stored)
            body += body.isEmpty ? embed : "\n\n\(embed)"
        }

        guard !body.isEmpty else { return }

        try storage.coordinatingRead {
            switch destination {
            case .daily:
                let daily = try handle.openDailyNote(dayOffset: 0)
                let existing = try handle.readNote(path: daily.path).content
                // Appended, never overwritten: the day's note belongs to the
                // user and a share is a visitor in it.
                let separator = existing.isEmpty || existing.hasSuffix("\n") ? "" : "\n"
                try handle.forceSaveNote(
                    path: daily.path,
                    content: existing + separator + "\n" + body + "\n"
                )
            case .newNote:
                _ = try handle.createNote(
                    path: "\(titleFor(body)).md",
                    content: body + "\n",
                    collision: .rename
                )
            }
        }
        log.notice("saved a share into the vault")
    }

    /// A title from the first line, so a new note is findable by name rather
    /// than being called "Untitled 4".
    private static func titleFor(_ body: String) -> String {
        let line = body
            .split(separator: "\n", omittingEmptySubsequences: true)
            .first
            .map(String.init) ?? "Shared"
        let cleaned = line
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "#", with: "")
            .trimmingCharacters(in: .whitespaces)
        return cleaned.isEmpty ? "Shared" : String(cleaned.prefix(60))
    }
}

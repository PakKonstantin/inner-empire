import Foundation
import OSLog

/// Remembering where a vault is, without remembering a path.
///
/// iOS hands the app a security-scoped URL from the document picker and then
/// reserves the right to move the container, re-issue the provider's
/// identifiers, or change the path across an OS update. A remembered path
/// therefore points at nothing sooner or later. A bookmark survives all of it,
/// and re-resolving a stale one is routine rather than an error.
///
/// Keyed by the vault's own id — the UUIDv7 in `.inner-empire/vault.json`,
/// which travels with the vault — so the same vault found again after a
/// reinstall matches up with its own index cache and recovery journal.
struct VaultBookmark: Codable, Sendable, Equatable {
    /// `VaultSettings.id` from inside the vault.
    let vaultID: String
    /// A display name for the picker list. Never used to find the vault.
    let name: String
    let data: Data
    let lastOpened: Date
}

/// The stored list of vaults the user has opened.
///
/// `UserDefaults` rather than a file: it is a handful of small records, it is
/// in the app container where it belongs, and it is the one store iOS restores
/// with the app. The vault's *contents* are the source of truth; this is only
/// how to find them again.
actor VaultBookmarkStore {
    private let defaults: UserDefaults
    private let key = "vault.bookmarks.v1"
    private let log = Logger(subsystem: Logging.subsystem, category: "bookmarks")

    /// Shared by default: a share extension cannot read the app's own
    /// `UserDefaults.standard`, so a bookmark stored there would leave it
    /// unable to find the vault at all.
    init(defaults: UserDefaults = AppGroup.defaults) {
        self.defaults = defaults
    }

    func all() -> [VaultBookmark] {
        guard let data = defaults.data(forKey: key) else { return [] }
        do {
            return try JSONDecoder().decode([VaultBookmark].self, from: data)
                .sorted { $0.lastOpened > $1.lastOpened }
        } catch {
            // A decode failure loses the list of vaults, not any vault. Say so
            // and carry on rather than refusing to launch.
            log.error("the stored vault list could not be read: \(error.localizedDescription, privacy: .public)")
            return []
        }
    }

    func bookmark(for vaultID: String) -> VaultBookmark? {
        all().first { $0.vaultID == vaultID }
    }

    func save(_ bookmark: VaultBookmark) {
        var list = all().filter { $0.vaultID != bookmark.vaultID }
        list.insert(bookmark, at: 0)
        persist(list)
    }

    func forget(vaultID: String) {
        persist(all().filter { $0.vaultID != vaultID })
    }

    private func persist(_ list: [VaultBookmark]) {
        do {
            defaults.set(try JSONEncoder().encode(list), forKey: key)
        } catch {
            log.error("the vault list could not be saved: \(error.localizedDescription, privacy: .public)")
        }
    }
}

/// A security-scoped URL, with its access balanced.
///
/// `startAccessingSecurityScopedResource` grants the *process* access to the
/// subtree until the matching stop. That is why the core's ordinary POSIX
/// calls work inside it, and why exactly one owner should hold it: a count
/// spread across call sites is a count that eventually fails to balance.
final class VaultAccess: @unchecked Sendable {
    let url: URL
    private let needsRelease: Bool
    private var released = false
    private let lock = NSLock()

    /// Resolve a bookmark and start accessing what it points at.
    ///
    /// `isStale` is not a failure. It happens when the file moves, when a
    /// provider re-issues its identifiers, and after some OS updates. The
    /// bookmark is re-made and handed back through `refreshed` so the caller
    /// can store it; the vault should then be rescanned, because anything
    /// could have changed while the app was not looking.
    static func resolve(
        _ bookmark: VaultBookmark,
        refreshed: (Data) -> Void
    ) throws -> VaultAccess {
        var stale = false
        let url: URL
        do {
            url = try URL(
                resolvingBookmarkData: bookmark.data,
                options: [],
                relativeTo: nil,
                bookmarkDataIsStale: &stale
            )
        } catch {
            throw VaultStorageError.bookmarkUnusable
        }

        let started = url.startAccessingSecurityScopedResource()
        guard started else {
            throw VaultStorageError.accessDenied(url.lastPathComponent)
        }

        if stale {
            // Re-made while access is held, which is the only time it can be.
            if let fresh = try? url.bookmarkData() {
                refreshed(fresh)
            }
        }
        return VaultAccess(url: url, needsRelease: true)
    }

    /// Start accessing a URL the picker just handed over.
    static func begin(at url: URL) throws -> VaultAccess {
        // A URL the app already owns — its own container, or a test directory —
        // is not security-scoped, and `start` returning false for one of those
        // is not a refusal. Distinguish by asking whether it is inside our own
        // container rather than by ignoring the result.
        let started = url.startAccessingSecurityScopedResource()
        if !started && !Self.isInsideOwnContainer(url) {
            throw VaultStorageError.accessDenied(url.lastPathComponent)
        }
        return VaultAccess(url: url, needsRelease: started)
    }

    private init(url: URL, needsRelease: Bool) {
        self.url = url
        self.needsRelease = needsRelease
    }

    /// Make a bookmark for this vault while access is held.
    func bookmarkData() throws -> Data {
        do {
            return try url.bookmarkData()
        } catch {
            throw VaultStorageError.bookmarkUnusable
        }
    }

    func stop() {
        lock.lock()
        defer { lock.unlock() }
        guard !released else { return }
        released = true
        if needsRelease {
            url.stopAccessingSecurityScopedResource()
        }
    }

    deinit {
        // Not a substitute for `stop()` — it is the backstop for a path that
        // forgot. An unbalanced access leaks a sandbox extension for the life
        // of the process.
        if !released && needsRelease {
            url.stopAccessingSecurityScopedResource()
        }
    }

    private static func isInsideOwnContainer(_ url: URL) -> Bool {
        let home = URL(fileURLWithPath: NSHomeDirectory()).standardizedFileURL
            .path(percentEncoded: false)
        return url.standardizedFileURL.path(percentEncoded: false).hasPrefix(home)
    }
}

enum Logging {
    static let subsystem = Bundle.main.bundleIdentifier ?? "com.innerempire.app"
}

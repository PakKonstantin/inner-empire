import Foundation

/// Everything the application does to a vault's files.
///
/// The core does not see this protocol — it talks to its own `FileSystem`
/// trait in Rust, and knows nothing about UIKit, SwiftUI or `NSFileCoordinator`.
/// This is the Swift side of the same idea: one place where file access is
/// defined, so a document picker, a share extension and a test can each supply
/// their own and nothing above them changes.
///
/// Paths are vault-relative and `/`-separated, matching `VaultPath` in the
/// core. Nothing here accepts or returns an absolute path, which is what stops
/// one being persisted by accident — on iOS the container path is reassigned
/// between launches, so a remembered absolute path points at nothing.
protocol VaultStorage: Sendable {
    /// Where the vault is, for the one call that has to hand Rust a path.
    /// Valid only while access is held.
    var root: URL { get }

    func readFile(at path: String) throws -> Data
    func writeFile(_ data: Data, to path: String) throws
    func createFile(_ data: Data, at path: String) throws
    func deleteFile(at path: String) throws
    func moveFile(from: String, to: String) throws
    func renameFile(at path: String, to newName: String) throws
    func listDirectory(at path: String) throws -> [VaultEntry]
    func fileExists(at path: String) -> Bool

    /// Run `body` inside whatever bracket this storage needs — a coordinated
    /// read for a provider-backed vault, nothing at all for a local folder.
    ///
    /// This is the seam that keeps a thousand-note scan at one coordination
    /// bracket rather than four thousand: the bracket wraps one call into the
    /// core, and the core does its own POSIX I/O inside it.
    func coordinatingRead<T>(_ body: () throws -> T) throws -> T
    func coordinatingWrite<T>(at path: String, _ body: () throws -> T) throws -> T

    /// Observe changes made by anything else — the Files app, iCloud, a Mac
    /// syncing the same folder. Returns a token that stops watching when it is
    /// released.
    func watchChanges(_ onChange: @escaping @Sendable ([VaultChange]) -> Void) throws -> VaultWatchToken
}

/// One entry in a directory listing.
struct VaultEntry: Sendable, Hashable {
    let path: String
    let name: String
    let isDirectory: Bool
    let size: Int64
    let modified: Date?
    /// True when the file's contents are not on the device yet. Listed anyway:
    /// an evicted note is still a note, and hiding it would be a lie.
    let isPlaceholder: Bool
}

/// A change observed in the vault.
enum VaultChange: Sendable, Hashable {
    case created(String)
    case modified(String)
    case deleted(String)
    case moved(from: String, to: String)
    /// Tracking was lost — the app was suspended, or a bookmark went stale.
    /// The only safe response is to walk the vault again.
    case lost
}

/// Stops a watch when released.
protocol VaultWatchToken: AnyObject, Sendable {
    func stop()
}

enum VaultStorageError: LocalizedError, Equatable {
    case accessDenied(String)
    case bookmarkUnusable
    case notFound(String)
    case escapesVault(String)
    case notDownloadable(String)
    case underlying(String)

    var errorDescription: String? {
        switch self {
        case .accessDenied(let path):
            String(localized: "Inner Empire is not allowed to open \(path).")
        case .bookmarkUnusable:
            String(localized: "The vault could not be found. It may have been moved, deleted, or its provider signed out.")
        case .notFound(let path):
            String(localized: "\(path) is not in this vault.")
        case .escapesVault(let path):
            String(localized: "\(path) points outside the vault.")
        case .notDownloadable(let path):
            String(localized: "\(path) could not be downloaded from iCloud.")
        case .underlying(let message):
            message
        }
    }
}

// MARK: - Path handling

/// Vault-relative paths, kept honest in one place.
///
/// The core re-parses and re-validates every path it is given, so this is not
/// the security boundary — it is the layer that stops obvious mistakes turning
/// into confusing errors from Rust, and that guarantees the string handed
/// across the bridge is the `/`-separated form the core expects.
enum VaultPathUtil {
    /// Normalise a relative path, refusing anything that would escape.
    static func normalize(_ path: String) throws -> String {
        let unified = path.replacingOccurrences(of: "\\", with: "/")
        var components: [String] = []
        for component in unified.split(separator: "/", omittingEmptySubsequences: true) {
            switch component {
            case ".":
                continue
            case "..":
                // Refused rather than resolved. A path containing `..` is
                // either a mistake or an escape attempt, and both deserve to
                // be visible.
                throw VaultStorageError.escapesVault(path)
            default:
                components.append(String(component))
            }
        }
        // Unicode normalisation matches the core, so a filename typed on iOS
        // and one typed on Linux compare equal even when composed differently.
        return components.joined(separator: "/").precomposedStringWithCanonicalMapping
    }

    /// Resolve against a root, refusing anything that lands outside it.
    static func resolve(_ path: String, under root: URL) throws -> URL {
        let relative = try normalize(path)
        guard !relative.isEmpty else { return root }
        let url = root.appending(path: relative, directoryHint: .notDirectory)

        // Belt and braces: `normalize` already refused `..`, but a symlink or a
        // provider's own path rewriting could still land elsewhere.
        let rootPath = root.standardizedFileURL.path(percentEncoded: false)
        let target = url.standardizedFileURL.path(percentEncoded: false)
        guard target == rootPath || target.hasPrefix(rootPath + "/") else {
            throw VaultStorageError.escapesVault(path)
        }
        return url
    }

    /// The vault-relative form of an absolute URL inside the vault.
    static func relative(_ url: URL, under root: URL) -> String? {
        let rootPath = root.standardizedFileURL.path(percentEncoded: false)
        let target = url.standardizedFileURL.path(percentEncoded: false)
        guard target.hasPrefix(rootPath) else { return nil }
        let suffix = target.dropFirst(rootPath.count)
        return String(suffix.drop(while: { $0 == "/" })).precomposedStringWithCanonicalMapping
    }
}

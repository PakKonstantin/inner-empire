import Foundation
import OSLog

/// The container the app and its extensions share.
///
/// Two things have to cross that boundary, and neither works without this:
///
/// - **The vault bookmark.** A share extension cannot read the app's
///   `UserDefaults.standard`, so without a shared suite it has no way to find
///   the vault at all and every share fails.
/// - **The index.** Each process has its own Library directory, so an
///   extension using its own would build a *second* copy of the index — slow,
///   wasteful, and pointless work for a share sheet that lives three seconds.
///
/// Falls back to the app's own container when the group is unavailable, which
/// happens in tests and when the entitlement is missing. The app still works;
/// the extension is the part that needs it, and it says so rather than
/// failing obscurely.
enum AppGroup {
    /// Derived from the bundle identifier rather than written out, because a
    /// literal here is a fourth place to keep in step with the two
    /// entitlements files and the xcconfig — and when they drift, the symptom
    /// is every share failing with "choose your vault first", which points
    /// nowhere near the cause.
    ///
    /// The extension's identifier ends in `.share`, so it is trimmed back to
    /// the app's to reach the same group.
    static let identifier: String = {
        let bundle = Bundle.main.bundleIdentifier ?? "com.innerempire.innerempire"
        let app = bundle.hasSuffix(".share") ? String(bundle.dropLast(6)) : bundle
        return "group.\(app)"
    }()

    static var containerURL: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: identifier)
    }

    /// Where shared preferences live.
    static var defaults: UserDefaults {
        guard let shared = UserDefaults(suiteName: identifier) else {
            Logger(subsystem: Logging.subsystem, category: "appgroup")
                .warning("no shared defaults; extensions will not find the vault")
            return .standard
        }
        return shared
    }

    /// Where the index cache belongs: shared, so there is one of it.
    static var libraryURL: URL? {
        guard let container = containerURL else { return nil }
        let library = container.appendingPathComponent("Library", isDirectory: true)
        try? FileManager.default.createDirectory(at: library, withIntermediateDirectories: true)
        return library
    }
}

import Foundation
import UniformTypeIdentifiers

/// Something handed over by another app.
struct SharedAttachment: Identifiable {
    enum Payload {
        case text(String)
        case file(URL)
    }

    let id = UUID()
    let name: String
    let payload: Payload

    var symbol: String {
        switch payload {
        case .text: "text.alignleft"
        case .file(let url):
            UTType(filenameExtension: url.pathExtension)?.conforms(to: .image) == true
                ? "photo"
                : "doc"
        }
    }

    /// Pull one item out of a provider.
    ///
    /// URLs are checked before plain text, because a shared web page offers
    /// both and the URL is what the user meant — text would give the page
    /// title alone.
    static func from(_ provider: NSItemProvider) async -> SharedAttachment? {
        if provider.hasItemConformingToTypeIdentifier(UTType.url.identifier),
           let url: URL = await load(provider, UTType.url) {
            return url.isFileURL
                ? SharedAttachment(name: url.lastPathComponent, payload: .file(url))
                : SharedAttachment(name: url.absoluteString, payload: .text(url.absoluteString))
        }
        if provider.hasItemConformingToTypeIdentifier(UTType.image.identifier),
           let url: URL = await loadFile(provider, UTType.image) {
            return SharedAttachment(name: url.lastPathComponent, payload: .file(url))
        }
        if provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier),
           let text: String = await load(provider, UTType.plainText) {
            return SharedAttachment(name: "Text", payload: .text(text))
        }
        if provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier),
           let url: URL = await loadFile(provider, UTType.fileURL) {
            return SharedAttachment(name: url.lastPathComponent, payload: .file(url))
        }
        return nil
    }

    private static func load<T>(_ provider: NSItemProvider, _ type: UTType) async -> T? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: type.identifier) { item, _ in
                continuation.resume(returning: item as? T)
            }
        }
    }

    /// Copy the file somewhere this process still owns once the provider's
    /// temporary URL goes away.
    private static func loadFile(_ provider: NSItemProvider, _ type: UTType) async -> URL? {
        await withCheckedContinuation { continuation in
            provider.loadFileRepresentation(forTypeIdentifier: type.identifier) { url, _ in
                guard let url else {
                    continuation.resume(returning: nil)
                    return
                }
                // The provider deletes its copy as soon as this closure
                // returns, so it has to be copied now rather than read later.
                let copy = FileManager.default.temporaryDirectory
                    .appendingPathComponent(UUID().uuidString)
                    .appendingPathExtension(url.pathExtension)
                try? FileManager.default.copyItem(at: url, to: copy)
                continuation.resume(returning: copy)
            }
        }
    }
}

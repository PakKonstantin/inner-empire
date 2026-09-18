import SwiftUI

/// The file tree, one level at a time.
///
/// Fetched per folder rather than as a whole tree: a vault with fifty thousand
/// files should cost one directory read to open, not fifty thousand.
struct NotesView: View {
    @Environment(AppModel.self) private var model
    @Binding var selection: String?

    @State private var folder = ""
    @State private var listing: DirectoryListing?
    @State private var loadError: String?

    init(selection: Binding<String?> = .constant(nil)) {
        _selection = selection
    }

    var body: some View {
        List(selection: $selection) {
            if let listing {
                if listing.folders.isEmpty && listing.files.isEmpty {
                    ContentUnavailableView(
                        "Nothing here yet",
                        systemImage: "doc.text",
                        description: Text("Create a note to get started.")
                    )
                }
                ForEach(listing.folders, id: \.path) { child in
                    NavigationLink(value: Route.folder(child.path)) {
                        Label {
                            VStack(alignment: .leading) {
                                Text(child.name)
                                Text(childSummary(child))
                                    .font(.caption)
                                    .foregroundStyle(DesignTokens.textMuted.color)
                            }
                        } icon: {
                            Image(systemName: "folder")
                        }
                    }
                }
                ForEach(listing.files, id: \.path) { file in
                    NavigationLink(value: Route.note(file.path)) {
                        Label {
                            // The title, which comes from frontmatter when the
                            // note has one — not the filename, which is often
                            // a slug nobody reads.
                            Text(file.title)
                        } icon: {
                            Image(systemName: icon(for: file.kind))
                        }
                    }
                    .tag(file.path)
                }
            } else if let loadError {
                ContentUnavailableView(
                    "This folder could not be read",
                    systemImage: "exclamationmark.triangle",
                    description: Text(loadError)
                )
            } else {
                ProgressView()
            }
        }
        .navigationTitle(folder.isEmpty ? (model.vault?.name ?? "Notes") : lastComponent(folder))
        .navigationDestination(for: Route.self) { route in
            switch route {
            case .folder(let path):
                NotesView(selection: $selection).task { await load(path) }
            case .note(let path):
                NoteDetailPlaceholder(path: path)
            }
        }
        .task { await load(folder) }
        .refreshable { await load(folder) }
    }

    enum Route: Hashable {
        case folder(String)
        case note(String)
    }

    private func load(_ path: String) async {
        folder = path
        do {
            listing = try await model.service.listDirectory(at: path)
            loadError = nil
        } catch {
            listing = nil
            loadError = error.localizedDescription
        }
    }

    private func childSummary(_ folder: FolderEntry) -> String {
        // Counted rather than pluralised by hand, so it reads correctly at
        // one as well as at none.
        let files = String(localized: "\(Int(folder.childFileCount)) notes")
        let folders = String(localized: "\(Int(folder.childFolderCount)) folders")
        return folder.childFolderCount == 0 ? files : "\(files), \(folders)"
    }

    private func lastComponent(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    private func icon(for kind: FileKind) -> String {
        switch kind {
        case .note: "doc.text"
        case .canvas: "rectangle.3.group"
        case .image: "photo"
        case .pdf: "doc.richtext"
        case .audio: "waveform"
        case .video: "film"
        case .other: "doc"
        }
    }
}

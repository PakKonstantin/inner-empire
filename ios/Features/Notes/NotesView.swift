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
    @State private var favourites: [FileEntry] = []
    @State private var pendingMove: PendingMove?
    @State private var moveError: String?
    @State private var newFolderName = ""
    @State private var creatingFolder = false
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
                // Only at the vault root: a folder deep in the tree showing
                // the whole vault's favourites would be noise, not help.
                if folder.isEmpty && !favourites.isEmpty {
                    Section("Favourites") {
                        ForEach(favourites, id: \.path) { note in
                            NavigationLink(value: Route.note(note.path)) {
                                Label(note.title, systemImage: "star.fill")
                            }
                        }
                    }
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
                    .dropDestination(for: String.self) { paths, _ in
                        Task { await move(paths, into: child.path) }
                        return true
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
                    // The path is the payload. A note dragged into another
                    // app gets its text, which is what a plain-text file
                    // should offer; within the app the path is what a folder
                    // needs to move it.
                    .draggable(file.path) {
                        Label(file.title, systemImage: icon(for: file.kind))
                    }
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
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    creatingFolder = true
                } label: {
                    Image(systemName: "folder.badge.plus")
                }
                .accessibilityLabel(Text("New folder"))
            }
        }
        .alert("New folder", isPresented: $creatingFolder) {
            TextField("Name", text: $newFolderName)
                .autocorrectionDisabled()
            Button("Cancel", role: .cancel) { newFolderName = "" }
            Button("Create") { Task { await createFolder() } }
        }
        // A drag that would rewrite links in many notes asks first: an
        // accidental drag on a touch screen is far easier than an accidental
        // menu choice, and undoing a link rewrite by hand is miserable.
        .alert(item: $pendingMove) { move in
            Alert(
                title: Text("Move this note?"),
                message: Text("\(move.linkCount) links in \(move.noteCount) notes will be updated to point at its new place."),
                primaryButton: .default(Text("Move")) {
                    Task { await performMove(move.from, to: move.to) }
                },
                secondaryButton: .cancel()
            )
        }
        .alert(
            "That note could not be moved",
            isPresented: .init(get: { moveError != nil }, set: { if !$0 { moveError = nil } })
        ) {
            Button("OK") { moveError = nil }
        } message: {
            Text(moveError ?? "")
        }
        .navigationDestination(for: Route.self) { route in
            switch route {
            case .folder(let path):
                NotesView(selection: $selection).task { await load(path) }
            case .note(let path):
                NoteEditorView(path: path)
            }
        }
        .task {
            await load(folder)
            // Only the root needs them, so nothing else pays for the query.
            if folder.isEmpty {
                favourites = (try? await model.service.favourites()) ?? []
            }
        }
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


private extension NotesView {
}


private extension NotesView {
    /// Move dragged notes into `folder`.
    ///
    /// A move rewrites every link that pointed at the note, which can touch
    /// many files. The plan is consulted first so a drag that would edit
    /// dozens of notes asks before doing it — an accidental drag on a touch
    /// screen is far easier than an accidental menu choice.
    func move(_ paths: [String], into folder: String) async {
        for path in paths {
            let destination = "\(folder)/\((path as NSString).lastPathComponent)"
            guard destination != path else { continue }

            if let plan = try? await model.service.planMove(path, to: destination),
               plan.totalLinks > linkEditThreshold {
                pendingMove = PendingMove(
                    from: path,
                    to: destination,
                    linkCount: Int(plan.totalLinks),
                    noteCount: plan.edits.count
                )
                continue
            }
            await performMove(path, to: destination)
        }
    }

    func performMove(_ path: String, to destination: String) async {
        do {
            try await model.service.move(path, to: destination)
            await load(folder)
        } catch {
            moveError = error.localizedDescription
        }
    }

    /// Above this many links, a drag asks first.
    var linkEditThreshold: UInt32 { 5 }

    func createFolder() async {
        let name = newFolderName.trimmingCharacters(in: .whitespaces)
        newFolderName = ""
        guard !name.isEmpty else { return }
        let path = folder.isEmpty ? name : "\(folder)/\(name)"
        do {
            try await model.service.createFolder(at: path)
            await load(folder)
        } catch {
            // Including a case collision: `Notes` and `notes` in one folder
            // make the vault unopenable on a case-folding filesystem, and the
            // core refuses rather than letting it happen.
            moveError = error.localizedDescription
        }
    }
}

/// A move waiting to be confirmed because of how much it would rewrite.
struct PendingMove: Identifiable {
    let id = UUID()
    let from: String
    let to: String
    let linkCount: Int
    let noteCount: Int
}

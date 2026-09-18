import SwiftUI

/// Every tag in the vault, with its nesting.
///
/// Nested tags are stored whole — `project/alpha` is one tag, not two — and
/// the hierarchy is derived here for display. That keeps the file format
/// simple and the tree a presentation concern, which is where it belongs.
struct TagBrowserView: View {
    @Environment(AppModel.self) private var model

    @State private var tags: [TagSummary] = []
    @State private var filter = ""

    var body: some View {
        List {
            ForEach(roots, id: \.name) { tag in
                TagRow(tag: tag, children: children(of: tag.name))
            }
        }
        .navigationTitle("Tags")
        // The rows navigated to a route nothing handled, so tapping a tag did
        // nothing at all. This is the screen they always implied.
        .navigationDestination(for: TagRoute.self) { route in
            TaggedNotesView(tag: route.tag)
        }
        .searchable(text: $filter, prompt: Text("Filter tags"))
        .overlay {
            if tags.isEmpty {
                ContentUnavailableView(
                    "No tags yet",
                    systemImage: "number",
                    description: Text("Write #something in a note and it appears here.")
                )
            }
        }
        .task { tags = (try? await model.service.tags()) ?? [] }
    }

    private var matching: [TagSummary] {
        guard !filter.isEmpty else { return tags }
        return tags.filter { $0.name.localizedCaseInsensitiveContains(filter) }
    }

    private var roots: [TagSummary] {
        matching
            .filter { !$0.name.contains("/") }
            .sorted { $0.totalCount > $1.totalCount }
    }

    private func children(of parent: String) -> [TagSummary] {
        matching
            .filter { $0.name.hasPrefix("\(parent)/") }
            .sorted { $0.name < $1.name }
    }
}

private struct TagRow: View {
    let tag: TagSummary
    let children: [TagSummary]

    var body: some View {
        if children.isEmpty {
            row(for: tag, indented: false)
        } else {
            DisclosureGroup {
                ForEach(children, id: \.name) { child in
                    row(for: child, indented: true)
                }
            } label: {
                row(for: tag, indented: false)
            }
        }
    }

    private func row(for tag: TagSummary, indented: Bool) -> some View {
        NavigationLink(value: TagRoute(tag: tag.name)) {
            HStack {
                Text(indented ? lastComponent(tag.name) : tag.name)
                Spacer()
                // Both numbers when they differ: "3" on a parent that covers
                // forty notes in its children is misleading on its own.
                Text(tag.count == tag.totalCount
                     ? "\(Int(tag.count))"
                     : "\(Int(tag.count)) · \(Int(tag.totalCount))")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(DesignTokens.textMuted.color)
            }
        }
        .accessibilityLabel(Text(tag.count == tag.totalCount
            ? "\(tag.name), \(Int(tag.count)) notes"
            : "\(tag.name), \(Int(tag.count)) notes directly, \(Int(tag.totalCount)) including nested"))
    }

    private func lastComponent(_ name: String) -> String {
        name.split(separator: "/").last.map(String.init) ?? name
    }
}


/// Which tag a row leads to.
struct TagRoute: Hashable {
    let tag: String
}

/// The notes carrying one tag.
struct TaggedNotesView: View {
    @Environment(AppModel.self) private var model
    let tag: String

    @State private var notes: [FileEntry] = []
    @State private var loaded = false

    var body: some View {
        List(notes, id: \.path) { note in
            NavigationLink(value: note.path) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(note.title)
                    Text(note.path)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.textFaint.color)
                        .lineLimit(1)
                }
            }
        }
        .navigationTitle("#\(tag)")
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(for: String.self) { NoteEditorView(path: $0) }
        .overlay {
            if notes.isEmpty && loaded {
                ContentUnavailableView(
                    "No notes with this tag",
                    systemImage: "number",
                    description: Text("It may have been removed since the list was built.")
                )
            }
        }
        .task {
            // Nested tags included, because the counts in the browser already
            // promise that a parent covers its children.
            notes = (try? await model.service.notes(withTag: tag)) ?? []
            loaded = true
        }
    }
}

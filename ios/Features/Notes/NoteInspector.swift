import SwiftUI

/// Backlinks and the outline, for the note being read.
///
/// A sheet on iPhone and a column on iPad, rather than a tab: these are facts
/// *about the current note*, and a destination competing with it in the tab
/// bar would make them harder to reach, not easier.
struct NoteInspector: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    let path: String
    /// True when shown as a sheet, which needs its own dismiss control.
    var presentedModally = false

    @State private var backlinks: [Backlink] = []
    @State private var outgoing: [ResolvedLink] = []
    @State private var outline: [Heading] = []
    @State private var loaded = false

    var body: some View {
        List {
            Section {
                if backlinks.isEmpty && loaded {
                    Text("Nothing links here yet.")
                        .foregroundStyle(DesignTokens.textMuted.color)
                }
                ForEach(Array(backlinks.enumerated()), id: \.offset) { _, link in
                    NavigationLink(value: link.sourcePath) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(link.sourceTitle).font(.callout)
                            Text(link.context)
                                .font(.caption)
                                .foregroundStyle(DesignTokens.textMuted.color)
                                .lineLimit(2)
                        }
                    }
                }
            } header: {
                Text("Linked from \(backlinks.count)")
            }

            Section("Outline") {
                if outline.isEmpty && loaded {
                    Text("This note has no headings.")
                        .foregroundStyle(DesignTokens.textMuted.color)
                }
                ForEach(Array(outline.enumerated()), id: \.offset) { _, heading in
                    Text(heading.text)
                        // Indented by depth, which is what makes an outline
                        // readable as structure rather than a flat list.
                        .padding(.leading, CGFloat(heading.level - 1) * DesignTokens.spacingMd)
                        .font(heading.level <= 2 ? .callout.weight(.medium) : .callout)
                        .accessibilityLabel(Text("Heading level \(Int(heading.level)), \(heading.text)"))
                }
            }

            Section("Links out") {
                if outgoing.isEmpty && loaded {
                    Text("This note links nowhere yet.")
                        .foregroundStyle(DesignTokens.textMuted.color)
                }
                ForEach(Array(outgoing.enumerated()), id: \.offset) { _, link in
                    if let target = link.targetPath {
                        NavigationLink(value: target) {
                            Label(link.link.alias ?? link.link.target, systemImage: "arrow.up.right")
                        }
                    } else {
                        // An unresolved link is shown, not hidden: it is what
                        // the UI offers to turn into a new note, and hiding it
                        // would make a typo invisible.
                        Label {
                            VStack(alignment: .leading) {
                                Text(link.link.target)
                                Text("No note with this name")
                                    .font(.caption)
                                    .foregroundStyle(DesignTokens.textMuted.color)
                            }
                        } icon: {
                            Image(systemName: "questionmark.circle")
                                .foregroundStyle(DesignTokens.linkUnresolvedColor.color)
                        }
                    }
                }
            }
        }
        .navigationTitle("Connections")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if presentedModally {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .task { await load() }
    }

    private func load() async {
        backlinks = (try? await model.service.backlinks(to: path)) ?? []
        outgoing = (try? await model.service.outgoingLinks(from: path)) ?? []
        outline = (try? await model.service.outline(of: path)) ?? []
        loaded = true
    }
}

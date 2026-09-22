import SwiftUI

/// Resolving a note that changed in two places.
///
/// Three actions, always all three, and never a default that discards. The
/// diff comes from the core so the choice is between visible alternatives
/// rather than two opaque blobs — "keep mine" with nothing shown is not a
/// choice, it is a coin toss the user is asked to call.
struct ConflictView: View {
    @Environment(\.dismiss) private var dismiss

    let path: String
    let local: String
    let remote: String
    let onKeepLocal: () -> Void
    let onKeepRemote: () -> Void
    let onMerge: (String) -> Void

    @State private var comparing = false
    /// Per hunk: true keeps the local version, false keeps the one on disk.
    @State private var keepLocalForHunk: [Int: Bool] = [:]

    private var diff: TextDiff {
        diffText(local: local, remote: remote, contextLines: 3)
    }

    var body: some View {
        NavigationStack {
            Group {
                if comparing {
                    comparison
                } else {
                    summary
                }
            }
            .navigationTitle(comparing ? "Compare" : "This note changed")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    // Cancelling changes nothing: the note stays as it is, on
                    // disk and in the editor, and the conflict is still there.
                    Button("Later") { dismiss() }
                }
                if comparing {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Use this") {
                            onMerge(merged)
                            dismiss()
                        }
                    }
                }
            }
        }
    }

    private var summary: some View {
        List {
            Section {
                Text("\(path) was changed somewhere else while you had it open — in the Files app, by iCloud, or on another device.")
                Text("Nothing has been overwritten.")
                    .font(.footnote)
                    .foregroundStyle(DesignTokens.textMuted.color)
            }

            Section("What changed") {
                LabeledContent("Lines you added", value: "\(diff.added)")
                LabeledContent("Lines on the other version", value: "\(diff.removed)")
            }

            Section {
                Button {
                    comparing = true
                } label: {
                    Label("Compare them", systemImage: "arrow.left.arrow.right")
                }
                Button {
                    onKeepLocal()
                    dismiss()
                } label: {
                    Label("Keep what I typed", systemImage: "iphone")
                }
                Button {
                    onKeepRemote()
                    dismiss()
                } label: {
                    Label("Keep the other version", systemImage: "icloud")
                }
            } footer: {
                Text("Keeping one replaces the other. Compare first if you are not sure.")
            }
        }
    }

    private var comparison: some View {
        List {
            ForEach(Array(diff.hunks.enumerated()), id: \.offset) { index, hunk in
                Section {
                    ForEach(Array(hunk.lines.enumerated()), id: \.offset) { _, line in
                        DiffLineRow(line: line)
                    }
                } header: {
                    if hunk.hasChanges {
                        Picker("", selection: binding(for: index)) {
                            Text("Mine").tag(true)
                            Text("Theirs").tag(false)
                        }
                        .pickerStyle(.segmented)
                        .textCase(nil)
                    }
                }
            }
        }
    }

    private func binding(for hunk: Int) -> Binding<Bool> {
        Binding(
            get: { keepLocalForHunk[hunk] ?? true },
            set: { keepLocalForHunk[hunk] = $0 }
        )
    }

    /// Assemble the chosen version of each hunk.
    private var merged: String {
        var lines: [String] = []
        for (index, hunk) in diff.hunks.enumerated() {
            let keepLocal = keepLocalForHunk[index] ?? true
            for line in hunk.lines {
                switch line.change {
                case .same:
                    lines.append(line.text)
                case .added where keepLocal:
                    lines.append(line.text)
                case .removed where !keepLocal:
                    lines.append(line.text)
                default:
                    continue
                }
            }
        }
        return lines.joined(separator: "\n")
    }
}

/// One line of the comparison.
///
/// Marked with a symbol and a label as well as a background tint, because a
/// red row and a green row look identical to a good many people.
private struct DiffLineRow: View {
    let line: DiffLine

    var body: some View {
        HStack(alignment: .top, spacing: DesignTokens.spacingSm) {
            Text(marker)
                .font(.caption.monospaced())
                .foregroundStyle(tint)
                .frame(width: 14, alignment: .leading)
                .accessibilityHidden(true)
            Text(line.text.isEmpty ? " " : line.text)
                .font(.callout.monospaced())
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .listRowBackground(background)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text("\(accessibilityPrefix). \(line.text)"))
    }

    private var marker: String {
        switch line.change {
        case .same: " "
        case .added: "+"
        case .removed: "−"
        }
    }

    private var accessibilityPrefix: String {
        switch line.change {
        case .same: String(localized: "Unchanged")
        case .added: String(localized: "Yours")
        case .removed: String(localized: "Theirs")
        }
    }

    private var tint: Color {
        switch line.change {
        case .same: DesignTokens.textMuted.color
        case .added: DesignTokens.textSuccess.color
        case .removed: DesignTokens.textError.color
        }
    }

    private var background: Color? {
        switch line.change {
        case .same: nil
        case .added: DesignTokens.backgroundModifierSuccess.color
        case .removed: DesignTokens.backgroundModifierError.color
        }
    }
}

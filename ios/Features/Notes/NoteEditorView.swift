import SwiftUI
import UIKit

/// Editing one note.
struct NoteEditorView: View {
    @Environment(AppModel.self) private var model
    @State private var buffer: NoteBuffer?
    @State private var loadError: String?
    @State private var showingConflict = false
    @State private var accessory: UIView?

    let path: String

    var body: some View {
        Group {
            if let buffer {
                editor(buffer)
            } else if let loadError {
                ContentUnavailableView(
                    "This note could not be opened",
                    systemImage: "exclamationmark.triangle",
                    description: Text(loadError)
                )
            } else {
                ProgressView()
            }
        }
        .task { await load() }
        .onReceive(NotificationCenter.default.publisher(for: .vaultShouldFlushBuffers)) { _ in
            // The app is losing the foreground. Whatever is typed goes to the
            // recovery journal now, because there may be no later.
            Task { await buffer?.flush() }
        }
    }

    @ViewBuilder
    private func editor(_ buffer: NoteBuffer) -> some View {
        @Bindable var buffer = buffer

        MarkdownTextView(
            text: $buffer.text,
            selection: $buffer.selection,
            isEditable: !buffer.state.isConflicted,
            accessory: accessory
        )
        .navigationTitle(buffer.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                SaveStateBadge(state: buffer.state)
            }
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button {
                        // Backlinks are about the note you are reading, so
                        // they belong to this screen rather than competing
                        // with it in the tab bar.
                    } label: {
                        Label("Backlinks", systemImage: "arrow.turn.up.left")
                    }
                    Button {
                    } label: {
                        Label("Outline", systemImage: "list.bullet.indent")
                    }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .accessibilityLabel(Text("Note actions"))
            }
        }
        .onAppear {
            accessory = makeAccessory(for: buffer)
        }
        .onChange(of: buffer.state) { _, state in
            // A conflict is never resolved silently; the sheet is the only way
            // past it, and it shows both versions before anything is written.
            showingConflict = state.isConflicted
        }
        .sheet(isPresented: $showingConflict) {
            if case .conflicted(let remote) = buffer.state {
                ConflictView(
                    path: path,
                    local: buffer.text,
                    remote: remote,
                    onKeepLocal: { Task { await buffer.overwriteWithLocal() } },
                    onKeepRemote: { Task { await buffer.reloadFromDisk() } },
                    onMerge: { merged in Task { await buffer.resolve(withMerged: merged) } }
                )
            }
        }
    }

    private func makeAccessory(for buffer: NoteBuffer) -> UIView {
        let bar = EditorAccessoryBar(
            onAction: { buffer.apply($0) },
            onDismissKeyboard: {
                UIApplication.shared.sendAction(
                    #selector(UIResponder.resignFirstResponder),
                    to: nil, from: nil, for: nil
                )
            }
        )
        let hosting = UIHostingController(rootView: bar)
        hosting.view.frame = CGRect(x: 0, y: 0, width: 0, height: 48)
        hosting.view.backgroundColor = .clear
        return hosting.view
    }

    private func load() async {
        guard buffer == nil else { return }
        do {
            buffer = NoteBuffer(note: try await model.service.note(at: path), service: model.service)
        } catch {
            loadError = error.localizedDescription
        }
    }
}

extension NoteBuffer.SaveState {
    var isConflicted: Bool {
        if case .conflicted = self { return true }
        return false
    }
}

/// Says whether the note is written, in words as well as a symbol.
///
/// §53 forbids colour as the only carrier of meaning; a coloured dot alone
/// would tell a colour-blind user nothing, and VoiceOver nothing at all.
struct SaveStateBadge: View {
    let state: NoteBuffer.SaveState

    var body: some View {
        Label {
            Text(text)
        } icon: {
            Image(systemName: symbol)
        }
        .labelStyle(.iconOnly)
        .foregroundStyle(tint)
        .accessibilityLabel(Text(text))
    }

    private var text: String {
        switch state {
        case .saved: String(localized: "Saved")
        case .dirty: String(localized: "Unsaved changes")
        case .saving: String(localized: "Saving")
        case .conflicted: String(localized: "Changed on another device")
        case .failed(let message): String(localized: "Not saved: \(message)")
        }
    }

    private var symbol: String {
        switch state {
        case .saved: "checkmark.circle"
        case .dirty: "pencil.circle"
        case .saving: "arrow.triangle.2.circlepath"
        case .conflicted: "exclamationmark.triangle"
        case .failed: "xmark.circle"
        }
    }

    private var tint: Color {
        switch state {
        case .saved: DesignTokens.textMuted.color
        case .dirty, .saving: DesignTokens.textMuted.color
        case .conflicted: DesignTokens.textWarning.color
        case .failed: DesignTokens.textError.color
        }
    }
}

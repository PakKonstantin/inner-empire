import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Saving something into the vault from another app.
///
/// The extension is a separate process with its own memory limit — far lower
/// than the app's — so it deliberately does not open the index. It appends to
/// a note, or writes an attachment, and lets the main app reindex when it next
/// runs. Trying to be the whole app in here is how a share sheet gets killed
/// mid-write.
///
/// Reaching the vault at all needs the security-scoped bookmark, which is why
/// it lives in the shared App Group rather than in the app's own container.
final class ShareViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()

        let content = ShareView(
            items: extensionContext?.inputItems as? [NSExtensionItem] ?? [],
            onFinish: { [weak self] in
                self?.extensionContext?.completeRequest(returningItems: nil)
            },
            onCancel: { [weak self] in
                self?.extensionContext?.cancelRequest(
                    withError: NSError(domain: "ie.share", code: NSUserCancelledError)
                )
            }
        )

        let hosting = UIHostingController(rootView: content)
        addChild(hosting)
        view.addSubview(hosting.view)
        hosting.view.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            hosting.view.topAnchor.constraint(equalTo: view.topAnchor),
            hosting.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
            hosting.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            hosting.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ])
        hosting.didMove(toParent: self)
    }
}

/// What the share sheet shows.
struct ShareView: View {
    let items: [NSExtensionItem]
    let onFinish: () -> Void
    let onCancel: () -> Void

    @State private var text = ""
    @State private var attachments: [SharedAttachment] = []
    @State private var destination: Destination = .daily
    @State private var saving = false
    @State private var failure: String?

    enum Destination: Hashable {
        /// Appended to today's note, which is where a link you saved on the
        /// bus belongs — filing it properly is a decision for later.
        case daily
        case newNote
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("Save") {
                    TextEditor(text: $text)
                        .frame(minHeight: 120)
                        .accessibilityLabel(Text("What to save"))
                    ForEach(attachments) { attachment in
                        Label(attachment.name, systemImage: attachment.symbol)
                            .foregroundStyle(DesignTokens.textMuted.color)
                    }
                }

                Section("Where") {
                    Picker("Where", selection: $destination) {
                        Text("Today's note").tag(Destination.daily)
                        Text("A new note").tag(Destination.newNote)
                    }
                    .pickerStyle(.inline)
                    .labelsHidden()
                }

                if let failure {
                    Section {
                        Label(failure, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(DesignTokens.textError.color)
                    }
                }
            }
            .navigationTitle("Save to vault")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", action: onCancel)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(saving || (text.isEmpty && attachments.isEmpty))
                }
            }
            .task { await load() }
        }
    }

    private func load() async {
        for item in items {
            for provider in item.attachments ?? [] {
                if let shared = await SharedAttachment.from(provider) {
                    switch shared.payload {
                    case .text(let value):
                        text += text.isEmpty ? value : "\n\n\(value)"
                    case .file:
                        attachments.append(shared)
                    }
                }
            }
        }
    }

    private func save() async {
        saving = true
        defer { saving = false }
        do {
            try await ShareWriter.save(
                text: text,
                attachments: attachments,
                destination: destination
            )
            onFinish()
        } catch {
            // Never silently: the user is handing over something they want
            // kept, and a share sheet that closes without saving is the worst
            // possible outcome.
            failure = error.localizedDescription
        }
    }
}

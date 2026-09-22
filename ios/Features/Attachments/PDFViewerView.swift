import PDFKit
import SwiftUI

/// Reading a PDF that lives in the vault.
///
/// PDFKit rather than a web view: it handles a hundred-page document without
/// loading all of it, and it knows about text selection and search, which a
/// rendered image would not.
struct PDFViewerView: View {
    @Environment(AppModel.self) private var model

    let path: String

    @State private var document: PDFDocument?
    @State private var failure: String?

    var body: some View {
        Group {
            if let document {
                PDFKitView(document: document)
            } else if let failure {
                ContentUnavailableView(
                    "This PDF could not be opened",
                    systemImage: "doc.richtext",
                    description: Text(failure)
                )
            } else {
                ProgressView()
            }
        }
        .navigationTitle((path as NSString).lastPathComponent)
        .navigationBarTitleDisplayMode(.inline)
        .task { await load() }
    }

    private func load() async {
        do {
            // Through the service, so an evicted iCloud file is downloaded
            // first rather than read as an empty stub.
            let bytes = try await model.service.readAttachment(at: path)
            guard let parsed = PDFDocument(data: bytes) else {
                failure = String(localized: "The file is not a readable PDF.")
                return
            }
            document = parsed
        } catch {
            failure = error.localizedDescription
        }
    }
}

private struct PDFKitView: UIViewRepresentable {
    let document: PDFDocument

    func makeUIView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        // Continuous vertical scrolling, which is how a phone reads a
        // document; page-at-a-time is a desktop habit.
        view.displayMode = .singlePageContinuous
        view.displayDirection = .vertical
        view.backgroundColor = .clear
        view.document = document
        return view
    }

    func updateUIView(_ view: PDFView, context: Context) {
        if view.document !== document {
            view.document = document
        }
    }
}

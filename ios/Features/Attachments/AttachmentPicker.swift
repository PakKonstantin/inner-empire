import PDFKit
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers
import VisionKit

/// Getting a file into the vault from the places a phone keeps them.
///
/// Four sources, because a phone has four: the photo library, the camera, the
/// document scanner, and the Files app. Where the result lands is not decided
/// here — `importAttachment` asks the core, which reads the vault's own
/// settings, so a screenshot filed on a phone goes where one filed on a
/// desktop goes.
struct AttachmentPicker: ViewModifier {
    @Environment(AppModel.self) private var model

    @Binding var isPresented: Bool
    /// The note the attachment is for, which decides the folder under the
    /// note-relative settings.
    let notePath: String
    /// Called with the Markdown to insert once the file is in the vault.
    let onImported: (String) -> Void

    @State private var source: Source?
    @State private var photo: PhotosPickerItem?
    @State private var failure: String?

    private enum Source: String, Identifiable {
        case photos, camera, scanner, files
        var id: String { rawValue }
    }

    func body(content: Content) -> some View {
        content
            .confirmationDialog("Add an attachment", isPresented: $isPresented) {
                Button("Photo Library") { source = .photos }
                // Only offered where they exist: a simulator has no camera,
                // and a button that cannot work is worse than no button.
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    Button("Take a Photo") { source = .camera }
                }
                if VNDocumentCameraViewController.isSupported {
                    Button("Scan a Document") { source = .scanner }
                }
                Button("Files") { source = .files }
                Button("Cancel", role: .cancel) {}
            }
            .photosPicker(
                isPresented: .init(
                    get: { source == .photos },
                    set: { if !$0 { source = nil } }
                ),
                selection: $photo,
                matching: .any(of: [.images, .videos])
            )
            .fullScreenCover(item: $source) { which in
                switch which {
                case .camera:
                    CameraPicker { image in
                        Task { await store(image.jpegData(compressionQuality: 0.9), as: "Photo.jpg") }
                    }
                    .ignoresSafeArea()
                case .scanner:
                    DocumentScanner { pdf in
                        Task { await store(pdf, as: "Scan.pdf") }
                    }
                    .ignoresSafeArea()
                case .photos, .files:
                    // Handled by their own presenters above and below.
                    EmptyView()
                }
            }
            .fileImporter(
                isPresented: .init(
                    get: { source == .files },
                    set: { if !$0 { source = nil } }
                ),
                allowedContentTypes: [.item],
                allowsMultipleSelection: true
            ) { result in
                Task { await importFiles(result) }
            }
            .onChange(of: photo) { _, item in
                guard let item else { return }
                Task {
                    let data = try? await item.loadTransferable(type: Data.self)
                    // `supportedContentTypes` is empty for some library items,
                    // so the extension is a best guess rather than a promise.
                    let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                    await store(data, as: "Image.\(ext)")
                    photo = nil
                }
            }
            .alert(
                "That attachment could not be saved",
                isPresented: .init(get: { failure != nil }, set: { if !$0 { failure = nil } })
            ) {
                Button("OK") { failure = nil }
            } message: {
                Text(failure ?? "")
            }
    }

    private func importFiles(_ result: Result<[URL], Error>) async {
        source = nil
        guard case .success(let urls) = result else {
            if case .failure(let error) = result { failure = error.localizedDescription }
            return
        }
        for url in urls {
            // A file from the picker is security-scoped and outside the vault,
            // so it has to be read under its own access, not the vault's.
            guard url.startAccessingSecurityScopedResource() else {
                failure = String(localized: "\(url.lastPathComponent) could not be opened.")
                continue
            }
            defer { url.stopAccessingSecurityScopedResource() }
            await store(try? Data(contentsOf: url), as: url.lastPathComponent)
        }
    }

    private func store(_ data: Data?, as name: String) async {
        source = nil
        guard let data else {
            failure = String(localized: "That file could not be read.")
            return
        }
        do {
            let stored = try await model.service.importAttachment(
                named: name,
                bytes: data,
                forNote: notePath
            )
            onImported(try await model.service.embed(for: stored))
        } catch {
            failure = error.localizedDescription
        }
    }
}

extension View {
    /// Offer the attachment sources, inserting Markdown when one is chosen.
    func attachmentPicker(
        isPresented: Binding<Bool>,
        notePath: String,
        onImported: @escaping (String) -> Void
    ) -> some View {
        modifier(
            AttachmentPicker(isPresented: isPresented, notePath: notePath, onImported: onImported)
        )
    }
}

/// The camera, which SwiftUI still has no native picker for.
private struct CameraPicker: UIViewControllerRepresentable {
    @Environment(\.dismiss) private var dismiss
    let onCapture: (UIImage) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let controller = UIImagePickerController()
        controller.sourceType = .camera
        controller.delegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onCapture: onCapture, dismiss: { dismiss() })
    }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let onCapture: (UIImage) -> Void
        let dismiss: () -> Void

        init(onCapture: @escaping (UIImage) -> Void, dismiss: @escaping () -> Void) {
            self.onCapture = onCapture
            self.dismiss = dismiss
        }

        func imagePickerController(
            _ picker: UIImagePickerController,
            didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
        ) {
            if let image = info[.originalImage] as? UIImage {
                onCapture(image)
            }
            dismiss()
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            dismiss()
        }
    }
}

/// The document scanner, which produces a multi-page PDF rather than photos —
/// which is what makes a scanned receipt one attachment instead of four.
private struct DocumentScanner: UIViewControllerRepresentable {
    @Environment(\.dismiss) private var dismiss
    let onScan: (Data) -> Void

    func makeUIViewController(context: Context) -> VNDocumentCameraViewController {
        let controller = VNDocumentCameraViewController()
        controller.delegate = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: VNDocumentCameraViewController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onScan: onScan, dismiss: { dismiss() })
    }

    final class Coordinator: NSObject, VNDocumentCameraViewControllerDelegate {
        let onScan: (Data) -> Void
        let dismiss: () -> Void

        init(onScan: @escaping (Data) -> Void, dismiss: @escaping () -> Void) {
            self.onScan = onScan
            self.dismiss = dismiss
        }

        func documentCameraViewController(
            _ controller: VNDocumentCameraViewController,
            didFinishWith scan: VNDocumentCameraScan
        ) {
            let document = PDFDocument()
            for page in 0..<scan.pageCount {
                if let pdfPage = PDFPage(image: scan.imageOfPage(at: page)) {
                    document.insert(pdfPage, at: document.pageCount)
                }
            }
            if let data = document.dataRepresentation() {
                onScan(data)
            }
            dismiss()
        }

        func documentCameraViewControllerDidCancel(_ controller: VNDocumentCameraViewController) {
            dismiss()
        }

        func documentCameraViewController(
            _ controller: VNDocumentCameraViewController,
            didFailWithError error: Error
        ) {
            dismiss()
        }
    }
}

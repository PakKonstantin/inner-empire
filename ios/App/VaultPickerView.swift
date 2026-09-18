import SwiftUI
import UniformTypeIdentifiers

/// Choosing a vault.
///
/// A vault is any folder of Markdown files — there is no import step and no
/// container format, which is the premise the whole thing rests on. So this
/// screen is a folder picker and a list of folders already seen, and nothing
/// else.
struct VaultPickerView: View {
    @Environment(AppModel.self) private var model

    /// Why the last attempt failed, when there was one. Shown rather than
    /// swallowed: "the folder moved or its provider signed out" tells the user
    /// what to do, and an empty picker does not.
    let reason: String?

    @State private var openingExisting = false
    /// Naming comes first, so the name is known by the time a folder is
    /// chosen — a picker cannot ask for one.
    @State private var namingNewVault = false
    @State private var choosingFolderForNewVault = false
    @State private var newVaultName = ""

    var body: some View {
        NavigationStack {
            List {
                if let reason {
                    Section {
                        Label {
                            Text(reason)
                        } icon: {
                            Image(systemName: "exclamationmark.triangle")
                        }
                        .foregroundStyle(DesignTokens.textError.color)
                    }
                }

                Section {
                    Button {
                        openingExisting = true
                    } label: {
                        Label("Open a folder", systemImage: "folder")
                    }
                    Button {
                        newVaultName = ""
                        namingNewVault = true
                    } label: {
                        Label("Create a vault", systemImage: "folder.badge.plus")
                    }
                } footer: {
                    Text("A vault is an ordinary folder of Markdown files. It can live on this device, in iCloud Drive, or anywhere Files can reach.")
                }

                if !model.knownVaults.isEmpty {
                    Section("Recent") {
                        ForEach(model.knownVaults, id: \.vaultID) { bookmark in
                            Button {
                                Task { await model.reopen(bookmark) }
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(bookmark.name)
                                    Text(bookmark.lastOpened, style: .relative)
                                        .font(.caption)
                                        .foregroundStyle(DesignTokens.textMuted.color)
                                }
                            }
                            .swipeActions {
                                Button(role: .destructive) {
                                    Task { await model.forget(bookmark) }
                                } label: {
                                    Label("Forget", systemImage: "minus.circle")
                                }
                            }
                        }
                    }
                }
            }
            .navigationTitle("Inner Empire")
            .fileImporter(
                isPresented: $openingExisting,
                allowedContentTypes: [.folder],
                allowsMultipleSelection: false
            ) { result in
                handle(result, createNamed: nil)
            }
            .fileImporter(
                isPresented: $choosingFolderForNewVault,
                allowedContentTypes: [.folder],
                allowsMultipleSelection: false
            ) { result in
                handle(result, createNamed: newVaultName.isEmpty ? "My Vault" : newVaultName)
            }
            .alert("Name your vault", isPresented: $namingNewVault) {
                TextField("My Vault", text: $newVaultName)
                Button("Choose a folder") { choosingFolderForNewVault = true }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Pick where it should live next. The folder you choose becomes the vault.")
            }
        }
    }

    private func handle(_ result: Result<[URL], Error>, createNamed name: String?) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            Task {
                if let name {
                    await model.createVault(at: url, named: name)
                } else {
                    await model.open(pickedURL: url)
                }
            }
        case .failure:
            // Cancelling the picker arrives here too, and is not an error.
            break
        }
    }
}

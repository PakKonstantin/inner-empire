import SwiftUI

/// What the app shows, decided by how far along opening a vault is.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            switch model.stage {
            case .starting:
                ProgressView()
                    .accessibilityLabel(Text("Starting"))
            case .choosingVault(let reason):
                VaultPickerView(reason: reason)
            case .opening(let name):
                OpeningView(name: name, progress: nil, scanned: 0)
            case .indexing(let progress, let scanned):
                OpeningView(name: model.vault?.name ?? "", progress: progress, scanned: scanned)
            case .ready:
                VaultShellView()
            case .failed(let message):
                FailureView(message: message)
            }
        }
        .background(DesignTokens.backgroundPrimary.color)
    }
}

/// The shell once a vault is open.
///
/// iPhone and iPad get genuinely different navigation rather than one shrunk
/// to fit: a phone is a stack of full-screen views reached from a tab bar, an
/// iPad is columns side by side. Chosen on the size class, not the idiom, so a
/// Slide Over window on iPad behaves like a phone — which is what it is.
struct VaultShellView: View {
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        if sizeClass == .compact {
            PhoneShell()
        } else {
            PadShell()
        }
    }
}

/// Four tabs, and not one more.
///
/// §27 warns against overloading the bar, and the warning is right: backlinks,
/// the outline and properties are things you want *about the note you are
/// reading*, so they belong to that screen as sheets, not as destinations
/// competing with it.
private struct PhoneShell: View {
    @State private var selection: Tab = .notes

    enum Tab: Hashable {
        case notes, search, graph, settings
    }

    var body: some View {
        TabView(selection: $selection) {
            Tab.notes.view
                .tabItem { Label("Notes", systemImage: "doc.text") }
                .tag(Tab.notes)

            Tab.search.view
                .tabItem { Label("Search", systemImage: "magnifyingglass") }
                .tag(Tab.search)

            Tab.graph.view
                .tabItem { Label("Graph", systemImage: "point.3.connected.trianglepath.dotted") }
                .tag(Tab.graph)

            Tab.settings.view
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .tag(Tab.settings)
        }
    }
}

private extension PhoneShell.Tab {
    @ViewBuilder var view: some View {
        switch self {
        case .notes: NavigationStack { NotesView() }
        case .search: NavigationStack { SearchView() }
        case .graph: NavigationStack { GraphView() }
        case .settings: NavigationStack { SettingsView() }
        }
    }
}

/// Columns, because there is room for them.
///
/// Two under roughly 1000pt and three above, so a note, its tree and its
/// backlinks can be visible at once — which is the actual reason to use an
/// iPad for this rather than a phone.
private struct PadShell: View {
    @Environment(AppModel.self) private var model
    @State private var columnVisibility: NavigationSplitViewVisibility = .all
    @State private var selectedNote: String?

    var body: some View {
        GeometryReader { geometry in
            NavigationSplitView(columnVisibility: $columnVisibility) {
                NotesView(selection: $selectedNote)
                    .navigationSplitViewColumnWidth(min: 240, ideal: 300, max: 420)
            } content: {
                if let selectedNote {
                    NavigationStack { NoteEditorView(path: selectedNote) }
                } else {
                    ContentUnavailableView(
                        "No note selected",
                        systemImage: "doc.text",
                        description: Text("Pick a note from the list.")
                    )
                }
            } detail: {
                if geometry.size.width >= 1000, let selectedNote {
                    NavigationStack { NoteInspector(path: selectedNote) }
                } else {
                    // Under three columns the inspector is a sheet from the
                    // note instead, so it is never simply missing.
                    EmptyView()
                }
            }
            .navigationSplitViewStyle(.balanced)
            .onCommand(.toggleSidebar) {
                withAnimation {
                    columnVisibility = columnVisibility == .all ? .detailOnly : .all
                }
            }
            .onCommand(.dailyNote) {
                Task {
                    guard let daily = try? await model.service.dailyNote() else { return }
                    selectedNote = daily.path
                }
            }
        }
    }
}

private struct OpeningView: View {
    let name: String
    let progress: Double?
    let scanned: Int

    var body: some View {
        VStack(spacing: DesignTokens.spacingMd) {
            if let progress {
                ProgressView(value: progress)
                    .frame(maxWidth: 260)
            } else {
                ProgressView()
            }
            Text(name.isEmpty ? String(localized: "Opening your vault") : name)
                .font(.headline)
            // Not "loading…": the number says what is happening and roughly
            // how long is left, which a spinner cannot.
            Text(scanned > 0
                 ? String(localized: "Indexing \(scanned) files")
                 : String(localized: "Reading your notes"))
                .font(.footnote)
                .foregroundStyle(DesignTokens.textMuted.color)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
    }
}

private struct FailureView: View {
    @Environment(AppModel.self) private var model
    let message: String

    var body: some View {
        ContentUnavailableView {
            Label("This vault could not be opened", systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button("Choose another vault") {
                Task { await model.start() }
            }
        }
    }
}

import SwiftUI

/// Search, over the same engine the desktop uses.
///
/// Not a second implementation: the query language, the FTS5 index and the
/// ranking all come from the core, so `tag:project status:active` means the
/// same thing here as it does on a laptop.
struct SearchView: View {
    @Environment(AppModel.self) private var model

    @State private var query = ""
    @State private var results: SearchResults?
    @State private var searchError: String?
    @State private var task: Task<Void, Never>?

    var body: some View {
        List {
            if let searchError {
                Label(searchError, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(DesignTokens.textError.color)
            } else if let results {
                if results.hits.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                } else {
                    if results.truncated {
                        Text("Showing the first \(results.hits.count) of \(Int(results.total))")
                            .font(.caption)
                            .foregroundStyle(DesignTokens.textMuted.color)
                    }
                    ForEach(results.hits, id: \.path) { hit in
                        NavigationLink(value: hit.path) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(hit.title).font(.headline)
                                if !hit.snippet.isEmpty {
                                    SnippetText(hit.snippet)
                                        .font(.callout)
                                        .lineLimit(3)
                                }
                                Text(hit.path)
                                    .font(.caption)
                                    .foregroundStyle(DesignTokens.textFaint.color)
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Search")
        .navigationDestination(for: String.self) { NoteDetailPlaceholder(path: $0) }
        .searchable(text: $query, prompt: Text("Search notes, tags and properties"))
        .onChange(of: query) { _, text in
            // Debounced rather than per-keystroke: the index is fast, but a
            // query per character is still work the battery pays for.
            task?.cancel()
            task = Task {
                try? await Task.sleep(for: .milliseconds(180))
                guard !Task.isCancelled else { return }
                await run(text)
            }
        }
    }

    private func run(_ text: String) async {
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty else {
            results = nil
            searchError = nil
            return
        }
        do {
            results = try await model.service.search(text)
            searchError = nil
        } catch let error as FfiError {
            // A half-typed query is not a failure worth shouting about; an
            // unparseable one should say why.
            if case .InvalidQuery(let message) = error {
                searchError = message
            } else {
                searchError = error.localizedDescription
            }
            results = nil
        } catch {
            searchError = error.localizedDescription
            results = nil
        }
    }
}

/// Renders the core's snippet, turning its delimiters into emphasis.
///
/// The core marks matches with two control characters rather than `<mark>`,
/// precisely so a native client can build an `AttributedString` and a note
/// containing the literal text `<mark>` is not mistaken for a highlight.
struct SnippetText: View {
    private let attributed: AttributedString

    init(_ snippet: String) {
        var result = AttributedString()
        var highlighted = false
        for run in snippet.split(separator: "\u{2}", omittingEmptySubsequences: false) {
            for part in run.split(separator: "\u{3}", omittingEmptySubsequences: false) {
                var piece = AttributedString(String(part))
                if highlighted {
                    piece.foregroundColor = DesignTokens.accent.color
                    piece.inlinePresentationIntent = .stronglyEmphasized
                }
                result += piece
                highlighted = false
            }
            highlighted = true
        }
        attributed = result
    }

    var body: some View {
        Text(attributed)
    }
}

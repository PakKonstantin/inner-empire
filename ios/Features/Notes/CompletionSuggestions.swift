import SwiftUI

/// What to offer while typing `[[` or `#`.
///
/// The decision of *whether* to offer anything, and what the partial query is,
/// comes from the core — that is the fiddly part, and it is tested. This holds
/// the results and the selection, which is view state.
@MainActor
@Observable
final class CompletionSuggestions {
    struct Item: Identifiable, Equatable {
        let id: String
        let title: String
        /// The path or parent tag, shown underneath so two notes with the same
        /// name are distinguishable.
        let detail: String
        /// What gets inserted, which is not always what is displayed.
        let insert: String
    }

    private(set) var items: [Item] = []
    private(set) var trigger: CompletionTrigger = .none
    var highlighted: Int = 0

    private let service: VaultService
    private var task: Task<Void, Never>?

    init(service: VaultService) {
        self.service = service
    }

    var isShowing: Bool { !items.isEmpty }

    /// Re-evaluate for the current cursor. Cheap when nothing is triggered,
    /// which is almost always.
    func update(text: String, cursor: UInt32) {
        let trigger = completionAt(text: text, cursor: cursor)
        guard trigger != self.trigger else { return }
        self.trigger = trigger
        highlighted = 0

        task?.cancel()
        switch trigger {
        case .none:
            items = []
        case .wikiLink(let query, _, _, _):
            task = Task { await loadNotes(matching: query) }
        case .tag(let query, _, _):
            task = Task { await loadTags(matching: query) }
        }
    }

    private func loadNotes(matching query: String) async {
        // The same fuzzy matcher the quick switcher uses, so "otn" finds
        // "Other Note" here exactly as it does there.
        let matches = (try? await service.quickSwitch(query, limit: 25)) ?? []
        guard !Task.isCancelled else { return }
        items = matches.map { match in
            Item(
                id: match.path,
                title: match.title,
                detail: match.path,
                // The shortest form that resolves: the stem, since the core
                // resolves a bare name when it is unambiguous.
                insert: (match.path as NSString).deletingPathExtension
            )
        }
    }

    private func loadTags(matching query: String) async {
        let all = (try? await service.tags()) ?? []
        guard !Task.isCancelled else { return }
        let lowered = query.lowercased()
        items = all
            .filter { lowered.isEmpty || $0.name.lowercased().contains(lowered) }
            // Most-used first: a tag on forty notes is a likelier target than
            // one used once, and alphabetical would bury it.
            .sorted { $0.totalCount > $1.totalCount }
            .prefix(25)
            .map { tag in
                Item(
                    id: tag.name,
                    title: tag.name,
                    detail: String(localized: "\(Int(tag.totalCount)) notes"),
                    insert: tag.name
                )
            }
    }

    /// Insert the highlighted item, returning the new text and cursor.
    func accept(_ item: Item, in text: String) -> EditResult? {
        guard trigger != .none else { return nil }
        return applyCompletion(text: text, trigger: trigger, choice: item.insert)
    }

    func dismiss() {
        task?.cancel()
        items = []
        trigger = .none
    }
}

/// The list that appears above the keyboard while completing.
struct CompletionSuggestionsBar: View {
    let suggestions: CompletionSuggestions
    let onAccept: (CompletionSuggestions.Item) -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: DesignTokens.spacingSm) {
                    ForEach(Array(suggestions.items.enumerated()), id: \.element.id) { index, item in
                        Button {
                            onAccept(item)
                        } label: {
                            VStack(alignment: .leading, spacing: 1) {
                                Text(item.title)
                                    .font(.callout)
                                    .lineLimit(1)
                                Text(item.detail)
                                    .font(.caption2)
                                    .foregroundStyle(DesignTokens.textFaint.color)
                                    .lineLimit(1)
                            }
                            .padding(.horizontal, DesignTokens.spacingSm)
                            .padding(.vertical, DesignTokens.spacingXs)
                            .frame(minHeight: 44)
                            .background(
                                RoundedRectangle(cornerRadius: DesignTokens.cornerRadiusSmall)
                                    .fill(index == suggestions.highlighted
                                          ? DesignTokens.backgroundModifierActive.color
                                          : DesignTokens.backgroundSecondary.color)
                            )
                        }
                        .id(item.id)
                        .accessibilityLabel(Text("\(item.title), \(item.detail)"))
                        .accessibilityHint(Text("Inserts this"))
                    }
                }
                .padding(.horizontal, DesignTokens.spacingSm)
            }
            .frame(height: 56)
            .background(.bar)
            .onChange(of: suggestions.highlighted) { _, index in
                guard suggestions.items.indices.contains(index) else { return }
                withAnimation { proxy.scrollTo(suggestions.items[index].id) }
            }
        }
    }
}

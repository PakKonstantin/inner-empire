import AppIntents
import Foundation

/// Shortcuts, Siri and the Action button.
///
/// Each intent opens the vault, does one thing, and closes. They run in the
/// app's process but often without its UI, and an intent that assumed a
/// running app would fail whenever it was invoked from the Lock Screen.
///
/// Deliberately few: capturing a thought, opening today's note, and finding
/// something. §36 asks for exactly the ones worth having on a Lock Screen,
/// and an intent nobody can describe out loud is one nobody will use.
struct AppendToDailyNoteIntent: AppIntent {
    static let title: LocalizedStringResource = "Add to Today's Note"
    static let description = IntentDescription(
        "Appends a line to today's daily note, creating it if it does not exist yet."
    )
    /// No UI needed, so it runs from the Lock Screen without unlocking.
    static let openAppWhenRun = false

    @Parameter(title: "Text", requestValueDialog: "What would you like to note?")
    var text: String

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let service = try await IntentVault.service()
        let daily = try await service.dailyNote()
        let existing = try await service.note(at: daily.path).content
        let separator = existing.isEmpty || existing.hasSuffix("\n") ? "" : "\n"
        // Appended, never overwritten: the day's note belongs to the user.
        _ = try await service.saveOverwriting(existing + separator + "- " + text + "\n", to: daily.path)
        return .result(dialog: "Added to today's note.")
    }
}

struct OpenDailyNoteIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Today's Note"
    static let description = IntentDescription("Opens today's daily note in Inner Empire.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        let service = try await IntentVault.service()
        let daily = try await service.dailyNote()
        await IntentVault.requestOpen(daily.path)
        return .result()
    }
}

struct SearchNotesIntent: AppIntent {
    static let title: LocalizedStringResource = "Search Notes"
    static let description = IntentDescription("Searches the vault and lists what matches.")
    static let openAppWhenRun = false

    @Parameter(title: "Query", requestValueDialog: "What are you looking for?")
    var query: String

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<[String]> & ProvidesDialog {
        let service = try await IntentVault.service()
        let results = try await service.search(query, options: SearchOptions(limit: 20))
        let titles = results.files.map(\.title)
        return .result(
            value: titles,
            // The count rather than the list: Siri reading twenty note titles
            // aloud is not an answer, it is a punishment.
            dialog: titles.isEmpty
                ? "Nothing matched \(query)."
                : "\(titles.count) notes matched \(query)."
        )
    }
}

/// What the Shortcuts app offers without being asked.
struct InnerEmpireShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: AppendToDailyNoteIntent(),
            phrases: [
                "Add to my \(.applicationName) daily note",
                "Note this in \(.applicationName)",
            ],
            shortTitle: "Add to Today",
            systemImageName: "square.and.pencil"
        )
        AppShortcut(
            intent: OpenDailyNoteIntent(),
            phrases: ["Open my \(.applicationName) daily note"],
            shortTitle: "Today's Note",
            systemImageName: "calendar"
        )
        AppShortcut(
            intent: SearchNotesIntent(),
            phrases: ["Search \(.applicationName)"],
            shortTitle: "Search",
            systemImageName: "magnifyingglass"
        )
    }
}

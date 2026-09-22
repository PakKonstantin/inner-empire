import SwiftUI
import WidgetKit

/// What the widget shows.
///
/// Read-only and deliberately small: a widget that tried to be the app would
/// be slow to refresh and useless at a glance. Today's note and a few pinned
/// ones is what someone actually wants from a home screen — the question
/// "what was I working on" answered without unlocking anything.
struct VaultEntry: TimelineEntry {
    let date: Date
    let dailyNoteTitle: String?
    let favourites: [String]
    /// Set when the vault could not be read, so the widget says why instead of
    /// showing an empty box that looks broken.
    let unavailable: String?

    static let placeholder = VaultEntry(
        date: .now,
        dailyNoteTitle: "Today",
        favourites: ["Project Notes", "Reading List"],
        unavailable: nil
    )
}

struct VaultProvider: TimelineProvider {
    func placeholder(in context: Context) -> VaultEntry { .placeholder }

    func getSnapshot(in context: Context, completion: @escaping (VaultEntry) -> Void) {
        // The gallery preview must never block on opening a vault.
        if context.isPreview {
            completion(.placeholder)
            return
        }
        Task { completion(await read()) }
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<VaultEntry>) -> Void) {
        Task {
            let entry = await read()
            // Refreshed at the next midnight rather than on a short interval:
            // what changes here is which day it is, and the system throttles
            // frequent refreshes anyway. A write from the app can ask for an
            // earlier reload with `WidgetCenter.reloadAllTimelines()`.
            let midnight = Calendar.current.nextDate(
                after: .now,
                matching: DateComponents(hour: 0, minute: 1),
                matchingPolicy: .nextTime
            ) ?? Date.now.addingTimeInterval(3600)
            completion(Timeline(entries: [entry], policy: .after(midnight)))
        }
    }

    /// Open the vault, read two small things, close.
    ///
    /// A widget process has the tightest memory limit of any of them, so this
    /// does the least it can and holds nothing.
    private func read() async -> VaultEntry {
        do {
            let service = try await IntentVault.service()
            let daily = try await service.dailyNotePath()
            let pinned = try await service.favourites()
            return VaultEntry(
                date: .now,
                dailyNoteTitle: (daily as NSString).lastPathComponent
                    .replacingOccurrences(of: ".md", with: ""),
                favourites: Array(pinned.prefix(3)).map(\.title),
                unavailable: nil
            )
        } catch {
            return VaultEntry(
                date: .now,
                dailyNoteTitle: nil,
                favourites: [],
                unavailable: String(localized: "Open Inner Empire to choose your vault.")
            )
        }
    }
}

struct VaultWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: VaultEntry

    var body: some View {
        if let unavailable = entry.unavailable {
            Text(unavailable)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                if let title = entry.dailyNoteTitle {
                    Link(destination: URL(string: "innerempire://daily")!) {
                        Label(title, systemImage: "calendar")
                            .font(.headline)
                            .lineLimit(1)
                    }
                }

                // The small family has room for the date and nothing else;
                // cramming three more lines in makes all four unreadable.
                if family != .systemSmall {
                    ForEach(entry.favourites, id: \.self) { note in
                        Label(note, systemImage: "star.fill")
                            .font(.callout)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct InnerEmpireWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "InnerEmpireVault", provider: VaultProvider()) { entry in
            VaultWidgetView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
        }
        .configurationDisplayName("Your Vault")
        .description("Today's note and the ones you pinned.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

@main
struct InnerEmpireWidgets: WidgetBundle {
    var body: some Widget {
        InnerEmpireWidget()
    }
}

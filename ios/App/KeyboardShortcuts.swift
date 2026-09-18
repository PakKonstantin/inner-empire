import SwiftUI

/// What a hardware keyboard can do.
///
/// §32 asks for these on iPad, and they are the difference between an iPad
/// with a keyboard being a laptop and being a large phone. They are declared
/// in one place so the Commands menu, the shortcut modifiers and the
/// discoverability overlay (hold ⌘) all say the same thing — three lists that
/// drift apart is how a shortcut ends up documented but not working.
enum AppCommand: String, CaseIterable, Identifiable {
    case newNote
    case quickSwitch
    case search
    case dailyNote
    case toggleSidebar
    case save

    var id: String { rawValue }

    var title: String {
        switch self {
        case .newNote: String(localized: "New Note")
        case .quickSwitch: String(localized: "Go to Note…")
        case .search: String(localized: "Search")
        case .dailyNote: String(localized: "Today's Note")
        case .toggleSidebar: String(localized: "Show or Hide Sidebar")
        case .save: String(localized: "Save")
        }
    }

    var key: KeyEquivalent {
        switch self {
        case .newNote: "n"
        case .quickSwitch: "o"
        case .search: "f"
        case .dailyNote: "d"
        case .toggleSidebar: "s"
        case .save: "s"
        }
    }

    var modifiers: EventModifiers {
        switch self {
        // ⌘S for save and ⌘⇧S for the sidebar: save is the one people press
        // by reflex, so it gets the unshifted chord even though the app
        // autosaves and it rarely has anything to do.
        case .newNote, .quickSwitch, .search, .save: [.command]
        case .dailyNote, .toggleSidebar: [.command, .shift]
        }
    }
}

/// Routes a command to whatever is on screen.
///
/// A notification rather than a binding threaded through every view: the
/// menu bar lives above the navigation stack and cannot reach into it, and
/// passing a closure down six levels to make ⌘F work would be worse.
@MainActor
enum CommandBus {
    static let notification = Notification.Name("ie.command")

    static func send(_ command: AppCommand) {
        NotificationCenter.default.post(
            name: notification,
            object: nil,
            userInfo: ["command": command.rawValue]
        )
    }
}

extension View {
    /// Act on one command while this view is on screen.
    func onCommand(_ wanted: AppCommand, perform action: @escaping () -> Void) -> some View {
        onReceive(NotificationCenter.default.publisher(for: CommandBus.notification)) { note in
            guard let raw = note.userInfo?["command"] as? String,
                  raw == wanted.rawValue else { return }
            action()
        }
    }
}

/// The menu bar, which on iPad is also the ⌘-hold overlay.
struct AppCommands: Commands {
    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            button(.newNote)
            button(.dailyNote)
            Divider()
            button(.quickSwitch)
            button(.search)
        }
        CommandGroup(replacing: .saveItem) {
            button(.save)
        }
        CommandGroup(after: .sidebar) {
            button(.toggleSidebar)
        }
    }

    private func button(_ command: AppCommand) -> some View {
        Button(command.title) {
            CommandBus.send(command)
        }
        .keyboardShortcut(command.key, modifiers: command.modifiers)
    }
}

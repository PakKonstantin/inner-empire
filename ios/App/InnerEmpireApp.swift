import SwiftUI

@main
struct InnerEmpireApp: App {
    @State private var model = AppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .task { await model.start() }
        }
        .onChange(of: scenePhase) { _, phase in
            Task { await model.scenePhaseChanged(to: phase) }
        }
        // On iPad this is both the menu bar and the overlay you get by holding
        // ⌘, which is how anyone discovers these exist.
        .commands { AppCommands() }
    }
}
